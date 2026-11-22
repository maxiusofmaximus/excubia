use serde_json::json;

use super::*;
use crate::model_approval::{Approval, ApprovalKind, Decision, NewApproval, DEFAULT_TTL_SECONDS};
use crate::model_contact::{CONTACT_DELETE_TOOL, MAX_CONTACTS};
use crate::store::ChatStore;

fn alfred(store: &ChatStore) -> SeatActor {
    SeatActor {
        agent_id: store.alfred_id().expect("alfred"),
        role_id: "alfred".to_string(),
        project_id: None,
        conversation_id: String::new(),
        run_id: String::new(),
    }
}

fn address_strings(contact: &Contact) -> Vec<&str> {
    contact
        .addresses
        .iter()
        .map(|item| item.address.as_str())
        .collect()
}

fn fields(name: &str, aliases: &[&str], addresses: &[&str]) -> ContactFields {
    ContactFields {
        name: name.to_string(),
        aliases: Some(aliases.iter().map(|alias| alias.to_string()).collect()),
        addresses: Some(
            addresses
                .iter()
                .map(|address| address.to_string())
                .collect(),
        ),
        notes: None,
    }
}

fn create(store: &ChatStore, name: &str, aliases: &[&str], addresses: &[&str]) -> Contact {
    store
        .save_contact(&alfred(store), None, fields(name, aliases, addresses))
        .expect("contact")
}

fn audit_text(store: &ChatStore) -> String {
    store
        .lock()
        .query_row(
            "SELECT group_concat(object_kind || ' ' || object_id || ' ' || action || ' '
                    || actor_kind || ' ' || actor_id || ' ' || at, ' ')
               FROM agenda_audit",
            [],
            |row| row.get::<_, Option<String>>(0),
        )
        .expect("audit")
        .unwrap_or_default()
}

fn delete_question(store: &ChatStore, contact_id: &str, digest: &str) -> Approval {
    store
        .create_delete_approval(NewApproval {
            conversation_id: None,
            seat_id: None,
            agent_label: None,
            run_id: None,
            pane_id: None,
            kind: ApprovalKind::Tool,
            tool: CONTACT_DELETE_TOOL.to_string(),
            summary: format!("Borrar el contacto {contact_id}"),
            cwd: None,
            paths: Vec::new(),
            risk: None,
            action_digest: digest.to_string(),
            payload_redacted: Some(json!({ "contact_id": contact_id })),
            ttl_seconds: DEFAULT_TTL_SECONDS,
        })
        .expect("question")
}

#[test]
fn an_exact_alias_finds_its_contact_alone_and_a_part_of_a_name_finds_several() {
    let store = ChatStore::in_memory().expect("store");
    let ana = create(
        &store,
        "Ana Ruiz",
        &["el cliente de Madrid"],
        &["ana@example.com"],
    );
    create(&store, "Madrid Logística", &[], &["compras@example.com"]);

    let by_alias = store
        .find_contacts("  El CLIENTE de madrid ")
        .expect("find");
    assert_eq!(by_alias.len(), 1);
    assert_eq!(by_alias[0].id, ana.id);

    let by_part = store.find_contacts("madrid").expect("find");
    assert_eq!(by_part.len(), 2, "the name and the alias both contain it");

    assert!(store.find_contacts("nadie").expect("find").is_empty());
}

#[test]
fn an_address_is_kept_lowercase_and_only_a_well_formed_one_is_accepted() {
    let store = ChatStore::in_memory().expect("store");
    let ana = create(&store, "Ana Ruiz", &[], &["  Ana@Example.COM "]);
    assert_eq!(address_strings(&ana), ["ana@example.com"]);

    let refused = store
        .save_contact(
            &alfred(&store),
            None,
            fields("Luis", &[], &["luis@example"]),
        )
        .expect_err("shape");
    assert!(matches!(refused, ChatError::InvalidAddress));
    assert!(store.find_contacts("luis").expect("find").is_empty());

    let unknown = store
        .admit_recipients(
            &alfred(&store),
            &[
                "ANA@example.com ".to_string(),
                "otra@example.com".to_string(),
            ],
        )
        .expect("check");
    assert_eq!(unknown, ["otra@example.com"]);
}

#[test]
fn an_address_or_an_alias_of_another_contact_is_refused_and_only_its_owner_is_named() {
    let store = ChatStore::in_memory().expect("store");
    create(&store, "Ana Ruiz", &["ana"], &["ana@example.com"]);
    let luis = create(&store, "Luis", &[], &["luis@example.com"]);

    let taken = store
        .save_contact(
            &alfred(&store),
            Some(&luis.id),
            fields("Luis", &[], &["ANA@example.com"]),
        )
        .expect_err("address");
    assert!(
        matches!(&taken, ChatError::AddressTaken(owner) if owner == "Ana Ruiz"),
        "{taken}"
    );

    let alias = store
        .save_contact(
            &alfred(&store),
            Some(&luis.id),
            fields("Luis", &[" ANA "], &[]),
        )
        .expect_err("alias");
    assert!(matches!(alias, ChatError::AliasTaken));

    let kept = store.contact(&luis.id).expect("luis");
    assert_eq!(
        address_strings(&kept),
        ["luis@example.com"],
        "a refusal changes nothing"
    );
    assert!(kept.aliases.is_empty());
}

