// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! How a [`StoredAccount`] is written as text: JSON, the same for every store. The keyring keeps
//! one blob per entry, the file a list of them.

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use crate::domain::StoredAccount;
use crate::ports::StoreError;

#[derive(Serialize, Deserialize)]
pub(super) struct Blob {
    pub(super) id: String,
    server: String,
    username: String,
    user_id: String,
    token: Option<String>,
}

impl From<&StoredAccount> for Blob {
    fn from(account: &StoredAccount) -> Self {
        Self {
            id: account.id.as_str().to_owned(),
            server: account.server.clone(),
            username: account.username.clone(),
            user_id: account.user_id.as_str().to_owned(),
            token: account
                .token
                .as_ref()
                .map(|token| token.expose_secret().to_owned()),
        }
    }
}

impl From<Blob> for StoredAccount {
    fn from(blob: Blob) -> Self {
        Self {
            id: blob.id.into(),
            server: blob.server,
            username: blob.username,
            user_id: blob.user_id.into(),
            token: blob.token.map(SecretString::from),
        }
    }
}

pub(super) fn encode(account: &StoredAccount) -> Result<String, StoreError> {
    serde_json::to_string(&Blob::from(account)).map_err(|e| StoreError::Corrupt(e.to_string()))
}

pub(super) fn decode(text: &str) -> Result<StoredAccount, StoreError> {
    let blob: Blob = serde_json::from_str(text).map_err(|e| StoreError::Corrupt(e.to_string()))?;
    Ok(blob.into())
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// An account with a token, made of `name`.
    pub(in crate::adapters) fn account(name: &str) -> StoredAccount {
        StoredAccount {
            id: format!("id-{name}").into(),
            server: format!("https://{name}.example.com"),
            username: format!("user-{name}"),
            user_id: format!("uid-{name}").into(),
            token: Some(SecretString::from(format!("tok-{name}"))),
        }
    }

    /// What a test compares: everything an account holds, the token exposed.
    pub(in crate::adapters) fn fields(
        account: &StoredAccount,
    ) -> (&str, &str, &str, &str, Option<&str>) {
        (
            account.id.as_str(),
            &account.server,
            &account.username,
            account.user_id.as_str(),
            account.token.as_ref().map(ExposeSecret::expose_secret),
        )
    }

    // AUTH-004
    #[test]
    fn the_blob_round_trips() {
        let original = account("a");
        let back = decode(&encode(&original).unwrap()).unwrap();
        assert_eq!(fields(&back), fields(&original));
    }

    // AUTH-004
    #[test]
    fn an_account_without_a_token_round_trips() {
        let mut original = account("a");
        original.token = None;
        let back = decode(&encode(&original).unwrap()).unwrap();
        assert_eq!(fields(&back), fields(&original));
    }

    // AUTH-004
    #[test]
    fn a_corrupt_blob_is_reported() {
        for text in ["", "not json", r#"{"server":"x"}"#] {
            assert!(
                matches!(decode(text), Err(StoreError::Corrupt(_))),
                "{text:?}"
            );
        }
    }
}
