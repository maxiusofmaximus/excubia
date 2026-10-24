//! The hook token: what an agent's hook presents on `/v1/hooks/report`.
//!
//! Not a device secret: it lives in the host's data directory, is only ever
//! accepted from loopback, and is handed to each spawned agent through its
//! environment (`HN_HOOK_TOKEN`). Generating it once and persisting it keeps
//! the hook configuration working across restarts.

use std::path::Path;

use anyhow::{Context, Result};
use rand_core::{OsRng, RngCore};

const TOKEN_BYTES: usize = 32;

/// Read the token, creating it on first run.
pub fn load_or_create(data_dir: &Path) -> Result<String> {
    let path = crate::paths::hook_token_path(data_dir);
    if path.is_file() {
        let token =
            std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let token = token.trim().to_string();
        if !token.is_empty() {
            return Ok(token);
        }
    }
    let mut bytes = [0u8; TOKEN_BYTES];
    OsRng.fill_bytes(&mut bytes);
    let token = hex::encode(bytes);
    hn_core_persist::write_atomic(&path, token.as_bytes())
        .with_context(|| format!("write {}", path.display()))?;
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hn_core_test_dir::TestDir;

    #[test]
    fn the_token_is_created_once_and_stable() {
        let dir = TestDir::new("hook-token-tests");

        let first = load_or_create(&dir).expect("first");
        assert_eq!(first.len(), TOKEN_BYTES * 2);
        let second = load_or_create(&dir).expect("second");
        assert_eq!(first, second, "the token survives a restart");
    }
}
