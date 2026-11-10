mod support;

use excubia_core_crafting::{App, Captured, CraftError, Driver};
use excubia_core_test_dir::TestDir;
use std::fs;
use support::{
    args, document, failed, fake_tools, ok, ooxml_zip, png_bytes, shape, value_after,
    ScriptedRunner, CSV, DOCX, PDF, XLSX,
};

/// Writes a PNG wherever the invocation asks for one (`--out` or a positional path).
fn png_to(out: &str, bytes: &[u8]) -> Result<Captured, CraftError> {
    fs::write(out, bytes).unwrap();
    Ok(ok(""))
}

#[test]
fn pdf_renders_the_page_through_pdfcraft() {
    let root = TestDir::new("craft-render-pdf");
    let tools = fake_tools(&root, &[App::PdfCraft]);
    let runner = ScriptedRunner::new(|inv| match args(inv)[0].as_str() {
        "text" => Ok(ok("uno\x0cdos\x0ctres")),
        _ => png_to(&value_after(inv, "--out"), &png_bytes()),
    });
    let doc = document(&root, "factura.pdf", PDF, b"%PDF-1.4\n");

    let preview = Driver::new(&tools, &runner, root.join("work"))
        .render_png(&doc, 3)
        .unwrap();

    assert_eq!(preview.png, png_bytes());
    assert_eq!(preview.pages, Some(3), "the count comes from PdfCraft's text");
    let calls = runner.calls();
    assert_eq!(
        shape(&calls[0]),
        [
            "render",
            "<ws>/factura.pdf",
            "--page",
            "3",
            "--out",
            "<ws>/page.png"
        ]
    );
    assert_eq!(calls[0].app, App::PdfCraft);
}

#[test]
fn docx_renders_through_wordcraft_with_the_output_as_a_positional_argument() {
    let root = TestDir::new("craft-render-docx");
    let tools = fake_tools(&root, &[App::WordCraft]);
    let runner = ScriptedRunner::new(|inv| {
        let out = args(inv)[2].clone();
        png_to(&out, &png_bytes())
    });
    let doc = document(&root, "Carta.docx", DOCX, &ooxml_zip());

    let preview = Driver::new(&tools, &runner, root.join("work"))
        .render_png(&doc, 1)
        .unwrap();

    assert_eq!(preview.pages, None, "no page count is known for Word");
    assert_eq!(
        shape(&runner.calls()[0]),
        ["render", "<ws>/Carta.docx", "<ws>/page.png", "--page", "1"]
    );
}

#[test]
fn xlsx_is_converted_to_pdf_first_and_then_rendered() {
    let root = TestDir::new("craft-render-xlsx");
    let tools = fake_tools(&root, &[App::GridCraft, App::PdfCraft]);
    let runner = ScriptedRunner::new(|inv| match inv.app {
        App::GridCraft => {
            let out = args(inv)[2].clone();
            fs::write(&out, b"%PDF-1.4\n").unwrap();
            Ok(ok(""))
        }
        _ if args(inv)[0] == "text" => Ok(ok("uno\x0cdos")),
        _ => png_to(&value_after(inv, "--out"), &png_bytes()),
    });
    let doc = document(&root, "Hoja.xlsx", XLSX, &ooxml_zip());

    let preview = Driver::new(&tools, &runner, root.join("work"))
        .render_png(&doc, 1)
        .unwrap();

    assert_eq!(preview.png, png_bytes());
    assert_eq!(preview.pages, Some(2), "the converted PDF has two pages");
    let calls = runner.calls();
    assert_eq!(
        shape(&calls[0]),
        ["convert", "<ws>/Hoja.xlsx", "<ws>/preview.pdf"]
    );
    assert_eq!(calls[0].app, App::GridCraft);
    assert_eq!(
        shape(&calls[1]),
        [
            "render",
            "<ws>/preview.pdf",
            "--page",
            "1",
            "--out",
            "<ws>/page.png"
        ]
    );
    assert_eq!(calls[1].app, App::PdfCraft);
}

#[test]
fn a_missing_second_app_is_named_as_missing() {
    let root = TestDir::new("craft-render-half");
    let tools = fake_tools(&root, &[App::GridCraft]);
    let runner = ScriptedRunner::new(|inv| {
        fs::write(args(inv)[2].clone(), b"%PDF-1.4\n").unwrap();
        Ok(ok(""))
    });
    let doc = document(&root, "Hoja.xlsx", XLSX, &ooxml_zip());

    let err = Driver::new(&tools, &runner, root.join("work"))
        .render_png(&doc, 1)
        .unwrap_err();

    assert_eq!(
        err.to_string(),
        "Falta PdfCraft en este PC. El Master puede instalarla desde Herramientas de documentos"
    );
}

