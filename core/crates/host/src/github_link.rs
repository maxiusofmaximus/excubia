//! `github.link` (GITHUB-INTEGRACION): Alfred and a lead ask to link GitHub for the managed
//! profile. The agent only asks. The host opens one question for the operator's device, and
//! a question the operator approved starts the link once. The agent reads a state, never a
//! code, a URL or a token.
//!
//! One step at a time: the check for an open question, the burn of an approved one and the
//! start of its attempt all happen under `track`. A waiter therefore never sees a question
//! "consumed" without the attempt that serves it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use excubia_chat::{
    action_digest, Approval, ApprovalKind, ApprovalStatus, ChatError, ChatStore, GithubLinkCard,
    NewApproval, SeatActor, ToolError, ToolReply, WaitHold, DEFAULT_TTL_SECONDS,
};
use excubia_toolchain::github::{LinkManager, Phase, View, HOST};
use serde_json::json;

use super::team_errors::tool_chat_error;

/// The approval's tool name. The tool name alone never identifies this purpose: a generic
/// `approval.request` can name any tool, so every check below uses `digest()` as well.
pub const TOOL: &str = "github.link";

/// The kind this purpose hashes with. It is not an `ApprovalKind`, so no generic request can
/// produce the same digest: the digest is the purpose marker, and the store forces the
/// `Credentials` category when the host creates the question.
const PURPOSE: &str = "github_link";
/// How often a waiting caller looks at the link.
const LINK_POLL: Duration = Duration::from_secs(1);

/// The question the card shows. The desktop words the card in the operator's language; this text
/// only names the approval where no card renders it.
const SUMMARY: &str = "¿Enlazar GitHub con Excubia?";

pub struct GithubLink {
    chat: Arc<ChatStore>,
    link: LinkManager,
    track: Mutex<Track>,
}

/// Which questions the link serves, and how each attempt ended.
#[derive(Default)]
struct Track {
    /// The questions of the latest attempt: more than one when a later approval joined a run.
    ids: Vec<String>,
    /// Whether the latest attempt reached a terminal link state.
    settled: bool,
    /// The word each question's attempt ended with, kept for every question served here.
    outcomes: HashMap<String, &'static str>,
}

impl Track {
    /// Records the end of the attempt once the link reports it: every question it served gets
    /// the same word. The link's view belongs to this attempt only; a new start replaces it.
    fn settle(&mut self, link: &LinkManager) {
        if self.settled || self.ids.is_empty() {
            return;
        }
        let Some(view) = link.view() else { return };
        if !view.phase.is_terminal() {
            return;
        }
        let word = state_word(&view.phase);
        for id in &self.ids {
            self.outcomes.insert(id.clone(), word);
        }
        self.settled = true;
    }
}

/// The fixed digest of this purpose: the profile's one link on github.com.
fn digest() -> String {
    action_digest(PURPOSE, TOOL, &json!({ "host": HOST }), "", &[])
}

impl GithubLink {
    pub fn new(chat: Arc<ChatStore>, link: LinkManager) -> Self {
        Self {
            chat,
            link,
            track: Mutex::new(Track::default()),
        }
    }

    /// The answer to `github.link`. It waits for the operator's decision and for the link to
    /// end, so the agent's turn resumes with the real state; an open question is shared, never
    /// duplicated, and a refusal holds for the run that asked.
    pub async fn request(
        self: Arc<Self>,
        actor: SeatActor,
        pane_id: Option<String>,
    ) -> Result<ToolReply, ToolError> {
        // The checks read the local Git and gh: they run off the async workers.
        let this = Arc::clone(&self);
        let step = tokio::task::spawn_blocking(move || this.open(&actor, pane_id))
            .await
            .map_err(|_| ToolError::unavailable("the GitHub check stopped; try again"))??;
        match step {
            Step::Reply(status) => Ok(reply(status)),
            Step::Wait(approval_id) => self.resolve(&approval_id).await,
            Step::Link => Ok(reply(self.wait_running().await)),
        }
    }

