//! The delegation tree at runtime: the assignment, its delivery to the
//! resident panel, its watcher, the card state and the coalesced synthesis.

use std::sync::Arc;

use hn_core_chat::{
    preview, ChatError, ChatSeat, Delegation, DelegationState, NewDelegation, SeatActor, ToolError,
    ToolReply,
};
use serde_json::json;

use super::team::{lock, TeamGrant, TeamService, Watcher, SYNTHESIS_BATCH};
use super::team_errors::tool_chat_error;

impl TeamService {
    pub(super) async fn assign(
        &self,
        actor: &SeatActor,
        assign: hn_core_seat::DelegationAssign,
    ) -> Result<ToolReply, ToolError> {
        let parent = if actor.role_id == "lead" {
            Some(
                self.chat
                    .active_delegation_for_agent(&actor.agent_id)
                    .map_err(tool_chat_error)?
                    .ok_or_else(|| {
                        ToolError::invalid(
                            "scope_denied",
                            "tool",
                            "the lead has no active delegation",
                        )
                    })?,
            )
        } else {
            None
        };
        let conversation_id = match &parent {
            Some(parent) => parent.conversation_id.clone(),
            None => assign
                .conversation_id
                .clone()
                .unwrap_or_else(|| actor.conversation_id.clone()),
        };
        if conversation_id.is_empty() {
            return Err(ToolError::invalid(
                "invalid_arguments",
                "conversation_id",
                "conversation_id is required",
            ));
        }
        let delegation = self
            .chat
            .create_delegation(NewDelegation {
                conversation_id,
                from_agent_id: actor.agent_id.clone(),
                to_agent_id: assign.to_agent_id,
                task: assign.task,
                turn_key: parent
                    .as_ref()
                    .map(|parent| parent.id.clone())
                    .unwrap_or_else(|| actor.run_id.clone()),
                parent_id: parent.as_ref().map(|parent| parent.id.clone()),
            })
            .map_err(tool_chat_error)?;
        self.dispatch(&delegation).await?;
        Ok(ToolReply {
            result: json!({
                "delegation_id": delegation.id,
                "status": DelegationState::Queued,
                "pane_id": delegation.pane_id,
                "depth": delegation.depth,
            }),
        })
    }

    /// Start the delegation and deliver it to the resident panel. A missing
    /// panel fails the delegation now, never leaves it queued forever.
    pub(super) async fn dispatch(&self, delegation: &Delegation) -> Result<(), ToolError> {
        let Some(pane_id) = delegation.pane_id.clone() else {
            // No panel: the delegation fails now, it is never left queued.
            let _ = self
                .chat
                .fail_delegation(&delegation.id, "seat_unavailable");
            return Err(ToolError::conflict(
                "seat_unavailable",
                "the assignee has no live panel",
            ));
        };
        let _ = self
            .chat
            .start_delegation(&delegation.id)
            .map_err(tool_chat_error)?;
        let task = json!({
            "delegation_id": delegation.id,
            "from": delegation.from_name,
            "task": delegation.task,
        })
        .to_string();
        if let Err(error) = self.host.write_line(&pane_id, &task) {
            let _ = self.chat.fail_delegation(&delegation.id, "pane_gone");
            return Err(ToolError::conflict(
                "seat_unavailable",
                format!("could not deliver the task: {error}"),
            ));
        }
        self.watch(delegation.id.clone(), pane_id);
        Ok(())
    }

    pub(super) fn watch(&self, delegation_id: String, _pane_id: String) {
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let chat = Arc::clone(&self.chat);
        let timeout = self.delegation_timeout;
        let id = delegation_id.clone();
        let task = tokio::spawn(async move {
            tokio::select! {
                _ = tokio::time::sleep(timeout) => {
                    let _ = chat.fail_delegation(&id, "delegation_timeout");
                }
                _ = stopped => {}
            }
        });
        if let Some(previous) = lock(&self.watchers).insert(delegation_id, Watcher { stop, task }) {
            let _ = previous.stop.send(());
            previous.task.abort();
        }
    }

    pub(super) fn stop_watcher(&self, delegation_id: &str) {
        if let Some(watcher) = lock(&self.watchers).remove(delegation_id) {
            let _ = watcher.stop.send(());
            watcher.task.abort();
        }
    }

