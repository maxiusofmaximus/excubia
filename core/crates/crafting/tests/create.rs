//! Creating a document from text (DOC8): Excel and Word/PDF are built by their CLIs
//! from a CSV or Markdown copy in a private folder; CSV, text, Markdown and JSON are
//! kept as written. Bad content is refused before any CLI runs.

mod support;

use excubia_core_crafting::{App, CraftError, Driver, Kind};
use excubia_core_test_dir::TestDir;
use std::fs;
use support::{args, failed, fake_tools, ok, ooxml_zip, shape, ScriptedRunner};

const TEN_MIB: usize = 10 * 1024 * 1024;

#[test]
fn xlsx_is_built_from_csv_through_gridcraft() {
    let root = TestDir::new("craft-create-xlsx");
    let tools = fake_tools(&root, &[App::GridCraft]);
    let runner = ScriptedRunner::new(|inv| {
        assert_eq!(fs::read(&args(inv)[1]).unwrap(), b"Mes,Total\nEnero,10");
        fs::write(&args(inv)[2], ooxml_zip()).unwrap();
        Ok(ok(""))
    });

    let bytes = Driver::new(&tools, &runner, root.join("work"))
        .create_file(Kind::Xlsx, "Mes,Total\nEnero,10")
        .unwrap();

    assert_eq!(bytes, ooxml_zip());
    let calls = runner.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].app, App::GridCraft);
    assert_eq!(
        shape(&calls[0]),
        ["convert", "<ws>/entrada.csv", "<ws>/salida.xlsx"]
    );
}

#[test]
fn docx_and_pdf_are_built_from_markdown_through_wordcraft() {
    let root = TestDir::new("craft-create-word");
    let tools = fake_tools(&root, &[App::WordCraft]);
    let runner = ScriptedRunner::new(|inv| {
        let out = args(inv)[2].clone();
        let bytes = if out.ends_with(".docx") {
            ooxml_zip()
        } else {
            b"%PDF-1.4\n".to_vec()
        };
        fs::write(out, bytes).unwrap();
        Ok(ok(""))
    });
    let driver = Driver::new(&tools, &runner, root.join("work"));

    assert_eq!(driver.create_file(Kind::Docx, "# Carta").unwrap(), ooxml_zip());
    assert_eq!(driver.create_file(Kind::Pdf, "# Carta").unwrap(), b"%PDF-1.4\n");

    let calls = runner.calls();
    assert_eq!(calls[0].app, App::WordCraft);
    assert_eq!(shape(&calls[0]), ["convert", "<ws>/entrada.md", "<ws>/salida.docx"]);
    assert_eq!(shape(&calls[1]), ["convert", "<ws>/entrada.md", "<ws>/salida.pdf"]);
}

#[test]
fn csv_txt_md_and_json_are_kept_as_written_and_need_no_app() {
    let root = TestDir::new("craft-create-plain");
    let tools = fake_tools(&root, &[]);
    let runner = ScriptedRunner::new(|_| Err(CraftError::Invalid("no debe ejecutarse")));
    let driver = Driver::new(&tools, &runner, root.join("work"));

    assert_eq!(driver.create_file(Kind::Csv, "a,b").unwrap(), b"a,b");
    assert_eq!(driver.create_file(Kind::Txt, "hola").unwrap(), b"hola");
    assert_eq!(driver.create_file(Kind::Md, "# hola").unwrap(), b"# hola");
    assert_eq!(driver.create_file(Kind::Json, "{\"a\": 1}").unwrap(), b"{\"a\": 1}");
    assert!(runner.calls().is_empty());
}

#[test]
fn a_missing_app_is_named_before_anything_runs() {
    let root = TestDir::new("craft-create-missing");
    let tools = fake_tools(&root, &[App::PdfCraft]);
    let runner = ScriptedRunner::new(|_| Ok(ok("")));

    let refusal = Driver::new(&tools, &runner, root.join("work")).create_file(Kind::Xlsx, "a,b");

    assert!(
        matches!(refusal, Err(CraftError::ToolMissing(App::GridCraft))),
        "{refusal:?}"
    );
    assert!(runner.calls().is_empty());
}

