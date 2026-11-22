//! Alfred edits a copy of a Word file the user attached (FIX2): `doc.edit` answers a new
//! ready `.docx` in the conversation, the original stays as it was, and every refusal has
//! its stable code. The bytes of the copy are checked in `excubia-crafting`.

mod harness;

use excubia_auth::Permission;
use excubia_chat::SeatActor;
use excubia_crafting::App;
use excubia_test_support::SignedClient;
use harness::docs::{send, upload, ooxml_zip};
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
async fn sent_file(
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
    let created = client.chat_create("Cartas", None).await.expect("create");
    created["conversation"]["id"]
        .as_str()
        .expect("id")
        .to_string()
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// A ZIP of stored entries with real sizes and checksums: a reader can inflate each part.
fn stored_zip(parts: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in parts {
        let offset = out.len() as u32;
        let size = data.len() as u32;
        let crc = crc32(data);
        out.extend(0x0403_4b50u32.to_le_bytes());
        out.extend(20u16.to_le_bytes());
        out.extend([0u8; 4]);
        out.extend([0u8; 4]);
        out.extend(crc.to_le_bytes());
        out.extend(size.to_le_bytes());
        out.extend(size.to_le_bytes());
        out.extend((name.len() as u16).to_le_bytes());
        out.extend([0u8; 2]);
        out.extend(name.as_bytes());
        out.extend_from_slice(data);

        central.extend(0x0201_4b50u32.to_le_bytes());
        central.extend([20, 0, 20, 0]);
        central.extend([0u8; 4]);
        central.extend([0u8; 4]);
        central.extend(crc.to_le_bytes());
        central.extend(size.to_le_bytes());
        central.extend(size.to_le_bytes());
        central.extend((name.len() as u16).to_le_bytes());
        central.extend([0u8; 12]);
        central.extend(offset.to_le_bytes());
        central.extend(name.as_bytes());
    }
    let directory_at = out.len() as u32;
    let directory_len = central.len() as u32;
    out.extend(central);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend([0u8; 4]);
    out.extend((parts.len() as u16).to_le_bytes());
    out.extend((parts.len() as u16).to_le_bytes());
    out.extend(directory_len.to_le_bytes());
    out.extend(directory_at.to_le_bytes());
    out.extend([0u8; 2]);
    out
}

/// A Word file: one paragraph and a table of one row and one cell.
fn letter() -> Vec<u8> {
    let document = "<w:document><w:body>\
        <w:p><w:r><w:t>Carta de prueba</w:t></w:r></w:p>\
        <w:tbl><w:tr><w:tc><w:p><w:r><w:t>Nombre</w:t></w:r></w:p></w:tc></w:tr></w:tbl>\
        </w:body></w:document>";
    stored_zip(&[
        ("[Content_Types].xml", b"<Types/>"),
        ("word/document.xml", document.as_bytes()),
    ])
}

fn edit_args(attachment: &str, changes: Value) -> Value {
    json!({ "attachment_id": attachment, "changes": changes })
}

#[tokio::test(flavor = "multi_thread")]
async fn an_edited_copy_is_a_new_attachment_and_the_original_stays() {
    let host = TestHost::start_team_with_docs("doc-edit-copy", &App::ALL).await;
    let client = host.paired_with("Max", READ).await;
    let conv = conversation(&client).await;
    let original = sent_file(&host, &client, &conv, "Carta.docx", &letter()).await;

    let (status, body) = call(
        &host,
        ALFRED_TOKEN,
        "doc.edit",
        edit_args(
            &original,
            json!([
                {"op": "replace_text", "find": "Carta de prueba", "replace": "Carta revisada"},
                {"op": "set_cell", "table": 1, "row": 1, "col": 1, "text": "Apellido"}
            ]),
        ),
    )
    .await;

    assert_eq!(status, 200, "{body}");
    let result = &body["result"];
    assert_eq!(result["ok"], true);
    assert_eq!(result["name"], "Carta (editado).docx");
    assert_eq!(result["kind"], "document");
    assert_eq!(result["changes"], 2);
    assert_eq!(result["original_unchanged"], true);
    let copy = result["attachment_id"].as_str().expect("copy id");
    assert_ne!(copy, original, "the copy is a file of its own");

    let (status, body) = call(
        &host,
        ALFRED_TOKEN,
        "doc.read",
        json!({ "attachment_id": original }),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["result"]["kind"], "docx", "the original still reads");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_that_is_not_a_word_document_is_refused() {
    let host = TestHost::start_team_with_docs("doc-edit-xlsx", &App::ALL).await;
    let client = host.paired_with("Max", READ).await;
    let conv = conversation(&client).await;
    let sheet = sent_file(&host, &client, &conv, "Ventas.xlsx", &ooxml_zip()).await;

    let (status, body) = call(
        &host,
        ALFRED_TOKEN,
        "doc.edit",
        edit_args(
            &sheet,
            json!([{"op": "replace_text", "find": "a", "replace": "b"}]),
        ),
    )
    .await;

    assert_eq!(status, 422, "{body}");
    assert_eq!(body["detail"]["code"], "unsupported_document");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_text_or_a_cell_that_is_not_there_is_refused_with_its_own_code() {
    let host = TestHost::start_team_with_docs("doc-edit-target", &App::ALL).await;
    let client = host.paired_with("Max", READ).await;
    let conv = conversation(&client).await;
    let original = sent_file(&host, &client, &conv, "Carta.docx", &letter()).await;

    let (status, body) = call(
        &host,
        ALFRED_TOKEN,
        "doc.edit",
        edit_args(
            &original,
            json!([{"op": "replace_text", "find": "no está", "replace": "x"}]),
        ),
    )
    .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["detail"]["code"], "target_not_found");

    let (status, body) = call(
        &host,
        ALFRED_TOKEN,
        "doc.edit",
        edit_args(
            &original,
            json!([{"op": "set_cell", "table": 2, "row": 1, "col": 1, "text": "x"}]),
        ),
    )
    .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["detail"]["code"], "target_not_found");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_word_file_without_its_text_is_a_bad_document() {
    let host = TestHost::start_team_with_docs("doc-edit-bad", &App::ALL).await;
    let client = host.paired_with("Max", READ).await;
    let conv = conversation(&client).await;
    let broken = sent_file(&host, &client, &conv, "Roto.docx", &ooxml_zip()).await;

    let (status, body) = call(
        &host,
        ALFRED_TOKEN,
        "doc.edit",
        edit_args(
            &broken,
            json!([{"op": "replace_text", "find": "a", "replace": "b"}]),
        ),
    )
    .await;

    assert_eq!(status, 422, "{body}");
    assert_eq!(body["detail"]["code"], "bad_document");
}
