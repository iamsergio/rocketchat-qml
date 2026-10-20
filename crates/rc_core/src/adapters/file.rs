// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! A secret store over a plain file, so a GUI test can restart the app without touching the user's
//! keyring. The token is written in clear text: only built with the `test-file-store` feature
//! (and in this crate's own tests), never in a release build.

use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use super::blob::{decode, encode};
use crate::domain::StoredSession;
use crate::ports::{SecretStore, StoreError};

/// Keeps the session in one file, as JSON.
pub struct FileStore {
    path: PathBuf,
}

impl FileStore {
    /// The file is created by the first save. Its directory must exist.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    fn unavailable(&self, error: &io::Error) -> StoreError {
        StoreError::Unavailable(format!("{}: {error}", self.path.display()))
    }
}

impl SecretStore for FileStore {
    fn load(&self) -> Result<Option<StoredSession>, StoreError> {
        match fs::read_to_string(&self.path) {
            Ok(text) => decode(&text).map(Some),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(self.unavailable(&error)),
        }
    }

    fn save(&self, session: &StoredSession) -> Result<(), StoreError> {
        let text = encode(session)?;
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        // Only the owner may read the token.
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        options
            .open(&self.path)
            .and_then(|mut file| file.write_all(text.as_bytes()))
            .map_err(|e| self.unavailable(&e))
    }

    fn clear(&self) -> Result<(), StoreError> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(self.unavailable(&error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::process;

    use secrecy::{ExposeSecret, SecretString};

    use super::*;
    use crate::domain::Credentials;

    /// A path in the temp directory that is unique to the test, and deleted with the guard.
    struct TempFile(PathBuf);

    impl TempFile {
        fn new(name: &str) -> Self {
            Self(
                std::env::temp_dir()
                    .join(format!("rc_core-file-store-{}-{name}.json", process::id())),
            )
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    fn session() -> StoredSession {
        StoredSession {
            server: "http://h".to_owned(),
            credentials: Credentials {
                user_id: "uid".to_owned().into(),
                token: SecretString::from("tok"),
            },
        }
    }

    // AUTH-004
    #[test]
    fn saves_loads_and_clears() {
        let file = TempFile::new("round-trip");
        let store = FileStore::new(&file.0);
        store.save(&session()).unwrap();
        // A second store on the same path, like the next run of the app.
        let loaded = FileStore::new(&file.0).load().unwrap().unwrap();
        assert_eq!(loaded.server, "http://h");
        assert_eq!(loaded.credentials.user_id.as_str(), "uid");
        assert_eq!(loaded.credentials.token.expose_secret(), "tok");
        store.clear().unwrap();
        assert!(!file.0.exists());
        assert!(store.load().unwrap().is_none());
    }

    // AUTH-004
    #[test]
    fn a_missing_file_is_nothing_stored() {
        let file = TempFile::new("missing");
        assert!(FileStore::new(&file.0).load().unwrap().is_none());
    }

    // AUTH-004
    #[test]
    fn clearing_a_missing_file_is_fine() {
        let file = TempFile::new("clear-missing");
        FileStore::new(&file.0).clear().unwrap();
    }

    // AUTH-004
    #[test]
    fn saving_replaces_the_previous_session() {
        let file = TempFile::new("replace");
        let store = FileStore::new(&file.0);
        store.save(&session()).unwrap();
        let mut other = session();
        other.server = "http://other".to_owned();
        store.save(&other).unwrap();
        assert_eq!(store.load().unwrap().unwrap().server, "http://other");
    }

    // AUTH-004
    #[test]
    fn a_corrupt_file_is_reported() {
        let file = TempFile::new("corrupt");
        fs::write(&file.0, "not json").unwrap();
        let result = FileStore::new(&file.0).load();
        assert!(matches!(result, Err(StoreError::Corrupt(_))), "{result:?}");
    }

    // AUTH-004
    #[cfg(unix)]
    #[test]
    fn only_the_owner_can_read_the_file() {
        use std::os::unix::fs::PermissionsExt;

        let file = TempFile::new("mode");
        FileStore::new(&file.0).save(&session()).unwrap();
        let mode = fs::metadata(&file.0).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "{mode:o}");
    }
}
