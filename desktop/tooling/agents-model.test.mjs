import { test } from "node:test";
import assert from "node:assert/strict";
import { oneRowPerPane, shouldCloseMenu } from "../ui/agents-model.js";

test("two agents with the same pane id are one row; the first one stays", () => {
  const rows = oneRowPerPane([
    { pane_id: "p1", title: "Ernesto — lead", restored: true },
    { pane_id: "p1", title: "Ernesto — lead", restored: false },
  ]);
  assert.equal(rows.length, 1);
  assert.equal(rows[0].restored, true);
});

test("two agents with different pane ids stay two rows", () => {
  const rows = oneRowPerPane([
    { pane_id: "p1", title: "Ernesto — lead" },
    { pane_id: "p2", title: "Ernesto — lead" },
  ]);
  assert.equal(rows.length, 2);
});

test("an empty or missing pane id is never a duplicate", () => {
  const rows = oneRowPerPane([{ pane_id: "" }, { pane_id: "" }, { title: "sin panel" }, { title: "sin panel" }]);
  assert.equal(rows.length, 4);
});

test("the order of the rows is the host's order", () => {
  const rows = oneRowPerPane([{ pane_id: "b" }, { pane_id: "a" }, { pane_id: "b" }]);
  assert.deepEqual(rows.map((row) => row.pane_id), ["b", "a"]);
});

test("Esc closes a card's ⋯ menu, wherever the focus is", () => {
  assert.equal(shouldCloseMenu("Escape", false), true);
  assert.equal(shouldCloseMenu("Escape", true), true);
});

test("a click outside the menu closes it; a click inside keeps it open", () => {
  assert.equal(shouldCloseMenu("click", false), true);
  assert.equal(shouldCloseMenu("click", true), false);
});

test("other keys leave the menu open", () => {
  assert.equal(shouldCloseMenu("Enter", false), false);
  assert.equal(shouldCloseMenu("Tab", false), false);
});
