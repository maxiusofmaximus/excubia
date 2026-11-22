//! The notes and contacts a paired device writes (AGENDA-REST): the origin of each
//! row and its audit line name the device, the lists page past a cursor, and the
//! phone import never overwrites what it finds.

use std::sync::Arc;

use super::*;
use crate::model_contact::{
    AddressSource, ContactFields, ImportOutcome, MAX_ADDRESSES_PER_CONTACT,
};
use crate::store::ChatStore;
use crate::{ManualClock, NewConversation};

/// 2026-10-21T00:00:00.000Z, the instant every clock-driven test starts from.
const START_MS: i64 = 1_792_540_800_000;
const DEVICE: &str = "dv_phone";

fn store_at_start() -> (ChatStore, Arc<ManualClock>) {
    let store = ChatStore::in_memory().expect("store");
    let clock = Arc::new(ManualClock::at(START_MS));
    store.set_clock(clock.clone());
    (store, clock)
}

fn conversation(store: &ChatStore) -> String {
    store
        .create_conversation(NewConversation {
            title: Some("Trabajo".to_string()),
            project_id: None,
        })
        .expect("conversation")
        .id
}

fn fields(name: &str, addresses: &[&str]) -> ContactFields {
    ContactFields {
        name: name.to_string(),
        aliases: None,
        addresses: Some(
            addresses
                .iter()
                .map(|address| address.to_string())
                .collect(),
        ),
        notes: None,
    }
}

/// The audit lines as `object action actor_kind actor_id`, one per row.
fn audit_lines(store: &ChatStore) -> Vec<String> {
    let connection = store.lock();
    let mut statement = connection
        .prepare(
            "SELECT object_kind || ' ' || action || ' ' || actor_kind || ' ' || actor_id
               FROM agenda_audit ORDER BY id",
        )
        .expect("audit");
    statement
        .query_map([], |row| row.get(0))
        .expect("rows")
        .collect::<Result<_, _>>()
        .expect("audit lines")
}

#[test]
fn a_device_note_names_the_device_in_its_origin_and_its_audit() {
    let (store, _) = store_at_start();
    let saved = store
        .save_note_by_device(DEVICE, Some("Cliente".into()), "presupuesto".into(), None)
        .expect("save");
    assert_eq!(saved.conversation_id, None);
    assert_eq!(saved.message_id, None);

    let origin: (String, String, String) = store
        .lock()
        .query_row(
            "SELECT origin_kind, origin_id, updated_by FROM note WHERE id = ?1",
            [&saved.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("origin");
    assert_eq!(origin, ("device".into(), DEVICE.into(), "device".into()));
    assert_eq!(
        audit_lines(&store),
        [format!("note create device {DEVICE}")]
    );
}

#[test]
fn a_device_note_names_a_message_that_exists_and_refuses_one_that_does_not() {
    let (store, _) = store_at_start();
    let conversation_id = conversation(&store);
    let accepted = store
        .append_user_message(&conversation_id, "client-1", "presupuesto 1.250 €", None)
        .expect("message");

    let refused = store.save_note_by_device(DEVICE, None, "x".into(), Some("msg_missing".into()));
    assert!(
        matches!(refused, Err(ChatError::NoSuchMessage(_))),
        "{refused:?}"
    );
    assert!(store.list_notes(None, None, 10).expect("list").is_empty());

    let kept = store
        .save_note_by_device(DEVICE, None, "x".into(), Some(accepted.message.id.clone()))
        .expect("save");
    assert_eq!(
        kept.message_id.as_deref(),
        Some(accepted.message.id.as_str())
    );
}

#[test]
fn notes_list_newest_first_and_a_cursor_pages_past_the_row_it_names() {
    let (store, clock) = store_at_start();
    let mut ids = Vec::new();
    for body in ["uno", "dos", "tres"] {
        ids.push(
            store
                .save_note_by_device(DEVICE, None, body.into(), None)
                .expect("save")
                .id,
        );
        clock.advance_secs(1);
    }

    let first = store.list_notes(None, None, 2).expect("first page");
    assert_eq!(first.len(), 2);
    assert_eq!(first[0].id, ids[2], "the newest comes first");
    let rest = store
        .list_notes(None, Some(&first[1].id), 10)
        .expect("second page");
    assert_eq!(rest.len(), 1);
    assert_eq!(rest[0].id, ids[0]);

    let matching = store.list_notes(Some("DOS"), None, 10).expect("search");
    assert_eq!(matching.len(), 1);
    assert_eq!(matching[0].id, ids[1]);

    let unknown = store.list_notes(None, Some("nt_missing"), 10);
    assert!(
        matches!(unknown, Err(ChatError::NoteNotFound(_))),
        "{unknown:?}"
    );
}

#[test]
fn a_device_delete_is_immediate_audited_and_a_missing_note_is_not_found() {
    let (store, _) = store_at_start();
    let saved = store
        .save_note_by_device(DEVICE, None, "borrar".into(), None)
        .expect("save");

    store
        .delete_note_by_device(DEVICE, &saved.id)
        .expect("delete");
    assert!(matches!(
        store.note(&saved.id),
        Err(ChatError::NoteNotFound(_))
    ));
    assert_eq!(
        audit_lines(&store).last().map(String::as_str),
        Some(format!("note delete device {DEVICE}").as_str())
    );
    let again = store.delete_note_by_device(DEVICE, &saved.id);
    assert!(
        matches!(again, Err(ChatError::NoteNotFound(_))),
        "{again:?}"
    );
}

#[test]
fn a_device_contact_keeps_its_addresses_as_app_addresses() {
    let (store, _) = store_at_start();
    let saved = store
        .save_contact_by_device(DEVICE, None, fields("Ana", &["ana@example.com"]))
        .expect("save");
    assert_eq!(saved.addresses.len(), 1);
    assert_eq!(saved.addresses[0].address, "ana@example.com");
    assert_eq!(saved.addresses[0].source, AddressSource::App);
    assert_eq!(
        audit_lines(&store),
        [format!("contact create device {DEVICE}")]
    );

    let changed = store
        .save_contact_by_device(
            DEVICE,
            Some(&saved.id),
            fields("Ana R.", &["ana@example.org"]),
        )
        .expect("update");
    assert_eq!(changed.addresses[0].source, AddressSource::App);
    assert_eq!(changed.name, "Ana R.");
}

#[test]
fn contacts_list_by_query_and_page_past_a_cursor() {
    let (store, clock) = store_at_start();
    let mut ids = Vec::new();
    for (name, address) in [("Ana", "ana@example.com"), ("Bruno", "bruno@example.com")] {
        ids.push(
            store
                .save_contact_by_device(DEVICE, None, fields(name, &[address]))
                .expect("save")
                .id,
        );
        clock.advance_secs(1);
    }

    let all = store.list_contacts(None, None, 10).expect("all");
    assert_eq!(
        all.iter().map(|c| c.id.clone()).collect::<Vec<_>>(),
        [ids[1].clone(), ids[0].clone()]
    );
    let after = store.list_contacts(None, Some(&ids[1]), 10).expect("after");
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].id, ids[0]);
    let found = store.list_contacts(Some("bruno"), None, 10).expect("query");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, ids[1]);
    let unknown = store.list_contacts(None, Some("ct_missing"), 10);
    assert!(
        matches!(unknown, Err(ChatError::ContactNotFound(_))),
        "{unknown:?}"
    );
}

