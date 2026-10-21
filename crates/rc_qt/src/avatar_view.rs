// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the models show for an avatar: the URL of the file, and the initials on a colour while
//! there is none. Plain Rust, so it can be unit-tested without Qt.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

use rc_core::AvatarUpdate;
use rc_core::domain::{AvatarOwner, Message, MessageKind, Room};

/// The avatar files by owner, as the `file://` URLs QML loads them from. An owner without a file
/// has no entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AvatarUrls {
    urls: HashMap<AvatarOwner, String>,
    /// Counts how often the owner's file was announced: QML keeps images by URL, so a replaced
    /// file needs a new one.
    versions: HashMap<AvatarOwner, u64>,
}

impl AvatarUrls {
    /// Takes the updates in. Returns the owners whose URL is not what it was, each with its URL
    /// (empty when the avatar is gone).
    pub fn apply(&mut self, updates: &[AvatarUpdate]) -> Vec<(AvatarOwner, String)> {
        let mut changed = Vec::new();
        for update in updates {
            let version = self.versions.entry(update.owner.clone()).or_default();
            *version += 1;
            let url = if let Some(path) = &update.path {
                let url = file_url(path, *version);
                self.urls.insert(update.owner.clone(), url.clone());
                url
            } else {
                self.urls.remove(&update.owner);
                String::new()
            };
            changed.push((update.owner.clone(), url));
        }
        changed
    }

    /// The URL, or nothing if the owner has no avatar file.
    #[must_use]
    pub fn url(&self, owner: &AvatarOwner) -> &str {
        self.urls.get(owner).map_or("", String::as_str)
    }

    /// Takes the URLs of `changes` in, as [`apply`](Self::apply) returned them.
    pub fn set(&mut self, changes: &[(AvatarOwner, String)]) {
        for (owner, url) in changes {
            if url.is_empty() {
                self.urls.remove(owner);
            } else {
                self.urls.insert(owner.clone(), url.clone());
            }
        }
    }
}

/// The `file://` URL of `path`, with `?v=<version>` so that a replaced file is another image to QML.
/// The query is not part of the file: Qt takes the path of a local URL.
#[must_use]
pub fn file_url(path: &Path, version: u64) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut url = String::from("file://");
    if !text.starts_with('/') {
        // A Windows drive: file:///C:/...
        url.push('/');
    }
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/' | b':') {
            url.push(char::from(byte));
        } else {
            let _ = write!(url, "%{byte:02X}");
        }
    }
    let _ = write!(url, "?v={version}");
    url
}

/// Whose avatar the author of the message shows; none for a system message.
#[must_use]
pub fn message_owner(message: &Message) -> Option<AvatarOwner> {
    match message.kind {
        MessageKind::Text(_) => Some(AvatarOwner::User(message.author.username.clone())),
        MessageKind::System(_) => None,
    }
}

/// The rows, as runs `(first, last)` of consecutive rows, for which `uses` says the row shows one
/// of the avatars that changed: the only ones to announce.
#[must_use]
pub fn rows_using(
    count: usize,
    changed: &HashSet<AvatarOwner>,
    owner: impl Fn(usize) -> Option<AvatarOwner>,
) -> Vec<(usize, usize)> {
    runs(count, |row| {
        owner(row).is_some_and(|owner| changed.contains(&owner))
    })
}

/// The rows `0..count` for which `wanted` holds, as runs `(first, last)` of consecutive rows.
#[must_use]
pub fn runs(count: usize, wanted: impl Fn(usize) -> bool) -> Vec<(usize, usize)> {
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for row in (0..count).filter(|row| wanted(*row)) {
        match runs.last_mut() {
            Some((_, last)) if *last + 1 == row => *last = row,
            _ => runs.push((row, row)),
        }
    }
    runs
}

/// The owners in `changes`.
#[must_use]
pub fn owners(changes: &[(AvatarOwner, String)]) -> HashSet<AvatarOwner> {
    changes.iter().map(|(owner, _)| owner.clone()).collect()
}

