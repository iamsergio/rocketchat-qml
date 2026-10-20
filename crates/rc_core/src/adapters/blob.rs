// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! How a [`StoredSession`] is written as text: JSON, the same for every store that keeps it as one
//! piece of text.

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use crate::domain::{Credentials, StoredSession};
use crate::ports::StoreError;

#[derive(Serialize, Deserialize)]
struct Blob {
    server: String,
    user_id: String,
    token: String,
}

pub(super) fn encode(session: &StoredSession) -> Result<String, StoreError> {
    serde_json::to_string(&Blob {
        server: session.server.clone(),
        user_id: session.credentials.user_id.as_str().to_owned(),
        token: session.credentials.token.expose_secret().to_owned(),
    })
    .map_err(|e| StoreError::Corrupt(e.to_string()))
}

pub(super) fn decode(text: &str) -> Result<StoredSession, StoreError> {
    let blob: Blob = serde_json::from_str(text).map_err(|e| StoreError::Corrupt(e.to_string()))?;
    Ok(StoredSession {
        server: blob.server,
        credentials: Credentials {
            user_id: blob.user_id.into(),
            token: SecretString::from(blob.token),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> StoredSession {
        StoredSession {
            server: "https://chat.example.com".to_owned(),
            credentials: Credentials {
                user_id: "uid".to_owned().into(),
                token: SecretString::from("tok"),
            },
        }
    }

    // AUTH-004
    #[test]
    fn the_blob_round_trips() {
        let text = encode(&session()).unwrap();
        let back = decode(&text).unwrap();
        assert_eq!(back.server, "https://chat.example.com");
        assert_eq!(back.credentials.user_id.as_str(), "uid");
        assert_eq!(back.credentials.token.expose_secret(), "tok");
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
