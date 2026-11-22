// Run with: node --test tooling/chatattach-preview-model.test.mjs
// The page viewer's state: the page and its end, the cache of six pages, the words of each
// refusal, which documents open the viewer, the keys and the focus wrap inside the dialog.
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  PREVIEW_CACHE_SIZE,
  canGoNext,
  canGoPrevious,
  createPageCache,
  keyAction,
  markLastPage,
  nextFocus,
  nextPage,
  opensPreview,
  pageKey,
  pageLabel,
  previewOutcome,
  previousPage,
  startPreview,
} from "../ui/chatattach-preview-model.js";

test("the viewer opens on page 1 and moves one page at a time", () => {
  let state = startPreview();
  assert.equal(state.page, 1);
  assert.equal(canGoPrevious(state), false);
  assert.equal(canGoNext(state), true);
  state = nextPage(state);
  assert.equal(state.page, 2);
  assert.equal(canGoPrevious(state), true);
  state = previousPage(state);
  assert.equal(state.page, 1);
  assert.equal(previousPage(state).page, 1, "there is no page before the first");
});

test("an out_of_range answer fixes the last page and keeps the page on screen", () => {
  const onThree = nextPage(nextPage(startPreview()));
  assert.equal(previewOutcome({ code: "out_of_range", message: "x" }, 4).kind, "end");
  const atEnd = markLastPage(onThree, 4);
  assert.equal(atEnd.page, 3);
  assert.equal(atEnd.last, 3);
  assert.equal(canGoNext(atEnd), false);
  assert.equal(nextPage(atEnd).page, 3, "Siguiente does nothing past the end");
  assert.equal(canGoPrevious(atEnd), true);
});

test("a document with no page at all is an error, not an end", () => {
  const outcome = previewOutcome({ code: "out_of_range", message: "" }, 1);
  assert.equal(outcome.kind, "error");
  assert.equal(outcome.retry, false);
  assert.ok(outcome.message.length > 0);
});

test("the cache keeps six pages and drops the least recently used", () => {
  assert.equal(PREVIEW_CACHE_SIZE, 6);
  const cache = createPageCache();
  const keys = [1, 2, 3, 4, 5, 6].map((page) => pageKey("att_1", page));
  for (const key of keys) cache.set(key, `url-${key}`);
  assert.equal(cache.size, 6);
  assert.equal(cache.get(keys[0]), "url-" + keys[0]);
  cache.set(pageKey("att_1", 7), "url-7");
  assert.equal(cache.size, 6);
  assert.equal(cache.has(keys[0]), true, "a page read again is kept");
  assert.equal(cache.has(keys[1]), false, "the oldest unread page goes");
  assert.equal(cache.has(pageKey("att_1", 7)), true);
});

test("page keys tell attachments and pages apart", () => {
  assert.notEqual(pageKey("att_1", 1), pageKey("att_1", 2));
  assert.notEqual(pageKey("att_1", 1), pageKey("att_2", 1));
});

test("unavailable means the old behaviour: no viewer, the system opens the file", () => {
  assert.deepEqual(previewOutcome({ code: "unavailable", message: "x" }, 1), { kind: "fallback" });
});

test("tool_missing shows the host's Spanish text and no retry", () => {
  const text = "Falta GridCraft en este PC. El Master puede instalarla desde Herramientas de documentos";
  assert.deepEqual(previewOutcome({ code: "tool_missing", message: text }, 1), {
    kind: "error",
    message: text,
    retry: false,
  });
  const fallback = previewOutcome({ code: "tool_missing", message: "" }, 1);
  assert.equal(fallback.kind, "error");
  assert.ok(fallback.message.length > 0);
  assert.equal(fallback.retry, false);
});

test("other, a network error and a shape nobody expected all offer a retry", () => {
  for (const error of [{ code: "other", message: "x" }, "invoke failed", undefined, {}, null]) {
    const outcome = previewOutcome(error, 2);
    assert.equal(outcome.kind, "error", JSON.stringify(error));
    assert.equal(outcome.retry, true, JSON.stringify(error));
    assert.ok(outcome.message.length > 0, JSON.stringify(error));
  }
});

test("only xlsx, docx and pdf open the viewer", () => {
  for (const name of ["Informe Q1.xlsx", "carta.DOCX", "bitacora.pdf"]) {
    assert.equal(opensPreview(name), true, name);
  }
  for (const name of ["datos.csv", "notas.txt", "readme.md", "config.json", "deck.pptx", "foto.png", "sin-extension", "", undefined]) {
    assert.equal(opensPreview(name), false, String(name));
  }
});

test("the keys: arrows move, Escape closes, the rest does nothing", () => {
  assert.equal(keyAction("ArrowLeft"), "previous");
  assert.equal(keyAction("ArrowRight"), "next");
  assert.equal(keyAction("Escape"), "close");
  assert.equal(keyAction("a"), null);
});

test("Tab wraps inside the dialog in both directions", () => {
  assert.equal(nextFocus(0, 3, false), 1);
  assert.equal(nextFocus(2, 3, false), 0);
  assert.equal(nextFocus(0, 3, true), 2);
  assert.equal(nextFocus(-1, 3, false), 0, "focus outside the dialog comes in at the start");
  assert.equal(nextFocus(-1, 3, true), 2);
  assert.equal(nextFocus(0, 0, false), -1, "nothing to focus");
});

test("the page label is Spanish", () => {
  assert.equal(pageLabel(3), "Página 3");
});
