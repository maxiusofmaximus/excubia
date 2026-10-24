//! Errors of the chat store. The API maps them to `docs/contracts/errors.md`;
//! nothing here knows about HTTP.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ChatError {
    #[error("no such conversation")]
    NoSuchConversation,
    #[error("the conversation is archived")]
    ConversationArchived,
    #[error("unknown project: {0}")]
    UnknownProject(String),
    #[error("unknown section: {0}")]
    UnknownSection(String),
    #[error("unknown seat: {0}")]
    NoSuchSeat(String),
    #[error("unknown delegation: {0}")]
    NoSuchDelegation(String),
    #[error("the delegation is not active: {0}")]
    DelegationNotActive(String),
    #[error("a project already has a lead: {0}")]
    LeadExists(String),
    #[error("the project already exists at that path")]
    ProjectExists,
    #[error("the seat limit for the project is reached")]
    SeatLimit,
    #[error("a delegation is already active for the assignee")]
    AssigneeBusy,
    #[error("a delegation from this agent is already active")]
    ActorBusy,
    #[error("the role cannot perform this action")]
    ScopeDenied,
    #[error("the delegation depth limit is reached")]
    DepthExceeded,
    #[error("the delegation would create a cycle")]
    Cycle,
    #[error("the per-turn delegation budget is exhausted")]
    TurnBudget,
    #[error("the hourly delegation budget is exhausted")]
    HourBudget,
    #[error("no such approval")]
    NoSuchApproval,
    #[error("the approval expired before the decision")]
    ApprovalExpired,
    #[error("the approval was already decided by another device")]
    ApprovalAlreadyDecided,
    #[error("the approval does not match the action")]
    ApprovalMismatch,
    #[error("the hook token does not cover this approval")]
    HookScopeMismatch,
    #[error("the action digest was already used")]
    ApprovalDigestReused,
    #[error("the approval was not approved")]
    ApprovalNotApproved,
    #[error("the conversation already has too many approvals waiting")]
    ApprovalLimit,
    #[error("the rules changed while the scorer answered: send the action again")]
    ApprovalRaced,
    #[error("the seat cannot decide this approval: it is outside its scope")]
    ApprovalScopeDenied,
    #[error("the approval is at another level than the one the seat decides")]
    ApprovalLevelMismatch,
    #[error("the approval cannot be delegated: only the user decides it")]
    ApprovalNotDelegable,
    #[error("a seat cannot decide its own approval")]
    ApprovalSelfApproval,
    #[error("the hourly budget of approval decisions is exhausted")]
    ApprovalBudget,
    #[error("no such run: {0}")]
    NoSuchRun(String),
    #[error("the run is not active: {0}")]
    RunNotActive(String),
    #[error("invalid cursor: {0}")]
    InvalidCursor(String),
    #[error("chat database is not initialised")]
    NotInitialised,
    #[error("inconsistent chat database: {0}")]
    Inconsistent(&'static str),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("stored event is not valid JSON: {0}")]
    StoredEvent(#[from] serde_json::Error),
}
