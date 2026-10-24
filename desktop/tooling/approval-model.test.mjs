// Run with: node --test tooling/approval-model.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  commandText,
  countdownText,
  cwdLabel,
  conversationContext,
  grantCommand,
  isChecking,
  isOperatorLevel,
  levelChain,
  localFor,
  patchLocal,
  remainingSeconds,
  seatName,
  setBootstrap,
  shortDigest,
  statusMeta,
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
