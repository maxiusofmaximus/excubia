import { app } from "./core.js";
import { noteTurn } from "./chat-turn-model.js";

// The chat's shared state: one object, read and written by the chat modules.
export const chat = {
  home: null,
  conversation: null,
  entries: [],
  pending: [],
  nextBeforeSeq: null,
  selectedId: null,
  query: "",
  searchResults: null,
  showArchived: false,
  // Computed on every render from the conversation's turn (see renderChat).
  waiting: false,
  // Furthest turn state seen per message id, from the feed, the 202 and frames.
  turns: {},
  // The step of the open turn per conversation id (see chat-progress-model.js).
  progress: {},
  // The conversation whose turn was in flight at the last render; its end clears the step.
  waitingOn: null,
  // Set by a stop request until the turn ends; the stop of a device without RESPOND stays off.
  stopSent: false,
  stopDenied: null,
  // The message the next send answers: { id, author_kind, preview } or null.
  reply: null,
  // Chip counts: undefined until loaded, null on an older host (no chips), {} when unreadable.
  counts: undefined,
  // The history filter («all» or a host-filtered chip) and its rows.
  filter: "all",
  filtered: [],
  filterToken: 0,
  // lastSeq when the reader left the bottom; null while at the bottom.
  unseenFrom: null,
  forceScroll: false,
  refreshing: false,
  refreshQueued: false,
  cacheOnly: false,
  lastSync: null,
  loadFailure: null,
  menuFor: null,
  openToken: 0,
  cancelTarget: null,
  delegationError: null,
};

export function failure(error) {
  if (error && typeof error === "object" && error.kind) return error;
  if (typeof error === "string") return { kind: "gateway", message: error };
  if (error && typeof error.message === "string") return { kind: "gateway", message: error.message };
  return { kind: "gateway", message: "No se pudo completar la operación." };
}

export function connection() {
  return app.connection || { kind: "idle" };
}

export function offline() {
  return connection().kind === "offline" || connection().kind === "fatal";
}

export function hostName() {
  if (app.boot && app.boot.mode === "local") return "el host local";
  try {
    return new URL(app.boot.gateway_url).hostname || "el host";
  } catch {
    return "el host";
  }
}

export function lastSeq() {
  return chat.entries.reduce((max, entry) => Math.max(max, entry.seq || 0), 0);
}

export function applyFeed(feed) {
  chat.entries = feed.entries || [];
  chat.approvals = feed.approvals || [];
  chat.nextBeforeSeq = feed.next_before_seq || null;
  if (chat.conversation) chat.conversation.last_read_seq = feed.last_read_seq;
  for (const entry of chat.entries) {
    if (entry.kind === "message") noteTurn(chat.turns, entry.message.id, entry.message.turn_state);
  }
  chat.forceScroll = chat.forceScroll || !chat.entries.length;
}
