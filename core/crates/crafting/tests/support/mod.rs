//! Helpers shared by the crafting tests: a scripted runner, fake CLI binaries
//! and a ZIP built by hand, so no test needs a real Office or PDF file.
#![allow(dead_code)]

use excubia_core_crafting::{
    App, Captured, CraftError, Document, Invocation, Runner, Tools, CONFIG_FILE,
};
use std::env::consts::EXE_SUFFIX;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const XLSX: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
pub const DOCX: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
pub const PDF: &str = "application/pdf";
pub const CSV: &str = "text/csv";
pub const TXT: &str = "text/plain";
pub const MD: &str = "text/markdown";
pub const JSON: &str = "application/json";

type Reply = dyn Fn(&Invocation) -> Result<Captured, CraftError>;

/// Records every invocation and answers with a scripted reply.
pub struct ScriptedRunner {
    calls: Mutex<Vec<Invocation>>,
    reply: Box<Reply>,
}

impl ScriptedRunner {
    pub fn new(reply: impl Fn(&Invocation) -> Result<Captured, CraftError> + 'static) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            reply: Box::new(reply),
        }
    }

    pub fn calls(&self) -> Vec<Invocation> {
        self.calls.lock().unwrap().clone()
    }
}

impl Runner for ScriptedRunner {
    fn run(&self, inv: &Invocation) -> Result<Captured, CraftError> {
        self.calls.lock().unwrap().push(inv.clone());
        (self.reply)(inv)
    }
}

pub fn ok(stdout: &str) -> Captured {
    Captured {
        success: true,
        stdout: stdout.as_bytes().to_vec(),
        ..Captured::default()
    }
}

pub fn failed(stderr: &str) -> Captured {
    Captured {
        success: false,
        stderr: stderr.as_bytes().to_vec(),
        ..Captured::default()
    }
}

pub fn args(inv: &Invocation) -> Vec<String> {
    inv.args
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect()
}

/// The arguments with the working folder replaced by `<ws>`, so a test can
/// compare the exact argument list without knowing the random folder name.
pub fn shape(inv: &Invocation) -> Vec<String> {
    let cwd = inv.cwd.to_string_lossy().into_owned();
    args(inv)
        .into_iter()
        .map(|arg| match arg.strip_prefix(&cwd) {
            Some(rest) => format!("<ws>/{}", rest.trim_start_matches(['\\', '/'])),
            None => arg,
        })
        .collect()
}

/// The value that follows `flag` in the arguments.
pub fn value_after(inv: &Invocation, flag: &str) -> String {
    let all = args(inv);
    let at = all.iter().position(|a| a == flag).expect("flag present");
    all[at + 1].clone()
}

/// Empty fake binaries for the given apps, plus a `crafting.json` that points
/// to them, so `Tools::discover` finds exactly those apps.
pub fn fake_tools(root: &Path, apps: &[App]) -> Tools {
    let data = root.join("data");
    fs::create_dir_all(&data).unwrap();
    let mut config = serde_json::Map::new();
    for app in apps {
        let exe = root.join("bin").join(format!("{}{EXE_SUFFIX}", app.cli()));
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::write(&exe, b"").unwrap();
        config.insert(
            app.config_key().to_string(),
            serde_json::Value::String(exe.display().to_string()),
        );
    }
    fs::write(
        data.join(CONFIG_FILE),
        serde_json::Value::Object(config).to_string(),
    )
    .unwrap();
    Tools::discover(&data, &data.join("tools"), None).unwrap()
}

/// A chat attachment as the host hands it over: the bytes live in `chat/`
/// under an opaque name, and the original name and mime travel apart.
pub fn document(root: &Path, name: &str, mime: &str, bytes: &[u8]) -> Document {
    let path: PathBuf = root.join("chat").join("att-1");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, bytes).unwrap();
    Document {
        path,
        name: name.to_string(),
        mime: mime.to_string(),
    }
}

pub fn png_bytes() -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(b"fake image body");
    bytes
}

fn le16(value: u16) -> [u8; 2] {
    value.to_le_bytes()
}

fn le32(value: u32) -> [u8; 4] {
    value.to_le_bytes()
}

/// A ZIP archive with one empty-data entry per `(name, declared size)` pair,
/// laid out exactly as the end-of-central-directory check reads it.
pub fn zip(entries: &[(&str, u32)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, size) in entries {
        let offset = out.len() as u32;
        out.extend(le32(0x0403_4b50));
        out.extend([0u8; 22]);
        out.extend(le16(name.len() as u16));
        out.extend(le16(0));
        out.extend(name.as_bytes());

        central.extend(le32(0x0201_4b50));
        central.extend([0u8; 12]);
        central.extend([0u8; 4]);
        central.extend(le32(*size));
        central.extend(le32(*size));
        central.extend(le16(name.len() as u16));
        central.extend(le16(0));
        central.extend(le16(0));
        central.extend([0u8; 8]);
        central.extend(le32(offset));
        central.extend(name.as_bytes());
    }
    let cd_offset = out.len() as u32;
    let cd_size = central.len() as u32;
    out.extend(central);
    out.extend(le32(0x0605_4b50));
    out.extend(le16(0));
    out.extend(le16(0));
    out.extend(le16(entries.len() as u16));
    out.extend(le16(entries.len() as u16));
    out.extend(le32(cd_size));
    out.extend(le32(cd_offset));
    out.extend(le16(0));
    out
}

/// The smallest archive that passes every OOXML check.
pub fn ooxml_zip() -> Vec<u8> {
    zip(&[("[Content_Types].xml", 10), ("xl/workbook.xml", 10)])
}