    /// The checks and the question, under the lock: one step at a time per profile.
    fn open(&self, actor: &SeatActor, pane_id: Option<String>) -> Result<Step, ToolError> {
        let mut track = lock(&self.track);
        track.settle(&self.link);
        if let Some(view) = self.link.view() {
            if !view.phase.is_terminal() {
                return Ok(Step::Link);
            }
        }
        // Reuses a session that already works: no card, no installation, no login.
        if self.link.session_ready() {
            return Ok(Step::Reply("linked"));
        }
        let ours = digest();
        let history: Vec<Approval> = self
            .chat
            .approvals_for_tool(TOOL)
            .map_err(tool_chat_error)?
            .into_iter()
            .filter(|approval| approval.action_digest == ours)
            .collect();
        if let Some(open) = history
            .iter()
            .find(|approval| approval.status == ApprovalStatus::Pending)
        {
            return Ok(Step::Wait(open.id.clone()));
        }
        if let Some(approved) = history
            .iter()
            .find(|approval| approval.status == ApprovalStatus::Approved)
        {
            return Ok(Step::Wait(approved.id.clone()));
        }
        // A refusal holds for the run that asked: asking again in the same run never opens a
        // new card. A later run asks again on its own.
        if let Some(last) = history.last() {
            if last.status == ApprovalStatus::Denied && same_run(last, actor) {
                return Ok(Step::Reply("declined"));
            }
        }
        // The desktop words the question; the host sends only the fixed names of the tools to install.
        let missing = self.link.missing_tools();
        let args = json!({ "host": HOST, "missing_tools": missing });
        let new = NewApproval {
            conversation_id: Some(actor.conversation_id.clone()).filter(|id| !id.is_empty()),
            seat_id: Some(actor.agent_id.clone()),
            agent_label: None,
            run_id: Some(actor.run_id.clone()).filter(|id| !id.is_empty()),
            pane_id,
            kind: ApprovalKind::Tool,
            tool: TOOL.to_string(),
            summary: SUMMARY.into(),
            cwd: None,
            paths: Vec::new(),
            risk: None,
            action_digest: ours,
            payload_redacted: Some(args),
            ttl_seconds: DEFAULT_TTL_SECONDS,
        };
        drop(track);
        match self.chat.create_github_link_approval(new) {
            Ok(approval) => Ok(Step::Wait(approval.id)),
            // The store refuses a second open question with the same digest: take the one open.
            Err(ChatError::ApprovalDigestReused) => Ok(Step::Reply("awaiting_operator")),
            Err(error) => Err(tool_chat_error(error)),
        }
    }

    /// Waits for the operator's answer. Approved: the question is launched once; when the
    /// ticker burned it first, this caller waits for that same attempt.
    async fn resolve(&self, approval_id: &str) -> Result<ToolReply, ToolError> {
        let decided = self
            .chat
            .await_decision(approval_id)
            .await
            .map_err(tool_chat_error)?;
        let status = match decided.status {
            ApprovalStatus::Approved => {
                // A launch that loses the burn to the ticker is the same attempt: wait for it.
                let _ = self.launch(&decided);
                self.await_outcome(approval_id).await
            }
            ApprovalStatus::Consumed => self.await_outcome(approval_id).await,
            ApprovalStatus::Denied => "declined",
            ApprovalStatus::Expired => "expired",
            _ => "failed",
        };
        Ok(reply(status))
    }

