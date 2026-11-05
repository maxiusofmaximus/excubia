// Run with: node --test tooling/alerts-model.test.mjs
// «Avisos»: las frases del modo, de cada regla y del formulario, y el texto de cada refusal.
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  alertErrorText,
  draftFor,
  EMPTY_RULES_TEXT,
  GUARANTEE_TEXT,
  hiredNames,
  modeWords,
  ruleSentence,
} from "../ui/alerts-model.js";

test("cada modo tiene su nombre y una frase que dice que el silencio gana", () => {
  assert.equal(modeWords("all").label, "Avisar de todo");
  assert.equal(modeWords("rules_only").label, "Solo lo que yo elija");
  for (const mode of ["all", "rules_only"]) {
    assert.match(modeWords(mode).phrase, /silencio gana siempre/);
  }
});

test("una regla de agente se lee en palabras, con su agente o con cualquiera", () => {
  const bob = { trigger: "pane_status", agent: "Bob", status: "done", action: "notify" };
  const anyBlocked = { trigger: "pane_status", agent: null, status: "blocked", action: "mute" };
  assert.equal(ruleSentence(bob), "Avisar cuando Bob termine");
  assert.equal(ruleSentence(anyBlocked), "Silenciar cuando cualquier agente se bloquee");
});

test("un recordatorio del calendario no habla de agentes ni de estados", () => {
  const reminder = { trigger: "calendar_reminder", agent: null, status: null, action: "notify" };
  assert.equal(ruleSentence(reminder), "Avisar de recordatorios del calendario");
});

test("el formulario arma el borrador que el host espera para cada tipo", () => {
  assert.deepEqual(
    draftFor({ kind: "agent", agent: "Bob", when: "done", action: "notify" }),
    { trigger: "pane_status", agent: "Bob", status: "done", action: "notify" },
  );
  assert.deepEqual(
    draftFor({ kind: "agent", agent: null, when: "blocked", action: "mute" }),
    { trigger: "pane_status", agent: null, status: "blocked", action: "mute" },
  );
  assert.deepEqual(
    draftFor({ kind: "reminder", agent: null, when: "done", action: "mute" }),
    { trigger: "calendar_reminder", agent: null, status: null, action: "mute" },
  );
});

test("los nombres del equipo salen una vez y solo de los agentes contratados", () => {
  const hires = [
    { name: "Bob", status: "hired" },
    { name: "Ana", status: "removed" },
    { name: "Bob", status: "hired" },
    { name: "Eva", status: "hired" },
  ];
  assert.deepEqual(hiredNames(hires), ["Bob", "Eva"]);
});

test("una regla repetida o el límite se dicen con el texto del host", () => {
  const duplicate = { kind: "alertRefused", code: "rule_exists", message: "Ya existe una regla igual" };
  const limit = { kind: "alertRefused", code: "rule_limit", message: "Has llegado al máximo de 100 reglas" };
  assert.equal(alertErrorText(duplicate), "Ya existe una regla igual");
  assert.equal(alertErrorText(limit), "Has llegado al máximo de 100 reglas");
});

test("un campo inválido se nombra en palabras y lo desconocido tiene un texto genérico", () => {
  assert.equal(
    alertErrorText({ kind: "invalid", field: "status", message: "x" }),
    "Elige cuándo avisa: se bloquea o termina.",
  );
  assert.equal(alertErrorText({ kind: "invalid", field: "otro", message: "x" }), "Revisa los datos de la regla.");
  assert.equal(alertErrorText({ kind: "offline" }), "Sin conexión con tu PC. Lo cargado sigue visible.");
});

test("las frases fijas y las visibles no nombran el software por dentro", () => {
  assert.equal(
    GUARANTEE_TEXT,
    "Los avisos siguen respetando los frenos del servidor (esperas entre avisos, horas de silencio).",
  );
  assert.match(EMPTY_RULES_TEXT, /Todavía no hay reglas/);
  const visible = [GUARANTEE_TEXT, EMPTY_RULES_TEXT, modeWords("all").phrase, modeWords("rules_only").phrase];
  for (const text of visible) {
    assert.doesNotMatch(text, /herdr|tailscale/i);
  }
});
