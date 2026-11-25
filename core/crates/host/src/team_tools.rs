//! The MCP surface of the team: `project.create`, `agent.spawn` and the
//! panels they start. Every call is scoped to the seat that asks for it.

use std::path::PathBuf;

use excubia_chat::{
    new_ulid, ChatSeat, HookScope, NewProject, NewSeat, Seat, SeatActor, ToolError, ToolErrorKind,
    ToolReply, ToolRequest,
};
use excubia_seat::{prepare_resident, ResidentInput, SelectedProfile, ToolCall, UacState};
use serde_json::json;

use crate::SpawnSpec;

use super::team::{lock, TeamGrant, TeamService, MAX_LIVE_PANES};
use super::team_errors::tool_chat_error;
use super::team_paths::{resolve_project_path, role_text};

/// A refusal that stops a seat before anything is written. Unavailable (503), with the
/// stable code the desktop and Alfred read; the message says what to do next.
fn resident_refusal(code: &'static str, message: String) -> ToolError {
    ToolError {
        kind: ToolErrorKind::Unavailable,
        code,
        field: None,
        message,
    }
}

impl TeamService {
    pub(super) async fn team_tool(
        &self,
        grant: TeamGrant,
        call: ToolCall,
    ) -> Result<ToolReply, ToolError> {
        match call {
            ToolCall::DelegationComplete(complete) => {
                self.complete(grant, complete.delegation_id, complete.result, complete.failed)
            }
            ToolCall::AgentSpawn(spawn) => match grant.actor.role_id.as_str() {
                "alfred" | "lead" => self.spawn_agent(&grant.actor, spawn).await,
                _ => Err(ToolError::invalid(
                    "scope_denied",
                    "tool",
                    "a specialist cannot spawn agents",
                )),
            },
            ToolCall::DelegationAssign(assign) => match grant.actor.role_id.as_str() {
                "alfred" | "lead" => self.assign(&grant.actor, assign).await,
                _ => Err(ToolError::invalid(
                    "depth_exceeded",
                    "tool",
                    "a specialist is depth 2 and cannot delegate",
                )),
            },
            ToolCall::ProjectCreate(create) => {
                if grant.actor.role_id != "alfred" {
                    return Err(ToolError::invalid(
                        "scope_denied",
                        "tool",
                        "only Alfred creates projects",
                    ));
                }
                self.create_project(&grant.actor, create).await
            }
            ToolCall::ApprovalRequest(request) => {
                self.request_approval(&grant.actor, Some(grant.pane_id.clone()), request)
                    .await
            }
            ToolCall::PlanPropose(propose) => {
                self.propose_plan(&grant.actor, propose.steps, propose.run_on_approval)
            }
            ToolCall::PlanRead(plan_id) => self.read_plan(&grant.actor, plan_id),
            ToolCall::ApprovalDecide(decide) => self.decide_delegated(&grant.actor, decide),
            ToolCall::ApprovalPending => self.pending_approvals(&grant.actor),
            ToolCall::StatusOverview => match grant.actor.role_id.as_str() {
                "alfred" => self.status_overview(&grant.actor).await,
                _ => Err(ToolError::invalid(
                    "scope_denied",
                    "tool",
                    "status.overview belongs to Alfred; a project seat reads its own panel",
                )),
            },
            ToolCall::GithubLink => match grant.actor.role_id.as_str() {
                "alfred" | "lead" => {
                    self.github_link(&grant.actor, Some(grant.pane_id.clone())).await
                }
                _ => Err(ToolError::invalid(
                    "scope_denied",
                    "tool",
                    "only Alfred and a project lead ask to link GitHub",
                )),
            },
            ToolCall::Mail(_) => Err(ToolError::invalid(
                "scope_denied",
                "tool",
                "the mail tools belong to Alfred, not to a project seat",
            )),
            ToolCall::StyleGet => match grant.actor.role_id.as_str() {
                "alfred" => self.mail.style(),
                _ => Err(ToolError::invalid(
                    "scope_denied",
                    "tool",
                    "style.get belongs to Alfred, not to a project seat",
                )),
            },
            ToolCall::StyleLearn(query) => match grant.actor.role_id.as_str() {
                "alfred" => self.mail.style_learn(&query).await,
                _ => Err(ToolError::invalid(
                    "scope_denied",
                    "tool",
                    "style.learn_sent belongs to Alfred, not to a project seat",
                )),
            },
            ToolCall::StylePropose(proposal) => match grant.actor.role_id.as_str() {
                "alfred" => self.mail.style_propose(&grant.actor, &proposal).await,
                _ => Err(ToolError::invalid(
                    "scope_denied",
                    "tool",
                    "style.propose_rule belongs to Alfred, not to a project seat",
                )),
            },
            ToolCall::Note(call) => match grant.actor.role_id.as_str() {
                "alfred" => self.note_tool(&grant.actor, call).await,
                _ => Err(ToolError::invalid(
                    "scope_denied",
                    "tool",
                    "the note tools belong to Alfred, not to a project seat",
                )),
            },
            ToolCall::Contact(call) => match grant.actor.role_id.as_str() {
                "alfred" => self.contact_tool(&grant.actor, call),
                _ => Err(ToolError::invalid(
                    "scope_denied",
                    "tool",
                    "the contact tools belong to Alfred, not to a project seat",
                )),
            },
            ToolCall::Calendar(call) => match grant.actor.role_id.as_str() {
                "alfred" => self.calendar_tool(&grant.actor, call),
                _ => Err(ToolError::invalid(
                    "scope_denied",
                    "tool",
                    "the calendar tools belong to Alfred, not to a project seat",
                )),
            },
            ToolCall::Alert(call) => match grant.actor.role_id.as_str() {
                "alfred" => self.alert_tool(&grant.actor, call),
                _ => Err(ToolError::invalid(
                    "scope_denied",
                    "tool",
                    "the alert tools belong to Alfred, not to a project seat",
                )),
            },
            ToolCall::Doc(call) => match grant.actor.role_id.as_str() {
                "alfred" => self.docs.call(&grant.actor, call).await,
                _ => Err(ToolError::invalid(
                    "scope_denied",
                    "tool",
                    "the document tools belong to Alfred, not to a project seat",
                )),
            },
            ToolCall::DocCreate(call) => match grant.actor.role_id.as_str() {
                "alfred" => self.docs.create(&grant.actor, call).await,
                _ => Err(ToolError::invalid(
                    "scope_denied",
                    "tool",
                    "the document tools belong to Alfred, not to a project seat",
                )),
            },
            ToolCall::DocEdit(call) => match grant.actor.role_id.as_str() {
                "alfred" => self.docs.edit(&grant.actor, call).await,
                _ => Err(ToolError::invalid(
                    "scope_denied",
                    "tool",
                    "the document tools belong to Alfred, not to a project seat",
                )),
            },
            ToolCall::Reply { .. } | ToolCall::AskUser(_) | ToolCall::Progress(_) => {
                Err(ToolError::invalid(
                    "scope_denied",
                    "tool",
                    "resident seats complete delegations, they do not reply to the chat",
                ))
            }
        }
    }

