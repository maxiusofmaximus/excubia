import { $, invoke, toast } from "./core.js";
import { composerMode, stopFailureText } from "./chat-turn-model.js";
import { chat, failure, offline } from "./chatstate.js";
import { hasPendingAttachments, renderAttachments } from "./chatattach.js";
import { setVoiceBlocked } from "./chatvoice.js";

export function renderComposer() {
  const input = $("chat-input");
  const send = $("btn-send");
  const archived = Boolean(chat.conversation && chat.conversation.archived);
  const blocked = archived || offline();
  input.disabled = blocked;
  $("chat-composer").classList.toggle("is-blocked", blocked);
  setVoiceBlocked(blocked);
  const mode = composerMode({
    inFlight: chat.waiting,
    offline: offline(),
    sent: chat.stopSent,
    denied: chat.stopDenied,
  });
  if (!chat.waiting) chat.stopSent = false;
  // While a turn is open the send button is the stop button.
  const label = mode.stop ? "Detener" : "Enviar mensaje";
  send.type = mode.stop ? "button" : "submit";
  send.classList.toggle("is-stop", mode.stop);
  send.disabled = mode.stop ? mode.disabled : blocked || hasPendingAttachments();
  send.title = label;
  send.setAttribute("aria-label", label);
  renderAttachments(blocked);
  if (archived) {
    input.placeholder = "Conversación archivada";
    $("chat-hint").textContent = "Desarchívala para escribir a Alfred";
  } else if (offline()) {
    input.placeholder = "Reconecta para escribir a Alfred";
    $("chat-hint").textContent = "La copia local es de solo lectura";
  } else {
    input.placeholder = "Mensaje a Alfred";
    $("chat-hint").textContent = mode.hint || "Enter envía · Shift+Enter salto de línea · Ctrl+Mayús+M dicta";
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
