import { app, $, invoke, toast } from "./core.js";
import { renderHistory, renderSide } from "./chatlist.js";
import { busySeconds, renderThread } from "./chatthread.js";
import { setBootstrap } from "./approval-model.js";
import { LAST_KEY, loadCache, saveCache } from "./chatcache.js";
import { createChatMenu } from "./chatmenu.js";
import { noteTurn, turnInFlight } from "./chat-turn-model.js";
import { createHistoryFilters, renderFilters } from "./chatfilters.js";
import { renderReplyBar } from "./chatreply.js";
import { renderHeader, renderBanner } from "./chatheader.js";
import { renderComposer } from "./chatcomposer.js";
import { hydrateAttachments } from "./chatattach-view.js";
import { dropAttachmentsOutside } from "./chatattach.js";
import { renderJump } from "./chatjump.js";
import { chat, applyFeed, connection, failure, lastSeq, offline } from "./chatstate.js";
import { applyProgress, syncProgress } from "./chat-progress-model.js";

export { chat, filters };

let refreshTimer = null;
let homeTimer = null;
let searchTimer = null;
let busyTimer = null;
let markingRead = false;

const filters = createHistoryFilters({ chat, render: () => renderChat(), failure });

function activeRuns() {
  return chat.conversation ? chat.conversation.active_runs || 0 : 0;
}

function alfredName() {
  return (chat.home && chat.home.bootstrap && chat.home.bootstrap.alfred.name) || "Alfred";
}

function ensureBusyTicker() {
  const busy = chat.pending.some(
    (pending) => pending.failure && pending.failure.kind === "agentBusy" && busySeconds(pending) > 0,
  );
  if (busy && !busyTimer) {
    busyTimer = setInterval(() => renderChat(), 1000);
  } else if (!busy && busyTimer) {
    clearInterval(busyTimer);
    busyTimer = null;
  }
}

export function renderChat() {
  const previous = chat.waitingOn;
  chat.waiting = turnInFlight(chat.entries, chat.turns, activeRuns());
  const open = chat.conversation ? chat.conversation.id : null;
  // The turn closed under the reader: its step goes, even when the close frame was lost.
  if (previous && previous === open && !chat.waiting) delete chat.progress[open];
  chat.waitingOn = chat.waiting ? open : null;
  renderHeader();
  renderHistory(chat);
  renderThread(chat);
  hydrateAttachments();
  renderSide(chat, app.state ? app.state.agents : []);
  renderBanner();
  renderComposer();
  renderJump();
  renderFilters(chat);
  renderReplyBar(chat, alfredName());
  ensureBusyTicker();
  $("btn-archived").textContent = `Archivados (${chat.home ? chat.home.archived.length : 0})`;
  $("btn-archived").classList.toggle("is-active", chat.showArchived);
}

async function markRead() {
  if (!chat.conversation || markingRead) return;
  const conversationId = chat.conversation.id;
  const seq = lastSeq();
  if (!seq || seq <= (chat.conversation.last_read_seq || 0)) return;
  markingRead = true;
  try {
    const value = await invoke("chat_mark_read", { conversationId, lastSeq: seq });
    if (chat.conversation && chat.conversation.id === conversationId) {
      chat.conversation.last_read_seq = value;
    }
    scheduleHome();
  } catch {
    // The cursor is a convenience: a failed mark must not block reading.
  } finally {
    markingRead = false;
  }
}

export async function refreshHome() {
  try {
    chat.home = await invoke("chat_home");
    setBootstrap(chat.home);
    chat.loadFailure = null;
    filters.scheduleCounts(0);
    if (offline()) chat.cacheOnly = true;
    for (const row of [...chat.home.conversations, ...chat.home.archived]) {
      syncProgress(chat.progress, row);
    }
    if (chat.conversation) {
      const fresh = chat.home.conversations.find((row) => row.id === chat.conversation.id);
      if (fresh) chat.conversation = { ...chat.conversation, ...fresh };
    }
    renderChat();
    return true;
  } catch (error) {
    const failureInfo = failure(error);
    chat.loadFailure = failureInfo;
    if (!chat.home) loadCache(chat);
    renderChat();
    return false;
  }
}

export async function selectConversation(id, options = {}) {
  if (!id) return;
  menu.closeMenu();
  const token = ++chat.openToken;
  chat.selectedId = id;
  dropAttachmentsOutside(id);
  chat.reply = null;
  chat.forceScroll = true;
  try {
    const opened = await invoke("chat_open", { conversationId: id });
    if (token !== chat.openToken) return;
    chat.conversation = opened.conversation;
    syncProgress(chat.progress, opened.conversation);
    chat.pending = [];
    chat.stopSent = false;
    chat.unseenFrom = null;
    chat.cacheOnly = false;
    chat.cancelTarget = null;
    chat.delegationError = null;
    applyFeed(opened.feed);
    chat.lastSync = Date.now();
    localStorage.setItem(LAST_KEY, id);
    renderChat();
    saveCache(chat);
    await markRead();
    if (options.focus !== false) $("chat-input").focus();
  } catch (error) {
    if (token !== chat.openToken) return;
    const failureInfo = failure(error);
    chat.loadFailure = failureInfo;
    const restored = loadCache(chat, id);
    if (!restored || !chat.conversation || chat.conversation.id !== id) {
      const cached = chat.home && chat.home.conversations.find((row) => row.id === id);
      chat.conversation = cached || { id, title: "Conversación", archived: false, last_read_seq: 0 };
      chat.entries = [];
    }
    renderChat();
    if (!offline()) toast(failureInfo.message);
  }
}

