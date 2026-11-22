// Run with: node --test tooling/mail-draft-model.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";

import { accountsState, changeRequestText, draftActions, isAtRest } from "../ui/mail-model.js";
import { SAVED_TEXT, createDraftEditor } from "../ui/mail-edit.js";

test("the draft actions are offered only at rest, where no send is open or scheduled", () => {
  for (const phase of [null, "idle", "failed", "cancelled"]) {
    assert.equal(isAtRest(phase), true, String(phase));
    assert.deepEqual(draftActions(phase, false), { edit: true, decline: true, requestChanges: false });
  }
  for (const phase of ["asking", "awaiting", "send", "window", "sent"]) {
    assert.equal(isAtRest(phase), false, phase);
    assert.deepEqual(draftActions(phase, true), { edit: false, decline: false, requestChanges: false });
  }
});

test("«Pedir cambios a Alfred» needs a draft opened from the chat and a draft at rest", () => {
  assert.equal(draftActions(null, true).requestChanges, true);
  assert.equal(draftActions("window", true).requestChanges, false);
});

test("the request for changes names the draft in the composer", () => {
  assert.equal(changeRequestText("drf_1"), "Cambia el borrador drf_1: ");
});

test("the accounts list says whether it waits, is empty or has accounts", () => {
  assert.equal(accountsState(null), "loading");
  assert.equal(accountsState([]), "empty");
  assert.equal(accountsState([{ label: "trabajo" }]), "list");
});

test("a saved edit sends the subject and the body, then reads the draft again", async () => {
  const calls = [];
  const forgotten = [];
  let reloads = 0;
  const save = createDraftEditor({
    invoke: async (command, args) => {
      calls.push({ command, args });
    },
    forget: (draftId) => forgotten.push(draftId),
  });

  const result = await save("trabajo", "drf_1", { subject: "Nuevo", body: "Hola" }, () => {
    reloads += 1;
  });

  assert.deepEqual(calls, [
    { command: "mail_draft_update", args: { account: "trabajo", draftId: "drf_1", subject: "Nuevo", body: "Hola" } },
  ]);
  assert.deepEqual(forgotten, ["drf_1"]);
  assert.equal(reloads, 1);
  assert.deepEqual(result, { ok: true, text: SAVED_TEXT });
});

test("a refused edit keeps the draft as it was and says what the host answered", async () => {
  const forgotten = [];
  let reloads = 0;
  const save = createDraftEditor({
    invoke: async () => {
      throw { code: "mail_invalid_input", message: "El asunto es demasiado largo." };
    },
    forget: (draftId) => forgotten.push(draftId),
  });

  const result = await save("trabajo", "drf_1", { subject: "x", body: "y" }, () => {
    reloads += 1;
  });

  assert.deepEqual(result, { ok: false, text: "El asunto es demasiado largo." });
  assert.deepEqual(forgotten, []);
  assert.equal(reloads, 0);
});
