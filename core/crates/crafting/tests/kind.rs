mod support;

use hn_core_crafting::{CraftError, Kind};
use support::{CSV, DOCX, JSON, MD, PDF, TXT, XLSX};

#[test]
fn the_extension_and_the_mime_pick_the_kind() {
    assert_eq!(Kind::of("Informe.xlsx", XLSX).unwrap(), Kind::Xlsx);
    assert_eq!(Kind::of("carta.DOCX", DOCX).unwrap(), Kind::Docx);
    assert_eq!(Kind::of("factura.pdf", PDF).unwrap(), Kind::Pdf);
    assert_eq!(Kind::of("datos.csv", CSV).unwrap(), Kind::Csv);
    assert_eq!(Kind::of("notas.txt", TXT).unwrap(), Kind::Txt);
    assert_eq!(Kind::of("notas.md", MD).unwrap(), Kind::Md);
    assert_eq!(Kind::of("config.json", JSON).unwrap(), Kind::Json);
}

#[test]
fn formats_without_a_reader_are_unsupported() {
    for name in [
        "diapos.pptx",
        "macro.xlsm",
        "viejo.xls",
        "viejo.doc",
        "sinextension",
    ] {
        assert!(
            matches!(Kind::of(name, XLSX), Err(CraftError::Unsupported(_))),
            "{name}"
        );
    }
}

#[test]
fn a_mime_that_disagrees_with_the_extension_is_a_bad_document() {
    assert!(matches!(
        Kind::of("informe.xlsx", PDF),
        Err(CraftError::BadDocument(_))
    ));
}

#[test]
fn only_office_packages_are_checked_as_zip() {
    assert!(Kind::Xlsx.is_ooxml());
    assert!(Kind::Docx.is_ooxml());
    assert!(!Kind::Pdf.is_ooxml());
    assert!(!Kind::Csv.is_ooxml());
}
