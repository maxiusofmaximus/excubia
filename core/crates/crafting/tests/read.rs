mod support;

use excubia_core_crafting::{App, CraftError, Driver, ReadOpts, MAX_OUTPUT_BYTES};
use excubia_core_test_dir::TestDir;
use support::{
    args, document, failed, fake_tools, ok, ooxml_zip, shape, ScriptedRunner, CSV, DOCX, JSON, PDF,
    TXT, XLSX,
};

const SHEETS: &str = r#"{"sheets":[{"name":"Hoja1"},{"name":"Resumen"}]}"#;

fn grid_reply(
    inv: &excubia_core_crafting::Invocation,
) -> Result<excubia_core_crafting::Captured, CraftError> {
    if args(inv)[0] == "info" {
        Ok(ok(SHEETS))
    } else {
        Ok(ok("a,b\n1,2\n"))
    }
}

#[test]
fn xlsx_lists_the_sheets_and_reads_the_first_one_in_200_rows() {
    let root = TestDir::new("craft-read-xlsx");
    let tools = fake_tools(&root, &[App::GridCraft]);
    let runner = ScriptedRunner::new(grid_reply);
    let doc = document(&root, "Presupuesto.xlsx", XLSX, &ooxml_zip());

    let out = Driver::new(&tools, &runner, root.join("work"))
        .read_text(&doc, &ReadOpts::default())
        .unwrap();

    assert_eq!(out.text, "a,b\n1,2\n");
    assert_eq!(out.pages_or_sheets, ["Hoja1", "Resumen"]);
    let calls = runner.calls();
    assert_eq!(
        shape(&calls[0]),
        ["info", "<ws>/Presupuesto.xlsx", "--json"]
    );
    assert_eq!(
        shape(&calls[1]),
        [
            "cat",
            "<ws>/Presupuesto.xlsx",
            "--sheet",
            "Hoja1",
            "--range",
            "A1:ZZ200",
            "--csv"
        ]
    );
    assert_eq!(calls[1].program, tools.path(App::GridCraft).unwrap());
}

#[test]
fn an_explicit_sheet_and_range_are_passed_as_they_are() {
    let root = TestDir::new("craft-read-range");
    let tools = fake_tools(&root, &[App::GridCraft]);
    let runner = ScriptedRunner::new(grid_reply);
    let doc = document(&root, "Presupuesto.xlsx", XLSX, &ooxml_zip());
    let opts = ReadOpts {
        sheet: Some("Resumen".into()),
        range: Some("B2:C9".into()),
        page: None,
    };

    Driver::new(&tools, &runner, root.join("work"))
        .read_text(&doc, &opts)
        .unwrap();

    let calls = runner.calls();
    assert_eq!(
        shape(&calls[1]),
        [
            "cat",
            "<ws>/Presupuesto.xlsx",
            "--sheet",
            "Resumen",
            "--range",
            "B2:C9",
            "--csv"
        ]
    );
}

#[test]
fn the_cli_runs_inside_the_working_folder_on_the_staged_copy() {
    let root = TestDir::new("craft-read-cwd");
    let tools = fake_tools(&root, &[App::GridCraft]);
    let runner = ScriptedRunner::new(grid_reply);
    let doc = document(&root, "Presupuesto.xlsx", XLSX, &ooxml_zip());

    Driver::new(&tools, &runner, root.join("work"))
        .read_text(&doc, &ReadOpts::default())
        .unwrap();

    let call = &runner.calls()[1];
    let input = std::path::Path::new(&args(call)[1]).to_path_buf();
    assert_eq!(input.parent(), Some(call.cwd.as_path()));
    assert!(
        !input.exists(),
        "the staged copy is removed when the read ends"
    );
}

#[test]
fn docx_uses_the_text_command() {
    let root = TestDir::new("craft-read-docx");
    let tools = fake_tools(&root, &[App::WordCraft]);
    let runner = ScriptedRunner::new(|_| Ok(ok("Hola mundo")));
    let doc = document(&root, "Carta.docx", DOCX, &ooxml_zip());

    let out = Driver::new(&tools, &runner, root.join("work"))
        .read_text(&doc, &ReadOpts::default())
        .unwrap();

    assert_eq!(out.text, "Hola mundo");
    assert!(out.pages_or_sheets.is_empty());
    assert_eq!(shape(&runner.calls()[0]), ["text", "<ws>/Carta.docx"]);
}

