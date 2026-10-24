// Approval cards for the operator's level, and the one-line timeline record
// for what was decided below him. Everything the agent wrote goes in as text.

import {
  CHECKING_TEXT,
  commandText,
  currentDeviceId,
  cwdLabel,
  grantCommand,
  isChecking,
  isOperatorLevel,
  localFor,
  riskLabel,
  seatName,
  shortDigest,
  statusMeta,
} from "./approval-model.js";

// Each card remembers its approval and its context so a decision can redraw it
// in place.
export const cardEntries = new WeakMap();

export function element(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function chip(text, className) {
  return element("span", `approval-chip ${className || ""}`.trim(), text);
}

// The host's final status wins; until it has one, our own decision shows.
function resolvedStatus(entry, mine) {
  const host = entry.status && entry.status !== "pending" ? entry.status : "";
  return host || mine.decided || entry.status || "pending";
}

function actionButton(label, action, id, className, disabled) {
  const button = element("button", className, label);
  button.type = "button";
  button.dataset.approvalAction = action;
  button.dataset.approvalId = id;
  button.disabled = disabled;
  return button;
}

// A 403 keeps the buttons open: the operator may have just been granted APPROVE,
// and a retry is the only way to find out. The note says what is missing.
function actionsRow(entry, status, mine) {
  const row = element("div", "approval-actions");
  const open = status === "pending" && !mine.busy;
  const id = entry.id;
  row.append(
    actionButton("Permitir una vez", "approve", id, "btn primary btn-small approval-btn", !open),
    actionButton("Denegar", "deny", id, "btn ghost btn-small approval-btn", !open),
  );
  return row;
}

function forbiddenNote() {
  const box = element("div", "approval-error");
  box.setAttribute("role", "alert");
  box.append(
    element("p", null, "Este equipo no tiene el permiso APPROVE: no puede decidir esta aprobación."),
    element("code", "approval-grant", grantCommand(currentDeviceId())),
  );
  return box;
}

// `context` is the conversation the request came from; it is kept with the card
// so a redraw does not lose it.
export function renderApprovalCard(entry, now = Date.now(), context = "") {
  const id = entry.id;
  const mine = localFor(id);
  const status = resolvedStatus(entry, mine);
  const meta = statusMeta(status);
  const card = element("article", `approval-card tone-${meta.tone}`);
  card.dataset.approval = id;
  cardEntries.set(card, { entry, context });

  const head = element("div", "approval-head");
  const titleWrap = element("div", "approval-title-wrap");
  titleWrap.append(
    element("p", "approval-kicker", "PERMISO"),
    element("h3", "approval-title", `${seatName(entry)} necesita permiso`),
  );
  head.append(titleWrap, chip(meta.label, `status tone-${meta.tone}`));
  card.append(head);

  if (context) card.append(element("p", "approval-where", context));
  // A request for lack of information asks its question where the command goes.
  if (entry.question) card.append(element("p", "approval-question", entry.question));
  else card.append(element("pre", "approval-command", commandText(entry) || "Sin comando"));

  const chips = element("div", "approval-chips");
  if (entry.cwd) {
    const cwd = chip(cwdLabel(entry.cwd), "cwd");
    cwd.title = entry.cwd;
    chips.append(cwd);
  }
  chips.append(chip(riskLabel(entry.risk), `risk risk-${entry.risk || "unknown"}`));
  if (entry.escalated_from) chips.append(chip("te lo pasa Alfred", "level"));
  card.append(chips);

  const footer = element("div", "approval-footer");
  const digest = element("code", "approval-digest", shortDigest(entry.action_digest));
  digest.setAttribute("aria-label", "Huella de la acción");
  footer.append(digest);
  if (status === "pending") {
    const checking = isChecking(entry, now);
    const ttl = element("span", "approval-ttl", checking ? CHECKING_TEXT : "");
    ttl.dataset.expires = entry.expires_at || "";
    footer.append(ttl);
  }
  card.append(footer);

  if (mine.forbidden) card.append(forbiddenNote());
  if (status === "pending") card.append(actionsRow(entry, status, mine));
  else if (status === "expired") card.append(element("p", "approval-result", "Se agotó el tiempo: no se ejecutó."));
  else card.append(element("p", "approval-result", resultText(status)));

  return card;
}

function resultText(status) {
  if (status === "approved" || status === "consumed") return "Permitida una vez. Solo esta acción.";
  if (status === "denied") return "Denegada. El agente no la ejecutó.";
  return statusMeta(status).label;
}

// A host approval of the operator's level is a card. Anything decided below him
// is a timeline line, and a lower-level request that is still open stays hidden.
export function renderApprovalEntry(approval, now = Date.now(), context = "") {
  if (!approval || !approval.id || !isOperatorLevel(approval)) return null;
  return renderApprovalCard(approval, now, context);
}

function fact(list, label, value) {
  if (!value) return;
  list.append(element("dt", null, label), element("dd", null, value));
}

// The timeline carries the host's own sentence and the time; the chain and the
// audit trail are not on the timeline, so the line does not invent them.
function lineDetail(entry, meta) {
  const detail = element("div", "approval-line-detail");
  const facts = element("dl", "approval-facts");
  fact(facts, "Estado", meta.label);
  if (entry.created_at) {
    fact(facts, "Hora", new Date(entry.created_at).toLocaleTimeString("es-ES", { hour: "2-digit", minute: "2-digit" }));
  }
  detail.append(facts);
  return detail;
}

// One timeline row (`kind=approval`) for an approval decided below the operator.
export function renderApprovalLine(entry) {
  if (!entry || !entry.ref_id) return null;
  const meta = statusMeta(entry.status);
  const line = element("details", `approval-line tone-${meta.tone}`);
  line.dataset.approval = entry.ref_id;
  line.append(element("summary", "approval-line-summary", entry.summary || meta.label), lineDetail(entry, meta));
  return line;
}
