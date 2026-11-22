//! What the host can do to a supervised agent, as the API's `AgentActions`.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use excubia_api::{AgentActions, AgentSpec, HookState, RestoreRefusal, StartedAgent};
use excubia_detect::DetectedState;
use excubia_state::EVENT_CLOSED;
use serde_json::{json, Value};

use crate::restore_points::{Point, RestoreError};
use crate::session::status_of;
use crate::{AgentHost, SpawnSpec};

/// How long a stop may take before the request gives up (the monitor closes
/// the child at 2 s; this leaves room for the exit and the pane removal).
const STOP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(6);

#[async_trait]
impl AgentActions for AgentHost {
    fn respond(&self, pane_id: &str, text: &str) -> Result<(), String> {
        let session = self.session(pane_id).ok_or("pane is gone")?;
        let mut payload = text.as_bytes().to_vec();
        payload.push(b'\r');
        session.write(&payload)
    }

    fn send_keys(&self, pane_id: &str, keys: &[String], _confirm: bool) -> Result<(), String> {
        let session = self.session(pane_id).ok_or("pane is gone")?;
        let mut payload = Vec::new();
        for key in keys {
            let bytes = encode_key(key).ok_or_else(|| format!("key not encodable: {key}"))?;
            payload.extend_from_slice(bytes);
        }
        session.write(&payload)
    }

    fn read(&self, pane_id: &str, lines: u16) -> Result<String, String> {
        let session = self.session(pane_id).ok_or("pane is gone")?;
        Ok(session.read(lines as usize))
    }

    async fn start_agent(
        &self,
        agent: &AgentSpec,
        cwd: &std::path::Path,
        label: Option<&str>,
    ) -> Result<StartedAgent, String> {
        let spec = SpawnSpec {
            program: agent.command.clone(),
            args: agent.args.clone(),
            cwd: Some(cwd.to_path_buf()),
            label: label.map(str::to_string),
            env: agent
                .env
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        };
        let pane_id = self
            .start(spec)
            .await
            .map_err(|error| format!("could not start {}: {error:#}", agent.command))?;
        let row = self.store_row(&pane_id).await?;
        Ok(StartedAgent {
            pane_id: row.pane_id,
            workspace_id: row.workspace_id,
            agent: row.agent,
            status: row.status.as_str().to_string(),
        })
    }

    async fn report_agent(
        &self,
        pane_id: &str,
        state: HookState,
        message: Option<&str>,
    ) -> Result<String, String> {
        let session = self.session(pane_id).ok_or("pane is gone")?;
        if !session.is_live() {
            return Err("the pane has no running agent".to_string());
        }
        let detected = detected_state_of(state);
        session.set_hook(detected, Instant::now());
        self.store.set_status(pane_id, status_of(detected)).await;
        // The monitor re-evaluates now (and arms the expiry check).
        self.hook_reports.notify_waiters();
        // A hook may name a pane that has no name yet; it never renames one.
        if let Some(message) = message.map(str::trim).filter(|text| !text.is_empty()) {
            if let Ok(row) = self.store_row(pane_id).await {
                if row.title.is_none() {
                    self.store.set_terminal_title(pane_id, message).await;
                }
            }
        }
        let row = self.store_row(pane_id).await?;
        Ok(row.status.as_str().to_string())
    }

    async fn stop_agent(&self, pane_id: &str) -> Result<(), String> {
        let session = self.session(pane_id).ok_or("pane is gone")?;
        if !session.is_live() {
            return Err("the pane has no running agent".to_string());
        }
        // The monitor closes the child (with escalation) and removes the pane;
        // this waits for that `pane.closed`, so a slow kill cannot look like a
        // success.
        session.request_stop();
        let mut events = self.store.receiver();
        let deadline = tokio::time::Instant::now() + STOP_TIMEOUT;
        loop {
            if !self.store.agent_exists(pane_id).await {
                return Ok(());
            }
            match tokio::time::timeout_at(deadline, events.recv()).await {
                Ok(Ok(frame))
                    if frame.event == EVENT_CLOSED && frame.body["pane_id"] == pane_id =>
                {
                    return Ok(());
                }
                Ok(Ok(_)) => continue,
                Ok(Err(_)) => return Err("the state channel closed".to_string()),
                Err(_) => return Err("the agent did not stop in time".to_string()),
            }
        }
    }

    async fn begin_turn(&self, pane_id: &str) -> Result<String, String> {
        let dir = self
            .session(pane_id)
            .and_then(|session| session.workdir.clone())
            .ok_or("the pane has no running agent")?;
        let store = Arc::clone(&self.restore);
        let agent = pane_id.to_string();
        let turn = excubia_chat::new_ulid();
        blocking(move || store.create_point(&agent, &turn, &dir))
            .await
            .map(|point| point.id)
            .map_err(|error| {
                eprintln!("host: no restore point for {pane_id}: {error}");
                error.to_string()
            })
    }

