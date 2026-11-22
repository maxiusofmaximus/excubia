// Run with: node --test tooling/plan-model.test.mjs
// «Plan propuesto»: etiquetas llanas de lo que toca, resumen, orden, nota y errores del host.

import { test } from "node:test";
import assert from "node:assert/strict";
import {
  NOTE_MAX,
  PERMISSION_TEXT,
  STATUS_LABELS,
  confirmApproveText,
  needsExtraConfirm,
  noteCount,
  noteError,
  planFailureText,
  planSignature,
  planView,
  summaryLine,
  sortPlans,
  touchLabels,
} from "../ui/plan-model.js";
import { LOAD_FAILED, OFFLINE_TEXT, REJECTED_TEXT } from "../ui/friendly-error.js";

function plan(overrides = {}) {
  return {
    id: "plan_1",
    status: "proposed",
    steps: [{ title: "Leer el repo", touches: ["files"] }],
    summary: "Plan de 1 paso · lo propone Ernesto · toca: ficheros",
    created_at: "2026-11-05T10:00:00.000Z",
    updated_at: "2026-11-05T10:00:00.000Z",
    ...overrides,
  };
}

test("cada toque del contrato tiene su etiqueta llana, sin repetir y en un orden fijo", () => {
  assert.deepEqual(touchLabels([{ title: "a", touches: ["tools", "files"] }, { title: "b", touches: ["files"] }]), [
    "Toca ficheros",
    "Usa herramientas",
  ]);
  assert.deepEqual(touchLabels([{ title: "a", touches: ["mail"] }, { title: "b", touches: ["network"] }]), [
    "Toca correo",
    "Usa internet",
  ]);
});

test("un plan que no toca nada se lee como «Solo lee»", () => {
  assert.deepEqual(touchLabels([{ title: "a", touches: ["nothing"] }]), ["Solo lee"]);
  assert.deepEqual(touchLabels([]), ["Solo lee"]);
});

test("el resumen cuenta los pasos y nombra lo que toca con «e» antes de i", () => {
  const steps = [
    { title: "a", touches: ["files"] },
    { title: "b", touches: ["network"] },
    { title: "c", touches: ["files"] },
    { title: "d", touches: ["files", "network"] },
  ];
  assert.equal(summaryLine(steps), "4 pasos · toca ficheros e internet");
  assert.equal(summaryLine([{ title: "a", touches: ["nothing"] }]), "1 paso · solo lee");
  assert.equal(summaryLine([{ title: "a", touches: ["mail"] }, { title: "b", touches: ["tools"] }]), "2 pasos · toca correo y herramientas");
});

test("los planes propuestos van primero y el resto por lo último decidido, sin tocar la lista original", () => {
  const list = [
    plan({ id: "old", status: "approved", updated_at: "2026-11-01T09:00:00.000Z" }),
    plan({ id: "new", status: "proposed", updated_at: "2026-11-02T09:00:00.000Z" }),
    plan({ id: "recent", status: "rejected", updated_at: "2026-11-04T09:00:00.000Z" }),
    plan({ id: "other", status: "proposed", updated_at: "2026-11-03T09:00:00.000Z" }),
  ];
  const before = list.map((item) => item.id);
  assert.deepEqual(
    sortPlans(list).map((item) => item.id),
    ["new", "other", "recent", "old"],
  );
  assert.deepEqual(list.map((item) => item.id), before);
});

test("un plan que toca correo o internet pide confirmación extra; los demás no", () => {
  assert.equal(needsExtraConfirm(plan({ steps: [{ title: "a", touches: ["mail"] }] })), true);
  assert.equal(needsExtraConfirm(plan({ steps: [{ title: "a", touches: ["files", "network"] }] })), true);
  assert.equal(needsExtraConfirm(plan({ steps: [{ title: "a", touches: ["files", "tools", "nothing"] }] })), false);
  assert.match(confirmApproveText(), /correo o internet/);
});

test("la nota de cambios tiene texto y no pasa de 300 caracteres", () => {
  assert.match(noteError("   ") ?? "", /Escribe/);
  assert.equal(noteError("Usa otro nombre"), null);
  assert.equal(noteError("a".repeat(NOTE_MAX)), null);
  assert.match(noteError("a".repeat(NOTE_MAX + 1)) ?? "", /300/);
  // Un emoji es un carácter para la persona, aunque sean dos unidades UTF-16.
  assert.equal(noteError("😀".repeat(NOTE_MAX)), null);
  assert.equal(noteCount("hola"), "4/300");
});

test("un rechazo del host se dice con sus palabras y lo demás con friendlyError", () => {
  assert.equal(planFailureText({ kind: "planRefused", code: "plan_decided" }), "Este plan ya se decidió desde otro dispositivo.");
  assert.equal(planFailureText({ kind: "planRefused", code: "no_such_plan" }), "Ese plan ya no existe. Se quitará de la lista.");
  assert.equal(planFailureText({ kind: "planRefused", code: "plan_not_approved" }), "Este plan todavía no está aprobado.");
  assert.equal(planFailureText({ kind: "offline" }), OFFLINE_TEXT);
  assert.equal(planFailureText({ kind: "host", message: "x" }), REJECTED_TEXT);
  assert.equal(planFailureText(new Error("boom")), LOAD_FAILED);
  assert.match(PERMISSION_TEXT, /APPROVE/);
});

test("la vista de un plan dice quién espera, qué toca y, si pidió cambios, su nota", () => {
  const waiting = planView(plan({ steps: [{ title: "Leer", touches: ["files"] }] }));
  assert.equal(waiting.waiting, true);
  assert.equal(waiting.decided, false);
  assert.equal(waiting.statusLabel, STATUS_LABELS.proposed);
  assert.equal(waiting.summary, "1 paso · toca ficheros");
  assert.deepEqual(waiting.steps, [{ title: "Leer", labels: ["Toca ficheros"] }]);
  assert.equal(waiting.note, null);

  const asked = planView(plan({ status: "changes_requested", note: "Mejor sin correo", decided_at: "2026-11-05T11:00:00.000Z" }));
  assert.equal(asked.waiting, false);
  assert.equal(asked.decided, true);
  assert.equal(asked.statusLabel, STATUS_LABELS.changes_requested);
  assert.equal(asked.note, "Mejor sin correo");
});

test("la firma de un plan cambia cuando cambia lo que se pinta y no cuando llega la misma respuesta", () => {
  const first = planSignature(planView(plan()));
  assert.equal(planSignature(planView(plan())), first);
  assert.notEqual(planSignature(planView(plan({ status: "approved" }))), first);
  assert.notEqual(planSignature(planView(plan({ steps: [{ title: "Otro", touches: ["files"] }] }))), first);
});
