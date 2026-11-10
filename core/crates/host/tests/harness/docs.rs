//! The document CLIs for the host tests (DOC5B): a fake GridCraft, WordCraft and
//! PdfCraft that answer by subcommand and write what a real run would write, so
//! no test needs an Office or PDF install. Every run is recorded.

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use excubia_core_chat::{AcceptedMessage, ChatStore};
use excubia_core_crafting::{App, Captured, CraftError, Invocation, Runner, Tools, CONFIG_FILE};
use excubia_core_host::craft::Craft;
use excubia_core_test_support::SignedClient;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// A PNG as a renderer writes it: the signature, then anything.
pub const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfake page";

/// The CLI runs of a test, in order: the app and its arguments.
#[derive(Default)]
pub struct FakeCli {
    calls: Mutex<Vec<(App, Vec<String>)>>,
}

impl FakeCli {
    /// How many times PdfCraft rendered a page.
    pub fn renders(&self) -> usize {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(app, args)| *app == App::PdfCraft && args[0] == "render")
            .count()
    }

    /// Every run, as `(app, arguments)`.
    pub fn calls(&self) -> Vec<(App, Vec<String>)> {
        self.calls.lock().unwrap().clone()
    }
}

fn args_of(inv: &Invocation) -> Vec<String> {
    inv.args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
}

fn value_after(args: &[String], flag: &str) -> String {
    let at = args
        .iter()
        .position(|arg| arg == flag)
        .expect("flag present");
    args[at + 1].clone()
}

/// What a conversion writes: a package for an Office name, a PDF otherwise.
fn converted(out: &str) -> Vec<u8> {
    if out.ends_with(".xlsx") || out.ends_with(".docx") {
        ooxml_zip()
    } else {
        b"%PDF-1.4\n".to_vec()
    }
}

fn ok(stdout: &str) -> Captured {
    Captured {
        success: true,
        stdout: stdout.as_bytes().to_vec(),
        ..Captured::default()
    }
}

fn failed(stderr: &str) -> Captured {
    Captured {
        success: false,
        stderr: stderr.as_bytes().to_vec(),
        ..Captured::default()
    }
}

impl Runner for FakeCli {
    fn run(&self, inv: &Invocation) -> Result<Captured, CraftError> {
        let args = args_of(inv);
        self.calls.lock().unwrap().push((inv.app, args.clone()));
        match (inv.app, args[0].as_str()) {
            (App::GridCraft, "info") => {
                Ok(ok(r#"{"sheets":[{"name":"Ventas"},{"name":"Gastos"}]}"#))
            }
            (App::GridCraft, "cat") => Ok(ok("a,b\n1,2")),
            (App::GridCraft | App::WordCraft, "convert") => {
                fs::write(&args[2], converted(&args[2])).unwrap();
                Ok(ok(""))
            }
            (App::WordCraft, "render") => {
                fs::write(&args[2], PNG).unwrap();
                Ok(ok(""))
            }
            (App::WordCraft, "text") => Ok(ok("Carta de prueba")),
            (App::PdfCraft, "text") => Ok(ok("uno\x0cdos")),
            (App::PdfCraft, "render") => {
                let page: u32 = value_after(&args, "--page").parse().unwrap();
                if page > 2 {
                    return Ok(failed("no such page"));
                }
                fs::write(value_after(&args, "--out"), PNG).unwrap();
                Ok(ok(""))
            }
            _ => Ok(failed("unexpected run in a test")),
        }
    }
}

/// A craft over fake executables for `apps` only, found through `crafting.json`.
pub fn craft(data_dir: &Path, apps: &[App], cli: Arc<FakeCli>) -> Craft {
    let bin = data_dir.join("fake-bin");
    fs::create_dir_all(&bin).unwrap();
    let mut config = serde_json::Map::new();
    for app in apps {
        let exe = bin.join(format!("{}{}", app.cli(), std::env::consts::EXE_SUFFIX));
        fs::write(&exe, b"").unwrap();
        config.insert(
            app.config_key().to_string(),
            Value::String(exe.display().to_string()),
        );
    }
    fs::write(
        data_dir.join(CONFIG_FILE),
        Value::Object(config).to_string(),
    )
    .unwrap();
    let tools = Tools::discover(data_dir, &data_dir.join("tools"), None).unwrap();
    Craft::new(tools, cli, data_dir.join("craft"))
}

/// The smallest ZIP that passes the OOXML check: two stored entries, no compression.
pub fn ooxml_zip() -> Vec<u8> {
    zip(&[("[Content_Types].xml", 10), ("xl/workbook.xml", 10)])
}

fn zip(entries: &[(&str, u32)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, size) in entries {
        let offset = out.len() as u32;
        out.extend(0x0403_4b50u32.to_le_bytes());
        out.extend([0u8; 22]);
        out.extend((name.len() as u16).to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out.extend(name.as_bytes());

        central.extend(0x0201_4b50u32.to_le_bytes());
        central.extend([0u8; 16]);
        central.extend(size.to_le_bytes());
        central.extend(size.to_le_bytes());
        central.extend((name.len() as u16).to_le_bytes());
        central.extend([0u8; 12]);
        central.extend(offset.to_le_bytes());
        central.extend(name.as_bytes());
    }
    let cd_offset = out.len() as u32;
    let cd_size = central.len() as u32;
    out.extend(central);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend([0u8; 4]);
    out.extend((entries.len() as u16).to_le_bytes());
    out.extend((entries.len() as u16).to_le_bytes());
    out.extend(cd_size.to_le_bytes());
    out.extend(cd_offset.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out
}

/// Uploads a whole file in one chunk and completes it; returns its id.
pub async fn upload(client: &SignedClient, conversation: &str, name: &str, bytes: &[u8]) -> String {
    let base = format!("/v1/chat/conversations/{conversation}/attachments");
    let init = json!({
        "client_id": format!("up-{name}"),
        "name": name,
        "size": bytes.len(),
        "sha256": Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
    });
    let (status, created) = request(client, "POST", &base, Some(init)).await;
    assert_eq!(status, 201, "{created}");
    let id = created["attachment"]["id"]
        .as_str()
        .expect("id")
        .to_string();
    let chunk = json!({
        "offset": 0,
        "data": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes),
    });
    let (status, sent) = request(client, "POST", &format!("{base}/{id}/chunks"), Some(chunk)).await;
    assert_eq!(status, 200, "{sent}");
    let (status, done) = request(client, "POST", &format!("{base}/{id}/complete"), None).await;
    assert_eq!(status, 200, "{done}");
    id
}

async fn request(
    client: &SignedClient,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (u16, Value) {
    let body = body.map(|value| value.to_string().into_bytes());
    let (status, text) = client.request(method, path, body).await.expect("request");
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

/// Sends `attachments` in a new user message of `conversation`, as the chat does.
pub fn send(chat: &ChatStore, conversation: &str, attachments: &[&str]) -> AcceptedMessage {
    chat.append_user_message_with_attachments(
        conversation,
        "msg-docs",
        "Mira esto",
        None,
        attachments,
    )
    .expect("message")
}