    /// The word the question's own attempt ends with. A question this profile never launched
    /// answers `failed` at once, so no caller waits for an attempt that is not its own.
    async fn await_outcome(&self, approval_id: &str) -> &'static str {
        let mut hold = self.chat.hold_approval_wait(approval_id).ok();
        loop {
            {
                let mut track = lock(&self.track);
                track.settle(&self.link);
                if let Some(word) = track.outcomes.get(approval_id).copied() {
                    return word;
                }
                if !track.ids.iter().any(|id| id == approval_id) {
                    return "failed";
                }
            }
            self.hold_while_code_shown(&mut hold);
            tokio::time::sleep(LINK_POLL).await;
        }
    }

    /// The word the link running for someone else ends with, for a caller without a question.
    async fn wait_running(&self) -> &'static str {
        let latest = lock(&self.track).ids.last().cloned();
        let mut hold = latest.and_then(|id| self.chat.hold_approval_wait(&id).ok());
        loop {
            {
                let mut track = lock(&self.track);
                track.settle(&self.link);
                if track.settled {
                    return track
                        .ids
                        .last()
                        .and_then(|id| track.outcomes.get(id).copied())
                        .unwrap_or("failed");
                }
                if track.ids.is_empty() {
                    return "failed";
                }
            }
            self.hold_while_code_shown(&mut hold);
            tokio::time::sleep(LINK_POLL).await;
        }
    }

    /// Holds the runs' budget while the code is on the card: the operator types it on
    /// github.com, and that time is not the agent's. The hold ends with the wait, so a
    /// login that ends, expires or is cancelled resumes the turn with its balance.
    fn hold_while_code_shown(&self, hold: &mut Option<WaitHold>) {
        let shown = self
            .link
            .view()
            .is_some_and(|view| view.phase == Phase::CodeReady);
        if let Some(hold) = hold {
            hold.set(shown);
        }
    }

    /// Burns an approved question and starts its attempt, as one step under the lock. A
    /// question already burned answers `Err`; a link still running for another question is
    /// joined and both questions share its end.
    fn launch(&self, approval: &Approval) -> Result<(), ChatError> {
        let mut track = lock(&self.track);
        self.chat
            .consume_for_host(&approval.id, &approval.action_digest)?;
        track.settle(&self.link);
        let joined = !track.ids.is_empty() && !track.settled;
        if !joined {
            track.ids.clear();
            track.settled = false;
        }
        track.ids.push(approval.id.clone());
        if let Err(code) = self.link.start(HOST) {
            track.ids.retain(|id| id != &approval.id);
            track.outcomes.insert(approval.id.clone(), "failed");
            eprintln!("host: the GitHub link did not start ({code})");
        }
        Ok(())
    }

    /// Starts the link for each question of this purpose the operator approved. Each approval
    /// is checked by its purpose digest and kind, then burned before the start, so one approval
    /// starts at most one link. Runs on a timer and never fails its caller.
    pub fn tick(&self) {
        let ours = digest();
        let approved: Vec<Approval> = match self.chat.approvals_for_tool(TOOL) {
            Ok(all) => all
                .into_iter()
                .filter(|approval| {
                    approval.status == ApprovalStatus::Approved
                        && approval.action_digest == ours
                        && approval.kind == ApprovalKind::Tool
                })
                .collect(),
            Err(error) => {
                eprintln!("host: could not read the approved GitHub link: {error}");
                return;
            }
        };
        for approval in approved {
            if let Err(error) = self.launch(&approval) {
                eprintln!("host: an approved GitHub link was not consumed: {error}");
            }
        }
    }

    /// The card of the latest attempt, with the questions it serves. The code is in it only
    /// while the login shows it; the card never carries another question's attempt.
    pub fn card(&self) -> Option<GithubLinkCard> {
        let mut track = lock(&self.track);
        track.settle(&self.link);
        if track.ids.is_empty() {
            return None;
        }
        let view = self.link.view()?;
        Some(card_of(&view, track.ids.clone()))
    }

    /// Stops the link when `approval_id` is one of its questions and it is still running.
    pub fn cancel(&self, approval_id: &str) -> bool {
        let track = lock(&self.track);
        track.ids.iter().any(|id| id == approval_id) && !track.settled && self.link.cancel()
    }
}

