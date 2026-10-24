//! The persistent chat surface: models, request plans and the `seq` reducer.

pub mod model;
pub mod model_team;
pub mod plan;
pub mod reducer;

pub use model::{
    parse_chat_bootstrap, parse_chat_conversation, parse_chat_conversations, parse_chat_delegation,
    parse_chat_event, parse_chat_section, parse_chat_sections, ChatBootstrap, ChatConversation,
    ChatConversationPage, ChatEntry, ChatEvent, ChatSection,
};
pub use model_team::{ChatDelegation, ChatDelegationChild, ChatProject, ChatSeat};
pub use plan::{
    new_chat_client_id, plan_chat_archive_conversation, plan_chat_bootstrap,
    plan_chat_cancel_delegation, plan_chat_conversations, plan_chat_create_conversation,
    plan_chat_delegation, plan_chat_messages, plan_chat_move_conversation,
    plan_chat_pin_conversation, plan_chat_read, plan_chat_rename_conversation, plan_chat_section,
    plan_chat_sections, plan_chat_send, plan_chat_timeline, ChatPlan,
};
pub use reducer::{new_chat_reducer, ChatReducer, ChatSnapshot};
