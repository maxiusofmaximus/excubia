// Read-only cache of the last conversation, kept in the webview so an
// offline start can still show the last readable page. Nothing secret: the
// host remains the source of truth.

import { app } from "./core.js";

const CACHE_PREFIX = "hn-chat-cache-v2";
export const LAST_KEY = "hn-chat-last";

function cacheKey() {
  const boot = app.boot || {};
  return `${CACHE_PREFIX}:${boot.device_id || "sin-dispositivo"}:${boot.gateway_url || "local"}`;
}

export function saveCache(chat) {
  if (!chat.conversation) return;
  try {
    const payload = {
      savedAt: Date.now(),
      bootstrap: chat.home ? chat.home.bootstrap : null,
      conversations: chat.home ? chat.home.conversations.slice(0, 30) : [],
      sections: chat.home ? chat.home.sections : [],
      conversation: chat.conversation,
      entries: chat.entries.slice(-40),
    };
    localStorage.setItem(cacheKey(), JSON.stringify(payload));
    localStorage.setItem(LAST_KEY, chat.conversation.id);
  } catch {
    // A full or unavailable storage must never break the chat.
  }
}

function readCache() {
  try {
    const raw = localStorage.getItem(cacheKey());
    if (!raw) return null;
    const cached = JSON.parse(raw);
    return cached && cached.conversation ? cached : null;
  } catch {
    return null;
  }
}

export function loadCache(chat, id) {
  const cached = readCache();
  if (!cached) return false;
  chat.home = chat.home || {
    bootstrap: cached.bootstrap,
    conversations: cached.conversations || [],
    archived: [],
    sections: cached.sections || [],
  };
  if (!chat.conversation || (id && cached.conversation.id === id)) {
    chat.conversation = cached.conversation;
    chat.selectedId = cached.conversation.id;
    chat.entries = cached.entries || [];
    chat.lastSync = cached.savedAt;
    chat.cacheOnly = true;
  }
  return true;
}
