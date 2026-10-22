// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Who the accounts are, so that one user is not added twice.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::domain::{AccountId, UserId};

/// The user each account is, shared by the accounts.
///
/// Two accounts are the same user if their servers are the same address (ignoring case and a
/// trailing `/`) and their user ids are equal. The same server under another address counts as
/// another server.
///
/// A lock is fine here: an account asks only when a login succeeds, and the lock is never held
/// across an await or while calling out.
#[derive(Default)]
pub(crate) struct Users(Mutex<HashMap<AccountId, (String, UserId)>>);

impl Users {
    /// Records that `id` is `user_id` on `server`, replacing what it was before. Returns `false`,
    /// recording nothing, if another account is that user.
    pub(crate) fn claim(&self, id: &AccountId, server: &str, user_id: &UserId) -> bool {
        let server = server.trim_end_matches('/').to_ascii_lowercase();
        let mut users = self.lock();
        let taken = users
            .iter()
            .any(|(other, (s, u))| other != id && *s == server && u == user_id);
        if !taken {
            users.insert(id.clone(), (server, user_id.clone()));
        }
        !taken
    }

    /// Forgets what `id` was, so that another account may be that user.
    pub(crate) fn release(&self, id: &AccountId) {
        self.lock().remove(id);
    }

    /// A poisoned lock is recovered: each update is one map call that leaves it consistent.
    fn lock(&self) -> MutexGuard<'_, HashMap<AccountId, (String, UserId)>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(id: &str) -> AccountId {
        id.to_owned().into()
    }

    fn user(id: &str) -> UserId {
        id.to_owned().into()
    }

    // ACCT-007
    #[test]
    fn the_same_user_on_the_same_server_is_taken_whatever_the_case_and_trailing_slashes() {
        let users = Users::default();
        assert!(users.claim(&id("a"), "https://Chat.example.com/", &user("u")));
        assert!(!users.claim(&id("b"), "https://chat.example.com", &user("u")));
        assert!(!users.claim(&id("b"), "https://chat.example.com//", &user("u")));
    }

    // ACCT-007
    #[test]
    fn another_user_or_another_server_is_not_taken() {
        let users = Users::default();
        assert!(users.claim(&id("a"), "http://h", &user("u")));
        assert!(users.claim(&id("b"), "http://h", &user("v")));
        assert!(users.claim(&id("c"), "http://h:3000", &user("u")));
    }

    // ACCT-007
    #[test]
    fn an_account_may_claim_again_and_a_released_user_is_free() {
        let users = Users::default();
        assert!(users.claim(&id("a"), "http://h", &user("u")));
        assert!(users.claim(&id("a"), "http://h", &user("u")));
        // Becoming someone else frees the user it was.
        assert!(users.claim(&id("a"), "http://h", &user("v")));
        assert!(users.claim(&id("b"), "http://h", &user("u")));
        assert!(!users.claim(&id("a"), "http://h", &user("u")));
        users.release(&id("b"));
        assert!(users.claim(&id("a"), "http://h", &user("u")));
    }
}
