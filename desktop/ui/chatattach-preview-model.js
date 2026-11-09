// The page viewer of a document, free of the DOM: which documents open it, the page and
// its end, the six pages kept in memory, what each refusal of the host means and the keys.
import { extensionOf } from "./chatattach-model.js";

export const PREVIEW_EXTENSIONS = ["xlsx", "docx", "pdf"];
export const PREVIEW_CACHE_SIZE = 6;

const TOOL_MISSING_TEXT = "Falta una herramienta de documentos en este PC.";
const EMPTY_TEXT = "Este documento no tiene páginas que mostrar.";
const OTHER_TEXT = "No se pudo mostrar la página. Reinténtalo.";

// Only the documents the host can draw a page of. Anything else keeps the card's old behaviour.
export function opensPreview(name) {
  return PREVIEW_EXTENSIONS.includes(extensionOf(name));
}

// `last` is null until the host answers out_of_range; then it is the number of the last page.
export function startPreview() {
  return { page: 1, last: null };
}

export function canGoPrevious(state) {
  return state.page > 1;
}

export function canGoNext(state) {
  return state.last === null || state.page < state.last;
}

export function previousPage(state) {
  return canGoPrevious(state) ? { ...state, page: state.page - 1 } : state;
}

export function nextPage(state) {
  return canGoNext(state) ? { ...state, page: state.page + 1 } : state;
}

// The page `requested` does not exist: the one before it is the last, and it stays on screen.
export function markLastPage(state, requested) {
  return { ...state, page: requested - 1, last: requested - 1 };
}

export function pageLabel(page) {
  return `Página ${page}`;
}

// What a failed page request means. `end` marks the last page, `fallback` hands the file
// to the system, `error` shows `message` and offers a retry only when `retry` is true.
export function previewOutcome(error, requested) {
  const failed = error && typeof error === "object" ? error : {};
  const code = failed.code;
  const message = typeof failed.message === "string" ? failed.message.trim() : "";
  if (code === "out_of_range" && requested > 1) return { kind: "end" };
  if (code === "out_of_range") return { kind: "error", message: EMPTY_TEXT, retry: false };
  if (code === "unavailable") return { kind: "fallback" };
  if (code === "tool_missing") return { kind: "error", message: message || TOOL_MISSING_TEXT, retry: false };
  return { kind: "error", message: OTHER_TEXT, retry: true };
}

// The pages on screen, most recently read last. A read moves its page to the end.
export function createPageCache(limit = PREVIEW_CACHE_SIZE) {
  const pages = new Map();
  return {
    get size() {
      return pages.size;
    },
    has(key) {
      return pages.has(key);
    },
    get(key) {
      if (!pages.has(key)) return undefined;
      const value = pages.get(key);
      pages.delete(key);
      pages.set(key, value);
      return value;
    },
    set(key, value) {
      pages.delete(key);
      pages.set(key, value);
      while (pages.size > limit) pages.delete(pages.keys().next().value);
    },
  };
}

export function pageKey(attachmentId, page) {
  return `${attachmentId}#${page}`;
}

export function keyAction(key) {
  if (key === "ArrowLeft") return "previous";
  if (key === "ArrowRight") return "next";
  if (key === "Escape") return "close";
  return null;
}

// The focusable element Tab moves to, wrapping at both ends. -1 when there is none.
export function nextFocus(index, count, backwards) {
  if (count === 0) return -1;
  if (index < 0 || index >= count) return backwards ? count - 1 : 0;
  return (index + (backwards ? -1 : 1) + count) % count;
}
