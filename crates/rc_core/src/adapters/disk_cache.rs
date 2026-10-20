// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The file cache on disk: `<root>/<server>/avatars/user/<username>.<ext>`.

use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::ports::{
    CacheError, CacheKey, CacheKind, FileCache, IMAGE_EXTENSIONS, Stored, image_extension,
};

/// Names longer than this keep their start and end with a hash of the whole.
const MAX_NAME: usize = 120;

pub struct DiskFileCache {
    root: PathBuf,
}

impl DiskFileCache {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn dir(&self, key: &CacheKey) -> PathBuf {
        let kind: &[&str] = match key.kind {
            CacheKind::UserAvatar => &["avatars", "user"],
            CacheKind::RoomAvatar => &["avatars", "room"],
        };
        let mut dir = self.root.join(file_name(&key.server));
        dir.extend(kind);
        dir
    }
}

impl FileCache for DiskFileCache {
    fn get(&self, key: &CacheKey) -> Option<PathBuf> {
        let dir = self.dir(key);
        let stem = file_name(&key.name);
        // Two copies exist only if a crash came between storing a new type and dropping the old.
        IMAGE_EXTENSIONS
            .iter()
            .filter_map(|extension| {
                let path = dir.join(format!("{stem}.{extension}"));
                let modified = fs::metadata(&path).ok()?.modified().ok()?;
                Some((modified, path))
            })
            .max()
            .map(|(_, path)| path)
    }

    fn put(
        &self,
        key: &CacheKey,
        content_type: Option<&str>,
        bytes: &[u8],
    ) -> Result<Stored, CacheError> {
        let extension = content_type
            .and_then(image_extension)
            .ok_or_else(|| CacheError::UnsupportedType(content_type.map(str::to_owned)))?;
        let io = |error: io::Error| CacheError::Io(error.to_string());
        let dir = self.dir(key);
        fs::create_dir_all(&dir).map_err(io)?;
        let stem = file_name(&key.name);
        let path = dir.join(format!("{stem}.{extension}"));
        let mut changed = fs::read(&path).map_or(true, |held| held != bytes);
        if changed {
            let temporary = dir.join(format!(".{stem}.{}.tmp", unique()));
            let written = fs::write(&temporary, bytes).and_then(|()| fs::rename(&temporary, &path));
            if let Err(error) = written {
                // Nothing to do if the clean-up fails too.
                let _ = fs::remove_file(&temporary);
                return Err(io(error));
            }
        }
        for other in IMAGE_EXTENSIONS.iter().filter(|other| **other != extension) {
            if fs::remove_file(dir.join(format!("{stem}.{other}"))).is_ok() {
                changed = true;
            }
        }
        Ok(Stored { path, changed })
    }

    fn remove(&self, key: &CacheKey) -> Result<bool, CacheError> {
        let dir = self.dir(key);
        let stem = file_name(&key.name);
        let mut removed = false;
        for extension in IMAGE_EXTENSIONS {
            match fs::remove_file(dir.join(format!("{stem}.{extension}"))) {
                Ok(()) => removed = true,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(CacheError::Io(error.to_string())),
            }
        }
        Ok(removed)
    }
}

/// Distinguishes the temporary files of this process.
fn unique() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

