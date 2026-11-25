//! The GitHub link against the `fake-gh` fixture: a real process per call, real
//! stderr, real kill. The tools are installed from simulated archives on the first
//! start, so the whole path is exercised. No account, no network.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use excubia_test_dir::TestDir;
use excubia_toolchain::github::{LinkManager, Phase, ProcessRunner, Start, DEVICE_URL};
use excubia_toolchain::install::{sha256_hex, ExecProbe};
use excubia_toolchain::{Distribution, Fetch, Layout};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

const FAKE_GH: &str = env!("CARGO_BIN_EXE_fake-gh");

/// Serves each fixture archive by its fake URL.
struct ByUrl(HashMap<String, PathBuf>);

impl Fetch for ByUrl {
    fn fetch(&self, url: &str, _max: u64, to: &Path) -> Result<(), String> {
        let from = self.0.get(url).ok_or("unknown url")?;
        fs::copy(from, to).map(|_| ()).map_err(|e| e.to_string())
    }
}

fn fixture_zip(dir: &Path, name: &str, entry: &str) -> PathBuf {
    let path = dir.join(name);
    let bytes = fs::read(FAKE_GH).expect("fake-gh is built with the tests");
    let mut writer = ZipWriter::new(File::create(&path).expect("zip"));
    writer
        .start_file(entry, SimpleFileOptions::default())
        .expect("start");
    writer.write_all(&bytes).expect("write");
    writer.finish().expect("finish");
    path
}

struct Harness {
    _dir: TestDir,
    layout: Layout,
    dists: [Distribution; 2],
    fetch: ByUrl,
}

fn harness(name: &str) -> Harness {
    let dir = TestDir::new(name);
    let git_zip = fixture_zip(&dir, "git.zip", "cmd/git.exe");
    let gh_zip = fixture_zip(&dir, "gh.zip", "bin/gh.exe");
    let mut git = Distribution::git();
    git.url = "fixture://git".into();
    git.size = fs::metadata(&git_zip).expect("meta").len();
    git.sha256 = sha256_hex(&git_zip).expect("digest");
    let mut gh = Distribution::gh();
    gh.url = "fixture://gh".into();
    gh.size = fs::metadata(&gh_zip).expect("meta").len();
    gh.sha256 = sha256_hex(&gh_zip).expect("digest");
    let fetch = ByUrl(HashMap::from([
        ("fixture://git".to_string(), git_zip),
        ("fixture://gh".to_string(), gh_zip),
    ]));
    let layout = Layout::new(&dir.join("data"));
    Harness {
        _dir: dir,
        layout,
        dists: [git, gh],
        fetch,
    }
}

impl Harness {
    /// A manager whose fake CLI behaves as `env` says. No trusted PATH tools: the machine's own
    /// Git and gh never stand in for the fixtures.
    fn manager(&self, env: &[(&str, &str)], deadline: Duration) -> LinkManager {
        self.manager_on(env, deadline, Vec::new())
    }

    /// As [`Harness::manager`], with `system` as the trusted PATH searched for Git and gh.
    fn manager_on(&self, env: &[(&str, &str)], deadline: Duration, system: Vec<PathBuf>) -> LinkManager {
        let extra = env
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        LinkManager::new(
            self.layout.clone(),
            Arc::new(ByUrl(self.fetch.0.clone())),
            Arc::new(ExecProbe),
            Arc::new(ProcessRunner::with_extra_env(extra)),
        )
        .with_distributions(self.dists.clone())
        .with_deadline(deadline)
        .with_system_path(system)
    }

    /// A folder outside the profile that holds `exe` as a working copy of the fixture.
    fn system_bin(&self, folder: &str, exes: &[&str]) -> PathBuf {
        let dir = self._dir.join(folder);
        fs::create_dir_all(&dir).expect("system folder");
        for exe in exes {
            fs::copy(FAKE_GH, dir.join(exe)).expect("system copy");
        }
        dir
    }

    fn gitconfig(&self) -> String {
        fs::read_to_string(self.layout.gitconfig()).unwrap_or_default()
    }
}

/// A fake CLI with no session yet, so a login runs.
const NO_SESSION: (&str, &str) = ("FAKE_GH_STATUS", "none");

fn wait_for(manager: &LinkManager, done: impl Fn(&excubia_toolchain::github::View) -> bool) {
    let started = Instant::now();
    loop {
        if let Some(view) = manager.view() {
            if done(&view) {
                return;
            }
        }
        assert!(started.elapsed() < Duration::from_secs(30), "the link did not reach the state");
        thread::sleep(Duration::from_millis(20));
    }
}

fn terminal(view: &excubia_toolchain::github::View) -> bool {
    view.phase.is_terminal()
}

