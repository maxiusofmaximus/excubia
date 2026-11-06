// Run with: node --test tooling/team-summary-model.test.mjs
// «Tu equipo» en la pestaña Equipo: etiqueta y tono por estado, edad, contador de
// pendientes, orden tal cual lo manda el host y errores con las mismas palabras que el resto.

import { test } from "node:test";
import assert from "node:assert/strict";
import {
  cardSignature,
  memberView,
  STATUS_VIEWS,
  summaryFailure,
  showsHeadline,
  summaryView,
  waitingText,
} from "../ui/team-summary-model.js";
import { OFFLINE_TEXT, REJECTED_TEXT } from "../ui/friendly-error.js";

const NOW = Date.UTC(2026, 10, 5, 12, 0, 0);
const MIN = 60_000;

function member(overrides = {}) {
  return {
    agent_id: "hire_1",
    name: "Bob",
    role_title: "Programador",
    status: "idle",
    headline: "Sin actividad",
    since: new Date(NOW - 12 * MIN).toISOString(),
    ...overrides,
  };
}

test("cada estado tiene su etiqueta en lenguaje llano y su tono", () => {
  assert.deepEqual(STATUS_VIEWS, {
    waiting_for_you: { label: "Espera tu visto bueno", tone: "waiting" },
    blocked: { label: "Necesita ayuda", tone: "blocked" },
    working: { label: "Trabajando", tone: "working" },
    done: { label: "Terminó", tone: "done" },
    idle: { label: "Sin actividad", tone: "idle" },
  });
});

test("un estado que el host no conoce se muestra como sin actividad, sin inventar nada", () => {
  const view = memberView(member({ status: "sleeping" }), NOW);
  assert.equal(view.label, "Sin actividad");
  assert.equal(view.tone, "idle");
  assert.equal(view.waiting, false);
});

test("la edad sale de since: «desde hace 12 min», «desde hace un momento», «desde hace 3 h», «desde hace 2 días»", () => {
  assert.equal(memberView(member(), NOW).age, "desde hace 12 min");
  assert.equal(memberView(member({ since: new Date(NOW - 20_000).toISOString() }), NOW).age, "desde hace un momento");
  assert.equal(memberView(member({ since: new Date(NOW - 3 * 60 * MIN).toISOString() }), NOW).age, "desde hace 3 h");
  assert.equal(memberView(member({ since: new Date(NOW - 2 * 24 * 60 * MIN).toISOString() }), NOW).age, "desde hace 2 días");
});

test("una fecha que no se lee deja la edad vacía en vez de romper la tarjeta", () => {
  assert.equal(memberView(member({ since: "no es una fecha" }), NOW).age, "");
});

test("el texto del host se muestra tal cual, sin deducir el estado de él", () => {
  const view = memberView(member({ status: "idle", headline: "Espera tu visto bueno de la factura" }), NOW);
  assert.equal(view.headline, "Espera tu visto bueno de la factura");
  assert.equal(view.label, "Sin actividad");
  assert.equal(view.waiting, false);
});

test("solo quien espera tu visto bueno cuenta como pendiente", () => {
  assert.equal(memberView(member({ status: "waiting_for_you" }), NOW).waiting, true);
  assert.equal(memberView(member({ status: "blocked" }), NOW).waiting, false);
});

test("la frase se pinta salvo que esté vacía o repita la etiqueta de estado", () => {
  assert.equal(showsHeadline(memberView(member(), NOW)), false);
  assert.equal(showsHeadline(memberView(member({ headline: "  sin ACTIVIDAD. " }), NOW)), false);
  assert.equal(showsHeadline(memberView(member({ status: "waiting_for_you", headline: "Espera tu visto bueno." }), NOW)), false);
  assert.equal(showsHeadline(memberView(member({ headline: "   " }), NOW)), false);
  assert.equal(showsHeadline(memberView(member({ status: "working", headline: "Sin actividad" }), NOW)), true);
  assert.equal(showsHeadline(memberView(member({ headline: "Revisando el presupuesto" }), NOW)), true);
});

test("las tarjetas siguen el orden del host y el contador lo cuenta bien", () => {
  const members = [
    member({ agent_id: "hire_2", status: "waiting_for_you" }),
    member({ agent_id: "hire_1", status: "waiting_for_you" }),
    member({ agent_id: "hire_3", status: "working" }),
  ];
  const views = summaryView(members, NOW);
  assert.deepEqual(views.map((view) => view.id), ["hire_2", "hire_1", "hire_3"]);
  assert.equal(views.filter((view) => view.waiting).length, 2);
});

test("el contador habla en singular o plural y con cero", () => {
  assert.equal(waitingText(0), "Nadie espera tu visto bueno.");
  assert.equal(waitingText(1), "1 espera tu visto bueno");
  assert.equal(waitingText(2), "2 esperan tu visto bueno");
});

test("la firma de una tarjeta cambia solo cuando cambia algo que se pinta", () => {
  const first = cardSignature(memberView(member(), NOW));
  assert.equal(cardSignature(memberView(member(), NOW)), first);
  assert.notEqual(cardSignature(memberView(member({ headline: "Otra frase" }), NOW)), first);
  assert.notEqual(cardSignature(memberView(member({ status: "working" }), NOW)), first);
});

test("un fallo de lectura usa las palabras del resto de la app: sin permiso espera, sin conexión lo dice", () => {
  assert.deepEqual(summaryFailure({ kind: "forbidden" }), {
    text: "Esperando que aprueben este dispositivo.",
    retry: false,
  });
  assert.deepEqual(summaryFailure({ kind: "offline" }), { text: OFFLINE_TEXT, retry: true });
  assert.deepEqual(summaryFailure({ kind: "rejected" }), { text: REJECTED_TEXT, retry: true });
  assert.equal(summaryFailure(new Error("boom")).retry, true);
});
