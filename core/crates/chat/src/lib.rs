//! Persistent chat domain for the herdr-notify core.
//!
//! This crate owns `chat.db`: the single company, seats, conversations,
//! messages, sections and the durable `chat_event` journal. It has no HTTP and
//! no agent-launch code; the API crate maps its errors to the contract and
//! `hn-core-seat` implements the seat port.

mod clock;
mod digest;
mod error;
mod id;
mod model;
mod model_approval;
mod model_hook;
mod model_mode;
mod model_team;
mod model_timeline;
mod policy;
mod schema;
mod schema_approval;
mod schema_base;
mod schema_team;
mod scorer;
mod seat;
mod store;
mod text;

pub use clock::{Clock, ManualClock, SystemClock};
pub use digest::{action_digest, canonical_json, is_well_formed};
pub use error::ChatError;
pub use id::new_ulid;
pub use model::{
    preview, AcceptedMessage, Bootstrap, CardLinks, ChatFrame, ChatSummary, Company, ContextPage,
    Conversation, ConversationDetail, ConversationFilter, ConversationPage, ConversationUpdate,
    Message, MessagePage, NewConversation, ProjectSummary, RunState, SeatSummary, Section,
    SectionChange, TimelineEntry, TimelinePage, Turn, Unread,
};
pub use model_approval::{
    Approval, ApprovalAudit, ApprovalFilter, ApprovalKind, ApprovalState, ApprovalStatus, Decision,
    NewApproval, Risk, DEFAULT_TTL_SECONDS, HOOK_CONSUMER, MAX_PENDING_PER_CONVERSATION,
    MAX_TTL_SECONDS,
};
pub use model_hook::{HookScope, HookTokenUse};
pub use model_mode::{ApprovalMode, ApprovalSettings, ModeChange, ProjectMode};
pub use model_team::{
    Card, Delegation, DelegationDetail, DelegationState, NewDelegation, NewProject, NewSeat,
    ProjectRecord, Seat, SynthesisInfo, SynthesisRow, UAC_SAFE, UAC_UNCHECKED, UAC_UNSAFE,
};
pub use schema::SearchMode;
pub use seat::{
    ChatSeat, SeatActor, SeatError, SeatErrorCode, ToolError, ToolErrorKind, ToolReply, ToolRequest,
};
pub use store::ChatStore;
pub use text::{message_text, TextError, MAX_MESSAGE_BYTES};