/// The initials to show while there is no image: those of the first two words of `name` (the
/// display name, or the room's), upper case; `?` if it has no letters.
#[must_use]
pub fn initials(name: &str) -> String {
    let mut words = name
        .split(|c: char| c.is_whitespace() || matches!(c, '.' | '_' | '-'))
        .filter_map(|word| word.chars().find(|c| c.is_alphanumeric()));
    let mut initials: String = words
        .next()
        .into_iter()
        .flat_map(char::to_uppercase)
        .collect();
    initials.extend(words.next().into_iter().flat_map(char::to_uppercase));
    if initials.is_empty() {
        initials.push('?');
    }
    initials
}

/// The initials and the colour a room shows while it has no avatar image: the initials of its
/// name (for a direct message, the other user's), never of its id, and the colour of the other
/// user's username, or of the room's name.
#[must_use]
pub fn room_placeholder(room: &Room) -> (String, String) {
    (
        initials(&room.name),
        color(room.peer.as_deref().unwrap_or(&room.name)),
    )
}

/// Dark enough for white text.
const COLORS: [&str; 12] = [
    "#c0392b", "#d35400", "#b9770e", "#1e8449", "#117a65", "#1f618d", "#2e4eb0", "#6c3483",
    "#a93a79", "#5d6d7e", "#7b5e3a", "#2c7873",
];

