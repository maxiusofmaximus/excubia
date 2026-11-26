// Pure rules for approval cards and timeline lines: no DOM, no invoke. The
// card, the line, the settings and the tests all read the same rules, so the
// host's words reach the screen through one path. The host owns the state.

import { t } from "./i18n.js";

export const LEVELS = ["policy", "scorer", "lead", "cto", "user"];

const LEVEL_LABELS = {
  policy: "approval.level_policy",
  scorer: "approval.level_scorer",
  lead: "approval.level_lead",
  cto: "approval.level_cto",
  user: "approval.level_user",
};

const STATUS_META = {
  pending: { label: "approval.status_pending", tone: "pending" },
  approved: { label: "approval.status_approved", tone: "approved" },
  consumed: { label: "approval.status_consumed", tone: "approved" },
  denied: { label: "approval.status_denied", tone: "denied" },
  expired: { label: "approval.status_expired", tone: "expired" },
};

const RISK_LABELS = {
  low: "approval.risk_low",
  medium: "approval.risk_medium",
  high: "approval.risk_high",
};

// An approval for the whole task says so in its badge, not «una vez».
export function statusMeta(status, entry = {}) {
  if (status === "approved" && entry.scope === "task") return { label: t("approval.status_task"), tone: "approved" };
  const meta = STATUS_META[status];
  return meta ? { label: t(meta.label), tone: meta.tone } : { label: status || t("approval.status_unknown"), tone: "unknown" };
}

export function levelLabel(level) {
  return t(LEVEL_LABELS[level] || "approval.level_none");
}

export function riskLabel(risk) {
  return t(RISK_LABELS[risk] || "approval.risk_unrated");
}

// Every level the approval climbed through, up to the one that holds it now.
export function levelChain(level) {
  const index = LEVELS.indexOf(level);
  return index < 0 ? [] : LEVELS.slice(0, index + 1);
}

// Only the operator's own level earns a card. Anything decided below him is a
// one-line record, and a lower-level request that is still open stays hidden.
export function isOperatorLevel(entry) {
  return entry.level === "user";
}

// The host writes a decision of the policy as «Política aprobó: …». Its timeline row
// carries no level, so the wording is the only sign. shortcut: matches the host's words, upgrade when the row sends `level`.
export function isPolicyLine(entry) {
  return entry.level === "policy" || /^pol[ií]tica aprob/i.test(String(entry.summary || ""));
}

// The inbox strip lists the open conversation's requests and those with no conversation.
// A request the operator still has to answer: not decided here, and the host says pending.
export function isPendingHere(entry) {
  return !localFor(entry.id).decided && (entry.status || "pending") === "pending";
}

// A send the operator approves: the host adds its `details` for this device only.
export function isMailSend(entry) {
  return entry.tool === "mail.send";
}

// A batch of mails to move. Each mail is one item of the host's `details`.
export function isMailMove(entry) {
  return entry.tool === "mail.move";
}

// The batch as the host sent it, `excluded` marks included: a decided card shows
// what was left out. A row without a message id cannot be answered, so it is dropped.
export function moveItems(entry) {
  const items = entry.details && Array.isArray(entry.details.items) ? entry.details.items : [];
  return items
    .filter((item) => item && typeof item.message_id === "string" && item.message_id !== "")
    .map((item) => ({ ...item, excluded: Boolean(item.excluded) }));
}

const DEST_LABELS = new Map([
  ["junkemail", "approval.dest_junk"],
  ["deleteditems", "approval.dest_deleted"],
  ["inbox", "approval.dest_inbox"],
]);

export function destLabel(dest) {
  return t(DEST_LABELS.get(dest) || "approval.dest_unknown");
}

// The core's triage verdict for a mail (not the level that holds the approval).
const TRIAGE_LABELS = new Map([
  ["phish_candidate", "approval.triage_phish"],
  ["suspicious", "approval.triage_suspicious"],
  ["bulk", "approval.triage_bulk"],
  ["clean", "approval.triage_clean"],
  ["unknown", "approval.triage_unknown"],
]);

