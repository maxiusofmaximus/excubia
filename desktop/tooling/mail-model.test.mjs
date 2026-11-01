// Run with: node --test tooling/mail-model.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  APPROVAL_DENIED_TEXT,
  APPROVAL_STALE_TEXT,
  SEND_FAILED_TEXT,
  addressLine,
  approvalStep,
  attachmentLine,
  countdownFor,
  digestRows,
  expiryMs,
  formatBytes,
  isOpen,
  needsSettings,
  phaseText,
  secondsUntil,
  sendStep,
  signInOutcome,
  signInTenant,
} from "../ui/mail-model.js";
import { isMailSend } from "../ui/approval-model.js";

test("the sign-in kinds map to the tenants the host takes", () => {
  assert.equal(signInTenant("personal"), "consumers");
  assert.equal(signInTenant("work"), "organizations");
  assert.equal(signInTenant("common"), null);
});

test("a pending sign-in keeps the poll going; the other states stop it", () => {
  assert.equal(signInOutcome({ status: "pending" }).done, false);
  assert.deepEqual(signInOutcome({ status: "ok", label: "trabajo", upn: "ana@ejemplo.invalid" }), {
    done: true,
    ok: true,
    text: "Cuenta conectada: ana@ejemplo.invalid.",
  });
  const denied = signInOutcome({ status: "denied", message: "No se completó el inicio de sesión." });
  assert.equal(denied.done, true);
  assert.equal(denied.ok, false);
  assert.equal(denied.text, "No se completó el inicio de sesión.");
});

test("an approval that is still pending keeps the flow waiting", () => {
  assert.deepEqual(approvalStep({ status: "pending" }), { next: "awaiting" });
  assert.deepEqual(approvalStep({ status: "approved" }), { next: "send" });
});

test("a denied or expired approval is a denial: nothing was sent", () => {
  assert.deepEqual(approvalStep({ status: "denied" }), { next: "failed", text: APPROVAL_DENIED_TEXT });
  assert.deepEqual(approvalStep({ status: "expired" }), { next: "failed", text: APPROVAL_DENIED_TEXT });
});

test("an approval already used is stale, and one nobody lists is gone", () => {
  assert.equal(approvalStep({ status: "consumed" }).text, APPROVAL_STALE_TEXT);
  assert.match(approvalStep(null).text, /No encuentro esa aprobación/);
});

test("the send settles on its own state and only pending keeps it open", () => {
  assert.deepEqual(sendStep({ state: "pending" }), { next: "window" });
  assert.deepEqual(sendStep({ state: "sent" }), { next: "sent" });
  assert.deepEqual(sendStep({ state: "cancelled" }), { next: "cancelled" });
  assert.deepEqual(sendStep({ state: "failed" }), { next: "failed", text: SEND_FAILED_TEXT });
});

test("open phases are the ones that still hold timers", () => {
  for (const phase of ["asking", "awaiting", "send", "window"]) assert.equal(isOpen(phase), true);
  for (const phase of ["sent", "cancelled", "failed"]) assert.equal(isOpen(phase), false);
});

test("the window names its countdown in seconds", () => {
  assert.equal(
    phaseText("window", { windowLeft: 7 }),
    "Se enviará en 7 s. Puedes cancelarlo hasta entonces.",
  );
  assert.match(phaseText("awaiting"), /Esperando tu aprobación/);
  assert.equal(phaseText("failed", { text: "Límite alcanzado." }), "Límite alcanzado.");
});

test("a failed phase without text still says something", () => {
  assert.equal(phaseText("failed"), SEND_FAILED_TEXT);
});

test("the countdown counts whole seconds and stops at zero", () => {
  const now = 1_000_000;
  assert.equal(secondsUntil(now + 9_200, now), 10);
  assert.equal(secondsUntil(now - 5_000, now), 0);
  assert.equal(countdownFor(now + 125_000, now), "2:05");
  assert.equal(expiryMs(120, now), now + 120_000);
});

test("sizes read in KB and MB, and recipients read as one line", () => {
  assert.equal(formatBytes(512), "512 B");
  assert.equal(formatBytes(12034), "12 KB");
  assert.equal(formatBytes(3 * 1024 * 1024), "3.0 MB");
  assert.equal(addressLine(["bea@ejemplo.invalid", "cy@ejemplo.invalid"]), "bea@ejemplo.invalid, cy@ejemplo.invalid");
  assert.equal(addressLine([]), "—");
});

test("the digest reads the host's details as rows, with dashes for what is empty", () => {
  const rows = digestRows({
    account: "trabajo",
    from: "ana@ejemplo.invalid",
    to: ["bea@ejemplo.invalid"],
    cc: [],
    subject: "Presupuesto",
    body_preview: "Hola Bea",
    attachments: [{ name: "presupuesto.pdf", size: 12034 }],
  });
  assert.deepEqual(rows, [
    ["Cuenta", "trabajo"],
    ["Remitente", "ana@ejemplo.invalid"],
    ["Para", "bea@ejemplo.invalid"],
    ["Cc", "—"],
    ["Asunto", "Presupuesto"],
    ["Adjuntos", "presupuesto.pdf (12 KB)"],
    ["Inicio del cuerpo", "Hola Bea"],
  ]);
  assert.equal(attachmentLine(undefined), "—");
  assert.equal(digestRows({}).find(([label]) => label === "Asunto")[1], "—");
});

test("only an approval for a mail send gets the digest", () => {
  assert.equal(isMailSend({ tool: "mail.send" }), true);
  assert.equal(isMailSend({ tool: "shell.exec" }), false);
});

test("only the codes the operator can fix in the settings point there", () => {
  assert.equal(needsSettings("mail_send_not_enabled"), true);
  assert.equal(needsSettings("forbidden"), true);
  assert.equal(needsSettings("mail_approval_stale"), false);
});
