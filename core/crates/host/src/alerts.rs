//! The live `alert` frame (F5). The phone hears only a kind, the id of the rule that let the
//! alert through, and the time. The agent's name, the title and any text stay out, so the
//! app shows a generic notice and the person opens the app for the detail.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use excubia_core_chat::{AlertDecision, AlertStatus, ChatStore};
use excubia_core_state::{StateStore, Status, EVENT_STATUS};
use serde_json::json;
use tokio::sync::broadcast::error::RecvError;
use tokio::task::JoinHandle;

use crate::alert_policy::{AlertPolicy, Change, Moment, PolicyConfig, Verdict};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertKind {
    AgentDone,
    AgentBlocked,
    CalendarReminder,
}

impl AlertKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::AgentDone => "agent_done",
            Self::AgentBlocked => "agent_blocked",
            Self::CalendarReminder => "calendar_reminder",
        }
    }
}

/// Sends one `alert` frame. The caller has already judged that the alert may go out.
pub fn publish(store: &StateStore, kind: AlertKind, rule_id: Option<String>, ts: &str) {
    store.publish_alert(json!({
        "kind": kind.as_str(),
        "rule_id": rule_id,
        "ts": ts,
    }));
}

/// Judges every status change of the store and publishes the `alert` frames that may go
/// out. A lagging subscriber misses frames; the next change of each pane is still judged.
pub fn spawn(store: Arc<StateStore>, chat: Arc<ChatStore>) -> JoinHandle<()> {
    let mut frames = store.receiver();
    let mut policy = AlertPolicy::new(PolicyConfig::default());
    tokio::spawn(async move {
        loop {
            match frames.recv().await {
                Ok(frame) if frame.event == EVENT_STATUS => {
                    if let Some(pane_id) = frame.body["pane_id"].as_str() {
                        notify_pane(&store, &chat, &mut policy, pane_id).await;
                    }
                }
                Ok(_) | Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => break,
            }
        }
    })
}

async fn notify_pane(
    store: &StateStore,
    chat: &ChatStore,
    policy: &mut AlertPolicy,
    pane_id: &str,
) {
    let Some(status) = store.status_of(pane_id).await else {
        return;
    };
    let (alert_status, kind) = match status {
        Status::Blocked => (AlertStatus::Blocked, AlertKind::AgentBlocked),
        Status::Done => (AlertStatus::Done, AlertKind::AgentDone),
        Status::Idle | Status::Working | Status::Unknown => return,
    };
    let rule_id = match chat.alert_for_pane(pane_id, alert_status) {
        Ok(AlertDecision::Alert { rule_id }) => rule_id,
        Ok(AlertDecision::Silent) => return,
        Err(error) => {
            // Without the rules the host cannot tell a muted change from a wanted one,
            // so it sends nothing rather than a notice the person may have muted.
            eprintln!("host: could not read the alert rules: {error}");
            return;
        }
    };
    let change = Change {
        pane_id,
        status,
        synthetic: false,
        removed: false,
    };
    if policy.decide(&change, now()).verdict == Verdict::Allow {
        publish(store, kind, rule_id, &chat.now_text());
    }
}

/// No quiet hours are configured, so the local offset is not read yet.
fn now() -> Moment {
    let unix_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |elapsed| elapsed.as_secs_f64());
    Moment {
        unix_secs,
        utc_offset_secs: 0,
    }
}
