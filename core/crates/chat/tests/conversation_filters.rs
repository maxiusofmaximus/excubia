//! Store-level tests of the listing filters `attention` and `working`, of the
//! counts that mirror them, and of how they combine with archives and paging.

mod common;

use common::{team, Team};
use hn_core_chat::{
    action_digest, Approval, ApprovalKind, ApprovalMode, ChatStore, ConversationFilter,
    ConversationUpdate, NewApproval, NewConversation, SectionChange, DEFAULT_TTL_SECONDS,
};
use serde_json::json;

fn conversation(store: &ChatStore, title: &str) -> String {
    store
        .create_conversation(NewConversation {
            title: Some(title.to_string()),
            project_id: None,
        })
        .expect("conversation")
        .id
}

/// An agent reply in a conversation, through a turn the user opened. The turn
/// stays queued, so the conversation is also working.
fn agent_reply(store: &ChatStore, conversation_id: &str) -> String {
    let accepted = store
        .append_user_message(conversation_id, "seed", "hola", None)
        .expect("message");
    store
        .append_agent_question(&accepted.turn.id, "¿Y ahora?")
        .expect("reply");
    accepted.turn.id
}

/// A command asked by the specialist from the project root (the same shape
/// as `approval_modes`): born at the lead level under the default policy.
fn ask(team: &Team, command: &str) -> Approval {
    let args = json!({ "command": command });
    let cwd = team.root.to_string_lossy().to_string();
    team.store
        .create_approval(NewApproval {
            conversation_id: Some(team.conversation.clone()),
            seat_id: Some(team.specialist.clone()),
            agent_label: Some("ana".to_string()),
            run_id: None,
            pane_id: None,
            kind: ApprovalKind::Command,
            tool: "bash".to_string(),
            summary: format!("Ejecutar comando: {command}"),
            cwd: Some(cwd.clone()),
            paths: Vec::new(),
            risk: None,
            action_digest: action_digest("command", "bash", &args, &cwd, &[]),
            payload_redacted: Some(args),
            ttl_seconds: DEFAULT_TTL_SECONDS,
        })
        .expect("approval")
}

fn filter(attention: bool, working: bool, archived: bool) -> ConversationFilter {
    ConversationFilter {
        q: None,
        section_id: None,
        pinned: false,
        archived,
        attention,
        working,
        cursor: None,
        limit: 100,
    }
}

/// The ids a listing returns, sorted: the order is by time, the tests compare sets.
fn listed(store: &ChatStore, attention: bool, working: bool, archived: bool) -> Vec<String> {
    let page = store
        .list_conversations(filter(attention, working, archived))
        .expect("list");
    assert!(page.next_cursor.is_none());
    sorted(page.conversations.into_iter().map(|row| row.id).collect())
}

fn sorted(mut ids: Vec<String>) -> Vec<String> {
    ids.sort();
    ids
}

/// Five conversations with an agent reply each. The first one has finished its
/// turn (unread, not working), the third one is archived.
fn inbox(store: &ChatStore) -> [String; 5] {
    let ids = ["Uno", "Dos", "Tres", "Cuatro", "Cinco"].map(|title| conversation(store, title));
    let turns: Vec<String> = ids.iter().map(|id| agent_reply(store, id)).collect();
    store
        .complete_run_with_reply(&turns[0], "Listo", &[])
        .expect("complete");
    store
        .update_conversation(
            &ids[2],
            ConversationUpdate {
                title: None,
                pinned: None,
                section: SectionChange::Keep,
                archived: Some(true),
            },
        )
        .expect("archive");
    ids
}

