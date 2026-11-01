// Run with: node --test tooling/chatdraft-model.test.mjs
// The mail draft card in the chat: its lines, the cut of long text and the button's name.
import { test } from "node:test";
import assert from "node:assert/strict";

import { PREVIEW_MAX, SUBJECT_MAX, clip, draftCardLines, reviewLabel } from "../ui/chatdraft-model.js";

const card = {
  account: "trabajo",
  draft_id: "drf_1",
  to: ["ana@example.com", "luis@example.com"],
  subject: "Presupuesto Q4",
  preview: "Hola Ana,   adjunto el presupuesto.",
};

test("the card shows its subject, its recipients and its preview on one line each", () => {
  assert.deepEqual(draftCardLines(card), {
    subject: "Presupuesto Q4",
    to: "ana@example.com, luis@example.com",
    preview: "Hola Ana, adjunto el presupuesto.",
  });
});

test("a draft without a subject or recipients reads as a placeholder, not an empty line", () => {
  const lines = draftCardLines({ ...card, to: [], subject: "  " });
  assert.equal(lines.subject, "(sin asunto)");
  assert.equal(lines.to, "—");
});

test("a long subject or preview is cut with an ellipsis and never split mid-character", () => {
  const long = "ñ😀".repeat(200);
  const lines = draftCardLines({ ...card, subject: long, preview: long });
  assert.equal(Array.from(lines.subject).length, SUBJECT_MAX);
  assert.equal(Array.from(lines.preview).length, PREVIEW_MAX);
  assert.ok(lines.subject.endsWith("…"));
  assert.ok(!lines.preview.includes("�"));
});

test("text within its limit is kept whole", () => {
  assert.equal(clip("exacto", 6), "exacto");
  assert.equal(clip("demasiado", 6), "demas…");
});

test("the review button names the draft it opens", () => {
  assert.equal(reviewLabel("Presupuesto Q4"), "Revisar borrador: Presupuesto Q4");
});