    async fn restore_points(&self, pane_id: &str) -> Result<Value, RestoreRefusal> {
        let store = Arc::clone(&self.restore);
        let agent = pane_id.to_string();
        let points = blocking(move || store.list_points(&agent))
            .await
            .map_err(refusal)?;
        let summaries: Vec<Value> = points.iter().map(summary).collect();
        Ok(json!({"agent": pane_id, "points": summaries}))
    }

    async fn restore_changes(&self, point_id: &str) -> Result<Value, RestoreRefusal> {
        let store = Arc::clone(&self.restore);
        let id = point_id.to_string();
        let changes = blocking(move || store.changed_files(&id))
            .await
            .map_err(refusal)?;
        Ok(json!({
            "point": point_id,
            "changes": changes.changes,
            "excluded": changes.excluded,
        }))
    }

    async fn restore_point(&self, point_id: &str) -> Result<Value, RestoreRefusal> {
        let store = Arc::clone(&self.restore);
        let id = point_id.to_string();
        let done = blocking(move || store.restore(&id))
            .await
            .map_err(refusal)?;
        Ok(json!({
            "point": point_id,
            "restored": done.restored,
            "removed": done.removed,
            "undo_point": done.undo_point,
        }))
    }
}

impl AgentHost {
    async fn store_row(&self, pane_id: &str) -> Result<excubia_state::TriageRow, String> {
        self.store
            .row_of(pane_id)
            .await
            .ok_or_else(|| "pane is gone".to_string())
    }
}

/// Runs a file-system step off the async workers. A task that cannot finish is a failure.
async fn blocking<T, F>(work: F) -> Result<T, RestoreError>
where
    F: FnOnce() -> Result<T, RestoreError> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .unwrap_or_else(|join| Err(RestoreError::Io(std::io::Error::other(join))))
}

/// What the host answers for one point: its id, kind, turn, time and counts, never a path.
fn summary(point: &Point) -> Value {
    json!({
        "id": point.id,
        "kind": point.kind,
        "turn": point.turn,
        "created_ms": point.created_ms,
        "files": point.entries.len(),
        "excluded": point.excluded.len(),
    })
}

/// The refusal for a restore error. Messages carry no folder path.
fn refusal(error: RestoreError) -> RestoreRefusal {
    match error {
        RestoreError::NotFound => RestoreRefusal::NotFound,
        RestoreError::InvalidPath => {
            RestoreRefusal::Invalid("a path of the restore point is outside its folder".to_string())
        }
        RestoreError::MissingBlob => {
            RestoreRefusal::Failed("a saved copy of the restore point is missing".to_string())
        }
        RestoreError::Io(_) => RestoreRefusal::Failed("restore point i/o failed".to_string()),
    }
}

fn detected_state_of(state: HookState) -> DetectedState {
    match state {
        HookState::Idle => DetectedState::Idle,
        HookState::Working => DetectedState::Working,
        HookState::Blocked => DetectedState::Blocked,
        HookState::Unknown => DetectedState::Unknown,
    }
}

/// Key encoding for `POST /keys` (the allowlist lives in `excubia-api`).
fn encode_key(key: &str) -> Option<&'static [u8]> {
    Some(match key {
        "enter" => b"\r",
        "esc" => b"\x1b",
        "tab" => b"\t",
        "space" => b" ",
        "backspace" => b"\x7f",
        "up" => b"\x1b[A",
        "down" => b"\x1b[B",
        "right" => b"\x1b[C",
        "left" => b"\x1b[D",
        "home" => b"\x1b[H",
        "end" => b"\x1b[F",
        "pgup" => b"\x1b[5~",
        "pgdown" => b"\x1b[6~",
        "ctrl+c" => b"\x03",
        "y" => b"y",
        "n" => b"n",
        "0" => b"0",
        "1" => b"1",
        "2" => b"2",
        "3" => b"3",
        "4" => b"4",
        "5" => b"5",
        "6" => b"6",
        "7" => b"7",
        "8" => b"8",
        "9" => b"9",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_encode_to_terminal_bytes() {
        assert_eq!(encode_key("enter"), Some(&b"\r"[..]));
        assert_eq!(encode_key("esc"), Some(&b"\x1b"[..]));
        assert_eq!(encode_key("up"), Some(&b"\x1b[A"[..]));
        assert_eq!(encode_key("ctrl+c"), Some(&b"\x03"[..]));
        assert_eq!(encode_key("f5"), None);
    }

    #[test]
    fn hook_states_map_to_screen_states() {
        assert_eq!(detected_state_of(HookState::Idle), DetectedState::Idle);
        assert_eq!(
            detected_state_of(HookState::Blocked),
            DetectedState::Blocked
        );
        assert_eq!(
            detected_state_of(HookState::Unknown),
            DetectedState::Unknown
        );
    }
}
