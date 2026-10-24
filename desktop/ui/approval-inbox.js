// The operator's inbox on the chat screen: every approval that waits for him,
// across conversations. The open thread draws its own approvals as cards; this
// strip draws the rest, so an approval with no conversation (and so no timeline
// line) is never out of sight.

import { $, invoke } from "./core.js";
import { element, renderApprovalCard } from "./approval-card.js";
import { conversationContext, isOperatorLevel, localFor, patchLocal } from "./approval-model.js";

// A card that settled stays on the strip this long, so the operator sees how.
const SETTLED_SHOWN_MS = 60_000;
// The host says `chat.approval` for each change; a burst costs one refresh.
const EVENT_DEBOUNCE_MS = 150;

let held = [];
let threadIds = new Set();
let eventTimer = null;

function recentlySettled(id, now) {
  const at = localFor(id).settledAt;
  return Boolean(at) && now - at < SETTLED_SHOWN_MS;
}

// A settled card stays while its answer is recent; an open one stays while the
// host lists it as pending.
function showable(entry, now) {
  if (localFor(entry.id).settledAt) return recentlySettled(entry.id, now);
  return (entry.status || "pending") === "pending";
}

function isPendingHere(entry) {
  return !localFor(entry.id).decided && (entry.status || "pending") === "pending";
}

function strip() {
  const existing = document.getElementById("approval-inbox");
  if (existing) return existing;
  const node = element("section", "approval-inbox");
  node.id = "approval-inbox";
  node.setAttribute("aria-label", "Aprobaciones pendientes de ti");
  const timeline = $("chat-timeline");
  timeline.parentNode.insertBefore(node, timeline);
  return node;
}

export function renderInbox() {
  const now = Date.now();
  const rows = held.filter(
    (entry) => isOperatorLevel(entry) && !threadIds.has(entry.id) && showable(entry, now),
  );
  const node = strip();
  node.hidden = rows.length === 0;
  node.replaceChildren();
  if (!rows.length) return;
  const waiting = rows.filter(isPendingHere).length;
  node.append(element("h2", "approval-inbox-title", `Pendientes de ti · ${waiting}`));
  for (const entry of rows) {
    const context = conversationContext(entry.conversation_id) || "Sin conversación";
    node.append(renderApprovalCard(entry, now, context));
  }
}

// The cards of the open thread are drawn there, so the strip skips them.
export function setThreadApprovals(ids) {
  threadIds = new Set(ids);
  renderInbox();
}

const RESOLVED = ["approved", "denied", "expired", "consumed"];

// An approval that left the inbox was settled by the host (expired, consumed, or
// decided elsewhere). The host reads a device's approvals by status, so the four
// resolved lists are read once and matched by id. An approval in none of them is dropped.
async function readStates(entries, now) {
  if (!entries.length) return [];
  const lists = await Promise.all(
    RESOLVED.map((status) => invoke("chat_approvals_with_status", { status }).catch(() => [])),
  );
  const rows = new Map(lists.flat().map((row) => [row.id, row]));
  return entries.flatMap((entry) => {
    const row = rows.get(entry.id);
    if (!row) return [];
    patchLocal(entry.id, { stateRead: true, settledAt: localFor(entry.id).settledAt || now });
    return [{ ...entry, status: row.status || entry.status }];
  });
}

export async function refreshInbox() {
  let fresh;
  try {
    fresh = await invoke("chat_pending_approvals");
  } catch {
    // An unreachable host keeps the last strip instead of emptying it.
    return;
  }
  const now = Date.now();
  const absent = held.filter((entry) => !fresh.some((row) => row.id === entry.id));
  // An allowed approval is read again while shown: the hook may consume it later.
  const needsRead = (entry) =>
    !localFor(entry.id).stateRead || (entry.status === "approved" && recentlySettled(entry.id, now));
  const unread = absent.filter(needsRead);
  const read = absent.filter((entry) => !needsRead(entry) && recentlySettled(entry.id, now));
  held = [...fresh, ...read, ...(await readStates(unread, now))];
  renderInbox();
}

export function inboxOnEvent(event) {
  if (!event || event.event !== "chat.approval") return;
  clearTimeout(eventTimer);
  eventTimer = setTimeout(refreshInbox, EVENT_DEBOUNCE_MS);
}
