//! Alfred reads a document the user attached (DOC5B): `doc.read` answers the text of
//! a ready file of this conversation, through the document CLIs of the host, and
//! audits the read by tool and attachment id only. Every other case is the same
//! `attachment_not_found`, a missing CLI is `tool_missing`, and a project seat
//! never reads documents.

mod harness;

use excubia_auth::Permission;
use excubia_chat::SeatActor;
use excubia_crafting::App;
use excubia_mail::AuditLog;
use excubia_test_support::SignedClient;
use harness::docs::{ooxml_zip, send, upload};
use harness::TestHost;
use serde_json::{json, Value};

const READ: &[Permission] = &[Permission::Respond];
const ALFRED_TOKEN: &str = "tok-alfred";

/// One MCP call with a seat token: the status and the JSON body.
async fn call(host: &TestHost, token: &str, tool: &str, arguments: Value) -> (u16, Value) {
    let response = reqwest::Client::new()
        .post(&host.tool_url)
        .header("X-Excubia-Tool-Token", token)
        .json(&json!({ "tool": tool, "arguments": arguments }))
        .send()
        .await
        .expect("tool call");
    let status = response.status().as_u16();
    let body: Value = response.json().await.expect("json");
    (status, body)
}

/// A conversation with `name` uploaded and sent in it, and Alfred's seat granted there.
async fn sent_document(
    host: &TestHost,
    client: &SignedClient,
    conversation: &str,
    name: &str,
    bytes: &[u8],
) -> String {
    let attachment = upload(client, conversation, name, bytes).await;
    let accepted = send(&host.chat, conversation, &[&attachment]);
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
    attachment
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

#[tokio::test(flavor = "multi_thread")]
async fn a_sheet_is_read_through_gridcraft_with_its_first_sheet() {
    let host = TestHost::start_team_with_docs("doc-sheet", &App::ALL).await;
    let client = host.paired_with("Max", READ).await;
    let conv = conversation(&client).await;
    let attachment = sent_document(&host, &client, &conv, "Ventas.xlsx", &ooxml_zip()).await;

    let (status, body) = call(
        &host,
        ALFRED_TOKEN,
        "doc.read",
        json!({ "attachment_id": attachment }),
    )
    .await;

    assert_eq!(status, 200, "{body}");
    let result = &body["result"];
    assert_eq!(result["name"], "Ventas.xlsx");
    assert_eq!(result["kind"], "xlsx");
    assert_eq!(result["text"], "a,b\n1,2");
    assert_eq!(
        result["parts"],
        json!(["Ventas", "Gastos"]),
        "every sheet is listed"
    );
    assert_eq!(result["untrusted_content"], true);
    let info = host.cli.calls();
    assert_eq!(info[0].0, App::GridCraft);
    assert_eq!(info[0].1[0], "info");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_word_file_is_read_through_wordcraft() {
    let host = TestHost::start_team_with_docs("doc-word", &App::ALL).await;
    let client = host.paired_with("Max", READ).await;
    let conv = conversation(&client).await;
    let attachment = sent_document(&host, &client, &conv, "Carta.docx", &ooxml_zip()).await;

    let (status, body) = call(
        &host,
        ALFRED_TOKEN,
        "doc.read",
        json!({ "attachment_id": attachment }),
    )
    .await;

    assert_eq!(status, 200, "{body}");
    assert_eq!(body["result"]["kind"], "docx");
    assert_eq!(body["result"]["text"], "Carta de prueba");
    assert_eq!(body["result"]["parts"], json!([]));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pdf_page_is_read_through_pdfcraft_and_named_as_the_part() {
    let host = TestHost::start_team_with_docs("doc-pdf", &App::ALL).await;
    let client = host.paired_with("Max", READ).await;
    let conv = conversation(&client).await;
    let attachment = sent_document(&host, &client, &conv, "factura.pdf", b"%PDF-1.4\n").await;

    let (status, body) = call(
        &host,
        ALFRED_TOKEN,
        "doc.read",
        json!({ "attachment_id": attachment, "page": 2 }),
    )
    .await;

    assert_eq!(status, 200, "{body}");
    assert_eq!(body["result"]["kind"], "pdf");
    assert_eq!(body["result"]["parts"], json!(["2"]));
    let calls = host.cli.calls();
    assert_eq!(calls[0].0, App::PdfCraft);
    assert_eq!(calls[0].1[0], "text");
    assert_eq!(calls[0].1[2..], ["--page", "2"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn plain_text_is_read_without_any_document_cli() {
    let host = TestHost::start_team_with_docs("doc-plain", &[]).await;
    let client = host.paired_with("Max", READ).await;
    let conv = conversation(&client).await;
    let attachment = sent_document(&host, &client, &conv, "notas.md", b"# Hola\n").await;

    let (status, body) = call(
        &host,
        ALFRED_TOKEN,
        "doc.read",
        json!({ "attachment_id": attachment }),
    )
    .await;

    assert_eq!(status, 200, "{body}");
    assert_eq!(body["result"]["kind"], "md");
    assert_eq!(body["result"]["text"], "# Hola\n");
    assert!(host.cli.calls().is_empty(), "no CLI runs for text");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sheet_without_its_cli_is_tool_missing_in_spanish() {
    let host = TestHost::start_team_with_docs("doc-missing", &[App::PdfCraft]).await;
    let client = host.paired_with("Max", READ).await;
    let conv = conversation(&client).await;
    let attachment = sent_document(&host, &client, &conv, "Ventas.xlsx", &ooxml_zip()).await;

    let (status, body) = call(
        &host,
        ALFRED_TOKEN,
        "doc.read",
        json!({ "attachment_id": attachment }),
    )
    .await;

    assert_eq!(status, 503, "{body}");
    assert_eq!(body["detail"]["code"], "tool_missing");
    assert_eq!(
        body["detail"]["message"],
        "Falta GridCraft en este PC. El Master puede instalarla desde Herramientas de documentos"
    );
    assert!(host.cli.calls().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_document_of_another_conversation_is_not_found() {
    let host = TestHost::start_team_with_docs("doc-foreign", &App::ALL).await;
    let client = host.paired_with("Max", READ).await;
    let mine = conversation(&client).await;
    let theirs = conversation(&client).await;
    let attachment = sent_document(&host, &client, &theirs, "Ventas.xlsx", &ooxml_zip()).await;
    // Alfred now answers in `mine`, where the file was never sent.
    let accepted = send(&host.chat, &mine, &[]);
    host.team.as_ref().expect("team").grant_for_tests(
        "tok-other",
        SeatActor {
            agent_id: host.chat.alfred_id().expect("alfred"),
            role_id: "alfred".to_string(),
            project_id: None,
            conversation_id: mine.clone(),
            run_id: accepted.turn.id.clone(),
        },
        "pane_test",
    );

    let (status, body) = call(
        &host,
        "tok-other",
        "doc.read",
        json!({ "attachment_id": attachment }),
    )
    .await;

    assert_eq!(status, 422, "{body}");
    assert_eq!(body["detail"]["code"], "attachment_not_found");
    assert!(host.cli.calls().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_that_is_not_sent_yet_is_not_found() {
    let host = TestHost::start_team_with_docs("doc-unsent", &App::ALL).await;
    let client = host.paired_with("Max", READ).await;
    let conv = conversation(&client).await;
    let attachment = sent_document(&host, &client, &conv, "notas.md", b"hola\n").await;
    let loose = upload(&client, &conv, "suelto.md", b"sin enviar\n").await;

    let (status, body) = call(
        &host,
        ALFRED_TOKEN,
        "doc.read",
        json!({ "attachment_id": loose }),
    )
    .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["detail"]["code"], "attachment_not_found");

    let (status, _) = call(
        &host,
        ALFRED_TOKEN,
        "doc.read",
        json!({ "attachment_id": attachment }),
    )
    .await;
    assert_eq!(status, 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_presentation_is_unsupported_and_never_run() {
    let host = TestHost::start_team_with_docs("doc-pptx", &App::ALL).await;
    let client = host.paired_with("Max", READ).await;
    let conv = conversation(&client).await;
    let attachment = sent_document(&host, &client, &conv, "Plan.pptx", &ooxml_zip()).await;

    let (status, body) = call(
        &host,
        ALFRED_TOKEN,
        "doc.read",
        json!({ "attachment_id": attachment }),
    )
    .await;

    assert_eq!(status, 422, "{body}");
    assert_eq!(body["detail"]["code"], "unsupported_document");
    assert!(host.cli.calls().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_broken_office_file_is_bad_document() {
    let host = TestHost::start_team_with_docs("doc-bad", &App::ALL).await;
    let client = host.paired_with("Max", READ).await;
    let conv = conversation(&client).await;
    let attachment = sent_document(
        &host,
        &client,
        &conv,
        "roto.xlsx",
        b"PK\x03\x04 no es un libro",
    )
    .await;

    let (status, body) = call(
        &host,
        ALFRED_TOKEN,
        "doc.read",
        json!({ "attachment_id": attachment }),
    )
    .await;

    assert_eq!(status, 422, "{body}");
    assert_eq!(body["detail"]["code"], "bad_document");
    assert!(host.cli.calls().is_empty(), "the check runs before any CLI");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_read_is_audited_by_tool_and_id_never_by_content() {
    let host = TestHost::start_team_with_docs("doc-audit", &App::ALL).await;
    let client = host.paired_with("Max", READ).await;
    let conv = conversation(&client).await;
    let attachment = sent_document(&host, &client, &conv, "Carta.docx", &ooxml_zip()).await;

    let (status, body) = call(
        &host,
        ALFRED_TOKEN,
        "doc.read",
        json!({ "attachment_id": attachment }),
    )
    .await;
    assert_eq!(status, 200, "{body}");

    let log = std::fs::read_to_string(AuditLog::new(&host.data_dir.join("mail/accounts")).path())
        .expect("audit log");
    assert!(log.contains("doc.read"), "{log}");
    assert!(log.contains(&attachment), "{log}");
    assert!(
        !log.contains("Carta de prueba"),
        "the content never reaches the audit"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_project_seat_never_reads_documents() {
    let host = TestHost::start_team_with_docs("doc-scope", &App::ALL).await;
    let client = host.paired_with("Max", READ).await;
    let conv = conversation(&client).await;
    let attachment = sent_document(&host, &client, &conv, "Ventas.xlsx", &ooxml_zip()).await;
    host.team.as_ref().expect("team").grant_for_tests(
        "tok-lead",
        SeatActor {
            agent_id: "lead_test".to_string(),
            role_id: "lead".to_string(),
            project_id: Some("proj_test".to_string()),
            conversation_id: conv.clone(),
            run_id: "run_test".to_string(),
        },
        "pane_test",
    );

    let (status, body) = call(
        &host,
        "tok-lead",
        "doc.read",
        json!({ "attachment_id": attachment }),
    )
    .await;

    assert_eq!(status, 422, "{body}");
    assert_eq!(body["detail"]["code"], "scope_denied");
    assert!(host.cli.calls().is_empty());
}