/// The `#rrggbb` colour of the disc behind the initials, from `seed` (the username, or the room's
/// name): the same every time, and mostly different for different seeds.
#[must_use]
pub fn color(seed: &str) -> String {
    // FNV-1a: the same on every platform and run.
    let hash = seed.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    });
    let at = usize::try_from(hash % COLORS.len() as u64).unwrap_or(0);
    COLORS[at].to_owned()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use rc_core::domain::{Author, Delivery, SystemEvent};

    use super::*;

    fn user(name: &str) -> AvatarOwner {
        AvatarOwner::User(name.to_owned())
    }

    fn update(owner: AvatarOwner, path: Option<&str>) -> AvatarUpdate {
        AvatarUpdate {
            owner,
            path: path.map(PathBuf::from),
        }
    }

    // RICH-004
    #[test]
    fn a_file_url_escapes_the_path_and_names_the_version() {
        assert_eq!(
            file_url(
                Path::new("/home/me/.cache/app/h%3A3000/avatars/user/a b.svg"),
                3
            ),
            "file:///home/me/.cache/app/h%253A3000/avatars/user/a%20b.svg?v=3"
        );
        assert_eq!(
            file_url(Path::new("C:\\Users\\me\\a.png"), 1),
            "file:///C:/Users/me/a.png?v=1"
        );
    }

    // RICH-004
    #[test]
    fn a_replaced_file_gets_a_new_url_so_that_qml_loads_it_again() {
        let mut urls = AvatarUrls::default();
        let first = urls.apply(&[update(user("alice"), Some("/c/alice.png"))]);
        let second = urls.apply(&[update(user("alice"), Some("/c/alice.png"))]);
        assert_eq!(first[0].1, "file:///c/alice.png?v=1");
        assert_eq!(second[0].1, "file:///c/alice.png?v=2");
        assert_eq!(urls.url(&user("alice")), "file:///c/alice.png?v=2");
    }

    // RICH-004
    #[test]
    fn a_forgotten_avatar_has_no_url() {
        let mut urls = AvatarUrls::default();
        urls.apply(&[update(user("alice"), Some("/c/alice.png"))]);
        let gone = urls.apply(&[update(user("alice"), None)]);
        assert_eq!(gone, [(user("alice"), String::new())]);
        assert_eq!(urls.url(&user("alice")), "");
        assert_eq!(urls.url(&user("bob")), "");
    }

    // RICH-004
    #[test]
    fn a_model_follows_the_cache_through_the_changes() {
        let mut cache = AvatarUrls::default();
        let mut model = AvatarUrls::default();
        let changes = cache.apply(&[
            update(user("alice"), Some("/c/alice.png")),
            update(user("bob"), Some("/c/bob.png")),
        ]);
        model.set(&changes);
        let changes = cache.apply(&[update(user("alice"), None)]);
        model.set(&changes);
        assert_eq!(model.url(&user("alice")), "");
        assert_eq!(model.url(&user("bob")), cache.url(&user("bob")));
    }

    fn message(author: &str, system: bool) -> Message {
        Message {
            id: "m".to_owned().into(),
            room_id: "r".to_owned().into(),
            author: Author {
                id: "u".to_owned().into(),
                username: author.to_owned(),
                display_name: None,
            },
            ts: jiff::Timestamp::UNIX_EPOCH,
            kind: if system {
                MessageKind::System(SystemEvent::Joined)
            } else {
                MessageKind::Text("hi".into())
            },
            delivery: Delivery::Sent,
            attachments: Vec::new(),
            quotes: Vec::new(),
            edited: false,
            reactions: Vec::new(),
        }
    }

    // RICH-004
    #[test]
    fn a_text_message_shows_its_authors_avatar_and_a_system_message_none() {
        assert_eq!(message_owner(&message("alice", false)), Some(user("alice")));
        assert_eq!(message_owner(&message("alice", true)), None);
    }

    // RICH-004
    #[test]
    fn only_the_rows_that_use_a_changed_avatar_are_announced_in_runs() {
        let authors = ["a", "b", "b", "c", "a", "d", "b"];
        let changed: HashSet<_> = [user("b"), user("d")].into();
        let runs = rows_using(authors.len(), &changed, |row| Some(user(authors[row])));
        assert_eq!(runs, [(1, 2), (5, 6)]);
    }

    // RICH-004
    #[test]
    fn rows_without_an_owner_are_never_announced() {
        let changed: HashSet<_> = [user("a")].into();
        let runs = rows_using(3, &changed, |row| (row != 1).then(|| user("a")));
        assert_eq!(runs, [(0, 0), (2, 2)]);
        assert!(rows_using(0, &changed, |_| Some(user("a"))).is_empty());
    }

    // RICH-004
    #[test]
    fn the_initials_are_those_of_the_first_two_words() {
        assert_eq!(initials("Alice Anderson"), "AA");
        assert_eq!(initials("alice"), "A");
        assert_eq!(initials("alice.b_smith"), "AB");
        assert_eq!(initials("one two three"), "OT");
        assert_eq!(initials("élan vital"), "ÉV");
        assert_eq!(initials("  "), "?");
        assert_eq!(initials("--"), "?");
        assert_eq!(initials("#general"), "G");
    }

    fn room(name: &str, peer: Option<&str>) -> Room {
        Room {
            id: "6ad7c1543a2871a71ede69f3".to_owned().into(),
            kind: if peer.is_some() {
                rc_core::domain::RoomKind::Direct
            } else {
                rc_core::domain::RoomKind::Channel
            },
            name: name.to_owned(),
            peer: peer.map(str::to_owned),
            avatar_etag: None,
            unread: 0,
            user_mentions: 0,
            group_mentions: 0,
            alert: false,
            favorite: false,
            last_activity: jiff::Timestamp::UNIX_EPOCH,
        }
    }

    // RICH-004
    #[test]
    fn a_room_shows_the_initials_of_its_name_not_of_its_id() {
        assert_eq!(room_placeholder(&room("dev ops", None)).0, "DO");
        assert_eq!(room_placeholder(&room("general", None)).0, "G");
    }

    // RICH-004
    #[test]
    fn a_direct_message_shows_the_other_users_name_on_the_colour_of_their_username() {
        let (initials, color_of_bob) = room_placeholder(&room("Bob Smith", Some("bob")));
        assert_eq!(initials, "BS");
        assert_eq!(color_of_bob, color("bob"));
        assert_eq!(room_placeholder(&room("dev", None)).1, color("dev"));
    }

    // RICH-004
    #[test]
    fn the_colour_comes_from_the_seed_alone() {
        assert_eq!(color("alice"), color("alice"));
        assert_ne!(color("alice"), color("bob"));
        for seed in ["alice", "bob", "", "ü", "ct-user-1-2-3"] {
            let color = color(seed);
            assert_eq!(color.len(), 7, "{color}");
            assert!(color.starts_with('#'), "{color}");
            assert!(color[1..].bytes().all(|b| b.is_ascii_hexdigit()), "{color}");
        }
    }
}
