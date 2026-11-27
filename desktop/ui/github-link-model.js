// The GitHub link card's rules, without the DOM (GITHUB-INTEGRACION). The card shows the approval's
// status and the link's phase as one stage. The code is shown only at `code_ready`; the button always
// opens the same fixed page, never an address the host or an agent sent.

export const GITHUB_DEVICE_URL = "https://github.com/login/device";
export const GITHUB_TOOL = "github.link";

// The stages that end the card: nothing more is polled and the result stays on screen.
const FINISHED = ["linked", "setup_failed", "failed", "expired", "cancelled", "declined", "superseded"];
// The stages in which the operator can still cancel this question's link.
const CANCELLABLE = ["starting", "preparing", "waiting_code", "code_ready", "verifying"];
// The stages that offer «Volver a intentar»: the link failed or its request expired.
const RETRYABLE = ["failed", "setup_failed", "expired"];
// The cause of a failed link, from the host's fixed code. A code not listed here is `other`.
const CAUSE_OF_CODE = new Map([
  ["download_failed", "offline"],
  ["api_failed", "offline"],
  ["login_failed", "login"],
  ["busy", "install"],
  ["size_mismatch", "install"],
  ["digest_mismatch", "install"],
  ["extract_failed", "install"],
  ["layout_missing", "install"],
  ["version_mismatch", "install"],
  ["swap_failed", "install"],
]);
// The result line for each stage that ends the card.
const RESULT_KEYS = {
  linked: "github.result.linked",
  setup_failed: "github.result.setup_failed",
  failed: "github.result.failed",
  expired: "github.result.expired",
  cancelled: "github.result.cancelled",
  declined: "github.result.declined",
  superseded: "github.result.superseded",
};

export function isGithubLinkTool(approval) {
  return Boolean(approval) && approval.tool === GITHUB_TOOL;
}

// Approval ids are `apr_` + a monotonic ULID, so their order is their creation order. An attempt
// whose questions all predate this one is an earlier attempt: this question has not started yet.
function predates(card, approvalId) {
  return card.approval_ids.every((id) => id < approvalId);
}

// One stage from this question's status and the link's state. The card speaks only for the
// questions it serves: a question served by a later attempt is `superseded`, so a card never
// shows another question's code or end. While the card still shows an earlier attempt, a
// freshly approved question is `starting`, not closed.
export function githubStage(status, card, approvalId) {
  if (status === "denied") return "declined";
  if (status === "expired") return "expired";
  if (status !== "approved" && status !== "consumed") return "awaiting";
  if (!card || !card.phase || !Array.isArray(card.approval_ids)) return "starting";
  if (card.approval_ids.includes(approvalId)) return card.phase;
  return predates(card, approvalId) ? "starting" : "superseded";
}

export function isGithubFinished(stage) {
  return FINISHED.includes(stage);
}

// The cause of a failed link as one key suffix (`offline`, `install`, `login`, `other`), or null
// when the stage has no failure to explain. The code comes from the host, never from text it wrote.
export function githubCause(stage, card) {
  if (stage !== "failed" && stage !== "setup_failed") return null;
  return CAUSE_OF_CODE.get(card && card.failure) || "other";
}

// Whether a later question than this one is on the card list. Approval ids are `apr_` + a monotonic
// ULID, so the larger id is the later question (as in `predates`).
export function githubNewer(approvalId, ids) {
  return ids.some((id) => id > approvalId);
}

// What the card shows for a stage. The code and the button appear only at `code_ready`, and the
// cancel button only while this question's link runs. `newer`: a later question exists, so an earlier
// card is history and offers no retry.
export function githubFacts(stage, card, newer = false) {
  const code = stage === "code_ready" && card && card.code ? card.code : null;
  return {
    stage,
    finished: isGithubFinished(stage),
    code,
    showOpen: stage === "code_ready" || stage === "waiting_code",
    openUrl: GITHUB_DEVICE_URL,
    showCancel: CANCELLABLE.includes(stage),
    showRetry: RETRYABLE.includes(stage) && !newer,
    cause: githubCause(stage, card),
    resultKey: RESULT_KEYS[stage] || null,
    login: card && card.login ? card.login : null,
  };
}

// The card of the approval has no link yet while it waits for the operator: it polls nothing.
export function githubPolls(stage) {
  return !isGithubFinished(stage) && stage !== "awaiting";
}

// The fixed tool names the host may list in the question's payload. Any other name is dropped, so
// the card never shows text the host wrote.
const TOOL_NAMES = ["git", "gh"];

export function githubMissing(entry) {
  const list = entry && entry.payload_redacted ? entry.payload_redacted.missing_tools : null;
  if (!Array.isArray(list)) return [];
  return TOOL_NAMES.filter((name) => list.includes(name));
}

// The card's explanation in the operator's language. `t` is the UI translator, which keeps this
// module free of the DOM.
export function githubExplanation(missing, t) {
  const names = missing.map((name) => t(`github.tool.${name}`)).join(t("github.names_joiner"));
  const install = missing.length
    ? t(missing.length === 1 ? "github.explain.install_one" : "github.explain.install_many", { names })
    : "";
  return [install, t("github.explain.open")].filter(Boolean).join(" ");
}
