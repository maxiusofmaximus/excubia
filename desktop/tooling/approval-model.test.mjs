// Run with: node --test tooling/approval-model.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  canApproveMove,
  commandText,
  conversationContext,
  countdownText,
  cwdLabel,
  destLabel,
  excludeFor,
  grantCommand,
  isChecking,
  isMailMove,
  isMoveBatch,
  isOperatorLevel,
  levelChain,
  localFor,
  moveApproveLabel,
  moveCounts,
  moveItems,
  triageLabel,
  moveTitle,
  patchLocal,
  remainingSeconds,
  sameIds,
  seatName,
  setBootstrap,
  shortDigest,
  statusMeta,
  subjectText,
  supersededIndexes,
} from "../ui/approval-model.js";

test("the chain stops at the level that holds the approval now", () => {
  assert.deepEqual(levelChain("lead"), ["policy", "scorer", "lead"]);
  assert.deepEqual(levelChain("user"), ["policy", "scorer", "lead", "cto", "user"]);
  assert.deepEqual(levelChain("unknown"), []);
});

test("only the operator's level earns a card", () => {
  assert.equal(isOperatorLevel({ level: "user" }), true);
  assert.equal(isOperatorLevel({ level: "lead" }), false);
  assert.equal(isOperatorLevel({}), false);
});

test("the countdown never goes below zero and ignores a bad date", () => {
  const now = Date.parse("2026-10-23T10:00:00.000Z");
  assert.equal(remainingSeconds("2026-10-23T10:02:00.000Z", now), 120);
  assert.equal(remainingSeconds("2026-10-23T09:59:00.000Z", now), 0);
  assert.equal(remainingSeconds("no es una fecha", now), null);
});

test("the countdown reads minutes and padded seconds", () => {
  assert.equal(countdownText(120), "2:00");
  assert.equal(countdownText(65), "1:05");
  assert.equal(countdownText(9), "0:09");
});

test("a Windows cwd reads as its last folder", () => {
  assert.equal(cwdLabel("D:\\proyectos\\demo"), "demo/…");
  assert.equal(cwdLabel("/home/max/app/"), "app/…");
  assert.equal(cwdLabel(""), "");
});

test("the digest is shortened but keeps its algorithm prefix", () => {
  assert.equal(shortDigest("sha256:9f86d0aabbccddeeff00"), "sha256:9f86d0a…");
  assert.equal(shortDigest("sha256:abc"), "sha256:abc");
  assert.equal(shortDigest(undefined), "");
});

test("the agent name comes from the seat id when the host sends none", () => {
  assert.equal(seatName({ seat_id: "ag_bob" }), "Bob");
  assert.equal(seatName({ seat_id: "ag_bob", agent_label: "Roberto" }), "Roberto");
  assert.equal(seatName({}), "Un agente");
  assert.equal(seatName({ agent_label: "01M5MA7C4KBNJ1H4Q6RSJHKVYP" }), "Un agente");
  assert.equal(seatName({ seat_id: "ag_01M5MA7C4KBNJ1H4Q6RSJHKVYP" }), "Un agente");
});

test("the canonical grant command names the device and the permission", () => {
  assert.equal(grantCommand("dev_01"), "host devices grant dev_01 --permission approve");
  assert.equal(grantCommand(null), "host devices grant <dispositivo> --permission approve");
});

test("the command comes from the redacted payload before the summary", () => {
  assert.equal(commandText({ payload_redacted: { command: "cargo test" }, summary: "x" }), "cargo test");
  assert.equal(commandText({ summary: "Ejecutar tests" }), "Ejecutar tests");
});

test("an unknown status gets a neutral label instead of a crash", () => {
  assert.deepEqual(statusMeta("weird"), { label: "weird", tone: "unknown" });
  assert.equal(statusMeta("expired").tone, "expired");
});

test("local facts are kept per approval and merged, not replaced", () => {
  patchLocal("apr_a", { clientId: "c1" });
  patchLocal("apr_a", { busy: true });
  assert.deepEqual(localFor("apr_a"), { clientId: "c1", busy: true });
  assert.deepEqual(localFor("apr_missing"), {});
});

test("one approval keeps only its newest row in the feed", () => {
  const entries = [
    { kind: "message", message: {} },
    { kind: "approval", entry: { ref_id: "apr_1", status: "pending" } },
    { kind: "approval", entry: { ref_id: "apr_2", status: "pending" } },
    { kind: "approval", entry: { ref_id: "apr_1", status: "approved" } },
  ];
  assert.deepEqual([...supersededIndexes(entries)], [1]);
});

test("an approval with a card is not drawn again as a timeline line", () => {
  const entries = [
    { kind: "approval", entry: { ref_id: "apr_1", status: "approved" } },
    { kind: "approval", entry: { ref_id: "apr_2", status: "approved" } },
  ];
  assert.deepEqual([...supersededIndexes(entries, new Set(["apr_1"]))], [0]);
});