#[test]
fn pdf_lists_its_pages_and_takes_one_when_asked() {
    let root = TestDir::new("craft-read-pdf");
    let tools = fake_tools(&root, &[App::PdfCraft]);
    let runner = ScriptedRunner::new(|_| Ok(ok("uno\u{c}dos")));
    let doc = document(&root, "factura.pdf", PDF, b"%PDF-1.4\n");
    let driver = Driver::new(&tools, &runner, root.join("work"));

    let all = driver.read_text(&doc, &ReadOpts::default()).unwrap();
    let second = driver
        .read_text(
            &doc,
            &ReadOpts {
                page: Some(2),
                ..ReadOpts::default()
            },
        )
        .unwrap();

    assert_eq!(all.pages_or_sheets, ["1", "2"]);
    assert_eq!(shape(&runner.calls()[0]), ["text", "<ws>/factura.pdf"]);
    assert_eq!(second.pages_or_sheets, ["2"]);
    assert_eq!(
        shape(&runner.calls()[1]),
        ["text", "<ws>/factura.pdf", "--page", "2"]
    );
}

#[test]
fn csv_is_read_directly_without_any_tool() {
    let root = TestDir::new("craft-read-csv");
    let tools = fake_tools(&root, &[]);
    let runner = ScriptedRunner::new(|_| Ok(ok("")));
    let doc = document(&root, "datos.csv", CSV, "nombre,valor\nhola,1\n".as_bytes());

    let out = Driver::new(&tools, &runner, root.join("work"))
        .read_text(&doc, &ReadOpts::default())
        .unwrap();

    assert_eq!(out.text, "nombre,valor\nhola,1\n");
    assert!(!out.truncated);
    assert!(runner.calls().is_empty());
}

#[test]
fn plain_text_is_cut_at_the_output_limit_on_a_character_boundary() {
    let root = TestDir::new("craft-read-cut");
    let tools = fake_tools(&root, &[]);
    let runner = ScriptedRunner::new(|_| Ok(ok("")));
    let body = "á".repeat(MAX_OUTPUT_BYTES);
    let doc = document(&root, "notas.md", support::MD, body.as_bytes());

    let out = Driver::new(&tools, &runner, root.join("work"))
        .read_text(&doc, &ReadOpts::default())
        .unwrap();

    assert!(out.truncated);
    assert_eq!(out.text.len(), MAX_OUTPUT_BYTES);
    assert!(out.text.chars().all(|c| c == 'á'));
}

#[test]
fn plain_text_that_is_not_utf8_is_a_bad_document() {
    let root = TestDir::new("craft-read-latin");
    let tools = fake_tools(&root, &[]);
    let runner = ScriptedRunner::new(|_| Ok(ok("")));
    let doc = document(&root, "viejo.txt", TXT, &[0x63, 0x61, 0xF1, 0x61]);

    let result =
        Driver::new(&tools, &runner, root.join("work")).read_text(&doc, &ReadOpts::default());

    assert!(matches!(result, Err(CraftError::BadDocument(_))));
    assert!(runner.calls().is_empty());
}

