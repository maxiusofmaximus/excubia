// Run with: node --test tooling/chat-progress-model.test.mjs
// The step of the open turn: plain text in one line, frames per run that never go
// backwards, a row from the host that seeds or clears the line, and a live reading
// at most once per change and every 5 s.
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  ANNOUNCE_GAP_MS,
  PROGRESS_MAX,
  announcement,
  applyProgress,
  cleanProgress,
  openStep,
  progressText,
  syncProgress,
} from "../ui/chat-progress-model.js";

const frame = (revision, runId, text, conversationId = "cnv_1") => ({
  event: "chat.progress",
  revision,
  conversation_id: conversationId,
  run_id: runId,
  text,
});

test("a frame sets the line of its conversation", () => {
  const store = {};
  applyProgress(store, frame(60, "run_a", "Revisando los borradores"));
  assert.equal(progressText(store, "cnv_1"), "Revisando los borradores");
  assert.equal(progressText(store, "cnv_2"), null);
});

test("a null frame clears the line of its own run", () => {
  const store = {};
  applyProgress(store, frame(60, "run_a", "Paso uno"));
  applyProgress(store, frame(61, "run_a", null));
  assert.equal(progressText(store, "cnv_1"), null);
  assert.deepEqual(store, {});
});

test("a frame of an older run never replaces a newer line", () => {
  const store = {};
  applyProgress(store, frame(70, "run_b", "Línea nueva"));
  applyProgress(store, frame(65, "run_a", "Línea vieja"));
  assert.equal(progressText(store, "cnv_1"), "Línea nueva");
  // A late close of the old run does not clear the new one either.
  applyProgress(store, frame(71, "run_a", null));
  assert.equal(progressText(store, "cnv_1"), "Línea nueva");
});

test("a frame that arrives before its conversation is kept for it", () => {
  const store = {};
  applyProgress(store, frame(80, "run_c", "Antes de la lista", "cnv_9"));
  assert.equal(progressText(store, "cnv_9"), "Antes de la lista");
  // The row comes later with its own line: the frame is newer and stays.
  syncProgress(store, { id: "cnv_9", progress: "Línea del listado" });
  assert.equal(progressText(store, "cnv_9"), "Antes de la lista");
});

test("a conversation without progress clears the line even if the close frame was lost", () => {
  const store = {};
  applyProgress(store, frame(90, "run_d", "Sigue trabajando"));
  syncProgress(store, { id: "cnv_1", active_runs: 0 });
  assert.equal(progressText(store, "cnv_1"), null);
});

test("a row with a line seeds the store when no frame has come", () => {
  const store = {};
  syncProgress(store, { id: "cnv_1", progress: "Leyendo el informe" });
  assert.equal(progressText(store, "cnv_1"), "Leyendo el informe");
});

test("the line is plain text: markup and links are shown as written", () => {
  assert.equal(cleanProgress("<b>negrita</b>"), "<b>negrita</b>");
  assert.equal(cleanProgress("mira https://example.com <img src=x>"), "mira https://example.com <img src=x>");
});

test("a long line is cut to 120 characters with an ellipsis", () => {
  const long = "palabra ".repeat(40);
  const line = cleanProgress(long);
  assert.ok([...line].length <= PROGRESS_MAX);
  assert.ok(line.endsWith("…"));
  assert.equal(cleanProgress("corto"), "corto");
});

test("control characters and whitespace collapse; empty text is no line", () => {
  assert.equal(cleanProgress("a\u0007b\n\t c\u009F  d"), "a b c d");
  assert.equal(cleanProgress("  \n\t "), null);
  assert.equal(cleanProgress(""), null);
  assert.equal(cleanProgress(null), null);
  assert.equal(cleanProgress(42), null);
});

test("the step of the open conversation shows only while its turn is in flight", () => {
  const store = {};
  applyProgress(store, frame(100, "run_f", "Buscando el archivo"));
  const open = { id: "cnv_1" };
  assert.equal(openStep({ waiting: true, conversation: open, progress: store }), "Buscando el archivo");
  assert.equal(openStep({ waiting: false, conversation: open, progress: store }), null);
  assert.equal(openStep({ waiting: true, conversation: null, progress: store }), null);
});

test("an older host without progress shows no line and stores none", () => {
  const store = {};
  syncProgress(store, { id: "cnv_1", title: "Plan" });
  assert.deepEqual(store, {});
  assert.equal(progressText(store, "cnv_1"), null);
});

test("a live reading announces each change once and never within 5 s of the last", () => {
  const store = {};
  applyProgress(store, frame(100, "run_e", "Primero"));
  const entry = store.cnv_1;
  assert.equal(announcement(entry, 1000), "polite");
  assert.equal(announcement(entry, 1200), "off");

  applyProgress(store, frame(101, "run_e", "Segundo"));
  assert.equal(announcement(store.cnv_1, 1000 + 1000), "off");
  assert.equal(announcement(store.cnv_1, 1000 + ANNOUNCE_GAP_MS), "polite");
  assert.equal(announcement(store.cnv_1, 1000 + ANNOUNCE_GAP_MS + 10), "off");
});