    pub(super) async fn alfred_tool(
        &self,
        actor: SeatActor,
        call: ToolCall,
        token: &str,
        request: ToolRequest,
    ) -> Result<ToolReply, ToolError> {
        match call {
            ToolCall::Reply { .. } | ToolCall::AskUser(_) | ToolCall::Progress(_) => {
                self.alfred.tool(token, request).await
            }
            ToolCall::ProjectCreate(create) => self.create_project(&actor, create).await,
            ToolCall::AgentSpawn(spawn) => self.spawn_agent(&actor, spawn).await,
            ToolCall::DelegationAssign(assign) => self.assign(&actor, assign).await,
            ToolCall::DelegationComplete(_) => Err(ToolError::invalid(
                "scope_denied",
                "tool",
                "Alfred does not complete delegated tasks",
            )),
            ToolCall::ApprovalRequest(request) => {
                self.request_approval(&actor, None, request).await
            }
            ToolCall::GithubLink => self.github_link(&actor, None).await,
            ToolCall::ApprovalDecide(decide) => self.decide_delegated(&actor, decide),
            ToolCall::PlanPropose(propose) => {
                self.propose_plan(&actor, propose.steps, propose.run_on_approval)
            }
            ToolCall::PlanRead(plan_id) => self.read_plan(&actor, plan_id),
            ToolCall::ApprovalPending => self.pending_approvals(&actor),
            ToolCall::StatusOverview => self.status_overview(&actor).await,
            ToolCall::Mail(call) => self.mail.call(&actor, call).await,
            ToolCall::StyleGet => self.mail.style(),
            ToolCall::StyleLearn(query) => self.mail.style_learn(&query).await,
            ToolCall::StylePropose(proposal) => self.mail.style_propose(&actor, &proposal).await,
            ToolCall::Doc(call) => self.docs.call(&actor, call).await,
            ToolCall::DocCreate(call) => self.docs.create(&actor, call).await,
            ToolCall::DocEdit(call) => self.docs.edit(&actor, call).await,
            ToolCall::Note(call) => self.note_tool(&actor, call).await,
            ToolCall::Contact(call) => self.contact_tool(&actor, call),
            ToolCall::Calendar(call) => self.calendar_tool(&actor, call),
            ToolCall::Alert(call) => self.alert_tool(&actor, call),
        }
    }

