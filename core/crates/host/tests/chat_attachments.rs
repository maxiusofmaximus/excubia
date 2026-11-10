//! Attachments at HTTP level: the signed upload in chunks, the host's check and
//! re-encode on `complete`, the stored WebP served with `nosniff`, and the
//! refusals of the contract. Real loopback sockets, no network beyond it.

mod harness;

use std::io::Cursor;

use base64::{engine::general_purpose::STANDARD, Engine};
use harness::TestHost;
use excubia_core_auth::Permission;
use excubia_core_test_support::SignedClient;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const READ_AND_RESPOND: &[Permission] = &[Permission::ReadState, Permission::Respond];

fn png() -> Vec<u8> {
    let picture = image::RgbImage::from_fn(64, 48, |x, y| {
        image::Rgb([(x * 4) as u8, (y * 5) as u8, 128])
    });
    let mut encoded = Cursor::new(Vec::new());
    image::DynamicImage::from(picture)
        .write_to(&mut encoded, image::ImageFormat::Png)
        .expect("encode png");
    encoded.into_inner()
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn attachments(conversation: &str, rest: &str) -> String {
    format!("/v1/chat/conversations/{conversation}/attachments{rest}")
}

async fn call(
    client: &SignedClient,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (u16, Value) {
    let body = body.map(|value| value.to_string().into_bytes());
    let (status, text) = client.request(method, path, body).await.expect("request");
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

async fn conversation(client: &SignedClient) -> String {
    let created = client.chat_create("Fotos", None).await.expect("create");
    created["conversation"]["id"]
        .as_str()
        .expect("conversation id")
        .to_string()
}

async fn init(
    client: &SignedClient,
    conversation: &str,
    client_id: &str,
    name: &str,
    bytes: &[u8],
) -> (u16, Value) {
    let body = json!({
        "client_id": client_id,
        "name": name,
        "size": bytes.len(),
        "sha256": sha256_hex(bytes),
    });
    call(client, "POST", &attachments(conversation, ""), Some(body)).await
}

async fn chunk(
    client: &SignedClient,
    conversation: &str,
    attachment: &str,
    offset: usize,
    bytes: &[u8],
) -> (u16, Value) {
    let body = json!({"offset": offset, "data": STANDARD.encode(bytes)});
    let path = attachments(conversation, &format!("/{attachment}/chunks"));
    call(client, "POST", &path, Some(body)).await
}

async fn complete(client: &SignedClient, conversation: &str, attachment: &str) -> (u16, Value) {
    let path = attachments(conversation, &format!("/{attachment}/complete"));
    call(client, "POST", &path, None).await
}

fn id_of(created: &Value) -> String {
    created["attachment"]["id"]
        .as_str()
        .expect("attachment id")
        .to_string()
}

/// A photo sent whole and completed; returns its id.
async fn ready_photo(client: &SignedClient, conversation: &str, client_id: &str) -> String {
    let photo = png();
    let (status, created) = init(client, conversation, client_id, "captura.png", &photo).await;
    assert_eq!(status, 201, "{created}");
    let attachment = id_of(&created);
    let (status, sent) = chunk(client, conversation, &attachment, 0, &photo).await;
    assert_eq!(status, 200, "{sent}");
    let (status, done) = complete(client, conversation, &attachment).await;
    assert_eq!(status, 200, "{done}");
    attachment
}

#[tokio::test(flavor = "multi_thread")]
async fn a_photo_is_stored_as_webp_and_served_with_nosniff() {
    let host = TestHost::start("attachments-flow").await;
    let client = host.paired_with("Desktop", READ_AND_RESPOND).await;
    let conversation = conversation(&client).await;
    let photo = png();
    let (status, created) = init(&client, &conversation, "foto-1", "captura.png", &photo).await;
    assert_eq!(status, 201, "{created}");
    assert_eq!(created["attachment"]["state"], "uploading");
    let attachment = id_of(&created);

    let half = photo.len() / 2;
    let (status, first) = chunk(&client, &conversation, &attachment, 0, &photo[..half]).await;
    assert_eq!(status, 200, "{first}");
    let (status, second) = chunk(&client, &conversation, &attachment, half, &photo[half..]).await;
    assert_eq!(status, 200, "{second}");
    assert_eq!(second["next_offset"], json!(photo.len()));

    let (status, done) = complete(&client, &conversation, &attachment).await;
    assert_eq!(status, 200, "{done}");
    assert_eq!(done["attachment"]["state"], "ready");
    assert_eq!(done["attachment"]["mime"], "image/webp");
    assert_eq!(done["attachment"]["w"], 64);
    assert_eq!(done["attachment"]["h"], 48);
    let (status, again) = complete(&client, &conversation, &attachment).await;
    assert_eq!(
        (status, &again["attachment"]["state"]),
        (200, &json!("ready"))
    );

    let path = attachments(&conversation, &format!("/{attachment}/content"));
    let (status, headers, bytes) = client
        .request_raw("GET", &path, None)
        .await
        .expect("content");
    assert_eq!(status, 200);
    assert_eq!(headers["content-type"].to_str().unwrap(), "image/webp");
    assert_eq!(
        headers["x-content-type-options"].to_str().unwrap(),
        "nosniff"
    );
    assert_eq!(
        headers["content-disposition"].to_str().unwrap(),
        "attachment; filename*=UTF-8''captura.png"
    );
    assert_eq!(
        headers["cache-control"].to_str().unwrap(),
        "private, no-store"
    );
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WEBP");
}

#[tokio::test(flavor = "multi_thread")]
async fn repeating_the_client_id_answers_with_the_first_attachment() {
    let host = TestHost::start("attachments-init").await;
    let client = host.paired_with("Desktop", READ_AND_RESPOND).await;
    let conversation = conversation(&client).await;
    let photo = png();
    let (_, first) = init(&client, &conversation, "dup", "captura.png", &photo).await;
    let (status, second) = init(&client, &conversation, "dup", "captura.png", &photo).await;
    assert_eq!(status, 201, "{second}");
    assert_eq!(id_of(&first), id_of(&second));
}

#[tokio::test(flavor = "multi_thread")]
async fn offsets_must_continue_the_stored_bytes() {
    let host = TestHost::start("attachments-offsets").await;
    let client = host.paired_with("Desktop", READ_AND_RESPOND).await;
    let conversation = conversation(&client).await;
    let bytes = vec![7u8; 100];
    let (_, created) = init(&client, &conversation, "offsets", "dato.bin", &bytes).await;
    let attachment = id_of(&created);

    let (status, first) = chunk(&client, &conversation, &attachment, 0, &bytes[..40]).await;
    assert_eq!(status, 200, "{first}");
    assert_eq!(first["next_offset"], json!(40));

    // A repeated offset and a skipped one are both refused, and the answer says where the upload stands.
    let (status, repeated) = chunk(&client, &conversation, &attachment, 0, &bytes[..40]).await;
    assert_eq!(status, 409);
    assert_eq!(repeated["detail"]["code"], "offset_mismatch");
    assert_eq!(repeated["detail"]["next_offset"], json!(40));
    let (status, skipped) = chunk(&client, &conversation, &attachment, 60, &bytes[60..]).await;
    assert_eq!(status, 409);
    assert_eq!(skipped["detail"]["next_offset"], json!(40));

    let (status, rest) = chunk(&client, &conversation, &attachment, 40, &bytes[40..]).await;
    assert_eq!(status, 200, "{rest}");
    assert_eq!(rest["next_offset"], json!(100));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_chunk_over_128_kib_is_refused() {
    let host = TestHost::start("attachments-chunk").await;
    let client = host.paired_with("Desktop", READ_AND_RESPOND).await;
    let conversation = conversation(&client).await;
    let bytes = vec![1u8; 200 * 1024];
    let (_, created) = init(&client, &conversation, "big", "grande.bin", &bytes).await;
    let attachment = id_of(&created);

    let (status, body) = chunk(&client, &conversation, &attachment, 0, &bytes[..129 * 1024]).await;
    assert_eq!(status, 413, "{body}");
    assert_eq!(body["detail"]["code"], "chunk_too_large");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wrong_hash_fails_and_leaves_nothing_behind() {
    let host = TestHost::start("attachments-hash").await;
    let client = host.paired_with("Desktop", READ_AND_RESPOND).await;
    let conversation = conversation(&client).await;
    let sent = b"no es la huella declarada".to_vec();
    let body = json!({
        "client_id": "hash",
        "name": "foto.png",
        "size": sent.len(),
        "sha256": sha256_hex(b"otra cosa"),
    });
    let (status, created) =
        call(&client, "POST", &attachments(&conversation, ""), Some(body)).await;
    assert_eq!(status, 201, "{created}");
    let attachment = id_of(&created);
    chunk(&client, &conversation, &attachment, 0, &sent).await;

    let (status, refused) = complete(&client, &conversation, &attachment).await;
    assert_eq!(status, 422);
    assert_eq!(refused["detail"]["code"], "hash_mismatch");
    let path = attachments(&conversation, &format!("/{attachment}"));
    let (status, _) = call(&client, "GET", &path, None).await;
    assert_eq!(status, 404, "a failed upload is dropped");
}

#[tokio::test(flavor = "multi_thread")]
async fn text_named_png_is_not_an_image() {
    let host = TestHost::start("attachments-text").await;
    let client = host.paired_with("Desktop", READ_AND_RESPOND).await;
    let conversation = conversation(&client).await;
    let text = b"<html>no soy una imagen</html>".to_vec();
    let (_, created) = init(&client, &conversation, "texto", "foto.png", &text).await;
    let attachment = id_of(&created);
    chunk(&client, &conversation, &attachment, 0, &text).await;

    let (status, refused) = complete(&client, &conversation, &attachment).await;
    assert_eq!(status, 422);
    assert_eq!(refused["detail"]["code"], "unsupported_type");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_gif_is_refused() {
    let host = TestHost::start("attachments-gif").await;
    let client = host.paired_with("Desktop", READ_AND_RESPOND).await;
    let conversation = conversation(&client).await;
    let gif = b"GIF89a\x01\x00\x01\x00\x80\x00\x00".to_vec();
    let (_, created) = init(&client, &conversation, "gif", "animado.gif", &gif).await;
    let attachment = id_of(&created);
    chunk(&client, &conversation, &attachment, 0, &gif).await;

    let (status, refused) = complete(&client, &conversation, &attachment).await;
    assert_eq!(status, 422);
    assert_eq!(refused["detail"]["code"], "unsupported_type");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_conversation_quota_refuses_the_eleventh_declared_upload() {
    let host = TestHost::start("attachments-quota").await;
    let client = host.paired_with("Desktop", READ_AND_RESPOND).await;
    let conversation = conversation(&client).await;
    let ten_mib = 10 * 1024 * 1024;
    for n in 0..10 {
        let body = json!({
            "client_id": format!("q-{n}"),
            "name": "grande.png",
            "size": ten_mib,
            "sha256": sha256_hex(b""),
        });
        let (status, created) =
            call(&client, "POST", &attachments(&conversation, ""), Some(body)).await;
        assert_eq!(status, 201, "{created}");
    }
    let body = json!({
        "client_id": "q-10",
        "name": "grande.png",
        "size": ten_mib,
        "sha256": sha256_hex(b""),
    });
    let (status, refused) =
        call(&client, "POST", &attachments(&conversation, ""), Some(body)).await;
    assert_eq!(status, 413, "{refused}");
    assert_eq!(refused["detail"]["code"], "conversation_quota");
}

#[tokio::test(flavor = "multi_thread")]
async fn another_conversations_attachment_is_not_found() {
    let host = TestHost::start("attachments-foreign").await;
    let client = host.paired_with("Desktop", READ_AND_RESPOND).await;
    let mine = conversation(&client).await;
    let other = conversation(&client).await;
    let attachment = ready_photo(&client, &mine, "mine-1").await;

    let read = attachments(&other, &format!("/{attachment}"));
    let (status, body) = call(&client, "GET", &read, None).await;
    assert_eq!(status, 404);
    assert_eq!(body["detail"], "no_such_attachment");
    let (status, _) = call(&client, "DELETE", &read, None).await;
    assert_eq!(status, 404);
    let (status, _) = chunk(&client, &other, &attachment, 0, b"x").await;
    assert_eq!(status, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_linked_attachment_cannot_be_deleted_but_a_loose_one_can() {
    let host = TestHost::start("attachments-link").await;
    let client = host.paired_with("Desktop", READ_AND_RESPOND).await;
    let conversation = conversation(&client).await;
    let linked = ready_photo(&client, &conversation, "linked-1").await;
    let sent = client
        .chat_send(&conversation, "msg-1", "mira esto")
        .await
        .expect("send");
    let message = sent["message"]["id"].as_str().expect("message id");
    host.chat
        .link_to_message(&conversation, message, &[&linked])
        .expect("link");

    let path = attachments(&conversation, &format!("/{linked}"));
    let (status, body) = call(&client, "DELETE", &path, None).await;
    assert_eq!(status, 409);
    assert_eq!(body["detail"]["code"], "already_linked");

    let loose = ready_photo(&client, &conversation, "loose-1").await;
    let path = attachments(&conversation, &format!("/{loose}"));
    let (status, _) = call(&client, "DELETE", &path, None).await;
    assert_eq!(status, 200);
    let (status, _) = call(&client, "GET", &path, None).await;
    assert_eq!(status, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn uploading_needs_respond_and_reading_needs_read_state() {
    let host = TestHost::start("attachments-permissions").await;
    let writer = host.paired_with("Escritor", &[Permission::Respond]).await;
    let reader = host.paired_with("Lector", &[Permission::ReadState]).await;
    let conversation = conversation(&writer).await;

    let (status, body) = init(&reader, &conversation, "lector", "foto.png", &png()).await;
    assert_eq!(status, 403, "{body}");

    let attachment = ready_photo(&writer, &conversation, "writer-1").await;
    let path = attachments(&conversation, &format!("/{attachment}"));
    let (status, meta) = call(&reader, "GET", &path, None).await;
    assert_eq!(status, 200, "{meta}");
    assert_eq!(meta["attachment"]["state"], "ready");
    let content = attachments(&conversation, &format!("/{attachment}/content"));
    let (status, _, _) = reader
        .request_raw("GET", &content, None)
        .await
        .expect("content");
    assert_eq!(status, 200);
}