#[test]
fn an_import_creates_merges_and_skips_and_never_overwrites() {
    let (store, _) = store_at_start();
    store
        .save_contact_by_device(DEVICE, None, fields("Luis", &["luis@example.com"]))
        .expect("luis");

    let created = store.import_contact_by_device(DEVICE, fields("Marta", &["marta@example.com"]));
    assert_eq!(created.expect("created"), ImportOutcome::Created);
    let skipped = store.import_contact_by_device(DEVICE, fields("Otra", &["marta@example.com"]));
    assert_eq!(skipped.expect("skipped"), ImportOutcome::Skipped);
    let merged = store.import_contact_by_device(DEVICE, fields("luis", &["luis@example.org"]));
    assert_eq!(merged.expect("merged"), ImportOutcome::Merged);

    let luis = store.find_contacts("luis").expect("find");
    let addresses: Vec<(&str, AddressSource)> = luis[0]
        .addresses
        .iter()
        .map(|address| (address.address.as_str(), address.source))
        .collect();
    assert_eq!(
        addresses,
        [
            ("luis@example.com", AddressSource::App),
            ("luis@example.org", AddressSource::Phone),
        ]
    );
    assert_eq!(store.find_contacts("otra").expect("find").len(), 0);
}

#[test]
fn an_import_past_the_address_limit_is_refused_and_keeps_the_contact() {
    let (store, _) = store_at_start();
    let addresses: Vec<String> = (0..MAX_ADDRESSES_PER_CONTACT)
        .map(|index| format!("a{index}@example.com"))
        .collect();
    let refs: Vec<&str> = addresses.iter().map(String::as_str).collect();
    store
        .save_contact_by_device(DEVICE, None, fields("Full", &refs))
        .expect("full contact");

    let refused = store.import_contact_by_device(DEVICE, fields("Full", &["nueva@example.com"]));
    assert!(
        matches!(refused, Err(ChatError::AddressLimit)),
        "{refused:?}"
    );
    let kept = store.find_contacts("full").expect("find");
    assert_eq!(kept[0].addresses.len(), MAX_ADDRESSES_PER_CONTACT);
}

#[test]
fn an_import_with_an_invalid_address_writes_nothing() {
    let (store, _) = store_at_start();
    let refused = store.import_contact_by_device(DEVICE, fields("Mal", &["no-es-correo"]));
    assert!(
        matches!(refused, Err(ChatError::InvalidAddress)),
        "{refused:?}"
    );
    assert!(store
        .list_contacts(None, None, 10)
        .expect("list")
        .is_empty());
    assert!(audit_lines(&store).is_empty());
}
