//! Restore points: the folder before and after a turn, the exclusions, the
//! limits and the restore with its undo. Small limits keep the tests quick.

use std::fs;
use std::path::Path;

use excubia_test_dir::TestDir;

use super::paths::{is_point_id, safe_relative};
use super::{store, Kind, Limits, Reason, RestoreError, RestoreStore, Status, POINT_LIMIT};

const AGENT: &str = "w1:p1";

fn small() -> Limits {
    Limits {
        dir_bytes: 4096,
        file_bytes: 4096,
    }
}

fn store_in(dir: &TestDir) -> RestoreStore {
    RestoreStore::with_limits(dir.path().join("restore"), small())
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent");
    }
    fs::write(path, text).expect("write");
}

/// A link to a file outside the folder. Windows may refuse it without the privilege.
fn link_file(target: &Path, link: &Path) -> bool {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(target, link).is_ok()
    }
}

#[test]
fn changes_are_listed_by_status_and_carry_no_content() {
    let dir = TestDir::new("restore-changes");
    let work = dir.path().join("work");
    write(&work.join("keep.txt"), "uno");
    write(&work.join("gone.txt"), "dos");
    write(&work.join("edit.txt"), "tres");
    let store = store_in(&dir);
    let point = store.create_point(AGENT, "turn-1", &work).expect("create");

    write(&work.join("edit.txt"), "tres cambiado");
    write(&work.join("new.txt"), "cuatro");
    fs::remove_file(work.join("gone.txt")).expect("remove");
    let changes = store.changed_files(&point.id).expect("changes");

    let mut seen: Vec<(String, Status)> = changes
        .changes
        .iter()
        .map(|change| (change.path.clone(), change.status))
        .collect();
    seen.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        seen,
        vec![
            ("edit.txt".to_string(), Status::Modified),
            ("gone.txt".to_string(), Status::Deleted),
            ("new.txt".to_string(), Status::Added),
        ]
    );
    let json = serde_json::to_string(&changes).expect("json");
    assert!(
        !json.contains("cambiado"),
        "the listing must not carry content"
    );
}

#[test]
fn restore_brings_the_folder_back_and_its_undo_is_a_point() {
    let dir = TestDir::new("restore-back");
    let work = dir.path().join("work");
    write(&work.join("a.txt"), "uno");
    let store = store_in(&dir);
    let point = store.create_point(AGENT, "turn-1", &work).expect("create");

    write(&work.join("a.txt"), "dos!!");
    write(&work.join("b.txt"), "extra");
    let back = store.restore(&point.id).expect("restore");
    assert_eq!((back.restored, back.removed), (1, 1));
    assert_eq!(fs::read_to_string(work.join("a.txt")).expect("a"), "uno");
    assert!(!work.join("b.txt").exists());

    // The undo rewrites a.txt and b.txt: b.txt is missing from the state it returns to.
    let undone = store.restore(&back.undo_point).expect("undo the restore");
    assert_eq!(undone.restored, 2);
    assert_eq!(fs::read_to_string(work.join("a.txt")).expect("a"), "dos!!");
    assert_eq!(fs::read_to_string(work.join("b.txt")).expect("b"), "extra");
}

#[test]
fn excluded_names_and_large_folders_are_listed_and_never_copied() {
    let dir = TestDir::new("restore-exclude");
    let work = dir.path().join("work");
    write(&work.join("ok.txt"), "visible");
    write(&work.join(".env"), "SECRETO");
    write(&work.join("keys/server.pem"), "SECRETO");
    write(&work.join("node_modules/lib.js"), "SECRETO");
    // Each file fits the file limit; the folder as a whole does not fit the folder limit.
    for part in 0..3 {
        write(&work.join(format!("big/part{part}.bin")), &"x".repeat(2000));
    }
    let store = store_in(&dir);
    let point = store.create_point(AGENT, "turn-1", &work).expect("create");

    let kept: Vec<&str> = point.entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(kept, vec!["ok.txt"]);
    let excluded: Vec<(&str, Reason)> = point
        .excluded
        .iter()
        .map(|e| (e.path.as_str(), e.reason))
        .collect();
    assert!(excluded.contains(&(".env", Reason::Name)));
    assert!(excluded.contains(&("keys/server.pem", Reason::Name)));
    assert!(excluded.contains(&("node_modules", Reason::Name)));
    assert!(excluded.contains(&("big", Reason::TooLarge)));
    for blob in fs::read_dir(dir.path().join("restore/blobs")).expect("blobs") {
        let bytes = fs::read(blob.expect("entry").path()).expect("blob");
        assert!(!bytes.windows(7).any(|w| w == b"SECRETO"));
    }
}

#[test]
fn symlinks_are_listed_and_never_followed() {
    let dir = TestDir::new("restore-links");
    let work = dir.path().join("work");
    write(&work.join("ok.txt"), "visible");
    let outside = dir.path().join("outside.txt");
    write(&outside, "fuera");
    if !link_file(&outside, &work.join("enlace.txt")) {
        eprintln!("skipped: this system does not allow symlinks here");
        return;
    }
    let point = store_in(&dir)
        .create_point(AGENT, "turn-1", &work)
        .expect("create");

    assert!(point
        .excluded
        .iter()
        .any(|e| e.path == "enlace.txt" && e.reason == Reason::Symlink));
    assert!(point.entries.iter().all(|e| e.path != "enlace.txt"));
}

