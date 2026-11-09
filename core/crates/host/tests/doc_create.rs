//! Alfred creates a document (DOC8): `doc.create` writes it as a ready attachment of
//! this conversation, never over a file that exists, and answers its id. A bad
//! content, a missing app, a full quota and a project seat each get their own code.
//! Every creation is audited by tool and id, never by content.

mod harness;

use harness::docs::ooxml_zip;
use harness::{docs::send, TestHost};
use hn_core_auth::Permission;
use hn_core_chat::{AttachmentState, SeatActor};
use hn_core_crafting::App;
use hn_core_mail::AuditLog;
use hn_core_test_support::SignedClient;
use serde_json::{json, Value};

const READ: &[Permission] = &[Permission::Respond];
const ALFRED_TOKEN: &str = "tok-alfred";
const TEN_MIB: u64 = 10 * 1024 * 1024;

/// One MCP call with a seat token: the status and the JSON body.
async fn call(host: &TestHost, token: &str, tool: &str, arguments: Value) -> (u16, Value) {
    let response = reqwest::Client::new()
        .post(&host.tool_url)
        .header("X-HN-Tool-Token", token)
        .json(&json!({ "tool": tool, "arguments": arguments }))
        .send()
        .await
        .expect("tool call");
    let status = response.status().as_u16();
    let body: Value = response.json().await.expect("json");
    (status, body)
}

async fn conversation(client: &SignedClient) -> String {
    let created = client
        .chat_create("Documentos", None)
        .await
        .expect("create");
    created["conversation"]["id"]
        .as_str()
        .expect("id")
        .to_string()
}

/// A turn of this conversation, and Alfred's seat granted for it.
fn alfred_turn(host: &TestHost, conversation: &str) {
    let accepted = send(&host.chat, conversation, &[]);
    let alfred = host.chat.alfred_id().expect("alfred");
    host.team.as_ref().expect("a team host").grant_for_tests(
        ALFRED_TOKEN,
        SeatActor {
            agent_id: alfred,
            role_id: "alfred".to_string(),
            project_id: None,
            conversation_id: conversation.to_string(),
            run_id: accepted.turn.id.clone(),
        },
        "pane_test",
    );
}

