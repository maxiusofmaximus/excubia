import { $ } from "./core.js";
import { replyBarText } from "./chat-reply-model.js";

// The bar over the composer names the message being answered.
export function renderReplyBar(chat, alfredName) {
  $("chat-reply").classList.toggle("hidden", !chat.reply);
  $("chat-reply-text").textContent = chat.reply ? replyBarText(chat.reply, alfredName) : "";
}

export function bubbleMenuOpen() {
  return !$("bubble-menu").classList.contains("hidden");
}

// The right-click menu of a bubble: one item, «Responder», for that message.
export function openBubbleMenu(x, y, id) {
  const menu = $("bubble-menu");
  menu.querySelector("[data-reply-id]").dataset.replyId = id;
  menu.style.left = `${Math.min(x, window.innerWidth - 140)}px`;
  menu.style.top = `${Math.min(y, window.innerHeight - 56)}px`;
  menu.classList.remove("hidden");
  menu.querySelector("button").focus();
}

export function closeBubbleMenu() {
  $("bubble-menu").classList.add("hidden");
}
