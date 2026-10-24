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
mod preflight;
mod profile;
mod prompt;
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
    parse as parse_tool, AgentSpawn, ApprovalDecide, ApprovalRequest, DelegationAssign,
    DelegationComplete, ProjectCreate, ToolCall,
};
