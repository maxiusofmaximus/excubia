// Replies and quick answers, free of the DOM so the tests can run them: which
// suggestions the thread offers, how a quote or the reply bar reads, and the
// words of a reply the host no longer knows.
import { isAgentMessage, isOwn } from "./chat-turn-model.js";
import { t } from "./i18n.js";

export const UNKNOWN_REPLY_NOTICE = t("chat.reply.unknown");

// One line, collapsed spaces, cut with an ellipsis when it runs long.
export function previewLine(text, max = 80) {
  const line = String(text || "").replace(/\s+/g, " ").trim();
  return line.length > max ? `${line.slice(0, max - 1)}…` : line;
}

// The operator reads as «ti»; every other author is Alfred.
export function authorName(authorKind, alfredName) {
  return authorKind === "user" ? t("chat.reply.you") : alfredName || "Alfred";
}

export function quoteLabel(quote, alfredName) {
  return t("chat.reply.quote", { who: authorName(quote.author_kind, alfredName), text: previewLine(quote.preview, 80) });
}

export function replyBarText(reply, alfredName) {
  return t("chat.reply.bar", { who: authorName(reply.author_kind, alfredName), text: previewLine(reply.preview, 80) });
}

// The quick answers belong to the last agent message, and only while the
// operator has not answered after it. An older message never offers them.
export function suggestionsAt(entries) {
  let last = -1;
  entries.forEach((entry, index) => {
    if (isAgentMessage(entry)) last = index;
  });
  if (last < 0 || entries.slice(last + 1).some(isOwn)) return null;
  const items = entries[last].message.suggestions || [];
  return items.length ? { index: last, items } : null;
}

// Quick answers wait while a send is in flight; a failed one leaves them on.
export function quickRepliesDisabled(pending) {
  return pending.some((item) => item.state === "sending");
}

// The loaded message with this id, if the thread has it.
export function messageById(entries, id) {
  const entry = entries.find((item) => item.kind === "message" && item.message.id === id);
  return entry ? entry.message : null;
}

// What one send carries: the text exactly as written, and the id it answers.
export function sendArgs(text, reply) {
  return { text, replyTo: reply ? reply.id : null };
}

// After a successful send the bar clears only when it still points at that message.
export function afterSend(current, sent) {
  return current && sent && current.id === sent.id ? null : current;
}

// A refused quote: the text goes back into the composer, what was typed stays.
export function restoredText(typed, body) {
  return typed.trim() ? `${body}\n${typed}` : body;
}
