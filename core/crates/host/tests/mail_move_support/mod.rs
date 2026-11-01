//! The fixtures of the organize tests: a scripted Microsoft, the mail tools over it,
//! the chat they ask through, and the batches the tests propose and decide.
#![allow(dead_code)]

use std::path::Path;
use std::sync::Arc;

use hn_core_chat::{ApprovalKind, ChatStore, NewApproval, NewConversation, SeatActor, ToolError};
use hn_core_host::mail_send::{MailSends, SendPolicy, DEFAULT_WINDOW};
use hn_core_host::mail_tools::{open_ledger, MailTools};
use hn_core_mail::auth::Tenant;
use hn_core_mail::{
    Account, AccountStore, ActionLog, FileKey, HttpResponse, HttpTransport, Secret, Tokens,
};
use hn_core_seat::{MailCall, MoveApply, MoveFolder, MoveProposal, Organize, ProposedMove};
use hn_core_test_dir::TestDir;
use serde_json::{json, Value};

use crate::mail_support::{ok, Scripted};

pub const ACCOUNT: &str = "trabajo";

pub fn actor() -> SeatActor {
    SeatActor {
        agent_id: "alfred".into(),
        role_id: "alfred".into(),
        project_id: None,
        conversation_id: String::new(),
        run_id: "run-1".into(),
    }
}

/// The fake Graph: the three well-known folders answer by path, the rest is scripted.
pub fn scripted(answers: Vec<HttpResponse>) -> Arc<Scripted> {
    Arc::new(
        Scripted::new(answers)
            .with_alias("junkemail", ok(json!({"id": "JUNK-ID"})))
            .with_alias("deleteditems", ok(json!({"id": "DEL-ID"})))
            .with_alias("inbox", ok(json!({"id": "INBOX-ID"}))),
    )
}

/// The mail tools over `transport`, with `ACCOUNT` signed in and the chat they ask through.
pub fn host(dir: &TestDir, transport: &Arc<Scripted>, chat: &Arc<ChatStore>) -> MailTools {
    let data = dir.join("data");
    let key = FileKey::new(dir.join("key"));
    let store = Arc::new(AccountStore::open(data.join("mail/accounts"), &key).expect("store"));
    let http: Arc<dyn HttpTransport> = transport.clone();
    let client_id = Some("app".to_owned());
    let sends = MailSends::new(
        Arc::clone(&http),
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
        http,
        Arc::clone(&store),
        client_id,
        &data,
        Arc::clone(chat),
        &dir.join("workspaces"),
        sends,
    )
    .expect("mail tools");
    let tokens = Tokens::new("ACCESO", Some(Secret::new("REFRESCO")), 3600);
    store
        .save(&Account::new(
            ACCOUNT,
            Tenant::Organizations,
            "ana@example.com",
            tokens,
        ))
        .expect("save");
    tools
}

pub fn message(id: &str) -> Value {
    json!({
        "id": id,
        "subject": format!("Oferta {id} confidencial"),
        "from": {"emailAddress": {"name": "", "address": "promo@tienda.example"}},
        "sender": {"emailAddress": {"name": "", "address": "promo@tienda.example"}},
        "replyTo": [],
        "toRecipients": [{"emailAddress": {"name": "Ana", "address": "ana@x.com"}}],
        "ccRecipients": [],
        "receivedDateTime": "2026-10-30T08:00:00Z",
        "internetMessageId": format!("<{id}@x.com>"),
        "hasAttachments": false,
        "body": {"contentType": "text", "content": "CUERPO-SECRETO"},
        "internetMessageHeaders": [],
    })
}

/// The two reads one message costs: the message itself and its empty attachment list.
pub fn read(id: &str) -> Vec<HttpResponse> {
    vec![ok(message(id)), ok(json!({"value": []}))]
}

/// One move as the batch makes it: the parent folder read, then the move itself.
pub fn moved_from(parent: &str, new_id: &str) -> Vec<HttpResponse> {
    vec![
        ok(json!({"parentFolderId": parent})),
        ok(json!({"id": new_id})),
    ]
}

