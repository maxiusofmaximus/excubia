//! What the host can see of its tailnet, for pairing auto-approval.
//!
//! Only the owner id is load-bearing: a code-less `POST /v1/pair` is accepted
//! when the source address belongs to the host's own tailnet account. A
//! missing binary, a stopped daemon, a tagged node or a different account all
//! answer "no owner", so none of them can auto-pair.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::Value;

const TIMEOUT: Duration = Duration::from_secs(5);

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
pub fn host_tailnet_owner() -> Option<String> {
    let output = run_tailscale(&["status", "--json"])?;
    parse_status_owner(&output)
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
    // PATH first, so an override still wins; the well-known install paths are
    // the fallback for a service account that has a short PATH.
    let probe = Command::new("tailscale")
        .arg("version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if probe.is_ok() {
        return Some(PathBuf::from("tailscale"));
    }
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
