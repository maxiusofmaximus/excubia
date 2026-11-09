mod support;

use hn_core_crafting::{ooxml, CraftError};
use support::{ooxml_zip, zip};

const MIB: u32 = 1024 * 1024;

fn is_bad(bytes: &[u8]) -> bool {
    matches!(ooxml::check(bytes), Err(CraftError::BadDocument(_)))
}

#[test]
fn a_minimal_package_passes() {
    assert!(ooxml::check(&ooxml_zip()).is_ok());
}

#[test]
fn a_package_without_content_types_is_rejected() {
    assert!(is_bad(&zip(&[("xl/workbook.xml", 10)])));
}

#[test]
fn bytes_that_are_not_a_zip_are_rejected() {
    assert!(is_bad(b""));
    assert!(is_bad(b"PK no es un zip, solo texto con firma PK"));
}

#[test]
fn a_zip_cut_before_its_directory_end_is_rejected() {
    let mut bytes = ooxml_zip();
    bytes.truncate(bytes.len() - 5);
    assert!(is_bad(&bytes));
}

#[test]
fn more_than_5000_entries_are_rejected() {
    let names: Vec<String> = (0..5001).map(|i| format!("part{i}.xml")).collect();
    let mut entries: Vec<(&str, u32)> = vec![("[Content_Types].xml", 10)];
    entries.extend(names.iter().map(|name| (name.as_str(), 1)));
    assert!(is_bad(&zip(&entries)));
}

#[test]
fn declared_sizes_above_200_mib_in_total_are_rejected() {
    let big = [
        ("[Content_Types].xml", 10),
        ("xl/a.xml", 150 * MIB),
        ("xl/b.xml", 60 * MIB),
    ];
    assert!(is_bad(&zip(&big)));
    // Exactly 200 MiB declared in total still passes; the boundary is inclusive.
    let fits = [
        ("[Content_Types].xml", 10),
        ("xl/a.xml", 150 * MIB),
        ("xl/b.xml", 50 * MIB - 10),
    ];
    assert!(ooxml::check(&zip(&fits)).is_ok());
}

#[test]
fn names_that_climb_or_are_absolute_are_rejected() {
    assert!(is_bad(&zip(&[
        ("[Content_Types].xml", 10),
        ("../evil.xml", 1)
    ])));
    assert!(is_bad(&zip(&[
        ("[Content_Types].xml", 10),
        ("/evil.xml", 1)
    ])));
    assert!(is_bad(&zip(&[
        ("[Content_Types].xml", 10),
        ("\\evil.xml", 1)
    ])));
}