#[test]
fn json_is_plain_text_too() {
    let root = TestDir::new("craft-read-json");
    let tools = fake_tools(&root, &[]);
    let runner = ScriptedRunner::new(|_| Ok(ok("")));
    let doc = document(&root, "config.json", JSON, br#"{"a":1}"#);

    let out = Driver::new(&tools, &runner, root.join("work"))
        .read_text(&doc, &ReadOpts::default())
        .unwrap();

    assert_eq!(out.text, r#"{"a":1}"#);
}

#[test]
fn pptx_has_no_reader_yet() {
    let root = TestDir::new("craft-read-pptx");
    let tools = fake_tools(&root, &[]);
    let runner = ScriptedRunner::new(|_| Ok(ok("")));
    let doc = document(
        &root,
        "diapos.pptx",
        "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        &ooxml_zip(),
    );

    let result =
        Driver::new(&tools, &runner, root.join("work")).read_text(&doc, &ReadOpts::default());

    assert!(matches!(result, Err(CraftError::Unsupported(_))));
    assert!(runner.calls().is_empty());
}

#[test]
fn an_office_file_that_is_not_a_package_never_reaches_a_cli() {
    let root = TestDir::new("craft-read-fake");
    let tools = fake_tools(&root, &[App::GridCraft]);
    let runner = ScriptedRunner::new(grid_reply);
    let doc = document(&root, "falso.xlsx", XLSX, b"<html>no</html>");

    let result =
        Driver::new(&tools, &runner, root.join("work")).read_text(&doc, &ReadOpts::default());

    assert!(matches!(result, Err(CraftError::BadDocument(_))));
    assert!(runner.calls().is_empty());
}

#[test]
fn a_missing_app_says_how_to_get_it_and_runs_nothing() {
    let root = TestDir::new("craft-read-missing");
    let tools = fake_tools(&root, &[]);
    let runner = ScriptedRunner::new(grid_reply);
    let doc = document(&root, "Presupuesto.xlsx", XLSX, &ooxml_zip());

    let result =
        Driver::new(&tools, &runner, root.join("work")).read_text(&doc, &ReadOpts::default());

    let err = result.unwrap_err();
    assert!(matches!(err, CraftError::ToolMissing(App::GridCraft)));
    assert_eq!(
        err.to_string(),
        "Falta GridCraft en este PC. El Master puede instalarla desde Herramientas de documentos"
    );
    assert!(runner.calls().is_empty());
}

#[test]
fn page_zero_is_an_invalid_option() {
    let root = TestDir::new("craft-read-page0");
    let tools = fake_tools(&root, &[App::PdfCraft]);
    let runner = ScriptedRunner::new(|_| Ok(ok("")));
    let doc = document(&root, "factura.pdf", PDF, b"%PDF-1.4\n");

    let result = Driver::new(&tools, &runner, root.join("work")).read_text(
        &doc,
        &ReadOpts {
            page: Some(0),
            ..ReadOpts::default()
        },
    );

    assert!(matches!(result, Err(CraftError::Invalid(_))));
    assert!(runner.calls().is_empty());
}

#[test]
fn a_failing_cli_reports_a_short_message_without_paths() {
    let root = TestDir::new("craft-read-fail");
    let tools = fake_tools(&root, &[App::GridCraft]);
    let runner = ScriptedRunner::new(|inv| {
        if args(inv)[0] == "info" {
            Ok(ok(SHEETS))
        } else {
            Ok(failed(&format!(
                "no se pudo abrir {}: {}",
                inv.cwd.display(),
                "x".repeat(400)
            )))
        }
    });
    let doc = document(&root, "Presupuesto.xlsx", XLSX, &ooxml_zip());

    let err = Driver::new(&tools, &runner, root.join("work"))
        .read_text(&doc, &ReadOpts::default())
        .unwrap_err();

    let CraftError::Failed(app, detail) = err else {
        panic!("expected Failed, got another error")
    };
    assert_eq!(app, App::GridCraft);
    assert!(detail.starts_with("no se pudo abrir"));
    assert!(detail.chars().count() <= 200);
    let cwd = runner.calls()[0].cwd.clone();
    assert!(!detail.contains(&cwd.display().to_string()));
    assert!(!detail.contains(&cwd.display().to_string().replace('\\', "/")));
}

#[test]
fn a_timeout_is_passed_through() {
    let root = TestDir::new("craft-read-timeout");
    let tools = fake_tools(&root, &[App::GridCraft]);
    let runner = ScriptedRunner::new(|inv| {
        if args(inv)[0] == "info" {
            Ok(ok(SHEETS))
        } else {
            Err(CraftError::Timeout(App::GridCraft))
        }
    });
    let doc = document(&root, "Presupuesto.xlsx", XLSX, &ooxml_zip());

    let result =
        Driver::new(&tools, &runner, root.join("work")).read_text(&doc, &ReadOpts::default());

    assert!(matches!(result, Err(CraftError::Timeout(App::GridCraft))));
}

#[test]
fn truncation_reported_by_the_runner_reaches_the_caller() {
    let root = TestDir::new("craft-read-trunc");
    let tools = fake_tools(&root, &[App::WordCraft]);
    let runner = ScriptedRunner::new(|_| {
        Ok(excubia_core_crafting::Captured {
            truncated: true,
            ..ok("parcial")
        })
    });
    let doc = document(&root, "Carta.docx", DOCX, &ooxml_zip());

    let out = Driver::new(&tools, &runner, root.join("work"))
        .read_text(&doc, &ReadOpts::default())
        .unwrap();

    assert!(out.truncated);
}
