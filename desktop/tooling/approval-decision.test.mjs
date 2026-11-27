// Run with: node --test tooling/approval-decision.test.mjs
// What a click on PERMITIR/DENEGAR may show: only the host's answer settles the card.
import { test } from "node:test";
import assert from "node:assert/strict";

import { t } from "../ui/i18n.js";
import { decisionOutcome, stampFor } from "../ui/approval-model.js";

const expiredRefusal = { kind: "approvalConflict", code: "approval_expired", message: "x" };
const otherConflict = { kind: "approvalConflict", code: "approval_already_decided", message: "x" };

test("a host reply with a settled status settles the card, and is not late", () => {
  const out = decisionOutcome({ status: "approved" }, null);
  assert.equal(out.patch.decided, "approved");
  assert.equal(out.patch.busy, false);
  assert.equal(out.late, false);
});

test("a host reply still pending does not settle the card; the card can be answered again", () => {
  const out = decisionOutcome({ status: "pending" }, null);
  assert.equal("decided" in out.patch, false);
  assert.equal(out.patch.busy, false);
});

test("a reply with status expired is late and shows the host's expired state", () => {
  const out = decisionOutcome({ status: "expired" }, null);
  assert.equal(out.late, true);
  assert.equal(out.patch.decided, "expired");
});

test("approval_expired is a late refusal: the card reads the host's expired state", () => {
  const out = decisionOutcome(null, expiredRefusal);
  assert.equal(out.late, true);
  assert.equal(out.patch.decided, "expired");
  assert.equal(out.patch.busy, false);
});

test("another conflict or a failed call never shows the card as decided", () => {
  for (const error of [otherConflict, { kind: "forbidden" }, new Error("network")]) {
    const out = decisionOutcome(null, error);
    assert.equal(out.late, false);
    assert.equal("decided" in out.patch, false);
    assert.equal(out.patch.busy, false);
  }
});

test("the stamp says «Enviando…» while the decision is in flight, not «¡Tu OK!»", () => {
  assert.equal(stampFor({ busy: true }), t("approval.sending"));
  assert.equal(stampFor({}), t("approval.stamp"));
  assert.notEqual(stampFor({ busy: true }), stampFor({}));
});

test("the late text exists in both languages", () => {
  assert.ok(t("approval.late_expired"));
  assert.notEqual(t("approval.late_expired"), "approval.late_expired");
});
