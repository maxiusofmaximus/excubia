//! Runs the real ArtCraft CLIs. Not part of the normal gate: it needs the
//! portable builds of GridCraft, WordCraft and PdfCraft on disk.
//!
//! Run it once by hand:
//!
//! ```text
//! set CRAFT_PROBE_DIR=<scratchpad>\craft-probe
//! cargo test -p excubia-crafting --test probe_real -- --ignored --nocapture
//! ```
//!
//! The probe folder must hold `grid/`, `wordcraft/`, `pdfcraft/` (each with the
//! unzipped portable build), `v.pdf` (two pages) and `b6.xlsx`.

use excubia_crafting::{App, Driver, ProcessRunner, ReadOpts, Tools, CONFIG_FILE};
use excubia_test_dir::TestDir;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The first file called `name` under `dir`, looking at most four folders deep.
fn find_exe(dir: &Path, name: &str, depth: u8) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    let mut children: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    children.sort();
    if let Some(hit) = children
        .iter()
        .find(|p| p.file_name().is_some_and(|n| n == name))
    {
        return Some(hit.clone());
    }
    if depth == 0 {
        return None;
    }
    children
        .iter()
        .filter(|p| p.is_dir())
        .find_map(|d| find_exe(d, name, depth - 1))
}

fn probe_tools(root: &Path, probe: &Path) -> Tools {
    let exe = |folder: &str, name: &str| find_exe(&probe.join(folder), &format!("{name}.exe"), 4);
    let config = serde_json::json!({
        "gridcraft_cli": exe("grid", "gridcraft-cli"),
        "wordcraft_cli": exe("wordcraft", "wordcraft-cli"),
        "pdfcraft_cli": exe("pdfcraft", "pdfcraft-cli"),
    });
    let data = root.join("data");
    fs::create_dir_all(&data).unwrap();
    fs::write(data.join(CONFIG_FILE), config.to_string()).unwrap();
    Tools::discover(&data, &data.join("tools"), None).unwrap()
}

#[test]
#[ignore = "needs the real CLIs: set CRAFT_PROBE_DIR (see the module docs)"]
fn real_clis_read_and_render() {
    let probe = PathBuf::from(env::var_os("CRAFT_PROBE_DIR").expect("CRAFT_PROBE_DIR is not set"));
    let root = TestDir::new("craft-probe");
    let tools = probe_tools(&root, &probe);
    for app in App::ALL {
        assert!(
            tools.available(app),
            "{} not found under the probe folder",
            app.name()
        );
    }
    let driver = Driver::new(&tools, &ProcessRunner, root.join("work"));

    // A small workbook made by the real CLI, then read back through the driver.
    let csv = root.join("pequeno.csv");
    fs::write(&csv, "nombre,valor\nhola,1\n").unwrap();
    let xlsx = root.join("pequeno.xlsx");
    let made = Command::new(tools.path(App::GridCraft).unwrap())
        .arg("convert")
        .arg(&csv)
        .arg(&xlsx)
        .status()
        .unwrap();
    assert!(made.success());
    let doc = support_document(
        &root,
        "pequeno.xlsx",
        &xlsx,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    );
    let sheet = driver.read_text(&doc, &ReadOpts::default()).unwrap();
    assert!(sheet.text.contains("hola"), "text was: {}", sheet.text);
    assert!(!sheet.pages_or_sheets.is_empty());

    let pdf = probe.join("v.pdf");
    let doc = support_document(&root, "v.pdf", &pdf, "application/pdf");
    let pages = driver.read_text(&doc, &ReadOpts::default()).unwrap();
    assert_eq!(pages.pages_or_sheets, ["1", "2"]);
    let page = driver
        .read_text(
            &doc,
            &ReadOpts {
                page: Some(2),
                ..ReadOpts::default()
            },
        )
        .unwrap();
    assert!(!page.text.is_empty());

    let png = driver.render_png(&doc, 1).unwrap().png;
    assert_eq!(&png[..4], [0x89, b'P', b'N', b'G']);

    let book = probe.join("b6.xlsx");
    let doc = support_document(
        &root,
        "b6.xlsx",
        &book,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    );
    let preview = driver.render_png(&doc, 1).unwrap();
    assert_eq!(&preview.png[..4], [0x89, b'P', b'N', b'G']);
    assert!(
        preview.pages.is_some(),
        "a sheet converted to PDF has pages"
    );

    let version = driver.version(App::GridCraft).unwrap();
    assert!(version.matches, "found {}", version.found);
}

/// A document whose bytes come from an existing file, as the host would hand them over.
fn support_document(
    root: &Path,
    name: &str,
    from: &Path,
    mime: &str,
) -> excubia_crafting::Document {
    let path = root.join(format!("chat-{name}"));
    fs::copy(from, &path).unwrap();
    excubia_crafting::Document {
        path,
        name: name.to_string(),
        mime: mime.to_string(),
    }
}
