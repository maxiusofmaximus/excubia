import { $, invoke, toast } from "./core.js";
import { composerMode, stopFailureText } from "./chat-turn-model.js";
import { chat, failure, offline } from "./chatstate.js";
import { hasPendingAttachments, readyAttachments, renderAttachments } from "./chatattach.js";
import { setVoiceBlocked } from "./chatvoice.js";
import { t } from "./i18n.js";

// What the send button does now. The click and the drawing both ask here, so they agree.
export function sendMode() {
  const draft = Boolean($("chat-input").value.trim()) || hasPendingAttachments() || readyAttachments().length > 0;
  return composerMode({
    inFlight: chat.waiting,
    offline: offline(),
    sent: chat.stopSent,
    denied: chat.stopDenied,
    hasDraft: draft,
  });
}

export function renderComposer() {
  const input = $("chat-input");
  const send = $("btn-send");
  const archived = Boolean(chat.conversation && chat.conversation.archived);
  const blocked = archived || offline();
  input.disabled = blocked;
  $("chat-composer").classList.toggle("is-blocked", blocked);
  setVoiceBlocked(blocked);
  const mode = sendMode();
  if (!chat.waiting) chat.stopSent = false;
  // While a turn is open and the field is empty the send button is the stop button.
  // It stays a submit button: the click handler stops only in stop mode.
  const label = mode.stop ? t("chat.composer.stop") : t("chat.composer.send");
  send.classList.toggle("is-stop", mode.stop);
  send.disabled = mode.stop ? mode.disabled : blocked || hasPendingAttachments();
  send.title = label;
  send.setAttribute("aria-label", label);
  renderAttachments(blocked);
  // The key list is for the one who is typing: it shows only while the composer has focus.
  // Any sentence that explains a block or a turn stays on screen.
  const hint = $("chat-hint");
  hint.classList.toggle("is-keys", !archived && !offline() && !mode.hint);
  if (archived) {
    input.placeholder = t("chat.composer.archived");
    hint.textContent = t("chat.composer.archived_hint");
  } else if (offline()) {
    input.placeholder = t("chat.composer.offline");
    hint.textContent = t("chat.composer.offline_hint");
  } else {
    input.placeholder = t("chat.composer.placeholder");
    hint.textContent = mode.hint || t("chat.composer.keys");
  }
}

export function autoGrow() {
  const input = $("chat-input");
  input.style.height = "auto";
  input.style.height = `${Math.min(input.scrollHeight, 150)}px`;
}

// Detener: one stop request in flight at a time. `render` and `refresh` come from
// the chat module, which owns the redraw and the feed refresh.
export async function stopTurn({ render, refresh }) {
  const conversation = chat.conversation;
  if (!conversation || chat.stopSent) return;
  chat.stopSent = true;
  renderComposer();
  try {
    const outcome = await invoke("chat_stop_conversation", { conversationId: conversation.id });
    // stopped:false means no turn was open: the state is already final.
    if (!outcome.stopped) chat.stopSent = false;
    refresh(0);
  } catch (error) {
    const failureInfo = failure(error);
    chat.stopSent = false;
    if (failureInfo.kind === "forbidden") chat.stopDenied = stopFailureText(failureInfo);
    toast(stopFailureText(failureInfo));
  }
  render();
}