#[test]
fn a_contact_keeps_at_most_ten_addresses_and_ten_aliases() {
    let store = ChatStore::in_memory().expect("store");
    let addresses: Vec<String> = (0..11).map(|n| format!("p{n}@example.com")).collect();
    let refused = store
        .save_contact(
            &alfred(&store),
            None,
            ContactFields {
                name: "Muchos".to_string(),
                aliases: None,
                addresses: Some(addresses.clone()),
                notes: None,
            },
        )
        .expect_err("addresses");
    assert!(matches!(refused, ChatError::AddressLimit));

    let aliases: Vec<String> = (0..11).map(|n| format!("alias {n}")).collect();
    let refused = store
        .save_contact(
            &alfred(&store),
            None,
            ContactFields {
                name: "Muchos".to_string(),
                aliases: Some(aliases),
                addresses: None,
                notes: None,
            },
        )
        .expect_err("aliases");
    assert!(matches!(refused, ChatError::AliasLimit));

    let ten = store
        .save_contact(
            &alfred(&store),
            None,
            ContactFields {
                name: "Diez".to_string(),
                aliases: None,
                addresses: Some(addresses[..10].to_vec()),
                notes: None,
            },
        )
        .expect("ten addresses are allowed");
    assert_eq!(ten.addresses.len(), 10);
}

#[test]
fn an_update_replaces_the_lists_it_is_given_and_keeps_the_rest() {
    let store = ChatStore::in_memory().expect("store");
    let actor = alfred(&store);
    let ana = store
        .save_contact(
            &actor,
            None,
            ContactFields {
                name: "Ana Ruiz".to_string(),
                aliases: Some(vec!["ana".to_string()]),
                addresses: Some(vec!["ana@example.com".to_string()]),
                notes: Some("prefiere la mañana".to_string()),
            },
        )
        .expect("contact");

    let renamed = store
        .save_contact(
            &actor,
            Some(&ana.id),
            ContactFields {
                name: "Ana R.".to_string(),
                aliases: Some(vec!["Ana Rubia".to_string()]),
                addresses: None,
                notes: None,
            },
        )
        .expect("update");
    assert_eq!(renamed.name, "Ana R.");
    assert_eq!(renamed.aliases, ["ana rubia"]);
    assert_eq!(
        address_strings(&renamed),
        ["ana@example.com"],
        "left out, so kept"
    );
    assert_eq!(renamed.notes.as_deref(), Some("prefiere la mañana"));

    let cleared = store
        .save_contact(&actor, Some(&ana.id), fields("Ana R.", &[], &[]))
        .expect("update");
    assert!(cleared.addresses.is_empty() && cleared.aliases.is_empty());
}

#[test]
fn the_company_keeps_at_most_the_contact_limit() {
    let store = ChatStore::in_memory().expect("store");
    for n in 0..MAX_CONTACTS {
        create(&store, &format!("Contacto {n}"), &[], &[]);
    }
    let refused = store
        .save_contact(&alfred(&store), None, fields("Uno más", &[], &[]))
        .expect_err("limit");
    assert!(matches!(refused, ChatError::ContactLimit));
}

#[test]
fn the_audit_keeps_metadata_and_never_a_name_or_an_address() {
    let store = ChatStore::in_memory().expect("store");
    let ana = create(&store, "Ana Ruiz", &["ana"], &["ana@example.com"]);
    store
        .save_contact(&alfred(&store), Some(&ana.id), fields("Ana R.", &[], &[]))
        .expect("update");

    let audit = audit_text(&store);
    assert!(
        audit.contains(&format!("contact {} create alfred", ana.id)),
        "{audit}"
    );
    assert!(
        audit.contains(&format!("contact {} update alfred", ana.id)),
        "{audit}"
    );
    for private in ["Ana Ruiz", "Ana R.", "ana@example.com"] {
        assert!(!audit.contains(private), "{audit}");
    }
}

#[test]
fn a_delete_is_asked_once_and_only_the_users_approval_deletes_the_contact() {
    let store = ChatStore::in_memory().expect("store");
    let ana = create(&store, "Ana Ruiz", &[], &["ana@example.com"]);

    let first = delete_question(&store, &ana.id, "sha256:borrar-1");
    let again = delete_question(&store, &ana.id, "sha256:borrar-2");
    assert_eq!(
        again.id, first.id,
        "a waiting question stands for the contact"
    );

    store
        .decide_approval(&first.id, Decision::Deny, "device-1", "device-1")
        .expect("denied");
    assert!(store.contact(&ana.id).is_ok(), "a denial keeps the contact");

    let asked = delete_question(&store, &ana.id, "sha256:borrar-3");
    store
        .decide_approval(&asked.id, Decision::Approve, "device-1", "device-1")
        .expect("approved");
    assert!(matches!(
        store.contact(&ana.id),
        Err(ChatError::ContactNotFound(_))
    ));
    assert!(
        store
            .admit_recipients(&alfred(&store), &["ana@example.com".to_string()])
            .expect("check")
            == ["ana@example.com"],
        "its addresses went with it"
    );
    assert!(audit_text(&store).contains(&format!("contact {} delete user device-1", ana.id)));
}