async fn create(
    host: &TestHost,
    format: &str,
    name: &str,
    content: &str,
) -> (u16, Value) {
    call(
        host,
        ALFRED_TOKEN,
        "doc.create",
        json!({ "format": format, "name": name, "content": content }),
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn each_format_becomes_a_ready_document_of_the_conversation() {
    let host = TestHost::start_team_with_docs("doc-create-formats", &App::ALL).await;
    let client = host.paired_with("Max", READ).await;
    let conv = conversation(&client).await;
    alfred_turn(&host, &conv);
    let cases = [
        ("xlsx", "Ventas.xlsx", "Mes,Total\nEnero,10", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
        ("docx", "Carta.docx", "# Carta", "application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
        ("pdf", "Informe.pdf", "# Informe", "application/pdf"),
        ("csv", "datos.csv", "a,b\n1,2", "text/csv"),
        ("txt", "nota.txt", "hola", "text/plain"),
        ("md", "nota.md", "# hola", "text/markdown"),
        ("json", "datos.json", "{\"a\": 1}", "application/json"),
    ];

    for (format, name, content, mime) in cases {
        let (status, body) = create(&host, format, name, content).await;

        assert_eq!(status, 200, "{format}: {body}");
        let result = &body["result"];
        assert_eq!(result["name"], name, "{format}");
        assert_eq!(result["kind"], "document", "{format}");
        assert_eq!(result["mime"], mime, "{format}");
        let id = result["attachment_id"].as_str().expect("id").to_string();
        let row = host.chat.attachment(&id).expect("row");
        assert_eq!(row.state, AttachmentState::Ready, "{format}");
        assert_eq!(row.message_id, None, "{format}: nothing sent yet");
        assert_eq!(row.conversation_id, conv, "{format}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn csv_is_kept_as_written_and_xlsx_is_a_package() {
    let host = TestHost::start_team_with_docs("doc-create-bytes", &App::ALL).await;
    let client = host.paired_with("Max", READ).await;
    let conv = conversation(&client).await;
    alfred_turn(&host, &conv);

    let (_, csv) = create(&host, "csv", "datos.csv", "a,b\n1,2").await;
    let (_, sheet) = create(&host, "xlsx", "Hoja.xlsx", "a,b\n1,2").await;

    let files = host.chat.attachment_files().expect("files");
    let csv_id = csv["result"]["attachment_id"].as_str().expect("id");
    let sheet_id = sheet["result"]["attachment_id"].as_str().expect("id");
    assert_eq!(files.read_file(&conv, csv_id).expect("csv"), b"a,b\n1,2");
    assert_eq!(files.read_file(&conv, sheet_id).expect("xlsx"), ooxml_zip());
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_json_is_bad_content_and_no_cli_runs() {
    let host = TestHost::start_team_with_docs("doc-create-json", &App::ALL).await;
    let client = host.paired_with("Max", READ).await;
    let conv = conversation(&client).await;
    alfred_turn(&host, &conv);

    let (status, body) = create(&host, "json", "datos.json", "{no es json").await;

    assert_eq!(status, 422, "{body}");
    assert_eq!(body["detail"]["code"], "bad_content");
    assert!(host.cli.calls().is_empty(), "the check runs before any CLI");
}

#[tokio::test(flavor = "multi_thread")]
async fn without_an_office_app_text_still_works_and_xlsx_names_the_app() {
    let host = TestHost::start_team_with_docs("doc-create-no-app", &[]).await;
    let client = host.paired_with("Max", READ).await;
    let conv = conversation(&client).await;
    alfred_turn(&host, &conv);

    let (text_status, text) = create(&host, "csv", "datos.csv", "a,b").await;
    let (sheet_status, sheet) = create(&host, "xlsx", "Hoja.xlsx", "a,b").await;

    assert_eq!(text_status, 200, "{text}");
    assert_eq!(sheet_status, 503, "{sheet}");
    assert_eq!(sheet["detail"]["code"], "tool_missing");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_name_with_separators_or_a_reserved_name_is_made_safe() {
    let host = TestHost::start_team_with_docs("doc-create-names", &App::ALL).await;
    let client = host.paired_with("Max", READ).await;
    let conv = conversation(&client).await;
    alfred_turn(&host, &conv);

    let (_, escaping) = create(&host, "txt", "../../fuera\\malo.txt", "x").await;
    let (_, reserved) = create(&host, "txt", "CON.txt", "x").await;

    assert_eq!(escaping["result"]["name"], "malo.txt");
    assert_eq!(reserved["result"]["name"], "_CON.txt");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_file_with_the_same_name_is_new_and_the_first_stays() {
    let host = TestHost::start_team_with_docs("doc-create-twice", &App::ALL).await;
    let client = host.paired_with("Max", READ).await;
    let conv = conversation(&client).await;
    alfred_turn(&host, &conv);

    let (_, first) = create(&host, "txt", "Mismo.txt", "uno").await;
    let (_, second) = create(&host, "txt", "Mismo.txt", "dos").await;

    let first_id = first["result"]["attachment_id"].as_str().expect("id");
    let second_id = second["result"]["attachment_id"].as_str().expect("id");
    assert_ne!(first_id, second_id);
    let files = host.chat.attachment_files().expect("files");
    assert_eq!(files.read_file(&conv, first_id).expect("first"), b"uno");
    assert_eq!(files.read_file(&conv, second_id).expect("second"), b"dos");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_full_conversation_quota_refuses_with_quota_exceeded() {
    let host = TestHost::start_team_with_docs("doc-create-quota", &App::ALL).await;
    let client = host.paired_with("Max", READ).await;
    let conv = conversation(&client).await;
    alfred_turn(&host, &conv);
    for number in 0..10 {
        host.chat
            .begin_attachment(&conv, &format!("pre-{number}"), "grande.csv", TEN_MIB, &"ab".repeat(32))
            .expect("declared");
    }

    let (status, body) = create(&host, "csv", "mas.csv", "a").await;

    assert_ne!(status, 200, "{body}");
    assert_eq!(body["detail"]["code"], "quota_exceeded");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_project_seat_never_creates_documents() {
    let host = TestHost::start_team_with_docs("doc-create-scope", &App::ALL).await;
    let client = host.paired_with("Max", READ).await;
    let conv = conversation(&client).await;
    let accepted = send(&host.chat, &conv, &[]);
    host.team.as_ref().expect("team").grant_for_tests(
        "tok-lead",
        SeatActor {
            agent_id: "lead_test".to_string(),
            role_id: "lead".to_string(),
            project_id: Some("proj_test".to_string()),
            conversation_id: conv.clone(),
            run_id: accepted.turn.id.clone(),
        },
        "pane_test",
    );

    let (status, body) = call(
        &host,
        "tok-lead",
        "doc.create",
        json!({ "format": "txt", "name": "x.txt", "content": "x" }),
    )
    .await;

    assert_eq!(status, 422, "{body}");
    assert_eq!(body["detail"]["code"], "scope_denied");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_creation_is_audited_by_tool_and_id_never_by_content() {
    let host = TestHost::start_team_with_docs("doc-create-audit", &App::ALL).await;
    let client = host.paired_with("Max", READ).await;
    let conv = conversation(&client).await;
    alfred_turn(&host, &conv);

    let (status, body) = create(&host, "txt", "nota.txt", "secreto-del-contenido").await;
    assert_eq!(status, 200, "{body}");
    let id = body["result"]["attachment_id"].as_str().expect("id");

    let log = std::fs::read_to_string(AuditLog::new(&host.data_dir.join("mail/accounts")).path())
        .expect("audit log");
    assert!(log.contains("doc.create"), "{log}");
    assert!(log.contains(id), "{log}");
    assert!(!log.contains("secreto-del-contenido"), "the content never reaches the audit");
}
