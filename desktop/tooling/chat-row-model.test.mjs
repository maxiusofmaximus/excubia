// Run with: node --test tooling/chat-row-model.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";

import { isBlankChat, rowBadge } from "../ui/chat-row-model.js";

test("una aprobación pendiente va antes que el trabajo en curso", () => {
  assert.deepEqual(rowBadge({ waiting_approvals: 1, active_runs: 1 }), { kind: "approval", text: "Pide tu OK" });
  assert.deepEqual(rowBadge({ waiting_approvals: 3 }), { kind: "approval", text: "3 piden tu OK" });
  assert.deepEqual(rowBadge({ active_runs: 2 }), { kind: "working", text: "En curso" });
  assert.equal(rowBadge({ unread: 4 }), null);
});

test("un «Nuevo chat» sin mensajes se oculta, salvo el abierto", () => {
  const empty = { id: "c1", last_message_at: "", last_seq: 0 };
  assert.equal(isBlankChat(empty, "otro"), true);
  assert.equal(isBlankChat(empty, "c1"), false);
  // The host stamps last_message_at when the chat is created: only last_seq decides.
  assert.equal(isBlankChat({ id: "c6", last_message_at: "2026-11-16T10:00:00Z", last_seq: 0 }, "x"), true);
  assert.equal(isBlankChat({ id: "c2", last_message_at: "2026-11-16T10:00:00Z", last_seq: 4 }, "x"), false);
  assert.equal(isBlankChat({ id: "c3", last_seq: 0, pinned: true }, "x"), false);
  assert.equal(isBlankChat({ id: "c4", last_seq: 0, waiting_approvals: 1 }, "x"), false);
  // A row without last_seq hides nothing.
  assert.equal(isBlankChat({ id: "c5", last_message_at: "" }, "x"), false);
});
