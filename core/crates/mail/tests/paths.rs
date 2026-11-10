mod support;

use std::fs;
use std::path::Path;

use excubia_core_mail::{
    CheckedFile, MailError, PathApproval, PathDecision, PathPolicy, DEFAULT_MAX_ATTACHMENT_BYTES,
    HARD_MAX_ATTACHMENT_BYTES,
};
use support::{sha256_hex, sparse_file, Harness};

const MIB: u64 = 1024 * 1024;

fn needs_approval(result: Result<PathDecision, MailError>) -> PathApproval {
    match result.expect("a decision") {
        PathDecision::NeedsApproval(pending) => pending,
        other => panic!("expected NeedsApproval, got {other:?}"),
    }
}

fn assert_rejected(result: Result<PathDecision, MailError>) {
    match result {
        Err(MailError::PathRejected(_)) => {}
        other => panic!("expected PathRejected, got {other:?}"),
    }
}

/// Creates a link at `link` pointing to `target`. False when this account may not create links.
fn try_symlink(target: &Path, link: &Path) -> bool {
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(target, link);
    #[cfg(windows)]
    let made = std::os::windows::fs::symlink_file(target, link);
    made.is_ok()
}

#[test]
fn a_file_under_the_root_is_allowed() {
    let h = Harness::new("paths-inside");
    let file = h.root.join("ventas.xlsx");
    fs::write(&file, b"datos").expect("file");

    match h.policy().decide(&file).expect("a decision") {
        PathDecision::Allowed(checked) => {
            assert_eq!(checked.size, 5);
            assert_eq!(checked.file_name(), Some("ventas.xlsx"));
        }
        other => panic!("expected Allowed, got {other:?}"),
    }
}

#[test]
fn a_file_outside_the_root_needs_approval_and_its_digest_follows_the_bytes() {
    let h = Harness::new("paths-digest");
    let file = h.outside.join("informe.pdf");
    fs::write(&file, b"contenido uno").expect("file");
    let policy = h.policy();

    let first = needs_approval(policy.decide(&file)).digest;
    let again = needs_approval(policy.decide(&file)).digest;
    assert_eq!(first, again, "the same path and bytes keep their digest");

    fs::write(&file, b"contenido dos").expect("change the file");
    let changed = needs_approval(policy.decide(&file)).digest;
    assert_ne!(first, changed, "one changed byte is a new approval");
}

#[test]
fn a_dot_dot_that_leaves_the_root_is_resolved_and_needs_approval() {
    let h = Harness::new("paths-dotdot");
    fs::write(h.outside.join("nota.txt"), b"x").expect("file");
    let requested = h.root.join("..").join("outside").join("nota.txt");

    let pending = needs_approval(h.policy().decide(&requested));
    assert!(
        !pending.file.path.to_string_lossy().contains(".."),
        "the canonical path must not keep the dot-dot: {:?}",
        pending.file.path
    );
    assert_eq!(pending.file.file_name(), Some("nota.txt"));
}

#[test]
fn a_directory_is_refused() {
    let h = Harness::new("paths-dir");
    let folder = h.root.join("carpeta");
    fs::create_dir(&folder).expect("folder");
    assert_rejected(h.policy().decide(&folder));
}

#[test]
fn an_empty_file_is_refused() {
    let h = Harness::new("paths-empty");
    let file = h.root.join("vacio.txt");
    fs::write(&file, b"").expect("file");
    assert_rejected(h.policy().decide(&file));
}

#[test]
fn a_file_over_the_default_limit_is_refused_without_being_read() {
    let h = Harness::new("paths-over");
    let file = h.root.join("grande.bin");
    sparse_file(&file, DEFAULT_MAX_ATTACHMENT_BYTES + 1);
    assert_rejected(h.policy().decide(&file));
}

#[test]
fn a_file_at_the_default_limit_is_accepted() {
    let h = Harness::new("paths-at-limit");
    let file = h.root.join("justo.bin");
    sparse_file(&file, DEFAULT_MAX_ATTACHMENT_BYTES);
    match h.policy().decide(&file).expect("a decision") {
        PathDecision::Allowed(checked) => assert_eq!(checked.size, 25 * MIB),
        other => panic!("expected Allowed, got {other:?}"),
    }
}