test("the bootstrap names the seats and the conversations the operator reads", () => {
  setBootstrap({
    bootstrap: {
      alfred: { agent_id: "01M5MA7C4KBNJ1H4Q6RSJHKVYP", name: "Alfred" },
      projects: [{ id: "prj_1", name: "Demo", lead: { agent_id: "ag_lead", name: "Ernesto" } }],
    },
    conversations: [{ id: "cnv_1", title: "Plan", project_id: "prj_1" }],
  });
  assert.equal(seatName({ seat_id: "01M5MA7C4KBNJ1H4Q6RSJHKVYP" }), "Alfred");
  assert.equal(seatName({ seat_id: "ag_lead" }), "Ernesto");
  assert.equal(conversationContext("cnv_1"), "Demo · Plan");
  assert.equal(conversationContext("cnv_missing"), "");
  setBootstrap(null);
  assert.equal(seatName({ seat_id: "ag_lead" }), "Lead");
});

test("a pending card at zero asks the host; the host still decides", () => {
  const now = Date.parse("2026-10-23T10:00:00.000Z");
  assert.equal(isChecking({ status: "pending", expires_at: "2026-10-23T09:59:59.000Z" }, now), true);
  assert.equal(isChecking({ status: "pending", expires_at: "2026-10-23T10:01:00.000Z" }, now), false);
  assert.equal(isChecking({ status: "expired", expires_at: "2026-10-23T09:59:59.000Z" }, now), false);
});

const batch = {
  tool: "mail.move",
  details: {
    account: "trabajo",
    items: [
      { message_id: "m1", dest: "junkemail", from_address: "ofertas@tienda.invalid", subject: "Oferta", level: "lead" },
      { message_id: "m2", dest: "inbox", subject: "Factura", level: "lead", excluded: true },
      { message_id: "m3", dest: "deleteditems", subject: "Boletín", level: "lead" },
    ],
  },
};

test("a mail.move with mails earns the checklist, anything else does not", () => {
  assert.equal(isMailMove({ tool: "mail.send" }), false);
  assert.equal(isMoveBatch(batch), true);
  assert.equal(isMoveBatch({ tool: "mail.move" }), false);
  assert.equal(isMoveBatch({ tool: "mail.move", details: { items: [] } }), false);
});

test("the batch keeps the host's excluded marks and drops rows that cannot be answered", () => {
  const items = moveItems({
    tool: "mail.move",
    details: { items: [{ message_id: "m1" }, { subject: "sin id" }, null, { message_id: 7 }, { message_id: "m2", excluded: 1 }] },
  });
  assert.deepEqual(items.map((item) => item.message_id), ["m1", "m2"]);
  assert.deepEqual(items.map((item) => item.excluded), [false, true]);
  assert.deepEqual(moveItems({ details: { items: "no es una lista" } }), []);
  assert.deepEqual(moveItems({}), []);
});

test("a destination reads in Spanish and an unknown one never shows raw", () => {
  assert.equal(destLabel("junkemail"), "No deseado");
  assert.equal(destLabel("deleteditems"), "Eliminados");
  assert.equal(destLabel("inbox"), "Bandeja de entrada");
  assert.equal(destLabel("constructor"), "Destino desconocido");
  assert.equal(triageLabel("bulk"), "Masivo");
  assert.equal(triageLabel("lead"), "Sin evaluar");
  assert.equal(triageLabel("constructor"), "Sin evaluar");
});

test("a long subject is trimmed to one short line, and an empty one says so", () => {
  assert.equal(subjectText("  Hola  "), "Hola");
  assert.equal(subjectText(""), "(sin asunto)");
  assert.equal(subjectText(undefined), "(sin asunto)");
  assert.equal(subjectText("x".repeat(100)).length, 80);
  assert.ok(subjectText("x".repeat(100)).endsWith("…"));
});

test("the unchecked mails are the ones the operator leaves out of this batch", () => {
  const unchecked = new Set(["m1", "m9"]);
  assert.deepEqual(excludeFor(batch, unchecked), ["m1"]);
  assert.deepEqual(excludeFor(batch), []);
  assert.deepEqual(excludeFor({}, unchecked), []);
});

test("the count and the label say how many mails stay checked", () => {
  assert.deepEqual(moveCounts(batch, new Set()), { checked: 2, total: 2 });
  assert.deepEqual(moveCounts(batch, new Set(["m1"])), { checked: 1, total: 2 });
  assert.equal(moveApproveLabel(batch, new Set(["m1"])), "Mover los seleccionados (1 de 2)");
  assert.equal(moveTitle(batch), "3 correos a mover");
});

test("the approval cannot go ahead with every mail unchecked", () => {
  assert.equal(canApproveMove(batch, new Set()), true);
  assert.equal(canApproveMove(batch, new Set(["m1", "m3"])), false);
  assert.equal(canApproveMove({ tool: "mail.move", details: { items: [] } }), false);
});

test("a retry reuses its id only for the same list of mails left out", () => {
  assert.equal(sameIds(["m1"], ["m1"]), true);
  assert.equal(sameIds(["m1"], ["m1", "m2"]), false);
  assert.equal(sameIds(undefined, []), true);
});