    pub(super) fn complete(
        &self,
        grant: TeamGrant,
        delegation_id: String,
        result: String,
    ) -> Result<ToolReply, ToolError> {
        let delegation = self
            .chat
            .delegation(&delegation_id)
            .map_err(tool_chat_error)?;
        if delegation.to_agent_id != grant.actor.agent_id {
            return Err(ToolError::conflict(
                "not_your_delegation",
                "this delegation belongs to another seat",
            ));
        }
        let message_id = self
            .chat
            .complete_delegation(&delegation_id, &result)
            .map_err(tool_chat_error)?;
        self.stop_watcher(&delegation_id);
        let _ = self.chat.set_seat_status(&grant.actor.agent_id, "idle");
        self.synthesis.notify_one();
        Ok(ToolReply {
            result: json!({
                "delegation_id": delegation_id,
                "status": "done",
                "message_id": message_id,
            }),
        })
    }

    /// One Alfred turn per pending batch. If Alfred is busy or the queue is
    /// full the cards stay authoritative and the batch is skipped, never
    /// retried in a loop.
    pub(super) async fn synthesis_loop(&self) {
        loop {
            self.synthesis.notified().await;
            loop {
                let rows = match self.chat.pending_synthesis(SYNTHESIS_BATCH) {
                    Ok(rows) => rows,
                    Err(error) => {
                        eprintln!("team: could not read pending synthesis: {error}");
                        break;
                    }
                };
                let Some(first) = rows.first() else { break };
                let conversation_id = first.conversation_id.clone();
                let batch: Vec<_> = rows
                    .into_iter()
                    .filter(|row| row.conversation_id == conversation_id)
                    .collect();
                let text = match self.synthesis_text(&batch) {
                    Ok(text) => text,
                    Err(error) => {
                        eprintln!("team: could not build synthesis: {error}");
                        let _ = self.chat.skip_synthesis(&batch, "synthesis_unavailable");
                        break;
                    }
                };
                let alfred_id = match self.chat.alfred_id() {
                    Ok(agent_id) => agent_id,
                    Err(error) => {
                        eprintln!("team: no Alfred seat: {error}");
                        break;
                    }
                };
                let turn = match self.chat.enqueue_synthesis(&batch, &alfred_id, &text) {
                    Ok(turn) => turn,
                    Err(error) => {
                        eprintln!("team: could not enqueue synthesis: {error}");
                        let _ = self.chat.skip_synthesis(&batch, "synthesis_unavailable");
                        break;
                    }
                };
                match self.alfred.try_enqueue(turn) {
                    Ok(()) => {}
                    Err(error) => {
                        let _ = self.chat.skip_synthesis(&batch, &format!("alfred_{error}"));
                    }
                }
            }
        }
    }

    pub(super) fn synthesis_text(
        &self,
        rows: &[hn_core_chat::SynthesisRow],
    ) -> Result<String, ChatError> {
        let mut lines = Vec::with_capacity(rows.len());
        for row in rows {
            let detail = self.chat.delegation_detail(&row.delegation_id)?;
            let result = detail
                .result_message
                .as_ref()
                .map(|message| preview(&message.body_md))
                .unwrap_or_default();
            lines.push(format!(
                "- {} → {}: {} => {}",
                detail.delegation.from_name,
                detail.delegation.to_name,
                preview(&detail.delegation.task),
                result
            ));
        }
        Ok(format!(
            "Resume al usuario, en una sola respuesta breve, el resultado de estas delegaciones:\n{}",
            lines.join("\n")
        ))
    }

    /// `pane.*` is the live source of the card state: blocked -> waiting,
    /// working -> running, closed -> failed and seat stopped.
    pub(super) async fn listen_to_panes(&self) {
        let mut events = self.host.subscribe();
        loop {
            let frame = match events.recv().await {
                Ok(frame) => frame,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            };
            let Some(pane_id) = frame.body["pane_id"].as_str().map(str::to_string) else {
                continue;
            };
            if frame.event == hn_core_state::EVENT_STATUS {
                let state = match frame.body["agent_status"].as_str() {
                    Some("blocked") => Some(DelegationState::Waiting),
                    Some("working") => Some(DelegationState::Running),
                    _ => None,
                };
                if let Some(state) = state {
                    if let Err(error) = self.chat.delegation_state_from_pane(&pane_id, state) {
                        eprintln!("team: pane state for {pane_id}: {error}");
                    }
                }
            } else if frame.event == hn_core_state::EVENT_CLOSED {
                if let Ok(Some(delegation)) = self.chat.active_delegation_for_pane(&pane_id) {
                    self.stop_watcher(&delegation.id);
                }
                let _ = self.chat.fail_delegation_for_pane(&pane_id);
                let _ = self.chat.clear_seat_pane(&pane_id);
                self.remove_grant_for_pane(&pane_id);
            }
        }
    }
}
