//! `mail.attachment_read` (TASK-ATT1) over a scripted Microsoft: a text attachment comes
//! back as untrusted data with its text, the read is audited without its name or text,
//! the quarantine copy is gone after a success and after a failure, and an attachment
//! that is not a file is refused with its own code.

mod mail_support;

use std::fs;
use std::path::Path;
use std::sync::Arc;

use excubia_chat::{ChatStore, SeatActor};
use excubia_host::craft::Craft;
use excubia_host::mail_send::{MailSends, SendPolicy, DEFAULT_WINDOW};
use excubia_host::mail_tools::{open_ledger, MailTools, QUARANTINE_DIR};
use excubia_mail::auth::Tenant;
use excubia_mail::{Account, AccountStore, FileKey, HttpTransport, Secret, Tokens};
use excubia_seat::{AttachmentRead, MailCall};
use excubia_test_dir::TestDir;
use mail_support::{ok, token_answer, Scripted, ACCESS, REFRESH};
use serde_json::json;

const OLD_REFRESH: &str = "REFRESCO-ANTIGUO";
const FILE: &str = "#microsoft.graph.fileAttachment";

fn actor() -> SeatActor {
    SeatActor {
        agent_id: "alfred".into(),
        role_id: "alfred".into(),
        project_id: None,
        conversation_id: String::new(),
        run_id: "run-1".into(),
    }
}

fn tools(
    dir: &TestDir,
    transport: &Arc<Scripted>,
    chat: &Arc<ChatStore>,
) -> (MailTools, Arc<AccountStore>) {
    let data = dir.join("data");
    let key = FileKey::new(dir.join("key"));
    let store = Arc::new(AccountStore::open(data.join("mail/accounts"), &key).expect("store"));
    let transport: Arc<dyn HttpTransport> = transport.clone();
    let client_id = Some("app".to_owned());
    let sends = MailSends::new(
        Arc::clone(&transport),
        Arc::clone(&store),
        client_id.clone(),
        open_ledger(&data).expect("ledger"),
        &data.join("mail/accounts"),
        Arc::clone(chat),
        SendPolicy {
            per_hour: 10,
            window: DEFAULT_WINDOW,
        },
    );
    let tools = MailTools::new(
        transport,
        Arc::clone(&store),
        client_id,
        &data,
        Arc::clone(chat),
        &dir.join("workspaces"),
        sends,
    )
    .expect("mail tools");
    (tools, store)
}

fn sign_in(store: &AccountStore, label: &str, refresh: &str) {
    let tokens = Tokens::new(ACCESS, Some(Secret::new(refresh)), 3600);
    let account = Account::new(label, Tenant::Organizations, "ana@example.com", tokens);
    store.save(&account).expect("save");
}

/// Every file under `dir` read as text and joined: the audit line is somewhere in there.
fn written(dir: &Path) -> String {
    let mut text = String::new();
    for entry in fs::read_dir(dir).expect("dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            text.push_str(&written(&path));
        } else if let Ok(body) = fs::read_to_string(&path) {
            text.push_str(&body);
        }
    }
    text
}

/// True when no read has left a copy in the quarantine.
fn quarantine_is_empty(data: &Path) -> bool {
    let root = data.join(QUARANTINE_DIR);
    !root.exists() || fs::read_dir(&root).expect("quarantine").count() == 0
}

fn read_of(attachment_id: &str) -> MailCall {
    MailCall::AttachmentRead(AttachmentRead {
        account: "trabajo".into(),
        message_id: "AAMk-1".into(),
        attachment_id: attachment_id.into(),
        sheet: None,
        range: None,
        page: None,
    })
}

fn metadata(odata: &str, name: &str, size: u64) -> serde_json::Value {
    json!({"@odata.type": odata, "id": "AAMk-att", "name": name, "size": size})
}

