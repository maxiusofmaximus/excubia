// Run with: node --test tooling/chat-filter-model.test.mjs
// The history chips: the order, the counts with «99+», what an older host hides,
// the request each chip makes and the empty words of each filter.
import { test } from "node:test";
import assert from "node:assert/strict";

import { EMPTY_TEXT, chipCount, chipName, chipsFor, filterArgs } from "../ui/chat-filter-model.js";

test("the chips keep their order: Todos, Pendientes de ti, Trabajando", () => {
  const labels = chipsFor({ all: 0, attention: 0, working: 0 }).map((chip) => chip.label);
  assert.deepEqual(labels, ["Todos", "Pendientes de ti", "Trabajando"]);
});

test("a count shows only above zero, and 99+ above ninety-nine", () => {
  assert.equal(chipCount(0), "");
  assert.equal(chipCount(undefined), "");
  assert.equal(chipCount(3), "3");
  assert.equal(chipCount(99), "99");
  assert.equal(chipCount(100), "99+");
  const [, attention] = chipsFor({ all: 120, attention: 100, working: 0 });
  assert.equal(attention.text, "99+");
  assert.equal(chipName(attention), "Pendientes de ti, 100");
});

test("an older host hides the chips: counts null means no row at all", () => {
  assert.deepEqual(chipsFor(null), []);
});

test("before the counts load, or when they fail, the chips show without numbers", () => {
  const loading = chipsFor(undefined);
  assert.equal(loading.length, 3);
  assert.ok(loading.every((chip) => chip.text === ""));
  assert.equal(chipName(loading[2]), "Trabajando");
});

test("each chip asks for its own filter; Todos reads the home list", () => {
  assert.equal(filterArgs("all"), null);
  assert.deepEqual(filterArgs("attention"), { filter: "attention" });
  assert.deepEqual(filterArgs("working"), { filter: "working" });
});

test("each filter has its own empty words", () => {
  assert.equal(EMPTY_TEXT.attention, "Nada pendiente de ti.");
  assert.equal(EMPTY_TEXT.working, "Ningún agente está trabajando ahora.");
});