/// `name` as one file name, the same on every platform: letters, digits, `_`, `-` and `.` stay,
/// every other byte becomes `%XX`, and so does a leading dot (no `.` or `..`, no hidden files). The
/// escaping is injective, so two names never share a file. An empty name is a lone `%`, which the
/// escaping can't make.
fn file_name(name: &str) -> String {
    let mut escaped = String::with_capacity(name.len());
    for (at, byte) in name.bytes().enumerate() {
        let plain =
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-') || (byte == b'.' && at > 0);
        if plain {
            escaped.push(char::from(byte));
        } else {
            let _ = write!(escaped, "%{byte:02X}");
        }
    }
    if escaped.is_empty() {
        return "%".to_owned();
    }
    if escaped.len() > MAX_NAME {
        // FNV-1a: stable across runs and platforms, unlike the std hasher.
        let hash = name.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        });
        // Cut on a character boundary: the escaped text is ASCII.
        escaped.truncate(MAX_NAME - 17);
        let _ = write!(escaped, "~{hash:016x}");
    }
    escaped
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;
    use std::thread;

    use super::*;
    use crate::domain::AvatarOwner;

    /// A directory that goes away with the test.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("rc_core-cache-{}", unique()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn user(server: &str, username: &str) -> CacheKey {
        CacheKey::avatar(server, &AvatarOwner::User(username.to_owned()))
    }

    fn files_below(dir: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                found.extend(files_below(&path));
            } else {
                found.push(path);
            }
        }
        found
    }

    // RICH-004
    #[test]
    fn a_stored_file_is_found_by_its_key_under_the_server_and_kind() {
        let dir = TempDir::new();
        let cache = DiskFileCache::new(&dir.0);
        let key = user("http://127.0.0.1:3000/", "alice");
        assert_eq!(cache.get(&key), None);

        let stored = cache.put(&key, Some("image/png"), b"png").unwrap();

        assert!(stored.changed);
        assert_eq!(cache.get(&key), Some(stored.path.clone()));
        assert_eq!(fs::read(&stored.path).unwrap(), b"png");
        let relative = stored.path.strip_prefix(&dir.0).unwrap();
        assert_eq!(
            relative,
            Path::new("127.0.0.1%3A3000/avatars/user/alice.png")
        );
    }

    // RICH-004
    #[test]
    fn servers_and_kinds_keep_their_files_apart() {
        let dir = TempDir::new();
        let cache = DiskFileCache::new(&dir.0);
        let one = cache
            .put(&user("http://a:3000", "x"), Some("image/png"), b"1")
            .unwrap();
        let other_port = cache
            .put(&user("http://a:3001", "x"), Some("image/png"), b"2")
            .unwrap();
        let room = CacheKey::avatar("http://a:3000", &AvatarOwner::Room("x".to_owned().into()));
        let of_room = cache.put(&room, Some("image/png"), b"3").unwrap();
        let paths = [&one.path, &other_port.path, &of_room.path];
        assert_eq!(fs::read(paths[0]).unwrap(), b"1");
        assert_eq!(fs::read(paths[1]).unwrap(), b"2");
        assert_eq!(fs::read(paths[2]).unwrap(), b"3");
    }

    // RICH-004
    #[test]
    fn storing_the_same_bytes_again_changes_nothing() {
        let dir = TempDir::new();
        let cache = DiskFileCache::new(&dir.0);
        let key = user("http://h", "alice");
        cache.put(&key, Some("image/png"), b"one").unwrap();

        let same = cache.put(&key, Some("image/png"), b"one").unwrap();
        let other = cache.put(&key, Some("image/png"), b"two").unwrap();

        assert!(!same.changed);
        assert!(other.changed);
        assert_eq!(fs::read(&other.path).unwrap(), b"two");
    }

    // RICH-004
    #[test]
    fn a_new_file_type_replaces_the_old_file() {
        let dir = TempDir::new();
        let cache = DiskFileCache::new(&dir.0);
        let key = user("http://h", "alice");
        let svg = cache.put(&key, Some("image/svg+xml"), b"<svg/>").unwrap();

        let png = cache.put(&key, Some("image/png"), b"png").unwrap();

        assert!(png.changed);
        assert!(png.path.extension().is_some_and(|e| e == "png"));
        assert!(!svg.path.exists());
        assert_eq!(cache.get(&key), Some(png.path));
        assert_eq!(files_below(&dir.0).len(), 1);
    }

    // RICH-004
    #[test]
    fn the_same_bytes_in_another_type_still_count_as_a_change() {
        let dir = TempDir::new();
        let cache = DiskFileCache::new(&dir.0);
        let key = user("http://h", "alice");
        cache.put(&key, Some("image/png"), b"x").unwrap();
        assert!(cache.put(&key, Some("image/gif"), b"x").unwrap().changed);
    }

    // RICH-004
    #[test]
    fn a_type_the_cache_doesnt_keep_is_refused_and_stores_nothing() {
        let dir = TempDir::new();
        let cache = DiskFileCache::new(&dir.0);
        let key = user("http://h", "alice");
        for content_type in [Some("text/html"), None] {
            let result = cache.put(&key, content_type, b"<html>");
            assert!(
                matches!(result, Err(CacheError::UnsupportedType(_))),
                "{result:?}"
            );
        }
        assert_eq!(cache.get(&key), None);
        assert!(files_below(&dir.0).is_empty());
    }

    // RICH-004
    #[test]
    fn a_removed_file_is_gone_whatever_its_type_and_the_others_stay() {
        let dir = TempDir::new();
        let cache = DiskFileCache::new(&dir.0);
        let key = user("http://h", "alice");
        let other = user("http://h", "bob");
        cache.put(&key, Some("image/gif"), b"a").unwrap();
        let kept = cache.put(&other, Some("image/gif"), b"b").unwrap();

        assert!(cache.remove(&key).unwrap());

        assert_eq!(cache.get(&key), None);
        assert_eq!(cache.get(&other), Some(kept.path));
        // Nothing to remove the second time.
        assert!(!cache.remove(&key).unwrap());
        assert!(!cache.remove(&user("http://never", "x")).unwrap());
    }

    // RICH-004
    #[test]
    fn a_username_cannot_leave_its_directory() {
        let dir = TempDir::new();
        let cache = DiskFileCache::new(&dir.0);
        let hostile = [
            "../../../etc/passwd",
            "..",
            ".",
            "a/b",
            "a\\b",
            "C:\\x",
            "/abs",
            "a\0b",
            ".hidden",
            "ü",
            "",
        ];
        for name in hostile {
            let key = user("http://h", name);
            let stored = cache.put(&key, Some("image/png"), b"x").unwrap();
            let relative = stored.path.strip_prefix(&dir.0).unwrap();
            let parts: Vec<_> = relative.components().collect();
            // <server>/avatars/user/<file>
            assert_eq!(parts.len(), 4, "{name:?} -> {relative:?}");
            assert!(
                parts
                    .iter()
                    .all(|part| matches!(part, std::path::Component::Normal(_))),
                "{name:?} -> {relative:?}"
            );
            assert!(
                !stored
                    .path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with('.')
            );
            assert_eq!(cache.get(&key), Some(stored.path), "{name:?}");
        }
        // Every one has a file of its own.
        assert_eq!(files_below(&dir.0).len(), hostile.len());
    }

    // RICH-004
    #[test]
    fn a_hostile_server_name_stays_inside_the_root_too() {
        let dir = TempDir::new();
        let cache = DiskFileCache::new(&dir.0);
        let key = user("http://../../x/", "alice");
        let stored = cache.put(&key, Some("image/png"), b"x").unwrap();
        assert!(stored.path.strip_prefix(&dir.0).is_ok());
        assert_eq!(files_below(&dir.0), [stored.path]);
    }

    // RICH-004
    #[test]
    fn file_names_are_injective_and_bounded() {
        assert_eq!(file_name("alice.b-c_d"), "alice.b-c_d");
        assert_eq!(file_name("a/b"), "a%2Fb");
        assert_ne!(file_name("a/b"), file_name("a%2Fb"));
        assert_ne!(file_name("a_b"), file_name("a/b"));
        assert_eq!(file_name(""), "%");
        let long = "x".repeat(500);
        let other = format!("{}y", "x".repeat(499));
        assert!(file_name(&long).len() <= MAX_NAME);
        assert_ne!(file_name(&long), file_name(&other));
        assert!(file_name(&"ü".repeat(200)).len() <= MAX_NAME);
    }

    // RICH-004
    #[test]
    fn a_reader_never_sees_a_half_written_file() {
        let dir = TempDir::new();
        let cache = Arc::new(DiskFileCache::new(&dir.0));
        let key = user("http://h", "alice");
        let (first, second) = (vec![1_u8; 2_000_000], vec![2_u8; 3_000_000]);
        let path = cache.put(&key, Some("image/png"), &first).unwrap().path;

        let writer = {
            let (cache, key) = (Arc::clone(&cache), key.clone());
            let (first, second) = (first.clone(), second.clone());
            thread::spawn(move || {
                for round in 0..20 {
                    let bytes = if round % 2 == 0 { &second } else { &first };
                    cache.put(&key, Some("image/png"), bytes).unwrap();
                }
            })
        };
        while !writer.is_finished() {
            let read = fs::read(&path).unwrap();
            assert!(read == first || read == second, "{} bytes", read.len());
        }
        writer.join().unwrap();

        // Nothing is left behind but the file.
        assert_eq!(files_below(&dir.0), [path]);
    }
}
