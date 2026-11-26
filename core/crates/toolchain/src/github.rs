//! Linking one GitHub account to this profile: the card «¿Enlazar GitHub con Excubia?».
//!
//! The host runs `gh auth login` itself, with fixed arguments, as a managed
//! background process. Only the one-time code is kept, in memory, while the login
//! waits for the user; it never reaches logs, the database or the agent. After a
//! successful login the account is verified with `gh api`, and the Git helper and
//! the no-reply identity are written to the profile's Git file, never to the user's.
//!
//! At most one link runs at a time. A second start while one runs returns the
//! running one. Nothing here retries a failed step on its own.

use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::fetch::Fetch;
use crate::install::{Installer, Probe};
use crate::layout::{Layout, Status};
use crate::manifest::{pinned, Distribution};

pub const HOST: &str = "github.com";
/// The only page the card opens. Fixed; never taken from the output of the CLI.
pub const DEVICE_URL: &str = "https://github.com/login/device";
/// The arguments of the login, fixed: web flow, no clipboard (gh copies the code by default,
/// `--clipboard=false` keeps it out), no insecure storage.
pub const LOGIN_ARGS: [&str; 9] = [
    "auth",
    "login",
    "--web",
    "--hostname",
    HOST,
    "--git-protocol",
    "https",
    "--skip-ssh-key",
    "--clipboard=false",
];
/// GitHub's device codes expire after 15 minutes.
pub const DEFAULT_DEADLINE: Duration = Duration::from_secs(15 * 60);
/// Git and gh, by the executable names a trusted PATH entry holds them under (`Context::dists` order).
const TOOL_EXES: [&str; 2] = ["git", "gh"];

/// gh 2.93 prints `one-time code: XXXX-XXXX`; gh 2.105 prints `One-time code (XXXX-XXXX) copied to clipboard`.
const CODE_PREFIX: &str = "one-time code:";
const COPIED_CODE_PREFIX: &str = "One-time code (";
const MAX_BUFFER: usize = 8 * 1024;
const KEEP_TAIL: usize = 256;
const POLL: Duration = Duration::from_millis(200);
const CALL_TIMEOUT: Duration = Duration::from_secs(60);
const EXIT_WAIT: Duration = Duration::from_secs(30);
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// The only inherited variables. Everything else is dropped, including any GitHub token.
const INHERITED: &[&str] = &[
    "SYSTEMROOT",
    "SYSTEMDRIVE",
    "WINDIR",
    "PATHEXT",
    "TEMP",
    "TMP",
    "USERPROFILE",
    "USERNAME",
    "COMPUTERNAME",
    "HOMEDRIVE",
    "HOMEPATH",
    "APPDATA",
    "LOCALAPPDATA",
    "PROCESSOR_ARCHITECTURE",
    "NUMBER_OF_PROCESSORS",
];

/// The environment a toolchain process gets: a short allowlist, the fixed PATH, and the
/// profile's own Git and gh configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChildEnv {
    vars: Vec<(String, String)>,
}

impl ChildEnv {
    pub fn for_profile(layout: &Layout, dists: &[Distribution]) -> Self {
        let mut vars: Vec<(String, String)> = INHERITED
            .iter()
            .filter_map(|key| {
                std::env::var_os(key).map(|value| (key.to_string(), value.to_string_lossy().into_owned()))
            })
            .collect();
        let mut path: Vec<PathBuf> = layout.path_dirs(dists);
        if let Some(current) = std::env::var_os("PATH") {
            path.extend(std::env::split_paths(&current));
        }
        if let Ok(joined) = std::env::join_paths(path) {
            vars.push(("PATH".into(), joined.to_string_lossy().into_owned()));
        }
        vars.extend(layout.profile_env());
        Self { vars }
    }

    pub fn vars(&self) -> &[(String, String)] {
        &self.vars
    }
}

/// What the login process produced on stderr since the last look.
#[derive(Debug, PartialEq, Eq)]
pub enum Fragment {
    Data(Vec<u8>),
    Idle,
    Closed,
}

/// A running login. Shared between the flow and a cancel, so every method takes `&self`.
pub trait LoginChild: Send + Sync {
    /// The next stderr fragment, waiting at most `wait`.
    fn next(&self, wait: Duration) -> Fragment;
    /// Waits for the exit code. `None` when it was killed or did not end in time.
    fn exit_code(&self) -> Option<i32>;
    /// Kills the process if it still runs. Safe to call twice.
    fn kill(&self);
}