export function triageLabel(level) {
  return t(TRIAGE_LABELS.get(level) || "approval.triage_unknown");
}

const SUBJECT_MAX = 80;

// The subject as the mail wrote it, trimmed to one short line.
export function subjectText(subject) {
  const text = String(subject || "").trim();
  if (!text) return t("approval.no_subject");
  return text.length > SUBJECT_MAX ? `${text.slice(0, SUBJECT_MAX - 1)}…` : text;
}

// The mails the host still moves: the ones nobody left out.
function liveItems(entry) {
  return moveItems(entry).filter((item) => !item.excluded);
}

// How many of the mails still in the batch stay checked, out of how many.
export function moveCounts(entry, unchecked = new Set()) {
  const live = liveItems(entry);
  const checked = live.filter((item) => !unchecked.has(item.message_id)).length;
  return { checked, total: live.length };
}

export function canApproveMove(entry, unchecked = new Set()) {
  return moveCounts(entry, unchecked).checked > 0;
}

export function moveTitle(entry) {
  const n = moveItems(entry).length;
  return t(n === 1 ? "approval.move_title.one" : "approval.move_title.other", { n });
}

export function moveApproveLabel(entry, unchecked = new Set()) {
  const { checked, total } = moveCounts(entry, unchecked);
  return t("approval.move_selected", { checked, total });
}

// A `mail.move` with mails in it earns the checklist; without them it is a plain card.
export function isMoveBatch(entry) {
  return isMailMove(entry) && moveItems(entry).length > 0;
}

// The ids the operator unchecked, as the host's `exclude` wants them: only mails of
// this batch that the host has not already left out.
export function excludeFor(entry, unchecked = new Set()) {
  return liveItems(entry)
    .filter((item) => unchecked.has(item.message_id))
    .map((item) => item.message_id);
}

// A retry reuses its client id only for the same decision with the same mails left out.
export function sameIds(a = [], b = []) {
  return a.length === b.length && a.every((id, index) => id === b[index]);
}

export function remainingSeconds(expiresAt, now = Date.now()) {
  const until = Date.parse(expiresAt);
  if (Number.isNaN(until)) return null;
  return Math.max(0, Math.ceil((until - now) / 1000));
}

