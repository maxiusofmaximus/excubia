// Approval cards for the operator's level, and the one-line timeline record
// for what was decided below him. Everything the agent wrote goes in as text.

import { locale, t } from "./i18n.js";
import {
  canAlwaysAllow,
  canApproveMove,
  CHECKING_TEXT,
  commandText,
  conversationProject,
  currentDeviceId,
  cwdLabel,
  detailFacts,
  grantCommand,
  humanHeadline,
  isChecking,
  isMailOrAgendaTool,
  isMailSend,
  isMoveBatch,
  isOperatorLevel,
  isPolicyLine,
  isWaitingLine,
  lineSummary,
  localFor,
  moveApproveLabel,
  moveTitle,
  projectFolder,
  resultText,
  riskBadge,
  riskNote,
  seatName,
  countdownStarted,
  seenState,
  settledLine,
  shellCommand,
  commandSizeLine,
  shortDigest,
  statusMeta,
} from "./approval-model.js";
import { moveBatch } from "./approval-move.js";
import { digestRows } from "./mail-model.js";
import { githubSection } from "./github-link-card.js";
import { githubExplanation, githubMissing, isGithubLinkTool } from "./github-link-model.js";

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

// A batch says how many mails stay checked, and needs one of them to move.
function approveButton(entry, open, unchecked) {
  const className = "btn primary btn-small approval-btn";
  if (!isMoveBatch(entry)) return actionButton(t("approval.allow_once"), "approve", entry.id, className, !open);
  const label = moveApproveLabel(entry, unchecked);
  return actionButton(label, "approve", entry.id, className, !open || !canApproveMove(entry, unchecked));
}

// A 403 keeps the buttons open: the operator may have just been granted APPROVE,
// and a retry is the only way to find out. The note says what is missing.
// «Siempre en esta tarea» and «Denegar con motivo…» sit beside the one-shot buttons.
function actionsRow(entry, status, mine) {
  const row = element("div", "approval-actions");
  const open = status === "pending" && !mine.busy;
  const ghost = "btn ghost btn-small approval-btn";
  row.append(approveButton(entry, open, mine.unchecked));
  if (canAlwaysAllow(entry)) row.append(actionButton(t("approval.allow_task"), "approve-task", entry.id, ghost, !open));
  row.append(
    actionButton(t("approval.deny"), "deny", entry.id, ghost, !open),
    actionButton(t("approval.deny_reason"), "deny-reason", entry.id, ghost, !open),
  );
  return row;
}

// What the operator reads before a send: the host's `details`, as text only.
function mailDigest(details) {
  const list = element("dl", "mail-digest");
  for (const [label, value] of digestRows(details)) {
    list.append(element("dt", null, label), element("dd", null, value));
  }
  return list;
}

function forbiddenNote() {
  const box = element("div", "approval-error");
  box.setAttribute("role", "alert");
  box.append(
    element("p", null, t("approval.no_permission")),
    element("code", "approval-grant", grantCommand(currentDeviceId())),
  );
  return box;
}

// A request for a plain action: the sentence, one primary button, and the rest
// one click away. What the operator does not need to decide sits under «Detalles».
export function renderApprovalCard(entry, now = Date.now(), context = "") {
  if (isGithubLinkTool(entry)) return renderGithubCard(entry, now, context);
  return isMailOrAgendaTool(entry) ? renderMailCard(entry, now, context) : renderPlainCard(entry, now);
}

function plainActions(entry, open, ttl) {
  const row = element("div", "approval-actions");
  const ghost = "btn ghost approval-btn";
  row.append(actionButton(t("approval.allow"), "approve", entry.id, "btn primary approval-btn", !open));
  row.append(actionButton(t("approval.deny"), "deny", entry.id, ghost, !open));
  const more = element("details", "approval-more");
  more.append(element("summary", ghost, t("approval.more")));
  const menu = element("div", "approval-more-menu");
  if (canAlwaysAllow(entry)) menu.append(actionButton(t("approval.allow_task"), "approve-task", entry.id, ghost, !open));
  menu.append(actionButton(t("approval.deny_reason"), "deny-reason", entry.id, ghost, !open));
  more.append(menu);
  row.append(more, ttl);
  return row;
}

// The whole command, with its size above it. Long lines wrap and a tall command scrolls in its own box,
// so the buttons under it stay in view.
function commandBlock(command) {
  const box = element("div", "approval-command-box");
  const full = element("pre", "approval-command-full", command);
  full.tabIndex = 0;
  box.append(element("p", "approval-command-size", commandSizeLine(command)), full);
  return box;
}

