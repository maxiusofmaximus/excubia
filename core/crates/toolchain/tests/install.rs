//! The installer end to end, with a simulated distribution: a zip that holds a copy
//! of the `fake-gh` fixture as `bin/gh.exe` or `cmd/git.exe`. The real probe runs
//! it, so the version check is a real process. No network: the fetcher copies a
//! file.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use excubia_test_dir::TestDir;
use excubia_toolchain::install::{sha256_hex, InstallError, Installer, Outcome};
use excubia_toolchain::{Distribution, ExecProbe, Fetch, Layout, Status};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

const FAKE_GH: &str = env!("CARGO_BIN_EXE_fake-gh");

/// Serves a fixture file as the download, or fails on purpose.
enum Serve {
    File(PathBuf),
    Fails(&'static str),
}

impl Fetch for Serve {
    fn fetch(&self, _url: &str, _max: u64, to: &Path) -> Result<(), String> {
        match self {
            Serve::File(from) => fs::copy(from, to).map(|_| ()).map_err(|e| e.to_string()),
            Serve::Fails(reason) => Err((*reason).to_string()),
        }
    }
}

fn zip_of(dir: &Path, name: &str, entries: &[(&str, &[u8])]) -> PathBuf {
    let path = dir.join(name);
    let mut writer = ZipWriter::new(File::create(&path).expect("create zip"));
    for (entry, bytes) in entries {
        writer
            .start_file(*entry, SimpleFileOptions::default())
            .expect("start");
        writer.write_all(bytes).expect("write");
    }
    writer.finish().expect("finish");
    path
}

/// The fixture binary, as bytes, placed at `entry`.
fn fake_exe_zip(dir: &Path, name: &str, entry: &str) -> PathBuf {
    let bytes = fs::read(FAKE_GH).expect("fake-gh is built with the tests");
    zip_of(dir, name, &[(entry, &bytes)])
}

/// A distribution that points at `zip`, with its real size and digest.
fn fixture_dist(base: &Distribution, zip: &Path) -> Distribution {
    let mut dist = base.clone();
    dist.size = fs::metadata(zip).expect("zip").len();
    dist.sha256 = sha256_hex(zip).expect("digest");
    dist
}

/// No staging, no previous copy and no lock left in the profile's toolchain folder.
fn nothing_left_over(layout: &Layout) -> bool {
    let staging_empty = fs::read_dir(layout.root().join(".staging"))
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(true);
    let no_previous = fs::read_dir(layout.root()).map_or(true, |entries| {
        entries
            .flatten()
            .all(|entry| !entry.file_name().to_string_lossy().starts_with(".old-"))
    });
    staging_empty && no_previous
}

fn installer<'a>(layout: &'a Layout, fetch: &'a dyn Fetch) -> Installer<'a> {
    Installer {
        layout,
        fetch,
        probe: &ExecProbe,
    }
}

#[test]
fn a_pinned_gh_installs_and_reports_its_version_then_is_a_no_op() {
    let dir = TestDir::new("install-happy");
    let zip = fake_exe_zip(&dir, "gh.zip", "bin/gh.exe");
    let dist = fixture_dist(&Distribution::gh(), &zip);
    let layout = Layout::new(&dir.join("data"));
    let fetch = Serve::File(zip);

    let first = installer(&layout, &fetch).install(&dist).expect("install");
    assert_eq!(first, Outcome::Installed { version: "2.105.0".into() });
    assert_eq!(
        layout.status(&dist),
        Status::Ready {
            version: "2.105.0".into()
        }
    );
    assert!(layout.exe(&dist).is_file());

    // The second call finds the verified copy and fetches nothing.
    let second = installer(&layout, &Serve::Fails("must not fetch"))
        .install(&dist)
        .expect("second");
    assert_eq!(second, Outcome::AlreadyInstalled { version: "2.105.0".into() });
    assert!(nothing_left_over(&layout));
}

#[test]
fn git_and_gh_install_into_their_fixed_folders_on_the_path() {
    let dir = TestDir::new("install-both");
    let layout = Layout::new(&dir.join("data"));
    let git_zip = fake_exe_zip(&dir, "git.zip", "cmd/git.exe");
    let gh_zip = fake_exe_zip(&dir, "gh.zip", "bin/gh.exe");
    let git = fixture_dist(&Distribution::git(), &git_zip);
    let gh = fixture_dist(&Distribution::gh(), &gh_zip);

    installer(&layout, &Serve::File(git_zip)).install(&git).expect("git");
    installer(&layout, &Serve::File(gh_zip)).install(&gh).expect("gh");

    let dirs = layout.path_dirs(&[git.clone(), gh.clone()]);
    assert!(dirs[0].join("git.exe").is_file());
    assert!(dirs[1].join("gh.exe").is_file());
    assert!(matches!(layout.status(&git), Status::Ready { .. }));
    assert!(matches!(layout.status(&gh), Status::Ready { .. }));
}

#[test]
fn a_wrong_digest_installs_nothing_and_leaves_no_staging() {
    let dir = TestDir::new("install-digest");
    let zip = fake_exe_zip(&dir, "gh.zip", "bin/gh.exe");
    let mut dist = fixture_dist(&Distribution::gh(), &zip);
    dist.sha256 = "0".repeat(64);
    let layout = Layout::new(&dir.join("data"));

    let error = installer(&layout, &Serve::File(zip)).install(&dist).expect_err("digest");
    assert_eq!(error.code(), "digest_mismatch");
    assert!(matches!(error, InstallError::Digest { .. }));
    assert_eq!(layout.status(&dist), Status::Missing);
    assert!(nothing_left_over(&layout));
}

#[test]
fn a_download_that_fails_or_has_the_wrong_size_installs_nothing() {
    let dir = TestDir::new("install-download");
    let zip = fake_exe_zip(&dir, "gh.zip", "bin/gh.exe");
    let dist = fixture_dist(&Distribution::gh(), &zip);
    let layout = Layout::new(&dir.join("data"));

    let error = installer(&layout, &Serve::Fails("HTTP 503"))
        .install(&dist)
        .expect_err("download");
    assert_eq!(error.code(), "download_failed");

    let mut wrong = dist.clone();
    wrong.size += 1;
    let error = installer(&layout, &Serve::File(zip))
        .install(&wrong)
        .expect_err("size");
    assert_eq!(error.code(), "size_mismatch");
    assert_eq!(layout.status(&dist), Status::Missing);
}

#[test]
fn an_archive_without_the_executable_is_a_layout_failure() {
    let dir = TestDir::new("install-layout");
    let zip = zip_of(&dir, "empty.zip", &[("readme.txt", b"no exe here")]);
    let dist = fixture_dist(&Distribution::gh(), &zip);
    let layout = Layout::new(&dir.join("data"));
    let error = installer(&layout, &Serve::File(zip)).install(&dist).expect_err("layout");
    assert_eq!(error.code(), "layout_missing");
    assert_eq!(layout.status(&dist), Status::Missing);
}

#[test]
fn an_executable_that_reports_another_version_is_refused() {
    let dir = TestDir::new("install-version");
    let zip = fake_exe_zip(&dir, "gh.zip", "bin/gh.exe");
    let mut dist = fixture_dist(&Distribution::gh(), &zip);
    dist.probe_marker = "gh version 9.9.9".into();
    let layout = Layout::new(&dir.join("data"));
    let error = installer(&layout, &Serve::File(zip)).install(&dist).expect_err("version");
    assert_eq!(error.code(), "version_mismatch");
    assert_eq!(layout.status(&dist), Status::Missing);
}

#[test]
fn a_damaged_archive_is_an_extraction_failure() {
    let dir = TestDir::new("install-extract");
    let zip = dir.join("damaged.zip");
    fs::write(&zip, b"PK not really a zip").expect("write");
    let dist = fixture_dist(&Distribution::gh(), &zip);
    let layout = Layout::new(&dir.join("data"));
    let error = installer(&layout, &Serve::File(zip)).install(&dist).expect_err("extract");
    assert_eq!(error.code(), "extract_failed");
}

#[test]
fn a_folder_left_by_a_broken_run_is_replaced_by_the_verified_copy() {
    let dir = TestDir::new("install-broken");
    let zip = fake_exe_zip(&dir, "gh.zip", "bin/gh.exe");
    let dist = fixture_dist(&Distribution::gh(), &zip);
    let layout = Layout::new(&dir.join("data"));
    let stale = layout.tool_dir(&dist).join("bin");
    fs::create_dir_all(&stale).expect("stale");
    fs::write(stale.join("gh.exe"), b"old, unverified").expect("stale exe");

    assert!(matches!(layout.status(&dist), Status::Broken { .. }));
    installer(&layout, &Serve::File(zip)).install(&dist).expect("replace");
    assert!(matches!(layout.status(&dist), Status::Ready { .. }));
    assert!(nothing_left_over(&layout));
}

#[test]
fn a_second_install_while_one_runs_is_busy_and_touches_nothing() {
    let dir = TestDir::new("install-busy");
    let zip = fake_exe_zip(&dir, "gh.zip", "bin/gh.exe");
    let dist = fixture_dist(&Distribution::gh(), &zip);
    let layout = Layout::new(&dir.join("data"));
    fs::create_dir_all(layout.root()).expect("root");
    // The other run holds the live lock on the same file until it ends, however slow it is.
    let other = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(layout.root().join(".install.lock"))
        .expect("lock");
    other.try_lock().expect("the other run takes the lock");

    let error = installer(&layout, &Serve::File(zip.clone())).install(&dist).expect_err("busy");
    assert_eq!(error.code(), "busy");
    assert!(layout.root().join(".install.lock").is_file(), "the other run keeps its lock file");
    assert_eq!(layout.status(&dist), Status::Missing);

    // When the other run ends, the lock is free again and the install goes through.
    other.unlock().expect("release");
    drop(other);
    installer(&layout, &Serve::File(zip)).install(&dist).expect("installs after the other run");
    assert!(matches!(layout.status(&dist), Status::Ready { .. }));
    assert!(nothing_left_over(&layout));
}
