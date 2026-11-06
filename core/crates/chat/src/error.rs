//! Errors of the chat store. The API maps them to `docs/contracts/errors.md`;
//! nothing here knows about HTTP.

use hn_core_persist::RandomError;
use thiserror::Error;

use crate::model_attachment::{CHUNK_MAX_BYTES, FILE_MAX_BYTES, MAX_ATTACHMENTS_PER_MESSAGE};
use crate::model_hire::MAX_HIRES_PER_AREA;
use crate::text::NameError;

#[derive(Debug, Error)]
pub enum ChatError {
    #[error("no such conversation")]
    NoSuchConversation,
    #[error("the conversation is archived")]
    ConversationArchived,
    #[error("the conversation has a turn, a delegation or an approval still open")]
    ConversationBusy,
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
    #[error("invalid plan: {0}")]
    InvalidPlan(&'static str),
    #[error("unknown plan: {0}")]
    NoSuchPlan(String),
    #[error("this delegation has more steps than the threshold: it needs an approved plan")]
    PlanRequired,
    #[error("the plan is not approved, or the delegation does not belong to its conversation")]
    PlanNotApproved,
    #[error("the plan already started another delegation")]
    PlanLinked,
    #[error("the plan was already decided")]
    PlanDecided,
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
    #[error("the owner refused this content; it must change before it is asked again")]
    ApprovalDigestRefused,
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
    #[error("invalid exclusion: {0}")]
    InvalidExclusion(&'static str),
    #[error("invalid recipe: {0}")]
    InvalidRecipe(&'static str),
    #[error("no such recipe: {0}")]
    NoSuchRecipe(String),
    #[error("the oficio already keeps the most recipes it can")]
    RecipeLimit,
    #[error("the oficio already has a recipe with that title")]
    RecipeTitleTaken,
    #[error("the recipe holds a secret and cannot be kept")]
    RecipeSecret(Vec<&'static str>),
    #[error("the recipe has risks that the person must confirm")]
    RecipeRisk(Vec<&'static str>),
    #[error("invalid schedule: {0}")]
    InvalidSchedule(&'static str),
    #[error("no such schedule: {0}")]
    NoSuchSchedule(String),
    #[error("the company already has the most active schedules it can")]
    ScheduleLimit,
    #[error("the schedule is done: it has no run left")]
    ScheduleDone,
    #[error("the schedule has no run after now")]
    ScheduleNoFutureRun,
    #[error("the plan was approved for an earlier text: approve a plan for this one")]
    PlanExpired,
    #[error("no active hire of the oficio {0}")]
    NoHireForRole(String),
    #[error("the delegation is not finished: {0}")]
    DelegationNotDone(String),
    #[error("the conversation has no project, so no hire can work in it")]
    ConversationWithoutProject,
    #[error("the company already keeps the most notes it can")]
    NoteLimit,
    #[error("no such note: {0}")]
    NoteNotFound(String),
    #[error("the company already keeps the most contacts it can")]
    ContactLimit,
    #[error("a contact keeps too many addresses")]
    AddressLimit,
    #[error("a contact keeps too many aliases")]
    AliasLimit,
    #[error("an address must be one local@domain.tld without spaces")]
    InvalidAddress,
    #[error("the address already belongs to the contact {0}")]
    AddressTaken(String),
    #[error("the alias already names another contact")]
    AliasTaken,
    #[error("no such contact: {0}")]
    ContactNotFound(String),
    #[error("the company already keeps the most alert rules it can")]
    AlertRuleLimit,
    #[error("the company already keeps this alert rule")]
    AlertRuleExists,
    #[error("no such alert rule: {0}")]
    AlertRuleNotFound(String),
    #[error("no such message: {0}")]
    NoSuchMessage(String),
    #[error("no such run: {0}")]
    NoSuchRun(String),
    #[error("the run is not active: {0}")]
    RunNotActive(String),
    #[error("reply_to does not name a message of this conversation: {0}")]
    InvalidReplyTo(String),
    /// A message that names files, refused as its attachments are (boxed: the
    /// attachment error already wraps this enum).
    #[error(transparent)]
    Attachment(Box<AttachmentError>),
    #[error("invalid suggestions: {0}")]
    InvalidSuggestions(&'static str),
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
    #[error(transparent)]
    Random(#[from] RandomError),
}

impl From<AttachmentError> for ChatError {
    fn from(error: AttachmentError) -> Self {
        Self::Attachment(Box::new(error))
    }
}

/// Errors of the attachments (ATT1). Apart from `ChatError`, like the hires: the
/// API maps them on its own. The messages never carry the content of a file.
#[derive(Debug, Error)]
pub enum AttachmentError {
    #[error("no such attachment")]
    NoSuchAttachment,
    #[error("the attachment is not in this conversation")]
    UnknownAttachment,
    #[error("the attachment is not ready")]
    NotReady,
    #[error("the attachment is not uploading")]
    NotUploading,
    #[error("the attachment is already in a message")]
    AlreadyLinked,
    #[error("a message takes at most {MAX_ATTACHMENTS_PER_MESSAGE} attachments")]
    TooManyAttachments,
    #[error("no such message in this conversation")]
    UnknownMessage,
    #[error("the conversation attachment quota would be exceeded")]
    ConversationQuota,
    #[error("the host attachment quota would be exceeded")]
    HostQuota,
    #[error("the file is larger than {FILE_MAX_BYTES} bytes")]
    FileTooLarge,
    #[error("the chunk is larger than {CHUNK_MAX_BYTES} bytes")]
    ChunkTooLarge,
    #[error("the offset does not continue the stored bytes")]
    OffsetMismatch { next_offset: u64 },
    #[error("invalid attachment or conversation id")]
    InvalidId,
    #[error("invalid name: one line, 1 to 120 characters")]
    InvalidName,
    #[error("invalid sha256: 64 hexadecimal characters")]
    InvalidSha256,
    #[error(transparent)]
    Chat(#[from] ChatError),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
}

/// Errors of the hires (ONBOARD-T1). Apart from `ChatError` on purpose: the API
/// and the MCP tools match that enum exhaustively, and they map these on their own.
#[derive(Debug, Error)]
pub enum HireError {
    #[error("the area already has {MAX_HIRES_PER_AREA} active hires: {0}")]
    AreaFull(String),
    #[error("no such hire: {0}")]
    NoSuchHire(String),
    #[error("the hire is not active: {0}")]
    HireNotActive(String),
    #[error("invalid name: {0}")]
    InvalidName(#[from] NameError),
    #[error(transparent)]
    Chat(#[from] ChatError),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("stored JSON is not valid: {0}")]
    Json(#[from] serde_json::Error),
}

/// Errors of the calendar (CALENDARIO-F4). Apart from `ChatError`, like the hires: the
/// API and the tools map these on their own, with the codes the contract names.
#[derive(Debug, Error)]
pub enum CalendarError {
    #[error("the company has no time zone yet")]
    TimezoneUnset,
    #[error("the company's time zone is already set")]
    TimezoneAlreadySet,
    #[error("the time zone is not an IANA zone name")]
    InvalidZone,
    #[error("the clock time does not exist in the company's time zone")]
    InvalidLocalTime,
    #[error("invalid {field}: {message}")]
    InvalidField {
        field: &'static str,
        message: &'static str,
    },
    #[error("the company already keeps the most events it can")]
    EventLimit,
    #[error("the calendar holds more events than one file takes: narrow the window")]
    TooManyEvents,
    #[error("no such event: {0}")]
    EventNotFound(String),
    #[error(transparent)]
    Chat(#[from] ChatError),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
}
