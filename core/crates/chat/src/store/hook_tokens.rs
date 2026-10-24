//! Hook tokens: minted per run (or per resident panel), stored as a hash.
//!
//! The plaintext only ever exists in the process the host launches; the
//! database keeps the SHA-256, so a copy of `chat.db` cannot be replayed
//! against the approval routes. A token is revoked when its run closes and
//! every live token dies with the process that owned it (`schema::open`).

use rand_core::{OsRng, RngCore};
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};

use crate::error::ChatError;
use crate::model_hook::{HookScope, HookTokenUse};
use crate::schema;

const TOKEN_BYTES: usize = 32;

impl super::ChatStore {
    /// Mint the token of one run or panel. Any previous live token of the same
    /// scope dies first: one scope, one credential.
    pub fn issue_hook_token(&self, scope: &HookScope) -> Result<String, ChatError> {
        let token = mint();
        let (seat_id, run_id, conversation_id) = match scope {
            HookScope::Run {
                seat_id,
                run_id,
                conversation_id,
            } => (seat_id.as_str(), run_id.as_str(), conversation_id.as_str()),
            HookScope::Seat { seat_id } => (seat_id.as_str(), "", ""),
        };
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let now = schema::now(&transaction)?;
        transaction.execute(
            "UPDATE hook_token SET revoked_at = ?1
              WHERE revoked_at IS NULL AND seat_id = ?2 AND run_id = ?3",
            params![now, seat_id, run_id],
        )?;
        transaction.execute(
            "INSERT INTO hook_token (token_hash, seat_id, run_id, conversation_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![hash(&token), seat_id, run_id, conversation_id, now],
        )?;
        transaction.commit()?;
        Ok(token)
    }

    /// What a presented token is. The hash is the only lookup.
    pub fn hook_token_use(&self, token: &str) -> Result<HookTokenUse, ChatError> {
        let connection = self.lock();
        let row: Option<(String, String, String, Option<String>)> = connection
            .query_row(
                "SELECT seat_id, run_id, conversation_id, revoked_at
                   FROM hook_token WHERE token_hash = ?1",
                [hash(token)],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        Ok(match row {
            None => HookTokenUse::Unknown,
            Some((_, _, _, Some(_))) => HookTokenUse::Revoked,
            Some((seat_id, run_id, conversation_id, None)) => {
                HookTokenUse::Live(if run_id.is_empty() {
                    HookScope::seat(&seat_id)
                } else {
                    HookScope::run(&run_id, &conversation_id, &seat_id)
                })
            }
        })
    }

    /// Revoke the token of a run when its turn closes. The row stays, so a
    /// late request is `revoked`, not an unknown token.
    pub fn revoke_hook_token(&self, run_id: &str) -> Result<bool, ChatError> {
        let connection = self.lock();
        let now = schema::now(&connection)?;
        let changed = connection.execute(
            "UPDATE hook_token SET revoked_at = ?1 WHERE run_id = ?2 AND revoked_at IS NULL",
            params![now, run_id],
        )?;
        Ok(changed > 0)
    }
}

/// A token the host hands to one process: 32 random bytes, hex.
fn mint() -> String {
    let mut bytes = [0u8; TOKEN_BYTES];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_is_live_until_its_run_closes() {
        let store = super::super::ChatStore::in_memory().expect("store");
        let scope = HookScope::run("run_1", "cnv_1", "ag_1");
        let token = store.issue_hook_token(&scope).expect("token");
        assert_eq!(
            store.hook_token_use(&token).expect("use"),
            HookTokenUse::Live(scope)
        );
        assert_eq!(
            store.hook_token_use("not-a-token").expect("use"),
            HookTokenUse::Unknown
        );

        assert!(store.revoke_hook_token("run_1").expect("revoke"));
        assert_eq!(
            store.hook_token_use(&token).expect("use"),
            HookTokenUse::Revoked
        );
        assert!(!store.revoke_hook_token("run_1").expect("idempotent"));
    }

    #[test]
    fn a_scope_has_one_live_token() {
        let store = super::super::ChatStore::in_memory().expect("store");
        let scope = HookScope::run("run_1", "cnv_1", "ag_1");
        let first = store.issue_hook_token(&scope).expect("first");
        let second = store.issue_hook_token(&scope).expect("second");
        assert_eq!(
            store.hook_token_use(&first).expect("use"),
            HookTokenUse::Revoked
        );
        assert!(matches!(
            store.hook_token_use(&second).expect("use"),
            HookTokenUse::Live(_)
        ));
    }

    #[test]
    fn the_database_never_holds_the_plaintext() {
        let store = super::super::ChatStore::in_memory().expect("store");
        let token = store
            .issue_hook_token(&HookScope::seat("ag_1"))
            .expect("token");
        let stored: String = store
            .lock()
            .query_row("SELECT token_hash FROM hook_token", [], |row| row.get(0))
            .expect("hash");
        assert_ne!(stored, token);
        assert_eq!(stored, hash(&token));
    }
}
