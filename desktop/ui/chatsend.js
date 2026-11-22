import { $, invoke, toast } from "./core.js";
import { busySeconds } from "./chatthread.js";
import { autoGrow } from "./chatcomposer.js";
import { LAST_KEY, saveCache } from "./chatcache.js";
import { UNKNOWN_REPLY_NOTICE, afterSend, restoredText, sendArgs } from "./chat-reply-model.js";
import { noteTurn } from "./chat-turn-model.js";
import { chat, applyFeed, failure } from "./chatstate.js";
import {
  dropAttachmentsOutside,
  readyAttachments,
  restoreAttachments,
  takeAttachments,
} from "./chatattach.js";
import { sendFailure } from "./chatattach-model.js";

function newClientId() {
  if (window.crypto && window.crypto.randomUUID) return window.crypto.randomUUID();
  return `c-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function reuseFor(pending) {
  if (!pending || !pending.failure) return null;
  return ["offline", "host", "protocol"].includes(pending.failure.kind)
    ? pending.clientId
    : null;
}

// `render` redraws the chat and `home` schedules the conversation list refresh;
// both belong to the chat module.
export function createSender({ render, home }) {
  async function create(title) {
    const conversation = await invoke("chat_create", { title: title || null });
    if (!chat.home) chat.home = { conversations: [], archived: [], sections: [], bootstrap: null };
    chat.home.conversations = [conversation, ...chat.home.conversations.filter((row) => row.id !== conversation.id)];
    chat.conversation = conversation;
    dropAttachmentsOutside(conversation.id);
    chat.selectedId = conversation.id;
    chat.reply = null;
    chat.entries = [];
    chat.pending = [];
    chat.nextBeforeSeq = null;
    chat.forceScroll = true;
    localStorage.setItem(LAST_KEY, conversation.id);
    render();
    return conversation;
  }

  // `pictures` are the rows of a send being retried; a fresh send takes its rows from the tray.
  async function send(text, reuseClientId, reply = null, pictures = null) {
    const body = String(text || "").replace(/\s+$/g, "");
    const fromTray = pictures === null;
    const rows = fromTray ? readyAttachments() : pictures;
    if (!body.trim() && !rows.length) return;
    if (!chat.conversation) {
      try {
        await create(null);
      } catch (error) {
        toast(failure(error).message);
        return;
      }
    }
    if (chat.conversation.archived) {
      toast("La conversación está archivada.");
      return;
    }
    const conversationId = chat.conversation.id;
    const clientId = reuseClientId || newClientId();
    if (fromTray) takeAttachments(rows);
    chat.pending.push({
      clientId,
      text: body,
      replyTo: reply,
      attachments: rows,
      state: "sending",
      failure: null,
      failureAt: 0,
      createdAt: Date.now(),
    });
    chat.forceScroll = true;
    render();
    try {
      const result = await invoke("chat_send", {
        conversationId,
        clientId,
        ...sendArgs(body, reply),
        attachments: rows.map((item) => item.id),
      });
      chat.pending = chat.pending.filter((pending) => pending.clientId !== clientId);
      chat.reply = afterSend(chat.reply, reply);
      if (!chat.conversation || chat.conversation.id !== conversationId) {
        home();
        render();
        return;
      }
      // The 202 says the turn is queued; a turn already further along keeps its state.
      noteTurn(chat.turns, result.message.id, result.turn && result.turn.state);
      applyFeed(result.feed);
      chat.conversation = { ...chat.conversation, last_seq: result.message.seq };
      chat.unseenFrom = null;
      chat.lastSync = Date.now();
      render();
      saveCache(chat);
      home();
    } catch (error) {
      if (failure(error).kind === "unknownReplyTo") {
        // The quote is gone for good: the text returns to the composer, nothing is lost.
        chat.pending = chat.pending.filter((pending) => pending.clientId !== clientId);
        chat.reply = afterSend(chat.reply, reply);
        const input = $("chat-input");
        input.value = restoredText(input.value, body);
        autoGrow();
        restoreAttachments(rows);
        toast(UNKNOWN_REPLY_NOTICE);
        render();
        return;
      }
      const pending = chat.pending.find((item) => item.clientId === clientId);
      if (pending) {
        pending.state = "failed";
        pending.failure = sendFailure(failure(error));
        pending.failureAt = Date.now();
      }
      render();
    }
  }

  function retry(clientId) {
    const pending = chat.pending.find((item) => item.clientId === clientId);
    if (!pending) return;
    if (pending.failure && pending.failure.kind === "agentBusy" && busySeconds(pending) > 0) return;
    const reuse = reuseFor(pending);
    chat.pending = chat.pending.filter((item) => item !== pending);
    send(pending.text, reuse, pending.replyTo, pending.attachments);
    render();
  }

  return { create, send, retry };
}