/// The card's words for a link. The code is kept only while the login shows it.
fn card_of(view: &View, approval_ids: Vec<String>) -> GithubLinkCard {
    let (phase, login, failure) = match &view.phase {
        Phase::Preparing => ("preparing", None, None),
        Phase::WaitingForCode => ("waiting_code", None, None),
        Phase::CodeReady => ("code_ready", None, None),
        Phase::Verifying => ("verifying", None, None),
        Phase::Linked { login } => ("linked", Some(login.clone()), None),
        Phase::SetupFailed { login, code } => ("setup_failed", Some(login.clone()), Some(*code)),
        Phase::Failed { code, .. } => ("failed", None, Some(*code)),
        Phase::Expired => ("expired", None, None),
        Phase::Cancelled => ("cancelled", None, None),
    };
    GithubLinkCard {
        phase,
        code: if phase == "code_ready" {
            view.code.clone()
        } else {
            None
        },
        url: view.url,
        login,
        failure,
        approval_ids,
    }
}

/// Whether a question was asked in the run of `actor`; a run without an id counts its seat.
fn same_run(approval: &Approval, actor: &SeatActor) -> bool {
    if actor.run_id.is_empty() {
        return approval.seat_id.as_deref() == Some(actor.agent_id.as_str());
    }
    approval.run_id.as_deref() == Some(actor.run_id.as_str())
}

/// What the checks decided, before any wait.
enum Step {
    Reply(&'static str),
    Wait(String),
    Link,
}

/// The state of a finished link, as the agent reads it.
fn state_word(phase: &Phase) -> &'static str {
    match phase {
        Phase::Linked { .. } => "linked",
        Phase::SetupFailed { .. } => "setup_failed",
        Phase::Expired => "expired",
        Phase::Cancelled => "cancelled",
        _ => "failed",
    }
}