    /// `github.link`: the answer waits for the operator and for the link to end (see `github_link`).
    pub(super) async fn github_link(
        &self,
        actor: &SeatActor,
        pane_id: Option<String>,
    ) -> Result<ToolReply, ToolError> {
        std::sync::Arc::clone(&self.github)
            .request(actor.clone(), pane_id)
            .await
    }

    pub(super) async fn create_project(
        &self,
        actor: &SeatActor,
        create: excubia_seat::ProjectCreate,
    ) -> Result<ToolReply, ToolError> {
        if actor.role_id != "alfred" {
            return Err(ToolError::invalid(
                "scope_denied",
                "tool",
                "only Alfred creates projects",
            ));
        }
        // The lead is created with the project, so a project that cannot get a panel is
        // refused before any row or folder exists: no half-made project to clean up.
        self.resident_gate()?;
        let root_path =
            resolve_project_path(&self.workspaces_root, &create.name, create.path.as_deref())
                .map_err(|message| ToolError::invalid("invalid_path", "path", message))?;
        let mut project = self
            .chat
            .create_project(NewProject {
                name: create.name,
                root_path,
            })
            .map_err(tool_chat_error)?;
        // The conversation moves to the new project unless it has open work elsewhere.
        // The project exists from here on, so nothing below may answer as if it did not.
        let focused = actor.conversation_id.is_empty()
            || self
                .chat
                .set_conversation_project(&actor.conversation_id, &project.id)
                .is_ok();
        let lead_id = project
            .lead
            .as_ref()
            .map(|lead| lead.agent_id.clone())
            .ok_or_else(|| ToolError::unavailable("the project has no lead seat"))?;
        let seat = self.chat.seat(&lead_id).map_err(tool_chat_error)?;
        // The reply says whether the lead runs: Alfred reports what happened, not what he hoped.
        let lead = match self.spawn_panel(&seat).await {
            Ok(pane_id) => {
                if let Some(lead) = project.lead.as_mut() {
                    lead.pane_id = Some(pane_id.clone());
                    lead.status = "idle".to_string();
                }
                json!({ "agent_id": lead_id, "running": true, "pane_id": pane_id })
            }
            Err(error) => json!({
                "agent_id": lead_id,
                "running": false,
                "error": { "code": error.code, "message": error.message },
            }),
        };
        Ok(ToolReply {
            result: json!({
                "project": project,
                "lead": lead,
                "conversation_on_project": focused,
            }),
        })
    }

    pub(super) async fn spawn_agent(
        &self,
        actor: &SeatActor,
        spawn: excubia_seat::AgentSpawn,
    ) -> Result<ToolReply, ToolError> {
        let project_id = match actor.role_id.as_str() {
            "lead" => actor.project_id.clone().ok_or_else(|| {
                ToolError::invalid("unknown_project", "project_id", "lead has no project")
            })?,
            _ => match spawn
                .project_id
                .clone()
                .or_else(|| actor.project_id.clone())
            {
                Some(project_id) => project_id,
                None => {
                    return Err(ToolError::invalid(
                        "invalid_arguments",
                        "project_id",
                        "project_id is required when Alfred creates a seat",
                    ))
                }
            },
        };
        if spawn.role == "lead" && actor.role_id != "alfred" {
            return Err(ToolError::invalid(
                "scope_denied",
                "role",
                "only Alfred creates the lead of a project",
            ));
        }
        // A seat that cannot get a panel is refused before its row exists.
        self.resident_gate()?;
        let seat = self
            .chat
            .create_seat(NewSeat {
                project_id,
                name: spawn.name,
                role_id: spawn.role,
            })
            .map_err(tool_chat_error)?;
        let pane_id = self.spawn_panel(&seat).await?;
        Ok(ToolReply {
            result: json!({
                "seat": {
                    "agent_id": seat.agent_id,
                    "name": seat.name,
                    "role_id": seat.role_id,
                    "project_id": seat.project_id,
                    "pane_id": pane_id,
                    "status": "idle",
                }
            }),
        })
    }

    /// The CLI that runs a resident panel. Pure: it writes nothing and runs nothing, so
    /// every entry point that creates a row or a folder asks it first.
    pub(super) fn resident_gate(&self) -> Result<SelectedProfile, ToolError> {
        let selected = self.alfred.selected().ok_or_else(|| {
            resident_refusal(
                "no_resident_engine",
                "No installed CLI can run a resident project panel. Install Claude Code, Codex or OpenCode at a version the host lists, then try again.".to_string(),
            )
        })?;
        if !selected.profile.supports_resident() {
            return Err(resident_refusal(
                "resident_unsupported",
                format!(
                    "{} has no resident project panel mode. Update Excubia or install Claude Code, Codex or OpenCode, which have one, then try again.",
                    selected.profile.id.as_str()
                ),
            ));
        }
        Ok(selected.clone())
    }

