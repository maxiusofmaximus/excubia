//! The page previews of the chat's documents (DOC5B): `GET …/attachments/{att}/preview`
//! answers the PNG of one page, rendered once and then kept beside the file; only
//! Excel, Word and PDF documents have one; a missing CLI, a page past the end and a
//! foreign id each answer with their own code; deleting the file deletes its previews.

mod harness;

use std::fs;
use std::path::{Path, PathBuf};

use harness::docs::{upload, PNG};
use harness::TestHost;
use hn_core_auth::Permission;
use hn_core_crafting::App;
use hn_core_host::paths;
use hn_core_test_support::SignedClient;
use reqwest::header::HeaderMap;
use serde_json::Value;

const READ_AND_RESPOND: &[Permission] = &[Permission::Respond];

fn attachments(conversation: &str, rest: &str) -> String {
    format!("/v1/chat/conversations/{conversation}/attachments{rest}")
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

/// One preview request: the status, the headers and the body.
async fn preview(
    client: &SignedClient,
    conversation: &str,
    attachment: &str,
    query: &str,
) -> (u16, HeaderMap, Vec<u8>) {
    let path = attachments(conversation, &format!("/{attachment}/preview{query}"));
    client
        .request_raw("GET", &path, None)
        .await
        .expect("preview")
}

fn json_body(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap_or(Value::Null)
}

/// Every cached page under `root`, by file name.
fn cached_pages(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(cached_pages(&path));
        } else if path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().contains("-p"))
            && path.extension().is_some_and(|e| e == "png")
        {
            found.push(path);
        }
    }
    found
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sheet_preview_is_a_png_with_the_served_headers() {
    let host = TestHost::start_with_docs("preview-png", &App::ALL).await;
    let client = host.paired_with("Max", READ_AND_RESPOND).await;
    let conv = conversation(&client).await;
    let sheet = upload(&client, &conv, "Ventas.xlsx", &harness::docs::ooxml_zip()).await;

    let (status, headers, body) = preview(&client, &conv, &sheet, "").await;

    assert_eq!(status, 200);
    assert_eq!(headers["content-type"], "image/png");
    assert_eq!(headers["x-content-type-options"], "nosniff");
    assert_eq!(headers["cache-control"], "private, no-store");
    assert_eq!(
        headers["content-disposition"],
        "inline; filename=\"preview.png\""
    );
    assert_eq!(body, PNG);
    assert_eq!(host.cli.renders(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_second_request_reuses_the_rendered_page() {
    let host = TestHost::start_with_docs("preview-cache", &App::ALL).await;
    let client = host.paired_with("Max", READ_AND_RESPOND).await;
    let conv = conversation(&client).await;
    let sheet = upload(&client, &conv, "Ventas.xlsx", &harness::docs::ooxml_zip()).await;

    let first = preview(&client, &conv, &sheet, "").await;
    let second = preview(&client, &conv, &sheet, "").await;

    assert_eq!(first.2, second.2);
    assert_eq!(host.cli.renders(), 1, "one render for two requests");
}

#[tokio::test(flavor = "multi_thread")]
async fn two_requests_at_once_render_the_page_once() {
    let host = TestHost::start_with_docs("preview-lock", &App::ALL).await;
    let client = host.paired_with("Max", READ_AND_RESPOND).await;
    let conv = conversation(&client).await;
    let sheet = upload(&client, &conv, "Ventas.xlsx", &harness::docs::ooxml_zip()).await;

    let (a, b) = tokio::join!(
        preview(&client, &conv, &sheet, ""),
        preview(&client, &conv, &sheet, "")
    );

    assert_eq!(a.0, 200);
    assert_eq!(b.0, 200);
    assert_eq!(host.cli.renders(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn each_page_is_rendered_and_kept_on_its_own() {
    let host = TestHost::start_with_docs("preview-pages", &App::ALL).await;
    let client = host.paired_with("Max", READ_AND_RESPOND).await;
    let conv = conversation(&client).await;
    let pdf = upload(&client, &conv, "factura.pdf", b"%PDF-1.4\n").await;

    let (status, _, _) = preview(&client, &conv, &pdf, "?page=2").await;
    assert_eq!(status, 200);
    let (status, _, _) = preview(&client, &conv, &pdf, "").await;
    assert_eq!(status, 200, "page 1 is the default");
    let (status, _, _) = preview(&client, &conv, &pdf, "?page=2").await;
    assert_eq!(status, 200);

    assert_eq!(host.cli.renders(), 2, "page 2 once and page 1 once");
    let renders: Vec<String> = host
        .cli
        .calls()
        .into_iter()
        .filter(|(app, args)| *app == App::PdfCraft && args[0] == "render")
        .map(|(_, args)| args[args.iter().position(|a| a == "--page").unwrap() + 1].clone())
        .collect();
    assert_eq!(renders, ["2", "1"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn csv_and_slides_have_no_preview() {
    let host = TestHost::start_with_docs("preview-none", &App::ALL).await;
    let client = host.paired_with("Max", READ_AND_RESPOND).await;
    let conv = conversation(&client).await;
    let csv = upload(&client, &conv, "datos.csv", b"a,b\n1,2\n").await;
    let slides = upload(&client, &conv, "Plan.pptx", &harness::docs::ooxml_zip()).await;

    for id in [&csv, &slides] {
        let (status, _, body) = preview(&client, &conv, id, "").await;
        assert_eq!(status, 404);
        assert_eq!(json_body(&body)["detail"]["code"], "preview_unavailable");
    }
    assert_eq!(host.cli.renders(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_cli_is_tool_missing_in_spanish() {
    let host = TestHost::start_with_docs("preview-missing", &[]).await;
    let client = host.paired_with("Max", READ_AND_RESPOND).await;
    let conv = conversation(&client).await;
    let sheet = upload(&client, &conv, "Ventas.xlsx", &harness::docs::ooxml_zip()).await;

    let (status, _, body) = preview(&client, &conv, &sheet, "").await;

    assert_eq!(status, 503);
    let body = json_body(&body);
    assert_eq!(body["detail"]["code"], "tool_missing");
    assert_eq!(
        body["detail"]["message"],
        "Falta GridCraft en este PC. El Master puede instalarla desde Herramientas de documentos"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_page_that_is_not_a_number_or_is_zero_is_a_bad_request() {
    let host = TestHost::start_with_docs("preview-page-bad", &App::ALL).await;
    let client = host.paired_with("Max", READ_AND_RESPOND).await;
    let conv = conversation(&client).await;
    let pdf = upload(&client, &conv, "factura.pdf", b"%PDF-1.4\n").await;

    for query in ["?page=0", "?page=abc", "?page=-1"] {
        let (status, _, body) = preview(&client, &conv, &pdf, query).await;
        assert_eq!(status, 400, "{query}");
        assert_eq!(
            json_body(&body)["detail"]["code"],
            "invalid_page",
            "{query}"
        );
    }
    assert_eq!(host.cli.renders(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_page_past_the_end_is_page_out_of_range() {
    let host = TestHost::start_with_docs("preview-range", &App::ALL).await;
    let client = host.paired_with("Max", READ_AND_RESPOND).await;
    let conv = conversation(&client).await;
    let pdf = upload(&client, &conv, "factura.pdf", b"%PDF-1.4\n").await;

    let (status, _, body) = preview(&client, &conv, &pdf, "?page=5").await;

    assert_eq!(status, 404);
    assert_eq!(json_body(&body)["detail"]["code"], "page_out_of_range");
    assert!(cached_pages(&paths::attachments_dir(&host.data_dir)).is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn another_conversations_document_is_not_found() {
    let host = TestHost::start_with_docs("preview-foreign", &App::ALL).await;
    let client = host.paired_with("Max", READ_AND_RESPOND).await;
    let mine = conversation(&client).await;
    let theirs = conversation(&client).await;
    let pdf = upload(&client, &theirs, "factura.pdf", b"%PDF-1.4\n").await;

    let (status, _, body) = preview(&client, &mine, &pdf, "").await;

    assert_eq!(status, 404);
    assert_eq!(json_body(&body)["detail"], "no_such_attachment");
    assert_eq!(host.cli.renders(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn deleting_the_document_deletes_its_previews() {
    let host = TestHost::start_with_docs("preview-delete", &App::ALL).await;
    let client = host.paired_with("Max", READ_AND_RESPOND).await;
    let conv = conversation(&client).await;
    let pdf = upload(&client, &conv, "factura.pdf", b"%PDF-1.4\n").await;
    let (status, _, _) = preview(&client, &conv, &pdf, "").await;
    assert_eq!(status, 200);
    let root = paths::attachments_dir(&host.data_dir);
    assert_eq!(cached_pages(&root).len(), 1);

    let (status, _) = client
        .request("DELETE", &attachments(&conv, &format!("/{pdf}")), None)
        .await
        .expect("delete");

    assert_eq!(status, 200);
    assert!(
        cached_pages(&root).is_empty(),
        "no preview outlives its file"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_view_says_which_documents_have_a_preview() {
    let host = TestHost::start_with_docs("preview-view", &App::ALL).await;
    let client = host.paired_with("Max", READ_AND_RESPOND).await;
    let conv = conversation(&client).await;
    let sheet = upload(&client, &conv, "Ventas.xlsx", &harness::docs::ooxml_zip()).await;
    let csv = upload(&client, &conv, "datos.csv", b"a,b\n").await;

    for (id, expected) in [(&sheet, true), (&csv, false)] {
        let (status, body) = client
            .request("GET", &attachments(&conv, &format!("/{id}")), None)
            .await
            .expect("view");
        assert_eq!(status, 200);
        let view: Value = serde_json::from_str(&body).expect("json");
        assert_eq!(view["attachment"]["previewable"], expected, "{body}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pdf_preview_says_how_many_pages_it_has_and_keeps_saying_it() {
    let host = TestHost::start_with_docs("preview-pages", &App::ALL).await;
    let client = host.paired_with("Max", READ_AND_RESPOND).await;
    let conv = conversation(&client).await;
    let pdf = upload(&client, &conv, "factura.pdf", b"%PDF-1.4\n").await;

    let (status, headers, _) = preview(&client, &conv, &pdf, "?page=2").await;
    assert_eq!(status, 200);
    assert_eq!(headers["x-preview-pages"], "2", "PdfCraft's text has two pages");

    let (status, headers, _) = preview(&client, &conv, &pdf, "?page=2").await;
    assert_eq!(status, 200);
    assert_eq!(headers["x-preview-pages"], "2", "the cached page keeps the count");
    assert_eq!(host.cli.renders(), 1, "the page is rendered once");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_word_preview_has_no_page_count_when_none_is_known() {
    let host = TestHost::start_with_docs("preview-no-count", &App::ALL).await;
    let client = host.paired_with("Max", READ_AND_RESPOND).await;
    let conv = conversation(&client).await;
    let word = upload(&client, &conv, "Carta.docx", &harness::docs::ooxml_zip()).await;

    let (status, headers, body) = preview(&client, &conv, &word, "").await;

    assert_eq!(status, 200);
    assert_eq!(body, PNG);
    assert!(headers.get("x-preview-pages").is_none());
}