// «Detalles» holds the file's path; «Detalles técnicos» is one level below it, or sits on the card when there is no path.
// The command is on the card already, so the technical level leaves it out.
function plainDetails(entry) {
  const { shown, technical: all } = detailFacts(entry);
  const command = shellCommand(entry);
  const technical = all.filter(([, value]) => value !== command);
  let tech = null;
  if (technical.length) {
    tech = element("details", "approval-tech");
    tech.append(element("summary", null, t("approval.tech_details")));
    const techFacts = element("dl", "approval-facts");
    for (const [label, value] of technical) fact(techFacts, label, value);
    tech.append(techFacts);
  }
  if (!shown.length) return tech;
  const box = element("details", "approval-details");
  box.append(element("summary", null, t("approval.details")));
  const facts = element("dl", "approval-facts");
  for (const [label, value] of shown) fact(facts, label, value);
  box.append(facts);
  if (tech) box.append(tech);
  return box;
}

// Decided, the request is one quiet line; pending, it is the sentence and the buttons.
function renderPlainCard(entry, now) {
  const id = entry.id;
  const mine = localFor(id);
  const status = resolvedStatus(entry, mine);
  const meta = statusMeta(status, entry);
  const card = element("article", `approval-card plain tone-${meta.tone}`);
  card.dataset.approval = id;
  card.dataset.seen = seenState(entry);
  card.dataset.stamp = t("approval.stamp");
  cardEntries.set(card, { entry, context: "" });

  if (status !== "pending") {
    card.append(element("p", "approval-settled", settledLine(status, entry)));
    return card;
  }
  card.append(element("h3", "approval-title", humanHeadline(entry)));
  const project = conversationProject(entry.conversation_id);
  if (project) card.append(element("p", "approval-where", project));
  if (entry.question) card.append(element("p", "approval-question", entry.question));
  // The folder's name stays on the card; the command and the full path wait under «Detalles técnicos».
  const folder = projectFolder(entry.cwd);
  if (folder) {
    const where = element("p", "approval-where", t("approval.folder", { folder }));
    where.title = entry.cwd;
    card.append(where);
  }
  const risk = riskNote(entry);
  if (risk) card.append(element("p", "approval-note", risk));
  const command = shellCommand(entry);
  if (command) card.append(commandBlock(command));

  const ttl = element("span", "approval-ttl", isChecking(entry, now) ? CHECKING_TEXT : "");
  ttl.dataset.expires = countdownStarted(entry) ? entry.expires_at || "" : "";
  card.append(plainActions(entry, !mine.busy && !isChecking(entry, now), ttl));
  if (mine.forbidden) card.append(forbiddenNote());
  const details = plainDetails(entry);
  if (details) card.append(details);
  return card;
}

// Mail and agenda cards keep their own layout: the checklist, the digest and the four buttons.
// `context` is the conversation the request came from; it is kept with the card
// so a redraw does not lose it.
function renderMailCard(entry, now = Date.now(), context = "") {
  const id = entry.id;
  const mine = localFor(id);
  const status = resolvedStatus(entry, mine);
  const meta = statusMeta(status, entry);
  const card = element("article", `approval-card tone-${meta.tone}`);
  card.dataset.approval = id;
  card.dataset.seen = seenState(entry);
  card.dataset.stamp = t("approval.stamp");
  cardEntries.set(card, { entry, context });

  const head = element("div", "approval-head");
  const batch = isMoveBatch(entry);
  const titleWrap = element("div", "approval-title-wrap");
  titleWrap.append(
    element("p", "approval-kicker", t("approval.kicker")),
    element("h3", "approval-title", batch ? moveTitle(entry) : t("approval.needs_permission", { seat: seatName(entry) })),
  );
  head.append(titleWrap, chip(meta.label, `status tone-${meta.tone}`));
  card.append(head);

  if (context) card.append(element("p", "approval-where", context));
  // A batch lists its mails in place of the command; a request for lack of information asks its question there.
  if (batch) card.append(moveBatch(entry, status, mine.unchecked));
  else if (entry.question) card.append(element("p", "approval-question", entry.question));
  else card.append(element("pre", "approval-command", commandText(entry) || t("approval.no_command")));

  const chips = element("div", "approval-chips");
  if (entry.cwd) {
    const cwd = chip(cwdLabel(entry.cwd), "cwd");
    cwd.title = entry.cwd;
    chips.append(cwd);
  }
  const badge = riskBadge(entry);
  if (badge) chips.append(chip(badge, `risk risk-${entry.risk}`));
  if (entry.escalated_from) chips.append(chip(t("approval.escalated"), "level"));
  card.append(chips);
  if (isMailSend(entry) && entry.details) card.append(mailDigest(entry.details));

  const footer = element("div", "approval-footer");
  const digest = element("code", "approval-digest", shortDigest(entry.action_digest));
  digest.setAttribute("aria-label", t("approval.digest_label"));
  footer.append(digest);
  if (status === "pending") {
    const checking = isChecking(entry, now);
    const ttl = element("span", "approval-ttl", checking ? CHECKING_TEXT : "");
    ttl.dataset.expires = countdownStarted(entry) ? entry.expires_at || "" : "";
    footer.append(ttl);
  }
  card.append(footer);

  if (mine.forbidden) card.append(forbiddenNote());
  if (status === "pending") card.append(actionsRow(entry, status, mine));
  else if (status === "expired") card.append(element("p", "approval-result", t("approval.expired")));
  else card.append(element("p", "approval-result", resultText(status, entry)));

  return card;
}