#[tokio::test]
async fn a_text_attachment_comes_back_as_untrusted_data_and_leaves_no_copy() {
    let dir = TestDir::new("mail-attachment-text");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    // "hola mundo" in base64.
    let transport = Arc::new(Scripted::new(vec![
        token_answer(),
        ok(metadata(FILE, "notas-secretas.txt", 10)),
        ok(json!({"contentBytes": "aG9sYSBtdW5kbw=="})),
    ]));
    let (tools, store) = tools(&dir, &transport, &chat);
    sign_in(&store, "trabajo", OLD_REFRESH);
    let tools = tools.with_craft(Arc::new(Craft::without_tools(&dir.join("data"))));

    let reply = tools
        .call(&actor(), read_of("AAMk-att"))
        .await
        .expect("the read");

    let result = &reply.result;
    assert_eq!(result["ok"], true);
    assert_eq!(result["untrusted_content"], true);
    assert_eq!(result["from_mail"], true);
    assert_eq!(result["kind"], "txt");
    assert_eq!(result["text"], "hola mundo");
    assert_eq!(result["name"], "notas-secretas.txt");
    let wire = result.to_string();
    assert!(!wire.contains(ACCESS) && !wire.contains(REFRESH), "{wire}");

    let asked = transport.asked();
    assert_eq!(
        asked.len(),
        3,
        "the token refresh, the metadata and the content"
    );
    assert!(asked[1].contains("/me/messages/AAMk-1/attachments/AAMk-att?$select=id,name,size"));

    let audit = written(&dir.join("data"));
    assert!(
        audit.contains("mail.attachment_read"),
        "the read is audited"
    );
    assert!(audit.contains("AAMk-att"), "by attachment id");
    assert!(!audit.contains("hola mundo"), "no content in the audit");
    assert!(!audit.contains("notas-secretas"), "no name in the audit");
    assert!(
        quarantine_is_empty(&dir.join("data")),
        "the copy is removed"
    );
}

#[tokio::test]
async fn a_read_that_fails_also_leaves_no_copy() {
    let dir = TestDir::new("mail-attachment-fails");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    // A PDF header; the document CLIs are not installed, so the read fails after the copy is made.
    let transport = Arc::new(Scripted::new(vec![
        token_answer(),
        ok(metadata(FILE, "factura.pdf", 8)),
        ok(json!({"contentBytes": "JVBERi0xLjQ="})),
    ]));
    let (tools, store) = tools(&dir, &transport, &chat);
    sign_in(&store, "trabajo", OLD_REFRESH);
    let tools = tools.with_craft(Arc::new(Craft::without_tools(&dir.join("data"))));

    let error = tools
        .call(&actor(), read_of("AAMk-att"))
        .await
        .expect_err("no CLI");

    assert_eq!(error.code, "tool_missing");
    assert!(
        quarantine_is_empty(&dir.join("data")),
        "the copy is removed on failure too"
    );
    let audit = written(&dir.join("data"));
    assert!(
        !audit.contains("mail.attachment_read"),
        "a failed read is not audited as read"
    );
}

#[tokio::test]
async fn an_attached_message_is_refused_with_its_own_code_and_its_content_is_never_asked() {
    let dir = TestDir::new("mail-attachment-item");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = Arc::new(Scripted::new(vec![
        token_answer(),
        ok(metadata(
            "#microsoft.graph.itemAttachment",
            "reenviado.msg",
            500,
        )),
    ]));
    let (tools, store) = tools(&dir, &transport, &chat);
    sign_in(&store, "trabajo", OLD_REFRESH);
    let tools = tools.with_craft(Arc::new(Craft::without_tools(&dir.join("data"))));

    let error = tools
        .call(&actor(), read_of("AAMk-att"))
        .await
        .expect_err("not a file");

    assert_eq!(error.code, "mail_attachment_not_file");
    assert_eq!(transport.asked().len(), 2, "no content request");
    assert!(quarantine_is_empty(&dir.join("data")));
}

#[tokio::test]
async fn without_the_document_cli_wiring_the_tool_is_unavailable() {
    let dir = TestDir::new("mail-attachment-no-craft");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = Arc::new(Scripted::new(vec![token_answer()]));
    let (tools, store) = tools(&dir, &transport, &chat);
    sign_in(&store, "trabajo", OLD_REFRESH);

    let error = tools
        .call(&actor(), read_of("AAMk-att"))
        .await
        .expect_err("no craft");

    assert_eq!(error.code, "tool_failed");
    assert!(
        transport.asked().is_empty(),
        "nothing is fetched without a reader"
    );
}