    pub(super) async fn spawn_panel(&self, seat: &Seat) -> Result<String, ToolError> {
        // Six live panels per host: the limit is checked where the panel is
        // born, so the lead of a project counts like any other seat.
        if self.host.live_panes() >= MAX_LIVE_PANES {
            return Err(ToolError::conflict(
                "pane_limit",
                "the host already has six live panels",
            ));
        }
        // No canary, folder or panel for a CLI that cannot run one.
        let selected = self.resident_gate()?;
        if !self.alfred.is_available() {
            return Err(ToolError::unavailable("the seat is shutting down"));
        }
        // The blocking UAC adapter is proven before the panel exists: a seat
        // whose adapter failed is `unsafe` and is never launched.
        let uac = self.verify_uac(seat, &selected.profile).await?;
        // The OpenCode plugin reaches a panel only when this host's canary proved it `Safe`,
        // the same gate a turn uses. Otherwise shell stays denied for the panel.
        let plugin_dir = match uac {
            UacState::Safe => self.uac_plugin_dir.as_deref(),
            UacState::Unchecked | UacState::Unsafe => None,
        };
        let project_id = seat.project_id.as_deref().ok_or_else(|| {
            ToolError::invalid("unknown_project", "project_id", "seat has no project")
        })?;
        let project = self.chat.project(project_id).map_err(tool_chat_error)?;
        let cwd = PathBuf::from(project.root_path);
        let seat_dir = self
            .data_dir
            .join("seats")
            .join(&seat.agent_id)
            .join("resident");
        let role = role_text(&seat.role_id, &seat.name);
        let plan = prepare_resident(&ResidentInput {
            profile: &selected.profile,
            agent_id: &seat.agent_id,
            role_id: &seat.role_id,
            seat_dir: &seat_dir,
            role: &role,
            tool_url: self.alfred.tool_url(),
            mcp_command: self.alfred.mcp_command(),
            hook_url: self.alfred.hook_url(),
            hook_command: self.alfred.hook_command(),
            plugin_dir,
        })
        .map_err(|error| {
            ToolError::unavailable(format!(
                "resident panel for {}: {error}",
                selected.profile.id.as_str()
            ))
        })?;
        plan.materialise()
            .map_err(|error| ToolError::unavailable(format!("resident panel files: {error}")))?;

        let token = new_ulid();
        let hook_token = self
            .chat
            .issue_hook_token(&HookScope::seat(&seat.agent_id))
            .map_err(tool_chat_error)?;
        let mut spec = SpawnSpec {
            program: plan.program,
            args: plan.args,
            cwd: Some(cwd),
            label: Some(format!(
                "{} — {}",
                seat.name,
                match seat.role_id.as_str() {
                    "lead" => "lead",
                    _ => "especialista",
                }
            )),
            env: plan.env,
            replaces_pane: seat.pane_id.clone(),
            seat_id: Some(seat.agent_id.clone()),
        };
        spec.env
            .push(("EXCUBIA_TOOL_TOKEN".to_string(), token.clone()));
        // The panel's own hook credential: it covers the seat, not one turn.
        spec.env
            .push(("EXCUBIA_HOOK_TOKEN".to_string(), hook_token));
        spec.env
            .push(("EXCUBIA_CHAT_AGENT_ID".to_string(), seat.agent_id.clone()));
        spec.env
            .push(("EXCUBIA_SEAT_ROLE".to_string(), seat.role_id.clone()));
        let pane_id = self
            .host
            .start(spec)
            .await
            .map_err(|error| ToolError::unavailable(format!("start panel: {error:#}")))?;
        self.chat
            .set_seat_pane(&seat.agent_id, Some(&pane_id))
            .map_err(tool_chat_error)?;
        self.chat
            .set_seat_status(&seat.agent_id, "idle")
            .map_err(tool_chat_error)?;
        lock(&self.grants).insert(
            token,
            TeamGrant {
                actor: SeatActor {
                    agent_id: seat.agent_id.clone(),
                    role_id: seat.role_id.clone(),
                    project_id: seat.project_id.clone(),
                    conversation_id: String::new(),
                    run_id: String::new(),
                },
                pane_id: pane_id.clone(),
            },
        );
        Ok(pane_id)
    }
}
