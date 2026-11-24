// Pure rules for the mail screens: no DOM, no invoke. The settings, the draft
// view and the send flow read the same rules. The host's failures arrive with
// their Spanish text already in the core; the texts below are only the ones the
// screen itself says, such as an approval that ended without a send.

import { countdownText } from "./approval-model.js";
import { t } from "./i18n.js";

// The sign-in kinds the operator picks; the host takes the tenant.
export const SIGN_IN_KINDS = {
  personal: { tenant: "consumers", label: t("mail.signin.personal_label") },
  work: { tenant: "organizations", label: t("mail.signin.work_label") },
};

export const APPROVAL_DENIED_TEXT = t("mail.approval_denied");
export const APPROVAL_STALE_TEXT = t("mail.approval_stale");
export const APPROVAL_GONE_TEXT = t("mail.approval_gone");
export const SEND_FAILED_TEXT = t("mail.send_failed");

// Where a send flow stands. `asking`, `awaiting`, `send` and `window` are open;
// the rest are final and stop every timer of the flow.
export const OPEN_PHASES = ["asking", "awaiting", "send", "window"];

export function isOpen(phase) {
  return OPEN_PHASES.includes(phase);
}

export function signInTenant(kind) {
  return (SIGN_IN_KINDS[kind] || {}).tenant || null;
}

export function accountKindLabel(tenant) {
  return tenant === "consumers" ? t("mail.kind_personal") : t("mail.kind_work");
}

export function secondsUntil(untilMs, now = Date.now()) {
  return Math.max(0, Math.ceil((untilMs - now) / 1000));
}

export function countdownFor(untilMs, now = Date.now()) {
  return countdownText(secondsUntil(untilMs, now));
}

// The sign-in card counts down to the moment the host gave: `expires_in` seconds
// from the answer, read here as the local clock.
export function expiryMs(expiresInSeconds, now = Date.now()) {
  return now + Math.max(0, Number(expiresInSeconds) || 0) * 1000;
}

// What a sign-in status says. `done` stops the poll; `ok` decides the copy.
export function signInOutcome(status) {
  if (!status || status.status === "pending") return { done: false };
  if (status.status === "ok") {
    return { done: true, ok: true, text: t("mail.signin.connected", { who: status.upn || status.label }) };
  }
  return { done: true, ok: false, text: status.message || t("mail.signin.not_completed") };
}

// The approval the host holds for a send. `send` means the operator approved and
// the send may be asked for now; every other end is final.
export function approvalStep(approval) {
  if (!approval) return { next: "failed", text: APPROVAL_GONE_TEXT };
  const status = approval.status || "pending";
  if (status === "pending") return { next: "awaiting" };
  if (status === "approved") return { next: "send" };
  if (status === "denied" || status === "expired") return { next: "failed", text: APPROVAL_DENIED_TEXT };
  return { next: "failed", text: APPROVAL_STALE_TEXT };
}

// The state of a scheduled send, as the host reports it.
export function sendStep(sendState) {
  const state = sendState && sendState.state;
  if (state === "pending") return { next: "window" };
  if (state === "sent") return { next: "sent" };
  if (state === "cancelled") return { next: "cancelled" };
  return { next: "failed", text: SEND_FAILED_TEXT };
}

// The line the draft view shows for a flow. The window names its countdown; the
// other phases name what the operator must do next.
export function phaseText(phase, { windowLeft = 0, text = "" } = {}) {
  switch (phase) {
    case "asking":
      return t("mail.phase.asking");
    case "awaiting":
      return t("mail.phase.awaiting");
    case "send":
      return t("mail.phase.send");
    case "window":
      return t("mail.phase.window", { s: windowLeft });
    case "sent":
      return t("mail.phase.sent");
    case "cancelled":
      return t("mail.phase.cancelled");
    default:
      return text || SEND_FAILED_TEXT;
  }
}

export function formatBytes(bytes) {
  const size = Number(bytes) || 0;
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${Math.round(size / 1024)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}

// The recipients of a draft as one line; an empty list reads as a dash.
export function addressLine(addresses) {
  return Array.isArray(addresses) && addresses.length ? addresses.join(", ") : "—";
}

// At rest no send is open or scheduled on the draft. The edit, «No enviar» and
// «Pedir cambios a Alfred» are offered only there, as in the APK.
const REST_PHASES = ["idle", "failed", "cancelled"];

export function isAtRest(phase) {
  return phase === null || REST_PHASES.includes(phase);
}

// «Pedir cambios a Alfred» needs a chat to answer in: only a draft opened from one has it.
export function draftActions(phase, alfredCanRevise) {
  const rest = isAtRest(phase);
  return { edit: rest, decline: rest, requestChanges: rest && alfredCanRevise };
}

// What the composer gets when the operator asks Alfred for changes: it names the draft.
export function changeRequestText(draftId) {
  return t("mail.change_request", { id: draftId });
}

export function accountsState(accounts) {
  if (!accounts) return "loading";
  return accounts.length ? "list" : "empty";
}

// A failure the operator can fix in the settings, not only read.
export function needsSettings(code) {
  return code === "mail_send_not_enabled" || code === "forbidden";
}

// "nombre (12 KB), otro (3.0 MB)"; no attachments read as a dash.
export function attachmentLine(attachments) {
  if (!Array.isArray(attachments) || !attachments.length) return "—";
  return attachments.map((file) => `${file.name} (${formatBytes(file.size)})`).join(", ");
}

// What the operator reads before he approves a send: the fields the digest
// covers, as label/value rows, read from the host's `details`.
export function digestRows(details) {
  return [
    [t("mail.digest.account"), details.account || "—"],
    [t("mail.digest.from"), details.from || "—"],
    [t("mail.digest.to"), addressLine(details.to)],
    ["Cc", addressLine(details.cc)],
    [t("mail.digest.subject"), details.subject || "—"],
    [t("mail.digest.attachments"), attachmentLine(details.attachments)],
    [t("mail.digest.body"), details.body_preview || "—"],
  ];
}

// The index a radio group moves to on an arrow key, wrapping at the ends; null for any other key.
const RADIO_STEPS = new Map([
  ["ArrowDown", 1],
  ["ArrowRight", 1],
  ["ArrowUp", -1],
  ["ArrowLeft", -1],
]);

export function radioStep(index, key, count) {
  const step = RADIO_STEPS.get(key);
  return step === undefined ? null : (index + step + count) % count;
}
