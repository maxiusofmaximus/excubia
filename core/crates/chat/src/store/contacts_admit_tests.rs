use rusqlite::params;

use super::*;
use crate::model::NewConversation;
use crate::model_contact::{ContactFields, CONTACT_NAME_MAX_CHARS, MAX_CONTACTS};
use crate::store::ChatStore;

fn alfred(store: &ChatStore, conversation_id: &str) -> SeatActor {
    SeatActor {
        agent_id: store.alfred_id().expect("alfred"),
        role_id: "alfred".to_string(),
        project_id: None,
        conversation_id: conversation_id.to_string(),
        run_id: String::new(),
    }
}

fn conversation(store: &ChatStore) -> String {
    store
        .create_conversation(NewConversation {
            title: None,
            project_id: None,
        })
        .expect("conversation")
        .id
}

/// Writes a message straight into the conversation, as its author wrote it.
fn say(store: &ChatStore, conversation_id: &str, seq: i64, author_kind: &str, body: &str) {
    store
        .lock()
        .execute(
            "INSERT INTO message (id, conversation_id, seq, author_kind, author_agent_id,
                                  body_md, state, client_id, created_at)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5, 'complete', NULL, '2026-10-28T10:00:00Z')",
            params![
                format!("msg_{conversation_id}_{seq}"),
                conversation_id,
                seq,
                author_kind,
                body
            ],
        )
        .expect("message");
}

fn admit(store: &ChatStore, actor: &SeatActor, addresses: &[&str]) -> Vec<String> {
    let addresses: Vec<String> = addresses
        .iter()
        .map(|address| address.to_string())
        .collect();
    store.admit_recipients(actor, &addresses).expect("check")
}

fn contact_count(store: &ChatStore) -> i64 {
    store
        .lock()
        .query_row("SELECT COUNT(*) FROM contact", [], |row| row.get(0))
        .expect("count")
}

fn source_of(store: &ChatStore, address: &str) -> String {
    store
        .lock()
        .query_row(
            "SELECT source FROM contact_address WHERE address = ?1",
            [address],
            |row| row.get(0),
        )
        .expect("source")
}

#[test]
fn an_address_the_user_typed_is_saved_as_a_contact_of_the_user() {
    let store = ChatStore::in_memory().expect("store");
    let conversation_id = conversation(&store);
    say(
        &store,
        &conversation_id,
        1,
        "user",
        "Escribe a Nuevo@Empresa.com que llego tarde",
    );
    let actor = alfred(&store, &conversation_id);

    assert!(admit(&store, &actor, &["  nuevo@EMPRESA.com "]).is_empty());
    assert_eq!(source_of(&store, "nuevo@empresa.com"), "user_message");
    let found = store.find_contacts("nuevo").expect("find");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "nuevo");
    assert_eq!(found[0].addresses[0].address, "nuevo@empresa.com");
    assert_eq!(found[0].addresses[0].source, AddressSource::UserMessage);
    assert!(found[0].aliases.is_empty());
    let audited: i64 = store
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM agenda_audit
              WHERE object_kind = 'contact' AND object_id = ?1 AND action = 'create'
                AND actor_kind = 'alfred' AND actor_id = ?2",
            params![found[0].id, actor.agent_id],
            |row| row.get(0),
        )
        .expect("audit");
    assert_eq!(audited, 1, "one audit line, with the actor and no address");
}

#[test]
fn a_user_address_mentioned_twice_is_saved_once() {
    let store = ChatStore::in_memory().expect("store");
    let conversation_id = conversation(&store);
    say(&store, &conversation_id, 1, "user", "dos@example.com");
    let actor = alfred(&store, &conversation_id);

    assert!(admit(&store, &actor, &["dos@example.com", "DOS@example.com"]).is_empty());
    assert_eq!(contact_count(&store), 1);
}

#[test]
fn an_address_only_in_alfreds_message_is_refused_and_nothing_is_saved() {
    let store = ChatStore::in_memory().expect("store");
    let conversation_id = conversation(&store);
    say(&store, &conversation_id, 1, "user", "escribe a ventas");
    say(
        &store,
        &conversation_id,
        2,
        "agent",
        "Le escribo a ventas@empresa.com",
    );
    let actor = alfred(&store, &conversation_id);

    assert_eq!(
        admit(&store, &actor, &["ventas@empresa.com"]),
        ["ventas@empresa.com"]
    );
    assert_eq!(contact_count(&store), 0);
}

#[test]
fn an_address_inside_a_longer_one_does_not_authorize_it() {
    let store = ChatStore::in_memory().expect("store");
    let conversation_id = conversation(&store);
    say(
        &store,
        &conversation_id,
        1,
        "user",
        "escribe a mariana@x.com",
    );
    let actor = alfred(&store, &conversation_id);

    assert_eq!(admit(&store, &actor, &["ana@x.com"]), ["ana@x.com"]);
    assert!(admit(&store, &actor, &["mariana@x.com"]).is_empty());
}

#[test]
fn a_message_of_another_conversation_or_none_does_not_authorize_it() {
    let store = ChatStore::in_memory().expect("store");
    let first = conversation(&store);
    let second = conversation(&store);
    say(&store, &first, 1, "user", "nuevo@x.com");

    assert_eq!(
        admit(&store, &alfred(&store, &second), &["nuevo@x.com"]),
        ["nuevo@x.com"]
    );
    assert_eq!(
        admit(&store, &alfred(&store, ""), &["nuevo@x.com"]),
        ["nuevo@x.com"]
    );
    assert_eq!(contact_count(&store), 0);
}