#[test]
fn a_fresh_profile_links_and_writes_only_the_profile_git_file() {
    let h = harness("link-happy");
    let manager = h.manager(&[NO_SESSION, ("FAKE_GH_LOGIN", "ok")], Duration::from_secs(30));
    assert_eq!(manager.start("github.com"), Ok(Start::Started));
    wait_for(&manager, terminal);

    let view = manager.view().expect("view");
    assert_eq!(view.phase, Phase::Linked { login: "octo".into() });
    assert_eq!(view.code, None, "the code is gone once the link ends");
    assert_eq!(view.url, DEVICE_URL);

    let config = h.gitconfig();
    assert!(config.contains("user.name = Octo Cat"), "{config}");
    assert!(config.contains("user.email = 4242+octo@users.noreply.github.com"), "{config}");
    assert!(!config.contains("private@example.com"), "the real email is never stored");
}

#[test]
fn a_fragmented_stderr_still_ends_in_one_verified_link() {
    let h = harness("link-fragmented");
    let manager = h.manager(&[NO_SESSION, ("FAKE_GH_LOGIN", "fragmented")], Duration::from_secs(30));
    manager.start("github.com").expect("start");
    wait_for(&manager, terminal);
    assert_eq!(manager.view().expect("view").phase, Phase::Linked { login: "octo".into() });
}

#[test]
fn the_code_is_shown_while_the_login_waits_and_is_dropped_on_cancel() {
    let h = harness("link-cancel");
    let manager = h.manager(&[NO_SESSION, ("FAKE_GH_LOGIN", "hang")], Duration::from_secs(60));
    manager.start("github.com").expect("start");
    wait_for(&manager, |view| view.phase == Phase::CodeReady);

    let shown = manager.view().expect("view");
    assert_eq!(shown.code.as_deref(), Some("WXYZ-9876"));
    assert_eq!(shown.url, DEVICE_URL);

    assert!(manager.cancel(), "a running link can be cancelled");
    let after = manager.view().expect("view");
    assert_eq!(after.phase, Phase::Cancelled);
    assert_eq!(after.code, None, "a cancelled link keeps no code");
    assert!(!manager.cancel(), "nothing is running any more");
}

#[test]
fn a_second_start_while_the_login_runs_joins_it_and_starts_nothing_new() {
    let h = harness("link-dedup");
    let manager = h.manager(&[NO_SESSION, ("FAKE_GH_LOGIN", "hang")], Duration::from_secs(60));
    assert_eq!(manager.start("github.com"), Ok(Start::Started));
    wait_for(&manager, |view| view.phase == Phase::CodeReady);
    match manager.start("github.com").expect("second start") {
        Start::Running(view) => {
            assert_eq!(view.phase, Phase::CodeReady);
            assert_eq!(view.code.as_deref(), Some("WXYZ-9876"));
        }
        Start::Started => panic!("a second start must not begin another link"),
    }
    manager.cancel();
}

#[test]
fn a_login_that_fails_leaves_no_code_and_no_identity() {
    let h = harness("link-fail");
    let manager = h.manager(&[NO_SESSION, ("FAKE_GH_LOGIN", "fail")], Duration::from_secs(30));
    manager.start("github.com").expect("start");
    wait_for(&manager, terminal);
    let view = manager.view().expect("view");
    assert_eq!(
        view.phase,
        Phase::Failed {
            step: "login",
            code: "login_failed"
        }
    );
    assert_eq!(view.code, None);
    assert!(!h.gitconfig().contains("user.name"));
}

#[test]
fn a_code_that_is_never_used_expires_and_the_login_is_killed() {
    let h = harness("link-expire");
    let manager = h.manager(&[NO_SESSION, ("FAKE_GH_LOGIN", "hang")], Duration::from_millis(800));
    manager.start("github.com").expect("start");
    wait_for(&manager, terminal);
    let view = manager.view().expect("view");
    assert_eq!(view.phase, Phase::Expired);
    assert_eq!(view.code, None);
    assert!(!h.gitconfig().contains("user.name"));
}

#[test]
fn a_login_whose_session_cannot_be_read_is_not_a_success() {
    let h = harness("link-api");
    let manager = h.manager(&[NO_SESSION, ("FAKE_GH_LOGIN", "ok"), ("FAKE_GH_API", "fail")], Duration::from_secs(30));
    manager.start("github.com").expect("start");
    wait_for(&manager, terminal);
    assert_eq!(
        manager.view().expect("view").phase,
        Phase::Failed {
            step: "session",
            code: "api_failed"
        }
    );
    assert!(!h.gitconfig().contains("user.name"));
}

#[test]
fn a_failed_setup_keeps_the_login_and_a_retry_needs_no_second_login() {
    let h = harness("link-setup");
    let first = h.manager(&[NO_SESSION, ("FAKE_GH_LOGIN", "ok"), ("FAKE_GH_SETUP", "fail")], Duration::from_secs(30));
    first.start("github.com").expect("start");
    wait_for(&first, terminal);
    assert_eq!(
        first.view().expect("view").phase,
        Phase::SetupFailed {
            login: "octo".into(),
            code: "setup_git_failed"
        }
    );

    // The session now works, and a login would fail: the retry must not run one.
    let retry = h.manager(
        &[("FAKE_GH_STATUS", "ok"), ("FAKE_GH_LOGIN", "fail"), ("FAKE_GH_SETUP", "ok")],
        Duration::from_secs(30),
    );
    retry.start("github.com").expect("retry");
    wait_for(&retry, terminal);
    assert_eq!(retry.view().expect("view").phase, Phase::Linked { login: "octo".into() });
}