pub fn missing() -> HttpResponse {
    HttpResponse {
        status: 404,
        body: json!({"error": {"code": "ErrorItemNotFound"}}).to_string(),
    }
}

pub fn propose(list: &[(&str, MoveFolder)]) -> MailCall {
    MailCall::Organize(Organize::Propose(MoveProposal {
        account: ACCOUNT.to_owned(),
        items: list
            .iter()
            .map(|(id, folder)| ProposedMove {
                message_id: (*id).to_owned(),
                folder: *folder,
                reason: "Publicidad".to_owned(),
            })
            .collect(),
    }))
}

pub fn apply(approval_id: &str) -> MailCall {
    MailCall::Organize(Organize::Apply(MoveApply {
        account: ACCOUNT.to_owned(),
        approval_id: Some(approval_id.to_owned()),
    }))
}

/// `mail.move_apply` without an id: the host finds the batch of the account itself.
pub fn apply_latest() -> MailCall {
    MailCall::Organize(Organize::Apply(MoveApply {
        account: ACCOUNT.to_owned(),
        approval_id: None,
    }))
}

/// A conversation of `chat` for the batches to be asked in.
pub fn open_conversation(chat: &ChatStore) -> String {
    chat.create_conversation(NewConversation {
        title: Some("Correo".to_owned()),
        project_id: None,
    })
    .expect("conversation")
    .id
}

/// The actor of a turn in `conversation`, the one the batches are asked in.
pub fn actor_in(conversation: &str) -> SeatActor {
    SeatActor {
        conversation_id: conversation.into(),
        ..actor()
    }
}

/// Asks the operator about `list` and returns the approval id the reply names.
pub async fn ask(tools: &MailTools, list: &[(&str, MoveFolder)]) -> String {
    ask_as(tools, &actor(), list).await
}

/// Asks in `conversation`, the turn the batch belongs to.
pub async fn ask_in(tools: &MailTools, conversation: &str, list: &[(&str, MoveFolder)]) -> String {
    ask_as(tools, &actor_in(conversation), list).await
}

async fn ask_as(tools: &MailTools, actor: &SeatActor, list: &[(&str, MoveFolder)]) -> String {
    let reply = tools
        .call(actor, propose(list))
        .await
        .expect("the batch is proposed");
    reply.result["approval_id"]
        .as_str()
        .expect("approval id")
        .to_owned()
}

/// The code of a refusal; a call that succeeds is a failed test.
pub fn code_of<T>(result: Result<T, ToolError>) -> &'static str {
    match result {
        Ok(_) => panic!("the call was expected to be refused"),
        Err(error) => error.code,
    }
}

/// An approval the host did not ask with its own digest or tool, to test the apply's checks.
pub fn raw_approval(chat: &ChatStore, tool: &str, account: &str, digest: &str) -> String {
    chat.create_mail_organize_approval(NewApproval {
        conversation_id: None,
        seat_id: None,
        agent_label: None,
        run_id: None,
        pane_id: None,
        kind: ApprovalKind::Tool,
        tool: tool.to_owned(),
        summary: "prueba".to_owned(),
        cwd: None,
        paths: Vec::new(),
        risk: None,
        action_digest: digest.to_owned(),
        payload_redacted: Some(json!({
            "account": account,
            "items": [{"message_id": "m1", "dest": "junkemail", "level": "clean"}],
        })),
        ttl_seconds: 1800,
    })
    .expect("approval")
    .id
}

pub fn action_log(dir: &TestDir) -> ActionLog {
    ActionLog::new(&dir.join("data").join("mail").join("accounts"))
}

/// Every file under `dir` read as text and joined: the audit line is somewhere in there.
pub fn written(dir: &Path) -> String {
    let mut text = String::new();
    for entry in std::fs::read_dir(dir).expect("dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            text.push_str(&written(&path));
        } else if let Ok(body) = std::fs::read_to_string(&path) {
            text.push_str(&body);
        }
    }
    text
}
