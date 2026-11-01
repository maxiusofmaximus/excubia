//! The ledger keeps ownership and attachments across a restart without holding
//! mail content, and it purges what is older than the retention window (MAIL1C §2).

mod support;

use std::fs;
use std::path::Path;

use hn_core_mail::{
    attach_file, Approval, AttachContext, AttachedFile, DraftInput, GraphClient, Ledger, MailError,
    RETENTION_SECONDS,
};
use rusqlite::Connection;
use support::mock::{MockResponse, MockTransport};
use support::{
    attachment_item, draft_input, message_body, CountingApproval, Harness, ACCESS_TOKEN, ACCOUNT,
    DRAFT_ID,
};

const SECRET_SUBJECT: &str = "ASUNTO-SECRETO-NO-DEBE-QUEDAR-EN-DISCO";

fn subject_input(subject: &str) -> DraftInput {
    DraftInput {
        subject: subject.to_owned(),
        ..draft_input()
    }
}

async fn attach(
    client: &GraphClient<'_>,
    h: &Harness,
    gate: &CountingApproval,
    file: &Path,
) -> Result<AttachedFile, MailError> {
    let policy = h.policy();
    let ctx = AttachContext {
        client,
        account: ACCOUNT,
        policy: &policy,
        approval: gate,
        audit: &h.audit,
        scratch: &h.scratch,
    };
    attach_file(&ctx, DRAFT_ID, file).await
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[tokio::test]
async fn ownership_and_attachments_survive_a_restart_without_duplicates() {
    let h = Harness::new("ledger-restart");
    let db = h.dir.join("ledger.sqlite");
    let file = h.root.join("ventas.xlsx");
    fs::write(&file, b"contenido de prueba").expect("file");
    let first = MockTransport::new(vec![
        MockResponse::created(message_body(DRAFT_ID, SECRET_SUBJECT, false)),
        MockResponse::created(attachment_item("att-1", "ventas.xlsx", 19)),
    ]);
    {
        let ledger = Ledger::open(&db).expect("ledger");
        let client = GraphClient::new(&first, ACCESS_TOKEN, &ledger, ACCOUNT);
        client
            .create_draft(&subject_input(SECRET_SUBJECT))
            .await
            .expect("draft");
        attach(&client, &h, &CountingApproval::new(Approval::Denied), &file)
            .await
            .expect("attached");
    }

    let on_disk = fs::read(&db).expect("ledger file");
    assert!(
        !contains(&on_disk, SECRET_SUBJECT.as_bytes()),
        "the ledger holds ids and hashes, never mail content"
    );

    let after = MockTransport::new(vec![MockResponse::ok(message_body(
        DRAFT_ID,
        SECRET_SUBJECT,
        false,
    ))]);
    let ledger = Ledger::open(&db).expect("reopen");
    let client = GraphClient::new(&after, "token-nuevo", &ledger, ACCOUNT);
    let gate = CountingApproval::new(Approval::Denied);
    let again = attach(&client, &h, &gate, &file)
        .await
        .expect("the same attachment");

    assert_eq!(again.attachment_id, "att-1");
    assert_eq!(gate.asked(), 0);
    let calls = after.calls();
    assert_eq!(calls.len(), 0, "the repeat is answered from the ledger");
    client
        .get_draft_meta(DRAFT_ID)
        .await
        .expect("the draft is still ours after the restart");
    assert_eq!(after.calls().len(), 1, "one read, no second attachment");
}

#[tokio::test]
async fn a_draft_of_another_account_is_not_ours() {
    let h = Harness::new("ledger-owner");
    let db = h.dir.join("ledger.sqlite");
    let transport = MockTransport::new(vec![MockResponse::created(message_body(
        DRAFT_ID, "Hola", false,
    ))]);
    let ledger = Ledger::open(&db).expect("ledger");
    let owner = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);
    owner.create_draft(&draft_input()).await.expect("draft");

    let other = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, "otra-cuenta");
    let err = other
        .get_draft_meta(DRAFT_ID)
        .await
        .expect_err("another account cannot touch it");

    assert!(matches!(err, MailError::DraftNotOwned { .. }), "{err:?}");
    assert_eq!(transport.calls().len(), 1, "refused before any request");
}

#[tokio::test]
async fn drafts_older_than_thirty_days_are_purged_when_the_ledger_opens() {
    let h = Harness::new("ledger-purge");
    let db = h.dir.join("ledger.sqlite");
    let transport = MockTransport::new(vec![
        MockResponse::created(message_body("draft-viejo", "Viejo", false)),
        MockResponse::created(message_body("draft-nuevo", "Nuevo", false)),
    ]);
    {
        let ledger = Ledger::open(&db).expect("ledger");
        let client = GraphClient::new(&transport, ACCESS_TOKEN, &ledger, ACCOUNT);
        client
            .create_draft(&subject_input("Viejo"))
            .await
            .expect("old");
        client
            .create_draft(&subject_input("Nuevo"))
            .await
            .expect("new");
    }
    let connection = Connection::open(&db).expect("sqlite");
    connection
        .execute(
            "UPDATE mail_draft SET created_at = created_at - ?1 WHERE draft_id = 'draft-viejo'",
            [RETENTION_SECONDS + 1],
        )
        .expect("age the old draft");
    drop(connection);

    let ledger = Ledger::open(&db).expect("reopen purges");
    let reads = MockTransport::new(vec![MockResponse::ok(message_body(
        "draft-nuevo",
        "Nuevo",
        false,
    ))]);
    let client = GraphClient::new(&reads, ACCESS_TOKEN, &ledger, ACCOUNT);

    let err = client
        .get_draft_meta("draft-viejo")
        .await
        .expect_err("a purged draft is no longer ours");
    assert!(matches!(err, MailError::DraftNotOwned { .. }), "{err:?}");
    client
        .get_draft_meta("draft-nuevo")
        .await
        .expect("a recent draft is kept");
}
