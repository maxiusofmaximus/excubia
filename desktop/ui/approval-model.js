// Pure rules for approval cards and timeline lines: no DOM, no invoke. The
// card, the line, the settings and the tests all read the same rules, so the
// host's words reach the screen through one path. The host owns the state.

export const LEVELS = ["policy", "scorer", "lead", "cto", "user"];

const LEVEL_LABELS = {
  policy: "Reglas",
  scorer: "Revisión automática",
  lead: "Responsable del proyecto",
  cto: "Alfred",
  user: "Tú",
};

const STATUS_META = {
  pending: { label: "Pendiente", tone: "pending" },
  approved: { label: "Permitida una vez", tone: "approved" },
  consumed: { label: "Permitida y usada", tone: "approved" },
  denied: { label: "Denegada", tone: "denied" },
  expired: { label: "Expirada: no se ejecutó", tone: "expired" },
};

const RISK_LABELS = {
  low: "Riesgo bajo",
  medium: "Riesgo medio",
  high: "Riesgo alto",
};

export function statusMeta(status) {
  return STATUS_META[status] || { label: status || "Sin estado", tone: "unknown" };
}

export function levelLabel(level) {
  return LEVEL_LABELS[level] || "Sin nivel";
}

export function riskLabel(risk) {
  return RISK_LABELS[risk] || "Riesgo sin evaluar";
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
  ["junkemail", "No deseado"],
  ["deleteditems", "Eliminados"],
  ["inbox", "Bandeja de entrada"],
]);

export function destLabel(dest) {
  return DEST_LABELS.get(dest) || "Destino desconocido";
}

// The core's triage verdict for a mail (not the level that holds the approval).
const TRIAGE_LABELS = new Map([
  ["phish_candidate", "Posible phishing"],
  ["suspicious", "Sospechoso"],
  ["bulk", "Masivo"],
  ["clean", "Limpio"],
  ["unknown", "Sin evaluar"],
]);

export function triageLabel(level) {
  return TRIAGE_LABELS.get(level) || "Sin evaluar";
}

const SUBJECT_MAX = 80;

// The subject as the mail wrote it, trimmed to one short line.
export function subjectText(subject) {
  const text = String(subject || "").trim();
  if (!text) return "(sin asunto)";
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
  return `${moveItems(entry).length} correos a mover`;
}

export function moveApproveLabel(entry, unchecked = new Set()) {
  const { checked, total } = moveCounts(entry, unchecked);
  return `Mover los seleccionados (${checked} de ${total})`;
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

// `D:\proyectos\demo` reads as `demo/…`: the last folder is the label.
export function cwdLabel(cwd) {
  const parts = String(cwd || "")
    .split(/[\\/]+/)
    .filter(Boolean);
  return parts.length ? `${parts[parts.length - 1]}/…` : "";
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

export function setBootstrap(home) {
  seatNames.clear();
  contexts.clear();
  const bootstrap = (home && home.bootstrap) || {};
  const projects = bootstrap.projects || [];
  const projectNames = new Map(projects.map((project) => [project.id, project.name]));
  for (const seat of [bootstrap.alfred, ...projects.map((project) => project.lead)]) {
    if (seat && seat.agent_id) seatNames.set(seat.agent_id, seat.name);
  }
  for (const row of (home && home.conversations) || []) {
    const project = projectNames.get(row.project_id);
    contexts.set(row.id, [project, row.title].filter(Boolean).join(" · "));
  }
}

// "Proyecto · Conversación" for the card of a conversation; empty when unknown.
export function conversationContext(conversationId) {
  return contexts.get(conversationId) || "";
}

// The label the CLI gave the agent. Without a readable one, a seat id such as
// `ag_bob` names "Bob". A seat of the bootstrap is named by its own record.
export function seatName(entry) {
  const known = seatNames.get(entry.seat_id);
  if (known) return known;
  const label = entry.agent_label && !OPAQUE_ID.test(entry.agent_label) ? entry.agent_label : "";
  if (label) return label;
  const match = /^ag_([a-z0-9_-]+)$/i.exec(String(entry.seat_id || ""));
  if (!match || OPAQUE_ID.test(match[1])) return "Un agente";
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
  return `host devices grant ${deviceId || "<dispositivo>"} --permission approve`;
}

// The text a card or a line shows for the command: the redacted payload when
// the host sent one, otherwise the summary. It is shown as-is, never parsed.
export function commandText(entry) {
  const payload = entry.payload_redacted || {};
  return String(payload.command || entry.summary || "");
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
export const CHECKING_TEXT = "Comprobando con el PC…";

export function isChecking(entry, now = Date.now()) {
  return (entry.status || "pending") === "pending" && remainingSeconds(entry.expires_at, now) === 0;
}
