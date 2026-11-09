//! Persistent chat domain for the Excubia core.
//!
//! This crate owns `chat.db`: the single company, seats, conversations,
//! messages, sections and the durable `chat_event` journal. It has no HTTP and
//! no agent-launch code; the API crate maps its errors to the contract and
//! `hn-core-seat` implements the seat port.

mod attachments;
mod clock;
mod digest;
mod error;
mod ics;
mod id;
mod model;
mod model_alert;
mod model_approval;
mod model_attachment;
mod model_calendar;
mod model_contact;
mod model_hire;
mod model_hook;
mod model_mode;
mod model_note;
mod model_plan;
mod model_recipe;
mod model_schedule;
mod schedule_due;
mod schedule_launch;
mod recipe_guard;
mod model_team;
mod model_team_status;
mod model_timeline;
mod policy;
mod recurrence;
mod recurrence_expand;
mod schema;
mod schema_agenda;
mod schema_alert;
mod schema_approval;
mod schema_attachment;
mod schema_base;
mod schema_calendar;
mod schema_contact;
mod schema_hire;
mod schema_ics;
mod schema_mail;
mod schema_plan;
mod schema_recipe;
mod schema_agent_file;
mod schema_schedule;
mod schema_progress;
mod schema_recurrence;
mod schema_reply;
mod schema_team;
mod scorer;
pub use scorer::local_endpoint;
mod seat;
mod store;
mod text;
mod timezone;

pub use attachments::AttachmentFiles;
pub use clock::{Clock, ManualClock, SystemClock};
pub use digest::{action_digest, canonical_json, is_well_formed};
pub use error::{AttachmentError, CalendarError, ChatError, HireError};
pub use ics::{export_calendar, read_calendar, IcsError, ImportReport, Parsed};
pub use id::new_ulid;
pub use model::{
    preview, AcceptedMessage, Bootstrap, CardLinks, ChatFrame, ChatSummary, Company,
    ContextAttachment, ContextPage, Conversation, ConversationCounts, ConversationDetail,
    ConversationFilter, ConversationPage, ConversationUpdate, Message, MessagePage,
    NewConversation, ProjectSummary, Quote, RunState, SeatSummary, Section, SectionChange,
    TimelineEntry, TimelinePage, Turn, Unread,
};
pub use model_alert::{
    decide, field_refusal, AlertAction, AlertDecision, AlertMode, AlertRule, AlertStatus,
    AlertSubject, AlertTrigger, NewAlertRule, RuleAuthor, MAX_ALERT_RULES,
};
pub use model_approval::{
    Approval, ApprovalAudit, ApprovalFilter, ApprovalKind, ApprovalState, ApprovalStatus, Decision,
    NewApproval, Reader, Risk, DEFAULT_TTL_SECONDS, HOOK_CONSUMER, MAX_PENDING_PER_CONVERSATION,
    MAX_TTL_SECONDS,
};
pub use model_attachment::{
    previewable, Attachment, AttachmentState, FileKind, LinkedFile, ReadyFile, ATTACHMENT_ONLY_TEXT,
    CHUNK_MAX_BYTES, CONVERSATION_QUOTA_BYTES, FILE_MAX_BYTES, HOST_QUOTA_BYTES,
    MAX_ATTACHMENTS_PER_MESSAGE,
};
pub use model_calendar::{
    CalendarEvent, EventPatch, Listed, Listing, NewEvent, Window, CALENDAR_DELETE_TOOL,
    EVENT_LIST_MAX, EVENT_LOCATION_MAX_CHARS, EVENT_NOTES_MAX_BYTES, EVENT_TITLE_MAX_CHARS,
    MAX_EVENTS, REMINDER_MAX_MINUTES,
};
pub use model_contact::{
    AddressSource, Contact, ContactAddress, ContactFields, ImportOutcome, ADDRESS_MAX_CHARS,
    CONTACT_DELETE_TOOL, CONTACT_NAME_MAX_CHARS, MAX_ADDRESSES_PER_CONTACT,
    MAX_ALIASES_PER_CONTACT,
};
pub use model_hire::{
    Hire, HireFilter, HireOutcome, HireStatus, NewHire, OnboardingState, MAX_HIRES_PER_AREA,
};
pub use model_hook::{HookScope, HookTokenUse};
pub use model_mode::{ApprovalMode, ApprovalSettings, ModeChange, ProjectMode};
pub use model_note::{
    Note, NoteHit, MAX_NOTES, NOTE_DELETE_TOOL, NOTE_SEARCH_MAX_RESULTS, NOTE_SUBJECT_MAX_CHARS,
};
pub use model_plan::{NewPlan, Plan, PlanDecision, PlanStatus, PlanStep, Touch};
pub use model_recipe::{
    draft_text, DraftSource, Recipe, RecipeRun, RecipeText, MAX_RECIPES_PER_ROLE,
    RECIPE_BODY_CHARS, RECIPE_TITLE_CHARS,
};
pub use model_team::{
    Card, Delegation, DelegationDetail, DelegationState, NewDelegation, NewProject, NewSeat,
    ProjectRecord, Seat, SynthesisInfo, SynthesisRow, UAC_SAFE, UAC_UNCHECKED, UAC_UNSAFE,
};
pub use model_team_status::{
    activity_of, status_of, summarize, Activity, MemberInput, TeamMemberSummary, TeamStatus,
};
pub use recurrence::{RecurrenceError, Rule};
pub use schema::SearchMode;
pub use seat::{
    ChatSeat, SeatActor, SeatError, SeatErrorCode, ToolError, ToolErrorKind, ToolReply, ToolRequest,
};
pub use store::{
    validate_suggestions, Claim, Launch, AlertSummary, AlertWriter, CalendarSummary, ChatStore, EventOrigin,
    HireLink, MailDraftCard, NewMailCard, NextEvent, PendingApproval, PendingPlan, ProgressOutcome,
    SenderHistory, StatusApproval, StatusDelegation, StatusFacts,
};
pub use model_schedule::{
    cadence_of, parse_start_date, parse_time_local, rule_text, timing_of, Cadence, Content,
    NewSchedule, RunOutcome, Schedule, ScheduleRun, ScheduleStatus, Timing, DEFAULT_PER_DAY_PER_AGENT,
    DEFAULT_PER_DAY_TOTAL, DEFAULT_PER_HOUR_PER_AGENT, HISTORY_KEEP, MAX_ACTIVE_SCHEDULES,
};
pub use schedule_launch::launch_schedule;
pub use text::{message_text, NameError, TextError, MAX_MESSAGE_BYTES};
pub use timezone::EventTime;
