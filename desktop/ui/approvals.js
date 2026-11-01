// Operator decisions on approval cards. One click sends one decision. The
// client id is kept per approval, so a retry asks the host the same question
// and it answers the same way instead of deciding twice.

import { $, invoke, toast } from "./core.js";
import { errorMessage } from "./format.js";
import {
  CHECKING_TEXT,
  countdownText,
  excludeFor,
  isMailMove,
  localFor,
  patchLocal,
  remainingSeconds,
  sameIds,
  setDeviceId,
} from "./approval-model.js";
import { cardEntries, renderApprovalCard } from "./approval-card.js";
import { refreshInbox } from "./approval-inbox.js";
import { refreshActive } from "./chat.js";

function newClientId() {
  return crypto.randomUUID();
}

// Redraws a card from its approval and returns the new node, which is the one a
// later redraw must use: the old node is no longer in the page.
function redraw(card) {
  const held = cardEntries.get(card);
  if (!held) return card;
  const next = renderApprovalCard(held.entry, Date.now(), held.context);
  card.replaceWith(next);
  return next;
}

// The 403 note names the device to grant. The host does not send it with an
// approval, so the settings call supplies it. A failure keeps the placeholder.
async function rememberDeviceId() {
  try {
    const settings = await invoke("chat_approval_settings");
    setDeviceId(settings.device_id);
  } catch {
    // The note still tells the operator the permission is missing.
  }
}

// The host had the last word on an approval: the thread and the strip read it again.
function readHostAgain() {
  refreshInbox();
  refreshActive();
}

// Only an approve of a mail batch carries the mails the operator left out.
function excludeOf(card, decision, unchecked) {
  const held = cardEntries.get(card);
  if (!held || decision !== "approve" || !isMailMove(held.entry)) return undefined;
  return excludeFor(held.entry, unchecked);
}

async function decide(id, decision, card) {
  const mine = localFor(id);
  if (mine.busy || mine.decided) return;
  const exclude = excludeOf(card, decision, mine.unchecked);
  // A retry reuses its id only for the same decision with the same mails left out.
  const sameAsk = mine.decision === decision && sameIds(mine.exclude, exclude);
  const clientId = sameAsk && mine.clientId ? mine.clientId : newClientId();
  let current = redraw(card);
  patchLocal(id, { busy: true, clientId, decision, exclude, forbidden: false });
  current = redraw(current);

  try {
    const record = await invoke("chat_approval_decision", {
      approvalId: id,
      decision,
      clientId,
      exclude,
    });
    // Only the host's own status counts as decided; the time keeps the card on the strip a while.
    patchLocal(id, { busy: false, decided: (record && record.status) || "", settledAt: Date.now() });
    refreshInbox();
  } catch (error) {
    if (error && error.kind === "forbidden") {
      patchLocal(id, { busy: false, forbidden: true });
      await rememberDeviceId();
    } else if (error && error.kind === "approvalConflict") {
      // Decided elsewhere, expired or consumed: the host's message, then its state.
      patchLocal(id, { busy: false });
      toast(errorMessage(error));
      readHostAgain();
    } else {
      patchLocal(id, { busy: false });
      toast(errorMessage(error));
    }
  }
  redraw(current);
}

// The countdown reads the host's `expires_at`. At zero this clock only asks the
// host once: whether the approval expired is the host's answer, never ours.
function tickCountdowns() {
  const now = Date.now();
  for (const ttl of document.querySelectorAll(".approval-ttl[data-expires]")) {
    const seconds = remainingSeconds(ttl.dataset.expires, now);
    if (seconds === null) continue;
    if (seconds > 0) {
      ttl.textContent = `Caduca en ${countdownText(seconds)}`;
      continue;
    }
    ttl.textContent = CHECKING_TEXT;
    askHostOnce(ttl.closest(".approval-card"));
  }
}

function askHostOnce(card) {
  const id = card.dataset.approval;
  if (localFor(id).asked) return;
  patchLocal(id, { asked: true });
  readHostAgain();
}

// One checkbox says whether its mail stays in the batch. The card is drawn again from
// that, and focus goes back to the same checkbox so a keyboard user keeps their place.
function toggleMail(box) {
  const card = box.closest(".approval-card");
  const id = card.dataset.approval;
  const mailId = box.dataset.moveId;
  const unchecked = new Set(localFor(id).unchecked);
  if (box.checked) unchecked.delete(mailId);
  else unchecked.add(mailId);
  patchLocal(id, { unchecked });
  const next = redraw(card);
  next.querySelector(`[data-move-id="${CSS.escape(mailId)}"]`)?.focus();
}

export function installApprovalHandlers() {
  // The strip sits beside the thread, so both share one listener.
  $("chat-main").addEventListener("click", (event) => {
    const box = event.target.closest("input[data-move-id]");
    if (box) {
      toggleMail(box);
      return;
    }
    const button = event.target.closest("[data-approval-action]");
    if (!button || button.disabled) return;
    const decision = button.dataset.approvalAction === "approve" ? "approve" : "deny";
    decide(button.dataset.approvalId, decision, button.closest(".approval-card"));
  });
  setInterval(tickCountdowns, 1000);
}