export function countdownText(seconds) {
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${String(seconds % 60).padStart(2, "0")}`;
}

// The last folder of a path, as the operator names it: «bench-proj» from `C:\Users\Max\bench-proj`.
export function folderName(cwd) {
  const parts = String(cwd || "")
    .split(/[\\/]+/)
    .filter(Boolean);
  return parts.length ? parts[parts.length - 1] : "";
}

// A run's folder («…\runs\run_01M7…») is the host's, not the operator's project: it is never named.
const RUN_ID = /^run_[0-9a-z]+$/i;
export function isRunFolder(cwd) {
  const parts = String(cwd || "").split(/[\\/]+/).filter(Boolean);
  // shortcut: a project under a folder called runs is hidden too, upgrade when the host marks run folders apart
  return parts.some((part) => RUN_ID.test(part)) || parts.slice(0, -1).some((part) => part.toLowerCase() === "runs");
}

// The folder name a card may show: none for a run folder.
export function projectFolder(cwd) {
  return isRunFolder(cwd) ? "" : folderName(cwd);
}

// `D:\proyectos\demo` reads as `demo/…`: the last folder is the label.
export function cwdLabel(cwd) {
  const name = folderName(cwd);
  return name ? `${name}/…` : "";
}

export function shortDigest(digest) {
  if (!digest) return "";
  return digest.length > 16 ? `${digest.slice(0, 14)}…` : digest;
}

// Seat ids such as `01M5MA7C4KBNJ1H4Q6RSJHKVYP` are opaque: the host sends them
// as the agent's label too, and the operator never reads them.
const OPAQUE_ID = /^[0-9a-z]{20,}$/i;

// An approval carries only ids. The names and the contexts the operator reads
// come from the chat bootstrap, which the chat loads once per refresh.
const seatNames = new Map();
const contexts = new Map();
const projectOf = new Map();

export function setBootstrap(home) {
  seatNames.clear();
  contexts.clear();
  projectOf.clear();
  const bootstrap = (home && home.bootstrap) || {};
  const projects = bootstrap.projects || [];
  const projectNames = new Map(projects.map((project) => [project.id, project.name]));
  for (const seat of [bootstrap.alfred, ...projects.map((project) => project.lead)]) {
    if (seat && seat.agent_id) seatNames.set(seat.agent_id, seat.name);
  }
  for (const row of (home && home.conversations) || []) {
    const project = projectNames.get(row.project_id);
    contexts.set(row.id, [project, row.title].filter(Boolean).join(" · "));
    if (project) projectOf.set(row.id, project);
  }
}

// "Proyecto · Conversación" for the card of a conversation; empty when unknown.
export function conversationContext(conversationId) {
  return contexts.get(conversationId) || "";
}

// Only the project's name, for the plain card's subtitle; empty when unknown.
export function conversationProject(conversationId) {
  return projectOf.get(conversationId) || "";
}

// The label the CLI gave the agent. Without a readable one, a seat id such as
// `ag_bob` names "Bob". A seat of the bootstrap is named by its own record.
export function seatName(entry) {
  const known = seatNames.get(entry.seat_id);
  if (known) return known;
  const label = entry.agent_label && !OPAQUE_ID.test(entry.agent_label) ? entry.agent_label : "";
  if (label) return label;
  const match = /^ag_([a-z0-9_-]+)$/i.exec(String(entry.seat_id || ""));
  if (!match || OPAQUE_ID.test(match[1])) return t("approval.agent_unnamed");
  return match[1].charAt(0).toUpperCase() + match[1].slice(1);
}

// The device this app signs as. The host does not send it with an approval, so
// it is learned from the settings call, the first time a decision is refused.
let knownDeviceId = null;

export function setDeviceId(deviceId) {
  knownDeviceId = deviceId || null;
}

export function currentDeviceId() {
  return knownDeviceId;
}

export function grantCommand(deviceId) {
  return `host devices grant ${deviceId || t("approval.device_placeholder")} --permission approve`;
}

// The text a card or a line shows for the command: the redacted payload when
// the host sent one, otherwise the summary. It is shown as-is, never parsed.
export function commandText(entry) {
  const payload = entry.payload_redacted || {};
  return String(payload.command || entry.summary || "");
}

// The whole command of a shell request, or "" for any other tool.
export function shellCommand(entry) {
  return SHELL_TOOLS.has(String(entry.tool || "").toLowerCase()) ? commandText(entry) : "";
}

// «N líneas · M caracteres»: how much the operator is about to approve, counted on the text as sent.
export function commandSizeLine(text) {
  const plural = (n, one, many) => t(n === 1 ? one : many, { n });
  const lines = text.split("\n").length;
  const chars = [...text].length;
  return t("approval.command_stats", {
    lines: plural(lines, "approval.count_line", "approval.count_lines"),
    chars: plural(chars, "approval.count_char", "approval.count_chars"),
  });
}

// Local-only facts about an approval the host has not echoed yet: a request in
// flight, a 403, or the client id a retry must reuse. The host row wins for
// status; this only colours the card in between.
const local = new Map();

export function localFor(id) {
  return local.get(id) || {};
}

export function patchLocal(id, patch) {
  local.set(id, { ...localFor(id), ...patch });
}

// One approval, one line: when the feed carries several timeline rows for the
// same approval, only the newest one is drawn. An approval that already has a
// card is not drawn as a line as well. Returns the indexes to skip.
export function supersededIndexes(entries, cardIds = new Set()) {
  const newest = new Map();
  entries.forEach((entry, index) => {
    if (entry.kind === "approval" && entry.entry) newest.set(entry.entry.ref_id, index);
  });
  const skip = new Set();
  entries.forEach((entry, index) => {
    if (entry.kind !== "approval" || !entry.entry) return;
    const id = entry.entry.ref_id;
    if (cardIds.has(id) || newest.get(id) !== index) skip.add(index);
  });
  return skip;
}

// The host decides when an approval expires. This clock only marks the moment
// to ask it again: a pending card at zero is still pending until the host says.
export const CHECKING_TEXT = t("approval.checking");

export function isChecking(entry, now = Date.now()) {
  return (entry.status || "pending") === "pending" && countdownStarted(entry) && remainingSeconds(entry.expires_at, now) === 0;
}

// The TTL starts at the host's first showing (`shown_at`). Before it the card shows no
// countdown: the expiry the host sends then is only provisional.
export function countdownStarted(entry) {
  return Boolean(entry && entry.shown_at);
}

// `wait` for an operator-level card nobody has shown yet, `shown` once the host has it,
// and empty for the rest (the policy's and an agent's levels never start the TTL).
export function seenState(entry) {
  if ((entry.status || "pending") !== "pending") return "";
  if (countdownStarted(entry)) return "shown";
  return isOperatorLevel(entry) ? "wait" : "";
}

// A showing: a strip of the card (48 px, or the whole card when shorter) is in view and
// the window is visible. A tall card is seen without its full height on screen; a hidden
// tab, a card scrolled away, or a collapsed one (height 0) is not.
export const SHOWING_STRIP_PX = 48;
export function isShowing({ intersecting, visibleHeight, cardHeight, visibility }) {
  if (!intersecting || !(cardHeight > 0) || visibility !== "visible") return false;
  return visibleHeight >= Math.min(cardHeight, SHOWING_STRIP_PX);
}

// The observer fires each time the card's visible ratio crosses one of these steps (0.5% of the
// card), so a card that enters as a sliver and grows into its strip is evaluated again there.
export const SHOWING_STEPS = 200;
export const SHOWING_THRESHOLDS = Array.from({ length: SHOWING_STEPS + 1 }, (_, step) => step / SHOWING_STEPS);

// The approval ids of the observer entries that are a showing. Each entry's target is its card.
export function showingApprovalIds(entries, visibility) {
  return entries
    .filter((entry) =>
      isShowing({
        intersecting: entry.isIntersecting,
        visibleHeight: entry.intersectionRect.height,
        cardHeight: entry.boundingClientRect.height,
        visibility,
      }),
    )
    .map((entry) => entry.target.dataset.approval)
    .filter(Boolean);
}

// Reports each showing card, one request at a time. A card is released (no longer observed) only
// when the host confirms it, and a confirmed id is never reported again. A failed report keeps it
// observed, so its next exposure reports it again.
export function createShownReporter({ report, confirm, release }) {
  const inFlight = new Set();
  const confirmed = new Set();
  return {
    async handle(ids) {
      for (const id of new Set(ids)) {
        if (inFlight.has(id) || confirmed.has(id)) continue;
        inFlight.add(id);
        let record = null;
        try {
          record = await report(id);
        } catch {
          record = null;
        }
        inFlight.delete(id);
        if (!record) continue;
        confirmed.add(id);
        release(id);
        confirm(id, record);
      }
    },
  };
}

// «Siempre en esta tarea» needs an agent's seat. A mail request is one send or one batch, never a task.
export function canApproveTask(entry) {
  return Boolean(entry.seat_id) && !String(entry.tool || "").startsWith("mail.");
}

// A permanent yes beside a delete or a high risk is never offered: the card's delete is one-shot.
export function canAlwaysAllow(entry) {
  if (!canApproveTask(entry) || entry.risk === "high") return false;
  return !shellDelete(entry) && !/delete|remove|unlink/i.test(String(entry.tool || ""));
}

export const REASON_MAX = 300;

// The operator's words for a denial: trimmed and cut to the host's limit. Empty means no reason.
export function denyReason(text) {
  return Array.from(String(text ?? "").trim()).slice(0, REASON_MAX).join("");
}

// The arguments of `chat_approval_decision`. A choice the operator did not make stays out of the call.
export function decisionArgs(id, decision, clientId, exclude, extra = {}) {
  const args = { approvalId: id, decision, clientId, exclude };
  if (extra.wholeTask !== undefined) args.wholeTask = extra.wholeTask;
  if (extra.reason !== undefined) args.reason = extra.reason;
  return args;
}

// A retry reuses its client id only when the decision, the mails left out and the choice all match.
export function sameDecision(mine = {}, ask) {
  return (
    mine.decision === ask.decision &&
    sameIds(mine.exclude, ask.exclude) &&
    Boolean(mine.wholeTask) === Boolean(ask.wholeTask) &&
    (mine.reason || "") === (ask.reason || "")
  );
}

// The settled card's line. The host's `scope` says whether the whole task was allowed. `reason` on a card is why it was sent on, not the operator's words, so it is not shown.
export function resultText(status, entry = {}) {
  if (status === "approved" || status === "consumed") {
    return entry.scope === "task" ? t("approval.result_task") : t("approval.result_once");
  }
  if (status === "denied") return t("approval.result_denied");
  return statusMeta(status).label;
}

// Mail and agenda approvals keep their own card; every other request is a plain one.
export function isMailOrAgendaTool(entry) {
  return /^(mail|calendar|agenda)\./.test(String(entry.tool || ""));
}

const WRITE_TOOLS = new Set(["write", "edit", "multiedit", "notebookedit"]);
const SHELL_TOOLS = new Set(["bash", "shell", "powershell", "pwsh", "cmd"]);
const DELETE_VERB = "(?:sudo\\s+)?(?:rm|del|erase|rmdir|remove-item|unlink)";
// A delete that starts a command segment: after a separator, a subshell, a brace or a
// shell keyword (`if x; then rm y`). A word that only contains «del» or «rm» is no delete.
const DELETE_COMMAND = new RegExp(`(?:^|[;&|({\`\\n]\\s*|\\b(?:then|do|else)\\s+)${DELETE_VERB}\\s`, "i");
// A command that is only a delete: its verb, options and names. An operator, glob,
// variable or redirection makes it more than a delete, so it names nothing.
const DELETE_ONLY = new RegExp(`^${DELETE_VERB}\\s+[^;&|\`$<>*?()\\r\\n]+$`, "i");
const DELETE_OPTION = /^(?:-|\/[a-z]$)/i;

// The last folder or file of the first path: «uno-t.txt» from `D:\obra\uno-t.txt`.
function fileName(entry) {
  const first = Array.isArray(entry.paths) ? entry.paths[0] : "";
  const parts = String(first || "").split(/[\\/]+/).filter(Boolean);
  return parts.length ? parts[parts.length - 1] : "";
}

// The file names a command that only deletes removes: the last folder or file of
// each path, quotes off. Null when the command is more than a delete or names
// nothing clear («.» and «..» do not count).
export function deleteNames(command) {
  const text = String(command || "").trim();
  if (!DELETE_ONLY.test(text)) return null;
  const words = text.replace(/^sudo\s+/i, "").match(/"[^"]*"|'[^']*'|\S+/g);
  const names = [];
  for (const word of words.slice(1)) {
    if (DELETE_OPTION.test(word)) continue;
    const bare = word.replace(/^(["'])(.*)\1$/, "$2");
    if (/["']/.test(bare)) return null;
    const name = bare.split(/[\\/]+/).filter(Boolean).pop() || "";
    if (!name || name === "." || name === "..") return null;
    names.push(name);
  }
  return names.length ? names : null;
}

// A shell line ends each command at `&&`, `||`, `;` or `|`.
// shortcut: a separator inside quotes also splits, upgrade with a real shell tokenizer.
const COMMAND_END = /&&|\|\||[;|]/;

// What a shell command deletes: null when it deletes nothing. `names` is empty when a
// delete cannot be named; `more` says other commands run too.
function shellDelete(entry) {
  const command = commandText(entry);
  if (!SHELL_TOOLS.has(String(entry.tool || "").toLowerCase()) || !DELETE_COMMAND.test(command)) return null;
  const parts = command.split(COMMAND_END).map((part) => part.trim()).filter(Boolean);
  const deletes = parts.filter((part) => DELETE_COMMAND.test(part));
  const named = deletes.map((part) => deleteNames(part));
  return {
    names: named.every(Boolean) ? named.flat() : [],
    // A read beside the delete (`cat x && rm x`) hides nothing; any other command is said.
    more: parts.some((part) => !DELETE_COMMAND.test(part) && !isLookCommand(part)),
  };
}

// «a», «b» y N más: two names at most, the rest counted.
function namesText(names) {
  const quoted = names.map((name) => `«${name}»`);
  if (quoted.length <= 2) return quoted.length === 2 ? t("approval.names_two", { a: quoted[0], b: quoted[1] }) : quoted[0];
  return t("approval.names_more", { names: quoted.slice(0, 2).join(", "), n: quoted.length - 2 });
}

// The agent's headline can come in English («create x.txt with text "uno"»). The operator reads
// Spanish: the verb picks the action, and the file (the card's path, or a name with an extension) names it.
const ENGLISH_ACTIONS = new Map([
  ["create", "approval.verb_save"], ["write", "approval.verb_save"], ["edit", "approval.verb_save"], ["update", "approval.verb_save"], ["add", "approval.verb_save"],
  ["fix", "approval.verb_save"], ["build", "approval.verb_save"], ["copy", "approval.verb_save"], ["rename", "approval.verb_save"], ["move", "approval.verb_save"],
  ["delete", "approval.verb_delete"], ["remove", "approval.verb_delete"], ["read", "approval.verb_read"],
  ["run", "approval.verb_run"], ["install", "approval.verb_run"], ["show", "approval.verb_run"], ["inspect", "approval.verb_run"], ["check", "approval.verb_run"],
  ["list", "approval.verb_run"], ["open", "approval.verb_run"], ["test", "approval.verb_run"], ["download", "approval.verb_run"], ["upload", "approval.verb_run"],
  ["git", "approval.verb_run"],
]);

// English: no Spanish accent or «ñ», and it opens with one of those verbs or carries a common English word
// («read-only check of workspaces and search for x»).
const ENGLISH_WORDS = /\b(the|and|of|for|with|from|read-only|search|check)\b/i;
function isEnglish(text) {
  if (/[áéíóúüñ]/i.test(text)) return false;
  return ENGLISH_ACTIONS.has(text.split(/\s+/)[0].toLowerCase()) || ENGLISH_WORDS.test(text);
}

// Commands that only look. Every part of the line starts with one of these; `git` counts only
// for its reading subcommands. A redirection, a lone «&», `$(`, `--output`, `--pre` or `--ext-diff`
// can write or run something, so such a line is never a look.
const LOOK_WORDS = new Set([
  "ls", "dir", "cat", "type", "head", "tail", "grep", "rg", "cd", "pwd", "echo", "wc",
  "get-content", "get-childitem", "gci", "set-location",
]);
const LOOK_GIT = new Set(["status", "diff", "log", "show", "ls-files", "rev-parse", "branch"]);
// «git branch» only lists: a name, -d, -m or any other flag can change branches, so only these forms look.
const BRANCH_LIST = /^git branch(\s+(--show-current|-a|--all|-r|--remotes|-v|-vv|--verbose|-l|--list))*$/i;
const WRITES = /[<>`]|\$\(|--output|--pre\b|--ext-diff|(?<!&)&(?!&)/;

export function isLookCommand(command) {
  const text = String(command || "").trim();
  if (!text || WRITES.test(text)) return false;
  const parts = text.split(/&&|\|\||[;|\r\n]/).map((part) => part.trim()).filter(Boolean);
  return parts.length > 0 && parts.every((part) => {
    const [word, ...args] = part.split(/\s+/);
    const first = word.toLowerCase();
    if (first !== "git") return LOOK_WORDS.has(first);
    // `git -C <folder> status` is `git status`; the host judges whether the folder is the project.
    const sub = ((args[0] === "-C" ? args[2] : args[0]) || "").toLowerCase();
    return LOOK_GIT.has(sub) && (sub !== "branch" || BRANCH_LIST.test(part));
  });
}

// The host's category decides when it sends one; the words of the command decide otherwise.
function looksOnly(entry) {
  if (!SHELL_TOOLS.has(String(entry.tool || "").toLowerCase())) return false;
  // A risk the host rated medium or high (a path outside the project) is never called a look.
  if (entry.risk === "high" || entry.risk === "medium") return false;
  if (entry.category) return entry.category === "read";
  return isLookCommand(commandText(entry));
}

// The one file a look names («notas.txt» in `grep rm notas.txt`). Two or none name the project.
function lookedAtFile(command) {
  const names = new Set();
  for (const word of String(command).match(/"[^"]*"|'[^']*'|\S+/g) || []) {
    const bare = word.replace(/^(["'])(.*)\1$/, "$2");
    const name = bare.split(/[\\/]+/).pop();
    if (/^[^-].*\.[a-z][a-z0-9]{0,5}$/i.test(name)) names.add(name);
  }
  return names.size === 1 ? [...names][0] : "";
}

function lookPhrase(entry) {
  const file = lookedAtFile(commandText(entry));
  return file ? t("approval.phrase_look_file", { file }) : t("approval.phrase_look_project");
}

// The host writes a search as «Buscar en internet: <consulta>»; a page's address is in its summary.
function webQuery(entry) {
  const said = /^buscar en internet:\s*(.+)$/i.exec(String(entry.summary || "").trim());
  return said ? said[1].trim() : "";
}

function pageAddress(entry) {
  const said = /https?:\/\/\S+/.exec(String(entry.summary || ""));
  return said ? said[0] : "";
}

// What the agent wants to do, in words a non-programmer reads. The shell's
// command is never shown here: a delete is named from its files, or as a delete.
export function actionPhrase(entry) {
  const tool = String(entry.tool || "").toLowerCase();
  const name = fileName(entry);
  if (WRITE_TOOLS.has(tool)) return name ? t("approval.phrase_named", { verb: t("approval.verb_save"), name }) : t("approval.phrase_save_file");
  if (SHELL_TOOLS.has(tool)) {
    const deleted = shellDelete(entry);
    if (deleted) {
      if (!deleted.names.length) return t("approval.phrase_delete_files");
      return t("approval.phrase_delete", { names: namesText(deleted.names), more: deleted.more ? t("approval.phrase_delete_more") : "" });
    }
    if (looksOnly(entry)) return lookPhrase(entry);
    // A headline that says delete keeps «borrar»: a delete shown as «ejecutar un comando» hides what it does.
    if (/^(delete|remove)\b/i.test(String(entry.headline || "").trim())) {
      return name ? t("approval.phrase_named", { verb: t("approval.verb_delete"), name }) : t("approval.phrase_delete_files");
    }
    // A command runs, it never «guarda»: the agent's own sentence stays under «Detalles técnicos», not in the title.
    return t("approval.phrase_run");
  }
  if (tool === "read") return name ? t("approval.phrase_named", { verb: t("approval.verb_read"), name }) : t("approval.phrase_read_file");
  // Without the words the host sent, the generic phrase stays.
  if (tool === "websearch" && webQuery(entry)) return t("approval.phrase_search", { query: webQuery(entry) });
  if (tool === "webfetch" && pageAddress(entry)) return t("approval.phrase_open_page", { address: pageAddress(entry) });
  return t("approval.phrase_tool");
}

// The card's title: «Ernesto quiere guardar «uno-t.txt»».
export function humanHeadline(entry) {
  return t("approval.headline", { name: seatName(entry), action: actionPhrase(entry) });
}

// A shell command or a delete can change files: say so, and only then. A look never does.
export function riskNote(entry) {
  const risky = SHELL_TOOLS.has(String(entry.tool || "").toLowerCase()) && !looksOnly(entry);
  return entry.risk === "high" && risky ? t("approval.risk_note") : "";
}

// The risk badge shows only a medium or high risk. A look shows none, and a missing risk is never guessed.
export function riskBadge(entry) {
  if (looksOnly(entry)) return "";
  return entry.risk === "medium" || entry.risk === "high" ? riskLabel(entry.risk) : "";
}

// «Detalles» of a plain card: only the file. Everything else sits in one level under «Detalles técnicos»,
// with the full folder. The folder's name is on the card; the command (any shell tool's) leads the technical level.
export function detailFacts(entry) {
  const paths = Array.isArray(entry.paths) ? entry.paths.join(", ") : "";
  const shown = [[t("approval.fact_path"), paths]];
  const said = String(entry.headline || "").trim();
  const command = shellCommand(entry);
  const technical = [
    [t("approval.fact_command"), command],
    [t("approval.fact_folder"), isRunFolder(entry.cwd) ? "" : entry.cwd || ""],
    [t("approval.fact_original"), isEnglish(said) ? said : ""],
    [t("approval.fact_risk"), riskBadge(entry)],
    [t("approval.fact_level"), entry.escalated_from ? t("approval.escalated") : ""],
  ];
  const keep = ([, value]) => Boolean(value);
  return { shown: shown.filter(keep), technical: technical.filter(keep) };
}

// The phrase each approval's card named, so its timeline line can name the same action.
const namedActions = new Map();

export function rememberAction(entry) {
  if (entry && entry.id) namedActions.set(entry.id, actionPhrase(entry));
}

// The host writes a timeline line as «Pendiente: command Bash». The tool's name is no words
// for the operator: the line names the action its card named, or a plain «ejecutar un comando».
const TOOL_WORDS = /^(?:(.+?):\s*)?(?:command|tool)\s+\S+$/i;

// A request still waiting has its card, which already asks for the operator's OK: no timeline line for it.
export function isWaitingLine(summary) {
  const match = TOOL_WORDS.exec(String(summary || "").trim());
  return Boolean(match) && /^pendiente$/i.test(match[1] || "");
}

// The host's «Operador aprobó» is the operator's own act: the line says «Aprobado: <what it was>», never the
// tool's name («edit Write»). Without the card's phrase it says only «Aprobado».
export function lineSummary(summary, refId) {
  if (isWaitingLine(summary)) return "";
  const said = String(summary || "").trim();
  if (/^operador aprobó:/i.test(said)) {
    const named = namedActions.get(refId);
    return named ? t("approval.line_approved_named", { what: named }) : t("approval.line_approved");
  }
  const text = said
    .replace(/^operador denegó:/i, t("approval.line_denied"))
    .replace(/^host expiró:/i, t("approval.line_expired"));
  const match = TOOL_WORDS.exec(text);
  if (!match) return text;
  const phrase = namedActions.get(refId);
  const named = phrase || t("approval.phrase_run");
  return match[1] ? `${match[1]}: ${named}` : named;
}

// A denial its card already settles («✕ Denegado · X») is not repeated as a timeline line.
export function repeatsCard(entry, cardIds) {
  return Boolean(entry) && entry.status === "denied" && cardIds.has(entry.ref_id);
}

// The one line a decided request leaves in the thread: what was decided, and what it was about.
export function settledLine(status, entry = {}) {
  const phrase = actionPhrase(entry);
  if (status === "approved" || status === "consumed") {
    return entry.scope === "task" ? t("approval.settled_task", { phrase }) : t("approval.settled_once", { phrase });
  }
  if (status === "denied") return t("approval.settled_denied", { phrase });
  if (status === "expired") return t("approval.settled_expired", { phrase });
  return `${statusMeta(status).label} · ${phrase}`;
}