export async function refreshActive() {
  if (!chat.conversation || chat.refreshing) {
    if (chat.refreshing) chat.refreshQueued = true;
    return;
  }
  const conversationId = chat.conversation.id;
  chat.refreshing = true;
  try {
    const feed = await invoke("chat_refresh", {
      conversationId,
      afterSeq: lastSeq(),
    });
    if (!chat.conversation || chat.conversation.id !== conversationId) return;
    applyFeed(feed);
    chat.lastSync = Date.now();
    chat.cacheOnly = false;
    renderChat();
    saveCache(chat);
    await markRead();
  } catch (error) {
    chat.loadFailure = failure(error);
    renderChat();
  } finally {
    chat.refreshing = false;
    if (chat.refreshQueued) {
      chat.refreshQueued = false;
      scheduleRefresh(0);
    }
  }
}

export function scheduleRefresh(delay = 180) {
  clearTimeout(refreshTimer);
  refreshTimer = setTimeout(refreshActive, delay);
}

export function scheduleHome(delay = 400) {
  clearTimeout(homeTimer);
  homeTimer = setTimeout(refreshHome, delay);
}

export function debounceSearch() {
  clearTimeout(searchTimer);
  const value = $("chat-search").value.trim();
  chat.query = value;
  if (!value) {
    chat.searchResults = null;
    renderChat();
    return;
  }
  searchTimer = setTimeout(async () => {
    try {
      chat.searchResults = await invoke("chat_search", { q: value });
      renderChat();
    } catch (error) {
      toast(failure(error).message);
    }
  }, 250);
}

export function chatOnEvent(event) {
  if (!event) return;
  if (event.event === "chat.progress") {
    applyProgress(chat.progress, event);
    renderChat();
    return;
  }
  if (event.message_id && event.turn_state) {
    // Kept even if the message is not on screen yet: it is applied when it arrives.
    noteTurn(chat.turns, event.message_id, event.turn_state);
    if (chat.conversation && event.conversation_id === chat.conversation.id) renderChat();
  }
  if (event.conversation_id && chat.conversation && event.conversation_id === chat.conversation.id) {
    const refreshes = ["chat.message", "chat.conversation", "chat.delegation", "chat.activity", "chat.approval"];
    if (refreshes.includes(event.event)) scheduleRefresh();
  }
  if (event.conversation_id) {
    scheduleHome();
    filters.scheduleCounts();
  }
}

export function chatOnConnection() {
  renderChat();
  if (connection().kind === "connected") {
    if (!chat.home) refreshHome();
    if (chat.conversation) scheduleRefresh(0);
  }
}

export function chatOnStateChange() {
  renderSide(chat, app.state ? app.state.agents : []);
  renderHeader();
}

function updateLocal(conversation) {
  if (!chat.home) return;
  const replace = (rows) => rows.map((row) => (row.id === conversation.id ? { ...row, ...conversation } : row));
  chat.home.conversations = replace(chat.home.conversations);
  chat.home.archived = replace(chat.home.archived);
  if (chat.conversation && chat.conversation.id === conversation.id) {
    chat.conversation = { ...chat.conversation, ...conversation };
  }
}

export async function patchConversation(conversation, args, message) {
  try {
    const updated = await invoke("chat_update", { conversationId: conversation.id, ...args });
    updateLocal(updated);
    renderChat();
    saveCache(chat);
    scheduleHome(150);
    if (message) toast(message);
    return updated;
  } catch (error) {
    toast(failure(error).message);
    return null;
  }
}

export const menu = createChatMenu({
  chat,
  render: renderChat,
  patch: patchConversation,
  failure,
});

export async function chatBoot() {
  if (offline()) loadCache(chat);
  renderChat();
  const loaded = await refreshHome();
  if (!loaded && !chat.home) {
    renderChat();
    return;
  }
  if (!chat.home || !chat.home.conversations.length) {
    renderChat();
    return;
  }
  let last = null;
  try {
    last = localStorage.getItem(LAST_KEY);
  } catch {
    last = null;
  }
  const exists = chat.home.conversations.some((row) => row.id === last);
  const target = exists ? last : chat.home.conversations[0].id;
  await selectConversation(target, { focus: false });
}

export async function chatAfterConnect() {
  await refreshHome();
  if (chat.home && !chat.conversation && chat.home.conversations.length) {
    await selectConversation(chat.home.conversations[0].id, { focus: false });
  } else if (chat.conversation) {
    scheduleRefresh(0);
  }
}

export function chatReset() {
  clearTimeout(refreshTimer);
  clearTimeout(homeTimer);
  clearTimeout(searchTimer);
  filters.cancel();
  clearInterval(busyTimer);
  busyTimer = null;
  chat.openToken += 1;
  chat.home = null;
  chat.conversation = null;
  dropAttachmentsOutside(null);
  chat.entries = [];
  chat.pending = [];
  chat.nextBeforeSeq = null;
  chat.selectedId = null;
  chat.searchResults = null;
  chat.showArchived = false;
  chat.reply = null;
  chat.counts = undefined;
  chat.filter = "all";
  chat.filtered = [];
  chat.turns = {};
  chat.progress = {};
  chat.waitingOn = null;
  chat.stopSent = false;
  chat.stopDenied = null;
  chat.unseenFrom = null;
  chat.cacheOnly = false;
  chat.loadFailure = null;
  chat.cancelTarget = null;
  chat.delegationError = null;
  renderChat();
}