#[test]
fn only_the_newest_fifty_messages_of_the_conversation_count() {
    let store = ChatStore::in_memory().expect("store");
    let inside = conversation(&store);
    say(&store, &inside, 1, "user", "dentro@x.com");
    for seq in 2..=50 {
        say(&store, &inside, seq, "user", "hola");
    }
    let outside = conversation(&store);
    say(&store, &outside, 1, "user", "fuera@x.com");
    for seq in 2..=51 {
        say(&store, &outside, seq, "user", "hola");
    }

    assert!(admit(&store, &alfred(&store, &inside), &["dentro@x.com"]).is_empty());
    assert_eq!(
        admit(&store, &alfred(&store, &outside), &["fuera@x.com"]),
        ["fuera@x.com"]
    );
}

#[test]
fn a_refusal_saves_none_of_the_addresses_the_user_typed() {
    let store = ChatStore::in_memory().expect("store");
    let conversation_id = conversation(&store);
    say(&store, &conversation_id, 1, "user", "bien@x.com");
    let actor = alfred(&store, &conversation_id);

    assert_eq!(
        admit(&store, &actor, &["bien@x.com", "otra@x.com"]),
        ["otra@x.com"]
    );
    assert_eq!(contact_count(&store), 0);
}

#[test]
fn an_address_already_in_the_book_passes_without_a_message() {
    let store = ChatStore::in_memory().expect("store");
    store
        .save_contact(
            &alfred(&store, ""),
            None,
            ContactFields {
                name: "Ana".to_string(),
                aliases: None,
                addresses: Some(vec!["ana@example.com".to_string()]),
                notes: None,
            },
        )
        .expect("contact");

    assert!(admit(&store, &alfred(&store, ""), &["ANA@example.com"]).is_empty());
    assert_eq!(source_of(&store, "ana@example.com"), "alfred");
}

#[test]
fn a_malformed_address_is_invalid_not_unknown() {
    let store = ChatStore::in_memory().expect("store");
    let conversation_id = conversation(&store);
    say(&store, &conversation_id, 1, "user", "escribe a ana@");

    let refused = store
        .admit_recipients(&alfred(&store, &conversation_id), &["ana@".to_string()])
        .expect_err("shape");
    assert!(matches!(refused, ChatError::InvalidAddress));
}

#[test]
fn the_name_of_a_typed_contact_is_its_local_part_cut_to_the_name_limit() {
    let store = ChatStore::in_memory().expect("store");
    let conversation_id = conversation(&store);
    let local = "a".repeat(200);
    say(
        &store,
        &conversation_id,
        1,
        "user",
        &format!("{local}@x.com"),
    );

    assert!(admit(
        &store,
        &alfred(&store, &conversation_id),
        &[&format!("{local}@x.com")]
    )
    .is_empty());
    let found = store.find_contacts("aaaa").expect("find");
    assert_eq!(found[0].name.chars().count(), CONTACT_NAME_MAX_CHARS);
}

#[test]
fn the_contact_limit_stops_a_typed_address() {
    let store = ChatStore::in_memory().expect("store");
    let alfred_actor = alfred(&store, "");
    for n in 0..MAX_CONTACTS {
        store
            .save_contact(
                &alfred_actor,
                None,
                ContactFields {
                    name: format!("Contacto {n}"),
                    aliases: None,
                    addresses: None,
                    notes: None,
                },
            )
            .expect("contact");
    }
    let conversation_id = conversation(&store);
    say(&store, &conversation_id, 1, "user", "nuevo@x.com");

    let refused = store
        .admit_recipients(
            &alfred(&store, &conversation_id),
            &["nuevo@x.com".to_string()],
        )
        .expect_err("limit");
    assert!(matches!(refused, ChatError::ContactLimit));
}

/// An address the user ends a sentence with is the address: the sentence's own
/// punctuation is not part of it.
#[test]
fn an_address_ending_a_sentence_or_a_question_is_recognised() {
    let store = ChatStore::in_memory().expect("store");
    let conversation_id = conversation(&store);
    say(
        &store,
        &conversation_id,
        1,
        "user",
        "escribe a nuevo@x.com.",
    );
    say(&store, &conversation_id, 2, "user", "¿Escribo a ana@x.com?");
    say(
        &store,
        &conversation_id,
        3,
        "user",
        "Dirección: luis@x.com:",
    );
    let actor = alfred(&store, &conversation_id);

    assert!(admit(&store, &actor, &["nuevo@x.com"]).is_empty());
    assert!(admit(&store, &actor, &["ana@x.com"]).is_empty());
    assert!(admit(&store, &actor, &["luis@x.com"]).is_empty());
    assert_eq!(contact_count(&store), 3);
}

/// Only the ends of a word lose punctuation: a dot inside a domain is part of it.
#[test]
fn a_dot_inside_the_domain_is_not_cut() {
    let store = ChatStore::in_memory().expect("store");
    let conversation_id = conversation(&store);
    say(
        &store,
        &conversation_id,
        1,
        "user",
        "escribe a ana@x.com.mx",
    );
    let actor = alfred(&store, &conversation_id);

    assert_eq!(admit(&store, &actor, &["ana@x.com"]), ["ana@x.com"]);
    assert!(admit(&store, &actor, &["ana@x.com.mx"]).is_empty());
}
