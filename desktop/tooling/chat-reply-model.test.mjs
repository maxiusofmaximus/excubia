// Run with: node --test tooling/chat-reply-model.test.mjs
// Quick answers and replies: where the suggestions sit, when they go away, how
// a quote and the reply bar read, and the words of a reply the host forgot.
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  UNKNOWN_REPLY_NOTICE,
  afterSend,
  messageById,
  previewLine,
  quickRepliesDisabled,
  quoteLabel,
  replyBarText,
  restoredText,
  sendArgs,
  suggestionsAt,
} from "../ui/chat-reply-model.js";

const own = (seq, id, body = "hola") => ({
  kind: "message",
  seq,
  message: { id, seq, author_kind: "user", body_md: body },
});
const agent = (seq, id, suggestions) => ({
  kind: "message",
  seq,
  message: { id, seq, author_kind: "agent", body_md: "¿Qué nombre?", ...(suggestions ? { suggestions } : {}) },
});

test("quick answers sit under the last agent message only", () => {
  const entries = [agent(1, "msg_1", ["Sí"]), own(2, "msg_2"), agent(3, "msg_3", ["Sí, créalo", "Cámbialo"])];
  assert.deepEqual(suggestionsAt(entries), { index: 2, items: ["Sí, créalo", "Cámbialo"] });
  // An older message with suggestions is never offered again.
  assert.equal(suggestionsAt([agent(1, "msg_1", ["Sí"]), agent(2, "msg_2")]), null);
});

test("quick answers vanish once the operator has answered after them", () => {
  const entries = [agent(1, "msg_1", ["Sí, créalo"]), own(2, "msg_2", "Sí, créalo")];
  assert.equal(suggestionsAt(entries), null);
});

test("no suggestions, or none at all, means no row", () => {
  assert.equal(suggestionsAt([agent(1, "msg_1")]), null);
  assert.equal(suggestionsAt([own(1, "msg_1")]), null);
  assert.equal(suggestionsAt([]), null);
});

test("quick answers wait while a send is in flight and come back when it fails", () => {
  assert.equal(quickRepliesDisabled([{ state: "sending" }]), true);
  assert.equal(quickRepliesDisabled([{ state: "failed" }]), false);
  assert.equal(quickRepliesDisabled([]), false);
});

test("a quote reads «Responde a <autor>: <preview>» and the operator is «ti»", () => {
  assert.equal(
    quoteLabel({ id: "msg_1", author_kind: "agent", preview: "¿Qué nombre?" }, "Alfred"),
    "Responde a Alfred: ¿Qué nombre?",
  );
  assert.equal(quoteLabel({ id: "msg_2", author_kind: "user", preview: "hola" }, "Alfred"), "Responde a ti: hola");
});

test("the reply bar shows one line, cut when it runs long", () => {
  assert.equal(
    replyBarText({ id: "msg_1", author_kind: "agent", preview: "¿Qué\n  nombre?" }, "Alfred"),
    "Respondiendo a Alfred: ¿Qué nombre?",
  );
  const long = previewLine("a".repeat(200), 20);
  assert.equal(long.length, 20);
  assert.ok(long.endsWith("…"));
});

test("the words of a reply the host forgot", () => {
  assert.equal(
    UNKNOWN_REPLY_NOTICE,
    "Ese mensaje ya no está en la conversación. Se quitó la cita; tu texto sigue escrito.",
  );
});

test("a loaded message is found by id, a missing one is null", () => {
  const entries = [agent(1, "msg_1"), own(2, "msg_2")];
  assert.equal(messageById(entries, "msg_2").seq, 2);
  assert.equal(messageById(entries, "msg_9"), null);
});

test("a quick answer sends its text exactly and no reply", () => {
  assert.deepEqual(sendArgs("Sí, créalo", null), { text: "Sí, créalo", replyTo: null });
});

test("a reply sends its id, and the bar clears only after that same send", () => {
  const reply = { id: "msg_1", author_kind: "agent", preview: "¿Qué nombre?" };
  assert.deepEqual(sendArgs("Demo", reply), { text: "Demo", replyTo: "msg_1" });
  assert.equal(afterSend(reply, reply), null);
  // A reply chosen while the first one was sending stays on screen.
  const other = { id: "msg_2", author_kind: "user", preview: "hola" };
  assert.equal(afterSend(other, reply), other);
});

test("a refused quote keeps the text: restored alone, or above what is typed", () => {
  assert.equal(restoredText("", "Demo"), "Demo");
  assert.equal(restoredText("  ", "Demo"), "Demo");
  assert.equal(restoredText("otra cosa", "Demo"), "Demo\notra cosa");
});
