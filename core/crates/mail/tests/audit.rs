mod support;

use std::fs;
use std::path::PathBuf;

use excubia_mail::{AuditRecord, CheckedFile, Decision};
use support::{Harness, ACCOUNT, DRAFT_ID};

fn file() -> CheckedFile {
    CheckedFile {
        path: PathBuf::from("plano.xlsx"),
        size: 5,
        sha256: "ab".repeat(32),
    }
}

#[test]
fn each_record_is_one_json_line_with_only_the_audit_fields() {
    let h = Harness::new("audit-lines");
    h.audit
        .append(&AuditRecord::new(
            ACCOUNT,
            DRAFT_ID,
            &file(),
            Decision::Auto,
        ))
        .expect("first record");
    h.audit
        .append(&AuditRecord::new(
            "personal",
            "draft-2",
            &file(),
            Decision::Denied,
        ))
        .expect("second record");

    let text = fs::read_to_string(h.audit.path()).expect("audit file");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2);
    for line in lines {
        let value: serde_json::Value = serde_json::from_str(line).expect("json line");
        let mut keys: Vec<&str> = value
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["account", "decision", "draft_id", "path", "sha256", "size", "ts"]
        );
    }
}

#[test]
fn the_decision_is_written_in_lowercase() {
    let h = Harness::new("audit-decision");
    h.audit
        .append(&AuditRecord::new(
            ACCOUNT,
            DRAFT_ID,
            &file(),
            Decision::Approved,
        ))
        .expect("record");
    let value: serde_json::Value =
        serde_json::from_str(fs::read_to_string(h.audit.path()).expect("file").trim())
            .expect("json");
    assert_eq!(value["decision"], "approved");
}

#[cfg(unix)]
#[test]
fn the_audit_file_is_owner_only() {
    use std::os::unix::fs::PermissionsExt;

    let h = Harness::new("audit-mode");
    h.audit
        .append(&AuditRecord::new(
            ACCOUNT,
            DRAFT_ID,
            &file(),
            Decision::Auto,
        ))
        .expect("record");
    let mode = fs::metadata(h.audit.path())
        .expect("metadata")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}
