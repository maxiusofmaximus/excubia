import { $ } from "./core.js";
import { unseenCount, jumpLabel } from "./chat-turn-model.js";
import { chat, lastSeq } from "./chatstate.js";

// «Ir al final»: the counter starts when the reader leaves the bottom (140 px,
// as renderThread decides «cerca del fondo») and clears when they come back.
export function renderJump() {
  const timeline = $("chat-timeline");
  const near = timeline.scrollHeight - timeline.scrollTop - timeline.clientHeight < 140;
  if (near) chat.unseenFrom = null;
  else if (chat.unseenFrom === null) chat.unseenFrom = lastSeq();
  const count = near ? 0 : unseenCount(chat.entries, chat.unseenFrom);
  const jump = $("chat-jump");
  jump.classList.toggle("hidden", near);
  jump.textContent = count ? `↓ ${count}` : "↓";
  jump.setAttribute("aria-label", jumpLabel(count));
}

export function jumpToBottom() {
  const timeline = $("chat-timeline");
  timeline.scrollTop = timeline.scrollHeight;
  renderJump();
}
