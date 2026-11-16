// Run with: node --test tooling/chat-row-model.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";

import { rowBadge } from "../ui/chat-row-model.js";

test("una aprobación pendiente va antes que el trabajo en curso", () => {
  assert.deepEqual(rowBadge({ waiting_approvals: 1, active_runs: 1 }), { kind: "approval", text: "Aprobación" });
  assert.deepEqual(rowBadge({ waiting_approvals: 3 }), { kind: "approval", text: "3 aprobaciones" });
  assert.deepEqual(rowBadge({ active_runs: 2 }), { kind: "working", text: "Trabajando" });
  assert.equal(rowBadge({ unread: 4 }), null);
});
