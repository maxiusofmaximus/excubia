// Run with: node --test tooling/chat-select-model.test.mjs
// The chat history's selection: the click, Ctrl and Shift rules, Ctrl+A, the rows that
// leave the list, and the batches of a bulk delete.
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  clear,
  clickRow,
  createSelection,
  deleteBatches,
  inOrder,
  only,
  prune,
  selectAll,
} from "../ui/chat-select-model.js";

const ORDER = ["a", "b", "c", "d", "e"];
const picked = (selection) => [...selection.ids].sort();

test("a plain click opens the chat and leaves no selection, the anchor moves", () => {
  const selection = createSelection();
  clickRow(selection, ORDER, "b", { ctrl: true });
  clickRow(selection, ORDER, "c");
  assert.deepEqual(picked(selection), []);
  assert.equal(selection.anchor, "c");
});

test("Ctrl+click adds a row, and a second Ctrl+click on it takes it out", () => {
  const selection = createSelection();
  clickRow(selection, ORDER, "a", { ctrl: true });
  clickRow(selection, ORDER, "d", { ctrl: true });
  assert.deepEqual(picked(selection), ["a", "d"]);
  clickRow(selection, ORDER, "a", { ctrl: true });
  assert.deepEqual(picked(selection), ["d"]);
});

test("Shift+click selects the range from the anchor, in both directions", () => {
  const down = createSelection();
  clickRow(down, ORDER, "b");
  clickRow(down, ORDER, "d", { shift: true });
  assert.deepEqual(picked(down), ["b", "c", "d"]);
  assert.equal(down.anchor, "b", "the anchor stays where the range starts");

  const up = createSelection();
  clickRow(up, ORDER, "d");
  clickRow(up, ORDER, "b", { shift: true });
  assert.deepEqual(picked(up), ["b", "c", "d"]);
});

test("Shift+click without an anchor in the list acts as a plain click", () => {
  const selection = createSelection();
  clickRow(selection, ORDER, "e", { shift: true });
  assert.deepEqual(picked(selection), []);
  assert.equal(selection.anchor, "e");
});

test("Ctrl+A selects every visible row, and Esc (clear) leaves none", () => {
  const selection = createSelection();
  selectAll(selection, ORDER);
  assert.deepEqual(picked(selection), ORDER);
  assert.equal(selection.anchor, "a");
  clear(selection);
  assert.deepEqual(picked(selection), []);
  assert.equal(selection.anchor, null);
});

test("only() makes the one row the whole selection, as a right click does", () => {
  const selection = createSelection();
  clickRow(selection, ORDER, "a", { ctrl: true });
  clickRow(selection, ORDER, "b", { ctrl: true });
  only(selection, "c");
  assert.deepEqual(picked(selection), ["c"]);
});

test("prune drops the rows that left the list and the anchor with them", () => {
  const selection = createSelection();
  selectAll(selection, ORDER);
  selection.anchor = "b";
  prune(selection, ["a", "c", "e"]);
  assert.deepEqual(picked(selection), ["a", "c", "e"]);
  assert.equal(selection.anchor, null);
});

test("inOrder keeps the list's order, whatever order the ids were picked in", () => {
  const selection = createSelection();
  clickRow(selection, ORDER, "e", { ctrl: true });
  clickRow(selection, ORDER, "b", { ctrl: true });
  assert.deepEqual(inOrder(selection, ORDER), ["b", "e"]);
});

test("a bulk delete sends at most 100 ids per call, in order", () => {
  const ids = Array.from({ length: 250 }, (_, index) => `c${index}`);
  const batches = deleteBatches(ids);
  assert.deepEqual(batches.map((batch) => batch.length), [100, 100, 50]);
  assert.deepEqual(batches.flat(), ids);
});

test("a selection of 100 or fewer is one call; none is no call", () => {
  assert.equal(deleteBatches(Array.from({ length: 100 }, (_, i) => `x${i}`)).length, 1);
  assert.deepEqual(deleteBatches([]), []);
});
