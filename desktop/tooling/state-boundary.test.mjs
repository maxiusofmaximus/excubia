// Run with: node --test tooling/state-boundary.test.mjs
// Un estado sin lista de agentes no entra en la app: antes reventaba con «reading 'length'».

import { test } from "node:test";
import assert from "node:assert/strict";
import { readableState } from "../ui/format.js";

test("un estado con su lista de agentes pasa tal cual", () => {
  const state = { agents: [], revision: 1 };
  assert.equal(readableState(state), state);
});

test("sin lista de agentes, o sin estado, no hay estado", () => {
  assert.equal(readableState({}), null);
  assert.equal(readableState({ agents: "x" }), null);
  assert.equal(readableState(null), null);
  assert.equal(readableState(undefined), null);
});
