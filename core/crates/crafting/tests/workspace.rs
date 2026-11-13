mod support;

use excubia_crafting::{CraftError, Kind, Workspace};
use excubia_test_dir::TestDir;
use std::fs;
use support::{document, ooxml_zip, CSV, XLSX};

#[test]
fn stage_copies_under_a_safe_name_and_drop_removes_the_folder() {
    let root = TestDir::new("craft-ws-copy");
    let doc = document(&root, "../../Informe final.xlsx", XLSX, &ooxml_zip());
    let ws = Workspace::create(&root.join("work")).unwrap();
    let dir = ws.path().to_path_buf();

    let staged = ws.stage(&doc, Kind::Xlsx).unwrap();

    assert_eq!(staged, ws.join("Informe final.xlsx"));
    assert_eq!(fs::read(&staged).unwrap(), ooxml_zip());
    drop(ws);
    assert!(!dir.exists());
}

#[test]
fn stage_refuses_an_office_file_that_is_not_a_valid_package() {
    let root = TestDir::new("craft-ws-bad");
    let doc = document(&root, "falso.xlsx", XLSX, b"no soy un zip");
    let ws = Workspace::create(&root.join("work")).unwrap();

    let result = ws.stage(&doc, Kind::Xlsx);

    assert!(matches!(result, Err(CraftError::BadDocument(_))));
    assert!(!ws.join("falso.xlsx").exists());
}

#[test]
fn plain_text_is_not_checked_as_a_package() {
    let root = TestDir::new("craft-ws-csv");
    let doc = document(&root, "datos.csv", CSV, b"a,b\n1,2\n");
    let ws = Workspace::create(&root.join("work")).unwrap();

    let staged = ws.stage(&doc, Kind::Csv).unwrap();

    assert_eq!(fs::read(staged).unwrap(), b"a,b\n1,2\n");
}

#[test]
fn files_over_10_mib_are_refused() {
    let root = TestDir::new("craft-ws-big");
    let too_big = vec![b'a'; 10 * 1024 * 1024 + 1];
    let doc = document(&root, "grande.csv", CSV, &too_big);
    let ws = Workspace::create(&root.join("work")).unwrap();

    assert!(matches!(
        ws.stage(&doc, Kind::Csv),
        Err(CraftError::BadDocument(_))
    ));
}