// «Enlazar GitHub»: the question says what the link does; the progress, the one-time code and the
// result take the card's middle, so the whole flow stays on this card.
function renderGithubCard(entry, now = Date.now(), context = "") {
  const id = entry.id;
  const mine = localFor(id);
  const status = resolvedStatus(entry, mine);
  const meta = statusMeta(status, entry);
  const card = element("article", `approval-card github-card tone-${meta.tone}`);
  card.dataset.approval = id;
  card.dataset.seen = seenState(entry);
  card.dataset.stamp = t("approval.stamp");
  cardEntries.set(card, { entry, context });

  const head = element("div", "approval-head");
  const titleWrap = element("div", "approval-title-wrap");
  titleWrap.append(element("p", "approval-kicker", t("github.kicker")), element("h3", "approval-title", t("github.title")));
  head.append(titleWrap, chip(meta.label, `status tone-${meta.tone}`));
  card.append(head);

  if (context) card.append(element("p", "approval-where", context));
  // The question and the explanation are the desktop's words in the operator's language; the host's
  // summary is not shown on this card.
  card.append(element("p", "approval-question", t("github.question")));
  card.append(element("p", "github-explain", githubExplanation(githubMissing(entry), t)));
  card.append(githubSection(entry, status));

  const footer = element("div", "approval-footer");
  const digest = element("code", "approval-digest", shortDigest(entry.action_digest));
  digest.setAttribute("aria-label", t("approval.digest_label"));
  footer.append(digest);
  if (status === "pending") {
    const checking = isChecking(entry, now);
    const ttl = element("span", "approval-ttl", checking ? CHECKING_TEXT : "");
    ttl.dataset.expires = countdownStarted(entry) ? entry.expires_at || "" : "";
    footer.append(ttl);
  }
  card.append(footer);

  if (mine.forbidden) card.append(forbiddenNote());
  if (status === "pending") {
    const disabled = isChecking(entry, now);
    const row = element("div", "approval-actions");
    row.append(
      actionButton(t("github.allow"), "approve", id, "btn primary approval-btn", disabled),
      actionButton(t("github.later"), "deny", id, "btn ghost approval-btn", disabled),
    );
    card.append(row);
  }
  return card;
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
  fact(facts, t("approval.fact_status"), meta.label);
  if (entry.created_at) {
    fact(facts, t("approval.fact_time"), new Date(entry.created_at).toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit" }));
  }
  detail.append(facts);
  return detail;
}

// One timeline row (`kind=approval`) for an approval decided below the operator.
export function renderApprovalLine(entry) {
  // A policy decision needed no answer from the operator: it is not on his thread, folded or not.
  if (!entry || !entry.ref_id || isPolicyLine(entry) || isWaitingLine(entry.summary)) return null;
  const meta = statusMeta(entry.status);
  const line = element("details", `approval-line tone-${meta.tone}`);
  line.dataset.approval = entry.ref_id;
  const words = lineSummary(entry.summary, entry.ref_id) || meta.label;
  line.append(element("summary", "approval-line-summary", words), lineDetail(entry, meta));
  return line;
}