fn reply(status: &str) -> ToolReply {
    ToolReply {
        result: json!({ "status": status }),
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::path::{Path, PathBuf};

    use excubia_chat::Decision;
    use excubia_toolchain::fetch::Fetch;
    use excubia_toolchain::github::{ChildEnv, LoginChild, Runner};
    use excubia_toolchain::install::Probe;
    use excubia_toolchain::Layout;

    use super::*;

    #[test]
    fn the_purpose_digest_is_not_what_a_generic_request_gets() {
        // A generic `approval.request` hashes its kind ("tool"); this purpose hashes its own.
        let generic = action_digest("tool", TOOL, &json!({ "host": HOST }), "", &[]);
        assert_ne!(generic, digest());
        assert_eq!(digest(), digest(), "the fixed digest is stable");
    }

    /// No network: every download fails, so an attempt ends at once without touching GitHub.
    struct Offline;
    impl Fetch for Offline {
        fn fetch(&self, _: &str, _: u64, _: &Path) -> Result<(), String> {
            Err("offline".into())
        }
    }

    struct Absent;
    impl Probe for Absent {
        fn version_output(&self, _: &Path) -> Result<String, String> {
            Err("absent".into())
        }
    }

    struct NoProcess;
    impl Runner for NoProcess {
        fn spawn_login(&self, _: &Path, _: &ChildEnv) -> io::Result<Arc<dyn LoginChild>> {
            Err(io::Error::other("no login in tests"))
        }
        fn run(&self, _: &Path, _: &[&str], _: &ChildEnv, _: Duration) -> Result<String, String> {
            Err("no process in tests".into())
        }
    }

    fn unique_dir(prefix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("{prefix}-{}", excubia_chat::new_ulid()));
        std::fs::create_dir_all(&dir).expect("test dir");
        dir
    }

    fn fixture() -> (Arc<ChatStore>, GithubLink) {
        let chat = Arc::new(ChatStore::open(&unique_dir("gh-chat").join("chat.db")).expect("store"));
        let link = LinkManager::new(
            Layout::new(&unique_dir("gh-data")),
            Arc::new(Offline),
            Arc::new(Absent),
            Arc::new(NoProcess),
        );
        let github = GithubLink::new(Arc::clone(&chat), link);
        (chat, github)
    }

    fn ask(chat: &ChatStore) -> String {
        let new = NewApproval {
            conversation_id: None,
            // No seat row: `approval.seat_id` references `agent_seat`, and these tests need none.
            seat_id: None,
            agent_label: None,
            run_id: Some("run_test".into()),
            pane_id: None,
            kind: ApprovalKind::Tool,
            tool: TOOL.into(),
            summary: SUMMARY.into(),
            cwd: None,
            paths: Vec::new(),
            risk: None,
            action_digest: digest(),
            payload_redacted: Some(json!({ "host": HOST })),
            ttl_seconds: DEFAULT_TTL_SECONDS,
        };
        chat.create_github_link_approval(new).expect("question").id
    }

    /// The question that `open` creates carries the fixed names of the missing tools in its payload,
    /// and its summary is the fixed question. Checked on the stored row, not on the compiled binary.
    #[test]
    fn the_created_question_carries_the_fixed_missing_tool_names() {
        let (chat, github) = fixture();
        // The question references a real conversation and the seeded Alfred seat.
        let conversation_id = chat
            .create_conversation(excubia_chat::NewConversation {
                title: Some("github".into()),
                project_id: None,
            })
            .expect("conversation")
            .id;
        let actor = SeatActor {
            agent_id: chat.bootstrap().expect("bootstrap").alfred.agent_id,
            role_id: "alfred".into(),
            project_id: None,
            conversation_id,
            run_id: "run_test".into(),
        };
        let id = match github.open(&actor, None).expect("open") {
            Step::Wait(id) => id,
            _ => panic!("the first request opens one question"),
        };
        let row = chat
            .approvals_for_tool(TOOL)
            .expect("list")
            .into_iter()
            .find(|approval| approval.id == id)
            .expect("the question is stored");
        assert_eq!(row.summary, SUMMARY);
        assert_eq!(
            row.payload_redacted,
            Some(json!({ "host": HOST, "missing_tools": ["git", "gh"] }))
        );
        assert_eq!(row.action_digest, digest());
    }

    fn status_of(chat: &ChatStore, id: &str) -> ApprovalStatus {
        chat.approvals_for_tool(TOOL)
            .expect("list")
            .into_iter()
            .find(|approval| approval.id == id)
            .expect("the question exists")
            .status
    }

    /// Focal case: the ticker burns an approved question before a waiter reads it. The waiter
    /// gets the word of its own attempt, never `failed` by mistake, and a foreign id stays out.
    #[tokio::test]
    async fn a_question_the_ticker_burned_is_waited_for_by_its_own_attempt() {
        let (chat, github) = fixture();
        let id = ask(&chat);
        chat.decide_approval(&id, Decision::Approve, "client_test", "device_test")
            .expect("approve");

        github.tick();
        assert_eq!(status_of(&chat, &id), ApprovalStatus::Consumed);

        // The waiter that read `Consumed` resolves to this attempt's end, not to a guess.
        assert_eq!(github.await_outcome(&id).await, "failed");
        // The card names only the question it serves, and never a code.
        let card = github.card().expect("card of the attempt");
        assert_eq!(card.approval_ids, vec![id.clone()]);
        assert!(card.code.is_none());
        // Another question cannot read this attempt or cancel it.
        assert_eq!(github.await_outcome("apr_foreign").await, "failed");
        assert!(!github.cancel("apr_foreign"));
    }

    /// A second tick burns nothing new: the question starts at most once.
    #[tokio::test]
    async fn a_second_tick_does_not_launch_the_same_question_again() {
        let (chat, github) = fixture();
        let id = ask(&chat);
        chat.decide_approval(&id, Decision::Approve, "client_test", "device_test")
            .expect("approve");
        github.tick();
        let first = github.card().expect("card");
        github.tick();
        assert_eq!(github.card().expect("card"), first);
        assert_eq!(github.await_outcome(&id).await, "failed");
    }
}
