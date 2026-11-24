// Run with: node --test tooling/chat-row-model.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";

import { conversationTitle, isBlankChat, rowBadge } from "../ui/chat-row-model.js";
import { setLang } from "../ui/i18n.js";

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

test("el «Nuevo chat» vacío del host se lee en el idioma elegido; los títulos propios no cambian", () => {
  try {
    const empty = { title: "Nuevo chat", last_seq: 0 };
    setLang("es");
    assert.equal(conversationTitle(empty), "Nuevo chat");
    setLang("en");
    assert.equal(conversationTitle(empty), "New chat");
    // A chat with messages, a title the user wrote, and a row without last_seq keep their title.
    assert.equal(conversationTitle({ title: "Nuevo chat", last_seq: 3 }), "Nuevo chat");
    assert.equal(conversationTitle({ title: "Plan semanal", last_seq: 0 }), "Plan semanal");
    assert.equal(conversationTitle({ title: "Nuevo chat" }), "Nuevo chat");
  } finally {
    setLang("es");
  }
});

test("«Avisos del sistema» se lee como System notices; el título guardado no cambia", () => {
  try {
    const notices = { title: "Avisos del sistema", last_seq: 4 };
    setLang("es");
    assert.equal(conversationTitle(notices), "Avisos del sistema");
    setLang("en");
    assert.equal(conversationTitle(notices), "System notices");
    assert.equal(notices.title, "Avisos del sistema");
    // A title that only resembles it is the user's own and keeps its text.
    assert.equal(conversationTitle({ title: "Avisos", last_seq: 4 }), "Avisos");
  } finally {
    setLang("es");
  }
});