/// How the link starts processes. Production uses [`ProcessRunner`]; tests may use a fake.
pub trait Runner: Send + Sync {
    fn spawn_login(&self, gh: &Path, env: &ChildEnv) -> io::Result<Arc<dyn LoginChild>>;
    /// Runs to completion and returns stdout on exit 0.
    fn run(&self, exe: &Path, args: &[&str], env: &ChildEnv, timeout: Duration) -> Result<String, String>;
}

/// The real runner: `std::process::Command`, no shell, the allowlisted environment.
#[derive(Debug, Default)]
pub struct ProcessRunner {
    /// Extra variables for every child. Empty in production; test fixtures use it to pick a scenario.
    extra: Vec<(String, String)>,
}

impl ProcessRunner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_extra_env(extra: Vec<(String, String)>) -> Self {
        Self { extra }
    }

    fn command(&self, program: &Path, args: &[&str], env: &ChildEnv) -> Command {
        let mut command = Command::new(program);
        command
            .args(args)
            .env_clear()
            .envs(env.vars().iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .envs(self.extra.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stdin(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        command
    }
}

impl Runner for ProcessRunner {
    fn spawn_login(&self, gh: &Path, env: &ChildEnv) -> io::Result<Arc<dyn LoginChild>> {
        let mut command = self.command(gh, &LOGIN_ARGS, env);
        // stdout is discarded: nothing the CLI prints there is read or kept.
        command.stdout(Stdio::null()).stderr(Stdio::piped());
        let mut child = command.spawn()?;
        let mut stderr = child.stderr.take().ok_or_else(|| io::Error::other("no stderr"))?;
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        thread::spawn(move || {
            let mut buffer = [0u8; 512];
            loop {
                match stderr.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => {
                        if tx.send(buffer[..read].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        Ok(Arc::new(ProcessLogin {
            child: Mutex::new(child),
            fragments: Mutex::new(rx),
        }))
    }

    fn run(&self, exe: &Path, args: &[&str], env: &ChildEnv, timeout: Duration) -> Result<String, String> {
        let mut command = self.command(exe, args, env);
        command.stdout(Stdio::piped()).stderr(Stdio::null());
        let mut child = command.spawn().map_err(|error| format!("start: {error}"))?;
        // Read stdout on its own thread so a large answer cannot block the child.
        let mut stdout = child.stdout.take().ok_or("no stdout")?;
        let reader = thread::spawn(move || {
            let mut text = String::new();
            let _ = stdout.read_to_string(&mut text);
            text
        });
        let started = Instant::now();
        loop {
            match child.try_wait().map_err(|error| format!("wait: {error}"))? {
                Some(status) => {
                    let text = reader.join().unwrap_or_default();
                    return if status.success() {
                        Ok(text)
                    } else {
                        Err(format!("exit {}", status.code().unwrap_or(-1)))
                    };
                }
                None if started.elapsed() >= timeout => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("timed out".into());
                }
                None => thread::sleep(Duration::from_millis(25)),
            }
        }
    }
}

struct ProcessLogin {
    child: Mutex<Child>,
    fragments: Mutex<mpsc::Receiver<Vec<u8>>>,
}

impl LoginChild for ProcessLogin {
    fn next(&self, wait: Duration) -> Fragment {
        let fragments = lock(&self.fragments);
        match fragments.recv_timeout(wait) {
            Ok(bytes) => Fragment::Data(bytes),
            Err(mpsc::RecvTimeoutError::Timeout) => Fragment::Idle,
            Err(mpsc::RecvTimeoutError::Disconnected) => Fragment::Closed,
        }
    }

    fn exit_code(&self) -> Option<i32> {
        let started = Instant::now();
        loop {
            {
                let mut child = lock(&self.child);
                match child.try_wait() {
                    Ok(Some(status)) => return status.code(),
                    Ok(None) => {}
                    Err(_) => return None,
                }
            }
            if started.elapsed() >= EXIT_WAIT {
                self.kill();
                return None;
            }
            thread::sleep(Duration::from_millis(25));
        }
    }

    fn kill(&self) {
        let mut child = lock(&self.child);
        if matches!(child.try_wait(), Ok(None)) {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for ProcessLogin {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Reassembles the one-time code from stderr fragments that may split anywhere.
#[derive(Debug, Default)]
pub struct CodeCollector {
    buffer: String,
}

impl CodeCollector {
    /// Adds a fragment; returns the code once a complete, valid one has arrived.
    pub fn push(&mut self, bytes: &[u8]) -> Option<String> {
        self.buffer.push_str(&String::from_utf8_lossy(bytes));
        if let Some(code) = parse_code(&self.buffer) {
            self.buffer.clear();
            return Some(code);
        }
        if self.buffer.len() > MAX_BUFFER {
            let mut cut = self.buffer.len() - KEEP_TAIL;
            while !self.buffer.is_char_boundary(cut) {
                cut += 1;
            }
            self.buffer.drain(..cut);
        }
        None
    }
}

/// The code of either gh form, when it has the GitHub shape `XXXX-XXXX`. The copied form ends at `)`,
/// so a fragment that stops before it waits for more.
pub fn parse_code(text: &str) -> Option<String> {
    let plain = text.find(CODE_PREFIX).map(|at| {
        text[at + CODE_PREFIX.len()..]
            .trim_start_matches([' ', '\t'])
            .chars()
            .take_while(|c| !c.is_whitespace())
            .collect::<String>()
    });
    let copied = text
        .find(COPIED_CODE_PREFIX)
        .and_then(|at| text[at + COPIED_CODE_PREFIX.len()..].split_once(')'))
        .map(|(token, _)| token.to_string());
    plain.into_iter().chain(copied).find(|token| is_code(token))
}

fn is_code(token: &str) -> bool {
    token.len() == 9
        && token.as_bytes()[4] == b'-'
        && token
            .char_indices()
            .filter(|(index, _)| *index != 4)
            .all(|(_, c)| c.is_ascii_uppercase() || c.is_ascii_digit())
}

/// The `host.log` line of a phase change. Codes and accounts never appear in it.
fn phase_line(phase: &Phase) -> String {
    let state = match phase {
        Phase::Preparing => "preparing".to_string(),
        Phase::WaitingForCode => "waiting_for_code".to_string(),
        Phase::CodeReady => "code_ready".to_string(),
        Phase::Verifying => "verifying".to_string(),
        Phase::Linked { .. } => "linked".to_string(),
        Phase::SetupFailed { code, .. } => format!("setup_failed {code}"),
        Phase::Failed { code, .. } => format!("failed {code}"),
        Phase::Expired => "expired".to_string(),
        Phase::Cancelled => "cancelled".to_string(),
    };
    format!("github link: {state}")
}

/// Moves the attempt to `phase` and logs the change.
fn enter(attempt: &mut Attempt, phase: Phase) {
    eprintln!("{}", phase_line(&phase));
    attempt.phase = phase;
}

/// The account the login verified: the fields the link needs, nothing else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub id: u64,
    pub login: String,
    pub display_name: String,
}

/// Reads `gh api user`. An email in the answer is ignored and never stored.
pub fn parse_identity(json: &str) -> Result<Identity, &'static str> {
    let value: Value = serde_json::from_str(json).map_err(|_| "identity_unreadable")?;
    let id = value["id"].as_u64().ok_or("identity_unreadable")?;
    let login = value["login"]
        .as_str()
        .filter(|login| valid_login(login))
        .ok_or("identity_invalid")?;
    let display_name = value["name"]
        .as_str()
        .map(str::trim)
        .filter(|name| valid_display_name(name))
        .unwrap_or(login)
        .to_string();
    Ok(Identity {
        id,
        login: login.to_string(),
        display_name,
    })
}

/// The GitHub no-reply address of an account: `ID+LOGIN@users.noreply.github.com`.
pub fn noreply_email(id: u64, login: &str) -> String {
    format!("{id}+{login}@users.noreply.github.com")
}

fn valid_login(login: &str) -> bool {
    !login.is_empty()
        && login.len() <= 39
        && !login.starts_with('-')
        && !login.ends_with('-')
        && login.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn valid_display_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= 100
        && !name.starts_with('-')
        && !name.chars().any(|c| c.is_control() || c == '<' || c == '>')
}

/// Where a link stands. Codes are the stable values the card and the logs use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// Installing the missing tools.
    Preparing,
    /// The login process runs; no code yet.
    WaitingForCode,
    /// The code is in memory and the card shows it.
    CodeReady,
    /// The login ended; the account and the setup are being checked.
    Verifying,
    Linked { login: String },
    /// The login worked; the Git setup did not. A new start completes the setup without a login.
    SetupFailed { login: String, code: &'static str },
    Failed { step: &'static str, code: &'static str },
    Expired,
    Cancelled,
}

impl Phase {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Linked { .. }
                | Self::SetupFailed { .. }
                | Self::Failed { .. }
                | Self::Expired
                | Self::Cancelled
        )
    }
}

/// What a client may read: the phase, the code while it is shown, and the fixed page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub phase: Phase,
    pub code: Option<String>,
    pub url: &'static str,
}

/// Whether a start began a link or joined the one already running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Start {
    Started,
    Running(View),
}

struct Attempt {
    phase: Phase,
    code: Option<String>,
    child: Option<Arc<dyn LoginChild>>,
    cancel: Arc<AtomicBool>,
}

struct Context {
    layout: Layout,
    /// Git first, then gh. Production uses the pinned pair; tests substitute fixtures.
    dists: [Distribution; 2],
    fetch: Arc<dyn Fetch>,
    probe: Arc<dyn Probe>,
    runner: Arc<dyn Runner>,
    /// The PATH searched for a Git or gh that already works. `None` reads the process PATH.
    system_path: Option<Vec<PathBuf>>,
}

type Shared = Arc<Mutex<Option<Attempt>>>;

/// Runs the link for one profile. One instance per profile.
pub struct LinkManager {
    context: Arc<Context>,
    deadline: Duration,
    shared: Shared,
}

impl LinkManager {
    pub fn new(
        layout: Layout,
        fetch: Arc<dyn Fetch>,
        probe: Arc<dyn Probe>,
        runner: Arc<dyn Runner>,
    ) -> Self {
        Self {
            context: Arc::new(Context {
                layout,
                dists: pinned(),
                fetch,
                probe,
                runner,
                system_path: None,
            }),
            deadline: DEFAULT_DEADLINE,
            shared: Arc::new(Mutex::new(None)),
        }
    }

    /// Changes how long a login may wait for the user. Production keeps the 15 minutes.
    pub fn with_deadline(mut self, deadline: Duration) -> Self {
        self.deadline = deadline;
        self
    }

    /// Git and gh to install and use, in that order. Tests only: production keeps the pinned pair.
    pub fn with_distributions(mut self, dists: [Distribution; 2]) -> Self {
        let context = Arc::get_mut(&mut self.context).expect("set before the first start");
        context.dists = dists;
        self
    }

    /// The PATH searched for a working Git or gh. Tests pass an empty list so the machine's own
    /// tools never stand in for the fixtures.
    pub fn with_system_path(mut self, dirs: Vec<PathBuf>) -> Self {
        let context = Arc::get_mut(&mut self.context).expect("set before the first start");
        context.system_path = Some(dirs);
        self
    }

    /// Starts the link unless one is already running. Only `github.com` is accepted.
    pub fn start(&self, host: &str) -> Result<Start, &'static str> {
        if host != HOST {
            return Err("host_not_allowed");
        }
        let mut guard = lock(&self.shared);
        if let Some(attempt) = guard.as_ref() {
            if !attempt.phase.is_terminal() {
                return Ok(Start::Running(view_of(attempt)));
            }
        }
        let cancel = Arc::new(AtomicBool::new(false));
        eprintln!("{}", phase_line(&Phase::Preparing));
        *guard = Some(Attempt {
            phase: Phase::Preparing,
            code: None,
            child: None,
            cancel: Arc::clone(&cancel),
        });
        drop(guard);

        let context = Arc::clone(&self.context);
        let shared = Arc::clone(&self.shared);
        let deadline = self.deadline;
        thread::spawn(move || run_flow(&context, &shared, cancel, deadline));
        Ok(Start::Started)
    }

    pub fn view(&self) -> Option<View> {
        lock(&self.shared).as_ref().map(view_of)
    }

    /// Whether the profile is finished: Git and gh usable, a session that answers, and the
    /// profile's Git file holding the credential helper and the identity. A session without the
    /// setup is not ready; the link reuses that session and completes the setup.
    /// Local checks only; no login starts and nothing is installed.
    pub fn session_ready(&self) -> bool {
        let context = &self.context;
        let (Some(git), Some(gh)) = (self.resolved(0), self.resolved(1)) else {
            return false;
        };
        let env = ChildEnv::for_profile(&context.layout, &context.dists);
        let session = context
            .runner
            .run(&gh, &["auth", "status", "--hostname", HOST], &env, CALL_TIMEOUT)
            .is_ok();
        session && setup_done(context, &git, &env)
    }

    /// The fixed names (`git`, `gh`) of the tools a link would have to install: missing, or broken
    /// with no working copy on the trusted PATH. They go in the question's payload, nothing else.
    pub fn missing_tools(&self) -> Vec<&'static str> {
        (0..TOOL_EXES.len())
            .filter(|&index| self.resolved(index).is_none())
            .map(|index| TOOL_EXES[index])
            .collect()
    }

    /// The executable this profile uses for tool `index`: the verified managed copy when it is
    /// there, else a working copy on the trusted PATH, else none.
    fn resolved(&self, index: usize) -> Option<PathBuf> {
        let context = &self.context;
        let dist = &context.dists[index];
        if matches!(context.layout.status(dist), Status::Ready { .. }) {
            return Some(context.layout.exe(dist));
        }
        system_copy(context, index)
    }

    /// Stops a link in progress and forgets its code. Returns whether one was running.
    pub fn cancel(&self) -> bool {
        let mut guard = lock(&self.shared);
        match guard.as_mut() {
            Some(attempt) if !attempt.phase.is_terminal() => {
                attempt.cancel.store(true, Ordering::SeqCst);
                if let Some(child) = attempt.child.take() {
                    child.kill();
                }
                attempt.code = None;
                enter(attempt, Phase::Cancelled);
                true
            }
            _ => false,
        }
    }
}

fn view_of(attempt: &Attempt) -> View {
    View {
        phase: attempt.phase.clone(),
        code: attempt.code.clone(),
        url: DEVICE_URL,
    }
}

/// Writes to the attempt this flow started, and only while it is still the current one
/// and not cancelled. A flow that outlived its attempt writes nothing.
fn mine(shared: &Shared, cancel: &Arc<AtomicBool>, write: impl FnOnce(&mut Attempt)) {
    if cancel.load(Ordering::SeqCst) {
        return;
    }
    if let Some(attempt) = lock(shared).as_mut() {
        if Arc::ptr_eq(&attempt.cancel, cancel) {
            write(attempt);
        }
    }
}

fn run_flow(context: &Context, shared: &Shared, cancel: Arc<AtomicBool>, deadline: Duration) {
    let result = flow(context, shared, &cancel, deadline);
    mine(shared, &cancel, |attempt| {
        enter(attempt, result);
        attempt.code = None;
        attempt.child = None;
    });
}

fn set_phase(shared: &Shared, cancel: &Arc<AtomicBool>, phase: Phase) {
    mine(shared, cancel, |attempt| {
        attempt.code = None;
        enter(attempt, phase);
    });
}

fn flow(context: &Context, shared: &Shared, cancel: &Arc<AtomicBool>, deadline: Duration) -> Phase {
    let dists = context.dists.clone();
    let installer = Installer {
        layout: &context.layout,
        fetch: context.fetch.as_ref(),
        probe: context.probe.as_ref(),
    };
    // The verified copy wins. A working Git or gh on the trusted PATH is used by its absolute
    // path with no download; a tool with neither is installed.
    let mut tools = Vec::with_capacity(dists.len());
    for (index, dist) in dists.iter().enumerate() {
        let managed = matches!(context.layout.status(dist), Status::Ready { .. });
        if !managed {
            if let Some(path) = system_copy(context, index) {
                tools.push(path);
                continue;
            }
            if let Err(error) = installer.install(dist) {
                return Phase::Failed {
                    step: "install",
                    code: error.code(),
                };
            }
        }
        tools.push(context.layout.exe(dist));
    }
    if cancel.load(Ordering::SeqCst) {
        return Phase::Cancelled;
    }
    if context.layout.ensure_profile().is_err() {
        return Phase::Failed {
            step: "profile",
            code: "io_failed",
        };
    }

    let git = tools[0].clone();
    let gh = tools[1].clone();
    let env = ChildEnv::for_profile(&context.layout, &dists);
    let runner = context.runner.as_ref();

    // A session that already works is reused: no second login.
    let has_session = runner
        .run(&gh, &["auth", "status", "--hostname", HOST], &env, CALL_TIMEOUT)
        .is_ok();
    if !has_session {
        let child = match runner.spawn_login(&gh, &env) {
            Ok(child) => child,
            Err(_) => {
                return Phase::Failed {
                    step: "login",
                    code: "spawn_failed",
                }
            }
        };
        if cancel.load(Ordering::SeqCst) {
            child.kill();
            return Phase::Cancelled;
        }
        let holder = Arc::clone(&child);
        mine(shared, cancel, move |attempt| attempt.child = Some(holder));
        set_phase(shared, cancel, Phase::WaitingForCode);

        let started = Instant::now();
        let mut collector = CodeCollector::default();
        loop {
            if cancel.load(Ordering::SeqCst) {
                child.kill();
                return Phase::Cancelled;
            }
            if started.elapsed() >= deadline {
                child.kill();
                return Phase::Expired;
            }
            match child.next(POLL) {
                Fragment::Data(bytes) => {
                    if let Some(code) = collector.push(&bytes) {
                        mine(shared, cancel, move |attempt| {
                            attempt.code = Some(code);
                            enter(attempt, Phase::CodeReady);
                        });
                    }
                }
                Fragment::Idle => {}
                Fragment::Closed => break,
            }
        }
        if child.exit_code() != Some(0) {
            return Phase::Failed {
                step: "login",
                code: "login_failed",
            };
        }
    }

    set_phase(shared, cancel, Phase::Verifying);
    let json = match runner.run(&gh, &["api", "--hostname", HOST, "user"], &env, CALL_TIMEOUT) {
        Ok(json) => json,
        Err(_) => {
            return Phase::Failed {
                step: "session",
                code: "api_failed",
            }
        }
    };
    let identity = match parse_identity(&json) {
        Ok(identity) => identity,
        Err(code) => {
            return Phase::Failed {
                step: "session",
                code,
            }
        }
    };
    if cancel.load(Ordering::SeqCst) {
        return Phase::Cancelled;
    }

    if runner
        .run(&gh, &["auth", "setup-git", "--hostname", HOST], &env, CALL_TIMEOUT)
        .is_err()
    {
        return Phase::SetupFailed {
            login: identity.login,
            code: "setup_git_failed",
        };
    }
    let gitconfig = context.layout.gitconfig();
    let gitconfig = gitconfig.to_string_lossy().into_owned();
    let email = noreply_email(identity.id, &identity.login);
    let identity_set = [
        ("user.name", identity.display_name.as_str()),
        ("user.email", email.as_str()),
    ]
    .iter()
    .all(|(key, value)| {
        runner
            .run(&git, &["config", "--file", &gitconfig, key, value], &env, CALL_TIMEOUT)
            .is_ok()
    });
    if !identity_set {
        return Phase::SetupFailed {
            login: identity.login,
            code: "identity_failed",
        };
    }
    Phase::Linked {
        login: identity.login,
    }
}

fn exe_name(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    }
}

/// A working Git or gh already on the trusted PATH: an absolute entry outside the profile's own
/// tool folders, whose copy answers `--version`. A relative entry resolves against the working
/// directory, so it never counts.
fn system_copy(context: &Context, index: usize) -> Option<PathBuf> {
    let managed = context.layout.path_dirs(&context.dists);
    let entries: Vec<PathBuf> = match &context.system_path {
        Some(dirs) => dirs.clone(),
        None => std::env::var_os("PATH")
            .map(|path| std::env::split_paths(&path).collect())
            .unwrap_or_default(),
    };
    let exe = exe_name(TOOL_EXES[index]);
    entries
        .into_iter()
        .filter(|dir| dir.is_absolute() && !managed.contains(dir))
        .map(|dir| dir.join(&exe))
        .find(|candidate| candidate.is_file() && context.probe.version_output(candidate).is_ok())
}

/// The profile's Git file holds the credential helper and the identity the link writes. Only
/// that file is read (`--file` does not follow the user's includes), so the user's own global
/// identity never counts as the link's.
fn setup_done(context: &Context, git: &Path, env: &ChildEnv) -> bool {
    let gitconfig = context.layout.gitconfig().to_string_lossy().into_owned();
    ["user.name", "user.email", "credential.https://github.com.helper"]
        .iter()
        .all(|key| {
            context
                .runner
                .run(git, &["config", "--file", &gitconfig, "--get", key], env, CALL_TIMEOUT)
                .is_ok()
        })
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_split_across_fragments_is_assembled_once() {
        let mut collector = CodeCollector::default();
        assert_eq!(collector.push(b"! First copy your "), None);
        assert_eq!(collector.push(b"one-time code: AB"), None);
        assert_eq!(collector.push(b"CD-12"), None);
        assert_eq!(collector.push(b"34\nOpen https://github.com/login/device\n"), Some("ABCD-1234".into()));
    }

    #[test]
    fn only_a_code_of_the_github_shape_is_taken() {
        assert_eq!(parse_code("one-time code: abcd-1234"), None);
        assert_eq!(parse_code("one-time code: ABCD1234"), None);
        assert_eq!(parse_code("one-time code: ABCD-123"), None);
        assert_eq!(parse_code("no code here"), None);
        assert_eq!(parse_code("one-time code: ABCD-1234 "), Some("ABCD-1234".into()));
        assert_eq!(parse_code("One-time code (abcd-1234) copied to clipboard"), None);
        assert_eq!(parse_code("One-time code (ABCD1234) copied to clipboard"), None);
        assert_eq!(parse_code("One-time code (ABCD-12345) copied to clipboard"), None);
        assert_eq!(parse_code("One-time code (ABCD-1234 extra) copied to clipboard"), None);
        assert_eq!(parse_code("One-time code (ABCD-1234"), None);
    }

    #[test]
    fn both_real_gh_forms_give_the_code() {
        // The text gh 2.105 prints when it copies the code, and the one gh 2.93 prints.
        assert_eq!(
            parse_code("! One-time code (ABCD-1234) copied to clipboard\n"),
            Some("ABCD-1234".into())
        );
        assert_eq!(
            parse_code("! First copy your one-time code: ABCD-1234\nOpen https://github.com/login/device\n"),
            Some("ABCD-1234".into())
        );
    }

    #[test]
    fn the_copied_form_split_across_fragments_is_assembled_once() {
        let mut collector = CodeCollector::default();
        assert_eq!(collector.push(b"! One-time code (AB"), None);
        assert_eq!(collector.push(b"CD-12"), None);
        assert_eq!(collector.push(b"34) copied to clipboard\n"), Some("ABCD-1234".into()));
    }

    #[test]
    fn the_phase_line_never_carries_the_code_or_the_account() {
        assert_eq!(phase_line(&Phase::CodeReady), "github link: code_ready");
        assert_eq!(phase_line(&Phase::Linked { login: "octo".into() }), "github link: linked");
        assert_eq!(
            phase_line(&Phase::Failed { step: "login", code: "login_failed" }),
            "github link: failed login_failed"
        );
    }

    #[test]
    fn the_login_asks_gh_not_to_copy_the_code() {
        assert!(LOGIN_ARGS.contains(&"--clipboard=false"));
    }

    #[test]
    fn the_identity_keeps_only_the_needed_fields() {
        let json = r#"{"id":4242,"login":"octo","name":"  Octo Cat ","email":"x@example.com"}"#;
        let identity = parse_identity(json).expect("identity");
        assert_eq!(identity.id, 4242);
        assert_eq!(identity.display_name, "Octo Cat");
        assert_eq!(noreply_email(identity.id, &identity.login), "4242+octo@users.noreply.github.com");
    }

    #[test]
    fn a_missing_or_empty_name_falls_back_to_the_login() {
        let identity = parse_identity(r#"{"id":1,"login":"octo","name":null}"#).expect("identity");
        assert_eq!(identity.display_name, "octo");
        let identity = parse_identity(r#"{"id":1,"login":"octo","name":"   "}"#).expect("identity");
        assert_eq!(identity.display_name, "octo");
    }

    #[test]
    fn an_invalid_login_or_identity_is_refused() {
        assert!(parse_identity(r#"{"id":1,"login":"bad login"}"#).is_err());
        assert!(parse_identity(r#"{"id":1,"login":"-x"}"#).is_err());
        assert!(parse_identity(r#"{"login":"octo"}"#).is_err());
        assert!(parse_identity("not json").is_err());
    }

    #[test]
    fn only_github_com_is_linked() {
        let dir = excubia_test_dir::TestDir::new("link-host");
        let manager = LinkManager::new(
            Layout::new(&dir),
            Arc::new(crate::fetch::HttpsFetch),
            Arc::new(crate::install::ExecProbe),
            Arc::new(ProcessRunner::new()),
        );
        assert_eq!(manager.start("gitlab.com"), Err("host_not_allowed"));
        assert!(manager.view().is_none(), "a refused host starts nothing");
    }
}
