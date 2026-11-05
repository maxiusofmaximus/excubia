// Run with: node --test tooling/mail-kinds.test.mjs
// The arrow keys of the sign-in kind picker, which is a radio group of our own.

import { test } from "node:test";
import assert from "node:assert/strict";

import { radioStep } from "../ui/mail-model.js";

test("las flechas avanzan y retroceden, dando la vuelta en los extremos", () => {
  assert.equal(radioStep(0, "ArrowDown", 3), 1);
  assert.equal(radioStep(0, "ArrowRight", 3), 1);
  assert.equal(radioStep(2, "ArrowDown", 3), 0);
  assert.equal(radioStep(0, "ArrowUp", 3), 2);
  assert.equal(radioStep(0, "ArrowLeft", 3), 2);
});

test("cualquier otra tecla no mueve la selección", () => {
  assert.equal(radioStep(1, "Enter", 3), null);
  assert.equal(radioStep(1, " ", 3), null);
  assert.equal(radioStep(1, "__proto__", 3), null);
});
