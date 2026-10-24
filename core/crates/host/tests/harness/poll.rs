//! Polling helpers of the harness: wait for a pane to reach a state or leave
//! the list, read one pane row, and ask the OS whether a process still exists.

#![allow(dead_code)]

use std::time::Duration;

use hn_core_test_support::SignedClient;

/// True while the OS still knows the process. Test-only, and deliberately
/// tool-based: no new dependency for one check.
pub fn process_exists(pid: u32) -> bool {
    #[cfg(windows)]
    {
        let filter = format!("PID eq {pid}");
        match std::process::Command::new("tasklist")
            .args(["/FI", &filter, "/NH"])
            .output()
        {
            Ok(output) => String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()),
            Err(_) => false,
        }
    }
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}

pub async fn wait_for_status(
    client: &SignedClient,
    pane_id: &str,
    want: &str,
    timeout: Duration,
) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Ok(state) = client.state().await {
            let status = state["agents"]
                .as_array()
                .and_then(|agents| {
                    agents
                        .iter()
                        .find(|row| row["pane_id"] == pane_id)
                        .map(|row| row["status"].as_str().unwrap_or("").to_string())
                })
                .unwrap_or_default();
            if status == want {
                return true;
            }
        }
        if tokio::time::Instant::now() > deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

pub async fn wait_until_gone(client: &SignedClient, pane_id: &str, timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Ok(state) = client.state().await {
            let present = state["agents"]
                .as_array()
                .is_some_and(|agents| agents.iter().any(|row| row["pane_id"] == pane_id));
            if !present {
                return true;
            }
        }
        if tokio::time::Instant::now() > deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// One pane's row from `/v1/state`.
pub async fn row_of(client: &SignedClient, pane_id: &str) -> Option<serde_json::Value> {
    let state = client.state().await.ok()?;
    state["agents"]
        .as_array()?
        .iter()
        .find(|row| row["pane_id"] == pane_id)
        .cloned()
}
