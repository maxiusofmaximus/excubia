//! The MCP tools CORE6B/CORE7 expose. Arguments are strict JSON objects; text
//! is bounded by the same 32 KiB rule as the HTTP API. An unknown tool is an
//! error, never a shell escape.
//!
//! The approval request and the `tools/list` schemas live in their own
//! modules: they grow with the surface and share nothing with the parser.

mod agenda_definitions;
mod alert;
mod alert_definitions;
mod approval;
mod args;
mod calendar;
mod calendar_definitions;
mod call;
mod contact;
mod decide;
mod definitions;
mod mail;
mod mail_organize_definitions;
mod note;
mod parse;
mod pending;
mod status;

#[cfg(test)]
mod tests;

pub use alert::AlertCall;
pub use approval::ApprovalRequest;
pub use calendar::CalendarCall;
pub use call::{AgentSpawn, DelegationAssign, DelegationComplete, ProjectCreate, ToolCall};
pub use contact::ContactCall;
pub use decide::ApprovalDecide;
pub use definitions::definitions;
pub use mail::{
    check_draft_fields, BodyFormat, DraftFields, DraftRef, DraftUpdate, FoldersQuery, InboxFolder,
    InboxQuery, MailAttach, MailCall, MessageRef, MoveApply, MoveFolder, MoveProposal, MoveUndo,
    Organize, ProposedMove, TriageQuery, UndoList,
};
pub use note::NoteCall;
pub use parse::parse;
use parse::{invalid_arguments, name_field, no_arguments, text_error};