#[test]
fn a_changed_file_with_the_same_size_is_not_taken_for_the_old_one() {
    // Same length and, on a coarse clock, the same modification time: the hash must
    // still come from the new content.
    let dir = TestDir::new("restore-racy");
    let work = dir.path().join("work");
    write(&work.join("a.txt"), "uno");
    let store = store_in(&dir);
    store.create_point(AGENT, "turn-1", &work).expect("first");
    write(&work.join("a.txt"), "dos");
    let second = store.create_point(AGENT, "turn-2", &work).expect("second");

    let entry = second
        .entries
        .iter()
        .find(|e| e.path == "a.txt")
        .expect("a");
    assert_eq!(entry.sha256, store::sha256_hex(b"dos"));
}

#[test]
fn a_manifest_path_outside_the_folder_is_refused_before_any_write() {
    let dir = TestDir::new("restore-escape");
    let work = dir.path().join("work");
    write(&work.join("ok.txt"), "uno");
    let store = store_in(&dir);
    let point = store.create_point(AGENT, "turn-1", &work).expect("create");

    let mut tampered = store::load_point(&dir.path().join("restore"), &point.id)
        .expect("load")
        .expect("point");
    let sha = tampered.entries[0].sha256.clone();
    tampered.entries.push(super::Entry {
        path: "../escape.txt".to_string(),
        sha256: sha,
        size: 3,
        mtime: 0,
    });
    store::save_point(&dir.path().join("restore"), &tampered).expect("save");
    write(&work.join("ok.txt"), "cambio");

    let refused = store.restore(&point.id);
    assert!(matches!(refused, Err(RestoreError::InvalidPath)));
    assert!(!dir.path().join("escape.txt").exists());
    assert_eq!(
        fs::read_to_string(work.join("ok.txt")).expect("ok"),
        "cambio"
    );
}

#[test]
fn safe_paths_are_plain_names_only() {
    assert!(safe_relative("dir/file.txt").is_ok());
    for bad in ["", "../x", "a/../b", "/etc/passwd", "a\\b", "./x"] {
        assert!(safe_relative(bad).is_err(), "{bad} must be refused");
    }
    assert!(is_point_id("01ARZ3NDEKTSV4RRFFQ69G5FAV"));
    assert!(!is_point_id("../../etc/passwd"));
}

#[test]
fn restore_never_removes_an_excluded_file() {
    let dir = TestDir::new("restore-keep-excluded");
    let work = dir.path().join("work");
    write(&work.join("ok.txt"), "uno");
    let store = store_in(&dir);
    let point = store.create_point(AGENT, "turn-1", &work).expect("create");
    write(&work.join(".env"), "clave");

    store.restore(&point.id).expect("restore");
    assert_eq!(fs::read_to_string(work.join(".env")).expect("env"), "clave");
}

#[test]
fn the_newest_hundred_points_are_kept() {
    let dir = TestDir::new("restore-limit");
    let work = dir.path().join("work");
    write(&work.join("ok.txt"), "uno");
    let store = store_in(&dir);
    let first = store.create_point(AGENT, "turn-0", &work).expect("first");
    let mut last = first.clone();
    for turn in 1..=POINT_LIMIT {
        last = store
            .create_point(AGENT, &format!("turn-{turn}"), &work)
            .expect("create");
    }

    let points = store.list_points(AGENT).expect("list");
    assert_eq!(points.len(), POINT_LIMIT);
    assert_eq!(points[0].id, last.id);
    assert!(matches!(
        store.changed_files(&first.id),
        Err(RestoreError::NotFound)
    ));
}

#[test]
fn points_older_than_thirty_days_are_pruned() {
    let dir = TestDir::new("restore-age");
    let work = dir.path().join("work");
    write(&work.join("ok.txt"), "uno");
    let root = dir.path().join("restore");
    let store = store_in(&dir);
    let old = store.create_point(AGENT, "turn-old", &work).expect("old");
    let mut aged = store::load_point(&root, &old.id)
        .expect("load")
        .expect("old");
    aged.created_ms -= 31 * 24 * 60 * 60 * 1000;
    store::save_point(&root, &aged).expect("save");

    let fresh = store.create_point(AGENT, "turn-new", &work).expect("new");
    let ids: Vec<String> = store
        .list_points(AGENT)
        .expect("list")
        .into_iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(ids, vec![fresh.id]);
}

#[test]
fn unknown_and_malformed_ids_are_not_found() {
    let dir = TestDir::new("restore-ids");
    let store = store_in(&dir);
    assert!(matches!(
        store.changed_files("../../etc"),
        Err(RestoreError::NotFound)
    ));
    assert!(matches!(
        store.restore("01ARZ3NDEKTSV4RRFFQ69G5FAZ"),
        Err(RestoreError::NotFound)
    ));
}

#[test]
fn a_point_of_one_agent_is_not_listed_for_another() {
    let dir = TestDir::new("restore-agents");
    let work = dir.path().join("work");
    write(&work.join("ok.txt"), "uno");
    let store = store_in(&dir);
    store.create_point("w1:p1", "turn-a", &work).expect("a");
    let other = store.create_point("w2:p1", "turn-b", &work).expect("b");

    let mine = store.list_points("w2:p1").expect("list");
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].id, other.id);
    assert_eq!(mine[0].kind, Kind::Turn);
}