#[test]
fn working_follows_the_turn_until_it_completes() {
    let dir = hn_core_test_dir::TestDir::new("core-conversation-filters-working");
    let store = ChatStore::open(&dir.join("chat.db")).expect("open store");
    let busy = conversation(&store, "Ocupado");
    conversation(&store, "Quieto");
    let turn = store
        .append_user_message(&busy, "seed", "hola", None)
        .expect("message")
        .turn
        .id;
    assert_eq!(listed(&store, false, true, false), vec![busy.clone()]);
    assert!(store.start_run(&turn).expect("start"));
    assert_eq!(listed(&store, false, true, false), vec![busy.clone()]);
    store
        .complete_run_with_reply(&turn, "Listo", &[])
        .expect("complete");
    assert!(listed(&store, false, true, false).is_empty());
    assert_eq!(listed(&store, false, false, false).len(), 2);
    assert_eq!(store.conversation_counts().expect("counts").working, 0);
}

#[test]
fn attention_is_an_unread_reply_or_an_approval_at_the_operator() {
    let team = team("attention");
    let store = &team.store;
    let replied = conversation(store, "Respondió");
    conversation(store, "Callado");
    agent_reply(store, &replied);
    let approval = ask(&team, "npm install left-pad");
    assert_eq!(approval.level.as_deref(), Some("lead"));
    assert_eq!(
        listed(store, true, false, false),
        sorted(vec![replied.clone()])
    );

    store.mark_read(&replied, 99).expect("read");
    assert!(
        listed(store, true, false, false).is_empty(),
        "a read reply and a lead-level approval do not need the operator"
    );

    store
        .set_approval_mode("req-mode", "device-mode", None, ApprovalMode::AlwaysUser)
        .expect("mode");
    assert_eq!(
        store
            .approval(&approval.id)
            .expect("approval")
            .level
            .as_deref(),
        Some("user")
    );
    assert_eq!(
        listed(store, true, false, false),
        vec![team.conversation.clone()]
    );
}

#[test]
fn filters_combine_with_archives_and_paging() {
    let dir = hn_core_test_dir::TestDir::new("core-conversation-filters-combine");
    let store = ChatStore::open(&dir.join("chat.db")).expect("open store");
    let [first, second, archived, fourth, fifth] = inbox(&store);

    assert_eq!(
        listed(&store, true, false, false),
        sorted(vec![
            first.clone(),
            second.clone(),
            fourth.clone(),
            fifth.clone()
        ])
    );
    assert_eq!(
        listed(&store, true, true, false),
        sorted(vec![second.clone(), fourth.clone(), fifth.clone()]),
        "the finished turn is unread but no longer working"
    );
    assert_eq!(listed(&store, true, false, true), vec![archived.clone()]);
    assert_eq!(listed(&store, false, true, true), vec![archived.clone()]);

    let mut seen = Vec::new();
    let mut cursor = None;
    loop {
        let page = store
            .list_conversations(ConversationFilter {
                cursor: cursor.clone(),
                limit: 1,
                ..filter(true, false, false)
            })
            .expect("page");
        seen.extend(page.conversations.into_iter().map(|row| row.id));
        match page.next_cursor {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    assert_eq!(
        sorted(seen),
        sorted(vec![first, second, fourth, fifth]),
        "paging by one returns every row once"
    );
}

#[test]
fn counts_agree_with_the_listings() {
    let dir = hn_core_test_dir::TestDir::new("core-conversation-filters-counts");
    let store = ChatStore::open(&dir.join("chat.db")).expect("open store");
    let _ = inbox(&store);

    let counts = store.conversation_counts().expect("counts");
    assert_eq!(counts.all, 4);
    assert_eq!(counts.attention, 4);
    assert_eq!(counts.working, 3);
    assert_eq!(counts.archived, 1);
    assert_eq!(
        counts.all as usize,
        listed(&store, false, false, false).len()
    );
    assert_eq!(
        counts.attention as usize,
        listed(&store, true, false, false).len()
    );
    assert_eq!(
        counts.working as usize,
        listed(&store, false, true, false).len()
    );
    assert_eq!(
        counts.archived as usize,
        listed(&store, false, false, true).len()
    );
}
