//! Alfred's seat: the real-CLI adapter layer of CORE6B.
//!
//! The crate has five responsibilities, each in its own module:
//! - [`profile`]: the declarative CLI capability matrix and version selection.
//! - [`service`]: the one-turn FIFO worker and run lifecycle.
//! - [`mcp`]: the `hn-mcp` stdio shim and its loopback client.
//! - [`http`]: the loopback HTTP client the seat's own processes share.
//! - [`preflight`]: the UAC canary that proves a blocking adapter works.
//!
//! The API talks to this crate only through `hn_core_chat::ChatSeat`.

mod approvals;
mod canary_provider;
pub mod hook_action;
pub mod http;
mod launch;
mod mcp;
mod output_tail;
mod preflight;
mod profile;
mod prompt;
mod role_seed;
mod role_seed_apply;
mod role_seed_auto;
mod role_seed_folders;
mod role_seed_inbox;
mod role_seed_moves;
mod role_seed_old;
mod role_seed_triage;
mod service;
mod service_tools;
mod tools;

pub use approvals::{approval_for, approval_result, ApprovalAsk};
pub use canary_provider::CanaryProvider;
pub use http::{HttpResponse, LoopbackUrl};
pub use launch::{claude_settings, prepare_resident, LaunchError, LaunchPlan, ResidentInput};
pub use mcp::run_stdio;
pub use preflight::{
    canary_plan, classify as classify_canary, run_canary, CanaryOutcome, CanaryPlan, UacState,
    BLOCKED_MARKER, CANARY_COMMAND, CANARY_MARKER, LOAD_FAILURE_MARKER,
};
pub use profile::{
    probe, select, shipped, test_profile, CliId, CliProfile, HookSupport, Isolation,
    McpDeclaration, PromptDelivery, SelectedProfile, TurnMode, Version,
};
pub use service::{SeatConfig, SeatService, CONTEXT_CHARS, CONTEXT_MESSAGES};
pub use tools::{
    check_draft_fields, parse as parse_tool, AgentSpawn, AlertCall, ApprovalDecide,
    ApprovalRequest, BodyFormat, CalendarCall, ContactCall, DelegationAssign, DelegationComplete,
    DraftFields, DraftRef, DraftUpdate, FoldersQuery, InboxFolder, InboxQuery, MailAttach,
    MailCall, MessageRef, MoveApply, MoveFolder, MoveProposal, MoveUndo, NoteCall, Organize,
    ProjectCreate, ProposedMove, ToolCall, TriageQuery,
};