#[test]
fn the_hard_ceiling_cannot_be_configured_away() {
    let h = Harness::new("paths-hard");
    assert!(matches!(
        PathPolicy::new(&h.root, HARD_MAX_ATTACHMENT_BYTES + 1),
        Err(MailError::InvalidInput(_))
    ));

    let file = h.root.join("enorme.bin");
    sparse_file(&file, HARD_MAX_ATTACHMENT_BYTES + 1);
    let policy = PathPolicy::new(&h.root, HARD_MAX_ATTACHMENT_BYTES).expect("policy");
    assert_rejected(policy.decide(&file));
}

#[test]
fn a_missing_workspaces_root_is_a_configuration_error() {
    let h = Harness::new("paths-root");
    let missing = h.dir.join("no-existe");
    assert!(matches!(
        PathPolicy::new(&missing, DEFAULT_MAX_ATTACHMENT_BYTES),
        Err(MailError::InvalidConfig(_))
    ));
}

#[test]
fn a_missing_file_is_refused() {
    let h = Harness::new("paths-missing");
    assert_rejected(h.policy().decide(&h.root.join("no-existe.xlsx")));
}

#[test]
fn a_device_is_refused() {
    let h = Harness::new("paths-device");
    let device = if cfg!(windows) { "NUL" } else { "/dev/null" };
    assert_rejected(h.policy().decide(Path::new(device)));
}

#[test]
fn a_symbolic_link_that_leaves_the_root_is_refused() {
    let h = Harness::new("paths-symlink");
    let target = h.outside.join("secreto.txt");
    fs::write(&target, b"x").expect("target");
    let link = h.root.join("enlace.txt");
    if !try_symlink(&target, &link) {
        eprintln!("skipped: this account cannot create symbolic links");
        return;
    }
    assert_rejected(h.policy().decide(&link));
}

#[cfg(windows)]
#[test]
fn a_junction_that_leaves_the_root_is_refused() {
    let h = Harness::new("paths-junction");
    let target = h.outside.join("carpeta");
    fs::create_dir(&target).expect("target folder");
    fs::write(target.join("doc.pdf"), b"x").expect("file");
    let junction = h.root.join("union");
    let made = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&junction)
        .arg(&target)
        .status();
    if !made.is_ok_and(|status| status.success()) {
        eprintln!("skipped: mklink /J is not available here");
        return;
    }
    assert_rejected(h.policy().decide(&junction.join("doc.pdf")));
}

#[test]
fn a_digest_already_approved_is_allowed() {
    let h = Harness::new("paths-approved");
    let file = h.outside.join("aprobado.pdf");
    fs::write(&file, b"ok").expect("file");
    let digest = needs_approval(h.policy().decide(&file)).digest;

    let policy = h.policy().with_approved([digest]);
    assert!(matches!(
        policy.decide(&file).expect("a decision"),
        PathDecision::Allowed(_)
    ));
}

/// The trusted path: a file the host resolved from the chat. It keeps the checks
/// that do not depend on where the file is, and never asks for an approval.
fn trusted_rejected(result: Result<CheckedFile, MailError>) {
    match result {
        Err(MailError::PathRejected(_)) => {}
        other => panic!("expected PathRejected, got {other:?}"),
    }
}

#[test]
fn a_trusted_file_outside_the_root_is_checked_and_allowed_without_approval() {
    let h = Harness::new("paths-trusted");
    let file = h.outside.join("adjunto.bin");
    fs::write(&file, b"contenido").expect("file");

    let checked = h.policy().decide_trusted(&file).expect("checked");
    assert_eq!(checked.size, 9);
    assert_eq!(checked.sha256, sha256_hex(b"contenido"));
}

#[test]
fn a_trusted_file_keeps_the_refusals_that_do_not_depend_on_its_place() {
    let h = Harness::new("paths-trusted-refused");
    let folder = h.outside.join("carpeta");
    fs::create_dir(&folder).expect("folder");
    trusted_rejected(h.policy().decide_trusted(&folder));

    let empty = h.outside.join("vacio.txt");
    fs::write(&empty, b"").expect("file");
    trusted_rejected(h.policy().decide_trusted(&empty));

    let big = h.outside.join("grande.bin");
    fs::write(&big, b"12345").expect("file");
    let small_limit = PathPolicy::new(&h.root, 4).expect("policy");
    trusted_rejected(small_limit.decide_trusted(&big));

    let device = if cfg!(windows) { "NUL" } else { "/dev/null" };
    trusted_rejected(h.policy().decide_trusted(Path::new(device)));
}
