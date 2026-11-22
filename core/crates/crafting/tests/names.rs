use excubia_crafting::safe_name;

#[test]
fn directories_and_traversal_are_dropped() {
    assert_eq!(safe_name("../../etc/passwd.xlsx", "xlsx"), "passwd.xlsx");
    assert_eq!(
        safe_name("C:\\Users\\Max\\Informe.xlsx", "xlsx"),
        "Informe.xlsx"
    );
    assert_eq!(safe_name("..\\..\\x.docx", "docx"), "x.docx");
}

#[test]
fn characters_windows_forbids_become_underscores() {
    assert_eq!(safe_name("a:b*c?.pdf", "pdf"), "a_b_c_.pdf");
    assert_eq!(safe_name("nota\u{7}.csv", "csv"), "nota_.csv");
}

#[test]
fn dots_never_survive_in_the_stem() {
    assert_eq!(
        safe_name("informe.final.xlsx", "xlsx"),
        "informe_final.xlsx"
    );
    assert_eq!(safe_name("..xlsx", "xlsx"), "_.xlsx");
}

#[test]
fn reserved_windows_names_get_a_prefix() {
    assert_eq!(safe_name("CON.xlsx", "xlsx"), "_CON.xlsx");
    assert_eq!(safe_name("lpt1.pdf", "pdf"), "_lpt1.pdf");
    assert_eq!(safe_name("COM10.pdf", "pdf"), "COM10.pdf");
}

#[test]
fn the_extension_is_the_one_the_cli_needs() {
    assert_eq!(safe_name("Datos.XLSX", "xlsx"), "Datos.xlsx");
    assert_eq!(safe_name("sin extension", "docx"), "sin extension.docx");
}

#[test]
fn names_are_bounded_and_never_empty() {
    let long = format!("{}.xlsx", "n".repeat(300));
    assert_eq!(safe_name(&long, "xlsx").chars().count(), 80 + ".xlsx".len());
    assert_eq!(safe_name(".xlsx", "xlsx"), "documento.xlsx");
    assert_eq!(safe_name("   .xlsx", "xlsx"), "documento.xlsx");
    assert_eq!(safe_name("a b .xlsx", "xlsx"), "a b.xlsx");
}