#[test]
fn a_session_that_already_works_is_reused_without_a_login() {
    let h = harness("link-reuse");
    let manager = h.manager(&[("FAKE_GH_STATUS", "ok"), ("FAKE_GH_LOGIN", "fail")], Duration::from_secs(30));
    manager.start("github.com").expect("start");
    wait_for(&manager, terminal);
    assert_eq!(manager.view().expect("view").phase, Phase::Linked { login: "octo".into() });
}

#[test]
fn a_working_git_on_the_trusted_path_is_used_and_only_gh_is_installed() {
    let h = harness("link-system-git");
    let bin = h.system_bin("system-git", &["git.exe"]);
    let manager = h.manager_on(&[NO_SESSION, ("FAKE_GH_LOGIN", "ok")], Duration::from_secs(30), vec![bin]);
    manager.start("github.com").expect("start");
    wait_for(&manager, terminal);
    assert_eq!(manager.view().expect("view").phase, Phase::Linked { login: "octo".into() });
    assert!(matches!(h.layout.status(&h.dists[0]), excubia_toolchain::Status::Missing), "git was not downloaded");
    assert!(matches!(h.layout.status(&h.dists[1]), excubia_toolchain::Status::Ready { .. }));
    assert!(h.gitconfig().contains("user.name = Octo Cat"), "the identity still goes to the profile file");
}

#[test]
fn working_git_and_gh_on_the_trusted_path_mean_no_download_at_all() {
    let h = harness("link-system-both");
    let bin = h.system_bin("system-both", &["git.exe", "gh.exe"]);
    let manager = h.manager_on(&[NO_SESSION, ("FAKE_GH_LOGIN", "ok")], Duration::from_secs(30), vec![bin]);
    manager.start("github.com").expect("start");
    wait_for(&manager, terminal);
    assert_eq!(manager.view().expect("view").phase, Phase::Linked { login: "octo".into() });
    assert!(matches!(h.layout.status(&h.dists[0]), excubia_toolchain::Status::Missing));
    assert!(matches!(h.layout.status(&h.dists[1]), excubia_toolchain::Status::Missing));
}

#[test]
fn the_card_names_only_the_tools_that_are_really_missing() {
    let h = harness("link-missing-names");
    let clean = h.manager_on(&[], Duration::from_secs(30), Vec::new());
    assert_eq!(clean.missing_tools(), vec!["git", "gh"]);

    let gh_only = h.system_bin("only-git", &["git.exe"]);
    let one = h.manager_on(&[], Duration::from_secs(30), vec![gh_only]);
    assert_eq!(one.missing_tools(), vec!["gh"]);

    let both = h.system_bin("both", &["git.exe", "gh.exe"]);
    let all = h.manager_on(&[], Duration::from_secs(30), vec![both]);
    assert!(all.missing_tools().is_empty());
}

#[test]
fn a_session_without_the_setup_is_not_ready_until_the_retry_completes_it() {
    let h = harness("link-ready-pending");
    let pending = h.manager(
        &[("FAKE_GH_STATUS", "ok"), ("FAKE_GH_SETUP", "fail")],
        Duration::from_secs(30),
    );
    pending.start("github.com").expect("start");
    wait_for(&pending, terminal);
    assert_eq!(
        pending.view().expect("view").phase,
        Phase::SetupFailed { login: "octo".into(), code: "setup_git_failed" }
    );
    assert!(!pending.session_ready(), "a valid session with pending setup is not ready");

    // The retry reuses the session (a login would fail) and completes the setup.
    let retry = h.manager(
        &[("FAKE_GH_STATUS", "ok"), ("FAKE_GH_LOGIN", "fail"), ("FAKE_GH_SETUP", "ok")],
        Duration::from_secs(30),
    );
    retry.start("github.com").expect("retry");
    wait_for(&retry, terminal);
    assert_eq!(retry.view().expect("view").phase, Phase::Linked { login: "octo".into() });
    assert!(retry.session_ready(), "the completed setup is ready");
}

#[test]
fn the_installed_tools_are_on_the_profile_path_after_a_link() {
    let h = harness("link-tools");
    let manager = h.manager(&[NO_SESSION, ("FAKE_GH_LOGIN", "ok")], Duration::from_secs(30));
    manager.start("github.com").expect("start");
    wait_for(&manager, terminal);
    assert!(matches!(h.layout.status(&h.dists[1]), excubia_toolchain::Status::Ready { .. }));
    assert!(matches!(h.layout.status(&h.dists[0]), excubia_toolchain::Status::Ready { .. }));
    let env = excubia_toolchain::github::ChildEnv::for_profile(&h.layout, &h.dists);
    let path = env.vars().iter().find(|(k, _)| k == "PATH").map(|(_, v)| v.clone()).expect("PATH");
    assert!(path.starts_with(&h.layout.path_dirs(&h.dists)[0].to_string_lossy().to_string()));

}