#[test]
fn empty_or_invalid_content_is_refused_before_any_run() {
    let root = TestDir::new("craft-create-content");
    let tools = fake_tools(&root, &App::ALL);
    let runner = ScriptedRunner::new(|_| Err(CraftError::Invalid("no debe ejecutarse")));
    let driver = Driver::new(&tools, &runner, root.join("work"));

    assert!(matches!(driver.create_file(Kind::Csv, "  \n "), Err(CraftError::Invalid(_))));
    assert!(matches!(driver.create_file(Kind::Txt, ""), Err(CraftError::Invalid(_))));
    assert!(matches!(driver.create_file(Kind::Json, "{no"), Err(CraftError::Invalid(_))));
    assert!(matches!(
        driver.create_file(Kind::Xlsx, &"x,y\n".repeat(25_001)),
        Err(CraftError::Invalid(_))
    ));
    assert!(runner.calls().is_empty());
}

#[test]
fn a_quoted_separator_is_not_a_cell_boundary() {
    let root = TestDir::new("craft-create-quoted");
    let tools = fake_tools(&root, &[App::GridCraft]);
    let runner = ScriptedRunner::new(|inv| {
        fs::write(&args(inv)[2], ooxml_zip()).unwrap();
        Ok(ok(""))
    });

    let accepted = Driver::new(&tools, &runner, root.join("work"))
        .create_file(Kind::Xlsx, &"\"x,y\"\n".repeat(25_000));

    assert!(accepted.is_ok(), "25 000 rows of 1 quoted cell each is within the limit");
}

#[test]
fn a_cli_failure_keeps_its_own_error() {
    let root = TestDir::new("craft-create-failed");
    let tools = fake_tools(&root, &[App::GridCraft]);
    let runner = ScriptedRunner::new(|_| Ok(failed("boom")));

    let refusal = Driver::new(&tools, &runner, root.join("work")).create_file(Kind::Xlsx, "a,b");

    assert!(matches!(refusal, Err(CraftError::Failed(App::GridCraft, _))), "{refusal:?}");
}

#[test]
fn an_office_file_that_is_not_a_package_is_refused() {
    let root = TestDir::new("craft-create-bad-output");
    let tools = fake_tools(&root, &[App::GridCraft]);
    let runner = ScriptedRunner::new(|inv| {
        fs::write(&args(inv)[2], b"no es un zip").unwrap();
        Ok(ok(""))
    });

    let refusal = Driver::new(&tools, &runner, root.join("work")).create_file(Kind::Xlsx, "a,b");

    assert!(matches!(refusal, Err(CraftError::BadOutput(_))), "{refusal:?}");
}

#[test]
fn a_pdf_without_the_pdf_signature_is_refused() {
    let root = TestDir::new("craft-create-pdf-signature");
    let tools = fake_tools(&root, &[App::WordCraft]);
    let runner = ScriptedRunner::new(|inv| {
        fs::write(&args(inv)[2], b"texto").unwrap();
        Ok(ok(""))
    });

    let refusal = Driver::new(&tools, &runner, root.join("work")).create_file(Kind::Pdf, "# x");

    assert!(matches!(refusal, Err(CraftError::BadOutput(_))), "{refusal:?}");
}

#[test]
fn an_output_past_ten_mebibytes_is_refused() {
    let root = TestDir::new("craft-create-too-big");
    let tools = fake_tools(&root, &[App::WordCraft]);
    let runner = ScriptedRunner::new(|inv| {
        let mut bytes = b"%PDF-1.4\n".to_vec();
        bytes.resize(TEN_MIB + 1, b' ');
        fs::write(&args(inv)[2], bytes).unwrap();
        Ok(ok(""))
    });

    let refusal = Driver::new(&tools, &runner, root.join("work")).create_file(Kind::Pdf, "# x");

    assert!(matches!(refusal, Err(CraftError::BadOutput(_))), "{refusal:?}");
}
