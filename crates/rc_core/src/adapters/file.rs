// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! A secret store over a plain file, so a GUI test can restart the app without touching the user's
//! keyring. The token is written in clear text: only built with the `test-file-store` feature
//! (and in this crate's own tests), never in a release build.

use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::blob::Blob;
use crate::domain::{AccountId, StoredAccount};
use crate::ports::{SecretStore, StoreError};

/// Keeps the accounts in one file, as JSON.
pub struct FileStore {
    path: PathBuf,
}

#[derive(Default, Serialize, Deserialize)]
struct Accounts {
    accounts: Vec<Blob>,
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

    /// A missing file is an empty list.
    fn read(&self) -> Result<Accounts, StoreError> {
        match fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| StoreError::Corrupt(e.to_string())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Accounts::default()),
            Err(error) => Err(self.unavailable(&error)),
        }
    }

    fn write(&self, accounts: &Accounts) -> Result<(), StoreError> {
        let text =
            serde_json::to_string(accounts).map_err(|e| StoreError::Corrupt(e.to_string()))?;
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        // Only the owner may read the tokens.
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        options
            .open(&self.path)
            .and_then(|mut file| file.write_all(text.as_bytes()))
            .map_err(|e| self.unavailable(&e))
    }
}

impl SecretStore for FileStore {
    fn load(&self) -> Result<Vec<StoredAccount>, StoreError> {
        Ok(self.read()?.accounts.into_iter().map(Into::into).collect())
    }

    fn save(&self, account: &StoredAccount) -> Result<(), StoreError> {
        let mut file = self.read()?;
        let blob = Blob::from(account);
        match file.accounts.iter_mut().find(|b| b.id == blob.id) {
            Some(stored) => *stored = blob,
            None => file.accounts.push(blob),
        }
        self.write(&file)
    }

    fn remove(&self, id: &AccountId) -> Result<(), StoreError> {
        let mut file = self.read()?;
        let before = file.accounts.len();
        file.accounts.retain(|blob| blob.id != id.as_str());
        if file.accounts.len() == before {
            return Ok(());
        }
        self.write(&file)
    }
}

#[cfg(test)]
mod tests {
    use std::process;

    use super::*;
    use crate::adapters::blob::tests::{account, fields};

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

    fn ids(store: &FileStore) -> Vec<String> {
        let accounts = store.load().unwrap();
        accounts.iter().map(|a| a.id.to_string()).collect()
    }

    // AUTH-004
    #[test]
    fn two_accounts_round_trip_in_order() {
        let file = TempFile::new("round-trip");
        let store = FileStore::new(&file.0);
        store.save(&account("a")).unwrap();
        store.save(&account("b")).unwrap();
        // A second store on the same path, like the next run of the app.
        let loaded = FileStore::new(&file.0).load().unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(fields(&loaded[0]), fields(&account("a")));
        assert_eq!(fields(&loaded[1]), fields(&account("b")));
    }

    // AUTH-004
    #[test]
    fn a_missing_file_is_nothing_stored() {
        let file = TempFile::new("missing");
        assert!(FileStore::new(&file.0).load().unwrap().is_empty());
    }

    // AUTH-004
    #[test]
    fn saving_an_existing_id_replaces_it_in_place() {
        let file = TempFile::new("replace");
        let store = FileStore::new(&file.0);
        store.save(&account("a")).unwrap();
        store.save(&account("b")).unwrap();
        let mut changed = account("a");
        changed.username = "renamed".to_owned();
        store.save(&changed).unwrap();
        assert_eq!(ids(&store), ["id-a", "id-b"]);
        assert_eq!(store.load().unwrap()[0].username, "renamed");
    }

    // AUTH-004
    #[test]
    fn removing_leaves_the_other_account() {
        let file = TempFile::new("remove");
        let store = FileStore::new(&file.0);
        store.save(&account("a")).unwrap();
        store.save(&account("b")).unwrap();
        store.remove(&account("a").id).unwrap();
        assert_eq!(ids(&store), ["id-b"]);
    }

    // AUTH-004
    #[test]
    fn removing_an_absent_id_is_fine() {
        let file = TempFile::new("remove-absent");
        let store = FileStore::new(&file.0);
        store.remove(&account("a").id).unwrap();
        store.save(&account("b")).unwrap();
        store.remove(&account("a").id).unwrap();
        assert_eq!(ids(&store), ["id-b"]);
    }

    // AUTH-004
    #[test]
    fn an_account_without_a_token_round_trips() {
        let file = TempFile::new("no-token");
        let store = FileStore::new(&file.0);
        let mut signed_out = account("a");
        signed_out.token = None;
        store.save(&signed_out).unwrap();
        assert_eq!(fields(&store.load().unwrap()[0]), fields(&signed_out));
    }

    // AUTH-004
    #[test]
    fn a_corrupt_file_is_reported() {
        let file = TempFile::new("corrupt");
        let store = FileStore::new(&file.0);
        for text in ["not json", r#"{"accounts":[{"server":"x"}]}"#] {
            fs::write(&file.0, text).unwrap();
            let result = store.load();
            assert!(matches!(result, Err(StoreError::Corrupt(_))), "{result:?}");
            // A write doesn't paper over what it can't read.
            let result = store.save(&account("a"));
            assert!(matches!(result, Err(StoreError::Corrupt(_))), "{result:?}");
        }
    }

    // AUTH-004
    #[cfg(unix)]
    #[test]
    fn only_the_owner_can_read_the_file() {
        use std::os::unix::fs::PermissionsExt;

        let file = TempFile::new("mode");
        FileStore::new(&file.0).save(&account("a")).unwrap();
        let mode = fs::metadata(&file.0).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "{mode:o}");
    }
}