/// A two-page PDF: the page count is what PdfCraft's text prints, one form feed between pages.
fn two_page_text(inv: &excubia_core_crafting::Invocation) -> Result<Captured, CraftError> {
    if args(inv)[0] == "text" {
        Ok(ok("uno\x0cdos"))
    } else {
        Ok(failed("no such page"))
    }
}

#[test]
fn a_page_past_the_end_is_out_of_range() {
    let root = TestDir::new("craft-render-range");
    let tools = fake_tools(&root, &[App::PdfCraft]);
    let runner = ScriptedRunner::new(two_page_text);
    let doc = document(&root, "factura.pdf", PDF, b"%PDF-1.4\n");

    let err = Driver::new(&tools, &runner, root.join("work"))
        .render_png(&doc, 5)
        .unwrap_err();

    assert!(matches!(err, CraftError::PageOutOfRange(5)), "{err:?}");
    assert_eq!(err.to_string(), "La página 5 no existe en el documento");
}

#[test]
fn a_failed_render_of_a_page_that_exists_keeps_its_error() {
    let root = TestDir::new("craft-render-broken");
    let tools = fake_tools(&root, &[App::PdfCraft]);
    let runner = ScriptedRunner::new(two_page_text);
    let doc = document(&root, "factura.pdf", PDF, b"%PDF-1.4\n");

    let err = Driver::new(&tools, &runner, root.join("work"))
        .render_png(&doc, 2)
        .unwrap_err();

    assert!(
        matches!(err, CraftError::Failed(App::PdfCraft, _)),
        "{err:?}"
    );
}

#[test]
fn an_image_over_8_mib_is_refused() {
    let root = TestDir::new("craft-render-big");
    let tools = fake_tools(&root, &[App::PdfCraft]);
    let huge = vec![0u8; 8 * 1024 * 1024 + 1];
    let runner = ScriptedRunner::new(move |inv| png_to(&value_after(inv, "--out"), &huge));
    let doc = document(&root, "factura.pdf", PDF, b"%PDF-1.4\n");

    let result = Driver::new(&tools, &runner, root.join("work")).render_png(&doc, 1);

    assert!(matches!(result, Err(CraftError::BadOutput(_))));
}

#[test]
fn an_output_that_is_not_png_is_refused() {
    let root = TestDir::new("craft-render-nopng");
    let tools = fake_tools(&root, &[App::PdfCraft]);
    let runner = ScriptedRunner::new(|inv| png_to(&value_after(inv, "--out"), b"GIF89a-nope"));
    let doc = document(&root, "factura.pdf", PDF, b"%PDF-1.4\n");

    let result = Driver::new(&tools, &runner, root.join("work")).render_png(&doc, 1);

    assert!(matches!(result, Err(CraftError::BadOutput(_))));
}

#[test]
fn a_cli_that_writes_no_image_is_refused() {
    let root = TestDir::new("craft-render-none");
    let tools = fake_tools(&root, &[App::PdfCraft]);
    let runner = ScriptedRunner::new(|_| Ok(ok("")));
    let doc = document(&root, "factura.pdf", PDF, b"%PDF-1.4\n");

    let result = Driver::new(&tools, &runner, root.join("work")).render_png(&doc, 1);

    assert!(matches!(result, Err(CraftError::BadOutput(_))));
}

#[test]
fn formats_without_a_renderer_are_unsupported() {
    let root = TestDir::new("craft-render-csv");
    let tools = fake_tools(&root, &[App::PdfCraft]);
    let runner = ScriptedRunner::new(|_| Ok(ok("")));
    let doc = document(&root, "datos.csv", CSV, b"a,b\n");

    let result = Driver::new(&tools, &runner, root.join("work")).render_png(&doc, 1);

    assert!(matches!(result, Err(CraftError::Unsupported(_))));
    assert!(runner.calls().is_empty());
}

#[test]
fn page_zero_is_an_invalid_option() {
    let root = TestDir::new("craft-render-p0");
    let tools = fake_tools(&root, &[App::PdfCraft]);
    let runner = ScriptedRunner::new(|_| Ok(ok("")));
    let doc = document(&root, "factura.pdf", PDF, b"%PDF-1.4\n");

    let result = Driver::new(&tools, &runner, root.join("work")).render_png(&doc, 0);

    assert!(matches!(result, Err(CraftError::Invalid(_))));
    assert!(runner.calls().is_empty());
}
