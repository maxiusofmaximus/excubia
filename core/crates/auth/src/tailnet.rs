//! What the host can see of its tailnet, for pairing auto-approval.
//!
//! Only the owner id is load-bearing: a code-less `POST /v1/pair` is accepted
//! when the source address belongs to the host's own tailnet account. A
//! missing binary, a stopped daemon, a tagged node or a different account all
//! answer "no owner", so none of them can auto-pair.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

const TIMEOUT: Duration = Duration::from_secs(5);
/// How long the host's own owner id is trusted before asking Tailscale again.
const OWNER_TTL: Duration = Duration::from_secs(30);

/// The owner id of `ip` (`tailscale whois --json`), or `None` when Tailscale
/// cannot say. Loopback has no tailnet owner by construction.
pub fn tailnet_owner_of(ip: &str) -> Option<String> {
    if ip.is_empty() || ip == "127.0.0.1" || ip == "::1" {
        return None;
    }
    let output = run_tailscale(&["whois", "--json", ip])?;
    parse_whois_owner(&output)
}

/// The host's own tailnet owner (`tailscale status --json` → `Self.UserID`).
/// Remembered for `OWNER_TTL`, so a burst of pairing requests runs one subprocess, not one each.
pub fn host_tailnet_owner() -> Option<String> {
    static CACHE: Mutex<Option<(Instant, Option<String>)>> = Mutex::new(None);
    let mut cache = CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((at, owner)) = cache.as_ref() {
        if at.elapsed() < OWNER_TTL {
            return owner.clone();
        }
    }
    let owner = run_tailscale(&["status", "--json"]).and_then(|output| parse_status_owner(&output));
    *cache = Some((Instant::now(), owner.clone()));
    owner
}

/// True when `ip` belongs to the host's own tailnet owner.
pub fn is_host_owner(ip: &str) -> bool {
    let Some(self_owner) = host_tailnet_owner() else {
        return false;
    };
    tailnet_owner_of(ip).is_some_and(|owner| owner == self_owner)
}

pub fn parse_whois_owner(data: &Value) -> Option<String> {
    let profile = data.get("UserProfile")?;
    let owner = profile.get("ID")?;
    match owner {
        Value::String(id) if !id.is_empty() => Some(id.clone()),
        Value::Number(id) => Some(id.to_string()),
        _ => None,
    }
}

pub fn parse_status_owner(data: &Value) -> Option<String> {
    let own = data.get("Self")?;
    match own.get("UserID")? {
        Value::String(id) if !id.is_empty() => Some(id.clone()),
        Value::Number(id) => Some(id.to_string()),
        _ => None,
    }
}

fn run_tailscale(args: &[&str]) -> Option<Value> {
    let binary = tailscale_binary()?;
    let mut child = Command::new(&binary)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    // The CLI can hang while the daemon restarts; a pairing request must not.
    let (tx, rx) = mpsc::channel();
    let mut stdout = child.stdout.take()?;
    std::thread::spawn(move || {
        use std::io::Read;
        let mut buffer = String::new();
        let _ = stdout.read_to_string(&mut buffer);
        let _ = tx.send(buffer);
    });
    let output = match rx.recv_timeout(TIMEOUT) {
        Ok(output) => output,
        Err(_) => {
            let _ = child.kill();
            return None;
        }
    };
    let status = child.wait().ok()?;
    if !status.success() {
        return None;
    }
    serde_json::from_str(&output).ok()
}

fn tailscale_binary() -> Option<PathBuf> {
    // The well-known install paths come first: a `tailscale.exe` planted in the
    // program directory or the PATH must not decide who the host's owner is.
    let candidates = [
        r"C:\Program Files\Tailscale\tailscale.exe",
        "/usr/bin/tailscale",
        "/usr/local/bin/tailscale",
    ];
    candidates
        .into_iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whois_owner_comes_from_the_user_profile() {
        let data: Value = serde_json::from_str(
            r#"{"UserProfile":{"ID":"12345","LoginName":"max@example.com"},"Node":{"Name":"phone."}}"#,
        )
        .expect("json");
        assert_eq!(parse_whois_owner(&data).as_deref(), Some("12345"));
    }

    #[test]
    fn a_tagged_node_has_no_owner() {
        let data: Value = serde_json::from_str(r#"{"Node":{"Name":"router."}}"#).expect("json");
        assert_eq!(parse_whois_owner(&data), None);
    }

    #[test]
    fn status_owner_is_the_self_user_id() {
        let data: Value =
            serde_json::from_str(r#"{"Self":{"UserID":"12345"},"Peer":{}}"#).expect("json");
        assert_eq!(parse_status_owner(&data).as_deref(), Some("12345"));
        let none: Value = serde_json::from_str(r#"{"Peer":{}}"#).expect("json");
        assert_eq!(parse_status_owner(&none), None);
    }

    #[test]
    fn loopback_never_auto_approves() {
        assert!(!is_host_owner("127.0.0.1"));
        assert_eq!(tailnet_owner_of("127.0.0.1"), None);
    }
}
