// Run with: node --test tooling/chat-suggestions.test.mjs
// The three suggestions of the empty chat: a general one, then one per profession
// contracted here, rotating so the same phrase does not come back every time.

import { test } from "node:test";
import assert from "node:assert/strict";

import { NEUTRAL_SUGGESTIONS, chatSuggestions } from "../ui/chat-suggestions.js";

const AREAS = [
  "programacion",
  "video",
  "diseno_grafico",
  "audio",
  "documentos",
  "investigacion",
  "ingenieria_cad",
  "marketing",
  "otras",
];
const DAY = 0;
const BANNED =
  /\b(lead|host|e2e|suite|demo|gateway|token|endpoint|herdr|tailscale|tls|pake|daemon|worktree|alfred|ernesto|ines)\b/i;

const hire = (area, status = "hired") => ({ name: "Persona", area, role: `rol-${area}`, status });

function texts(list) {
  return list.map((item) => `${item.title} ${item.detail} ${item.prompt}`);
}

test("sin equipo: tres sugerencias neutras de arranque", () => {
  const list = chatSuggestions([], DAY);
  assert.deepEqual(
    list.map((item) => item.title),
    ["Cuéntame a qué te dedicas y armo tu equipo", "¿Qué puedes hacer por mí?", "Apunta una nota para mañana"],
  );
  assert.equal(list.length, NEUTRAL_SUGGESTIONS.length);
});

test("personas contratadas que ya no están (despedidas) no cuentan como equipo", () => {
  const list = chatSuggestions([hire("video", "dismissed")], DAY);
  assert.equal(list[0].title, NEUTRAL_SUGGESTIONS[0].title);
});

test("la primera sugerencia es siempre la general, con detalle", () => {
  for (const hires of [[hire("documentos")], [hire("video"), hire("programacion")], [hire("area-inventada")]]) {
    for (let turn = 0; turn < 5; turn += 1) {
      const first = chatSuggestions(hires, turn)[0];
      assert.equal(first.title, "¿Cómo va el equipo?");
      assert.equal(first.detail, "qué hace cada persona y qué está pendiente");
    }
  }
});

test("un área: dos frases de ese oficio, distintas entre sí", () => {
  const list = chatSuggestions([hire("documentos")], DAY);
  assert.equal(list.length, 3);
  const [, a, b] = list;
  assert.notEqual(a.title, b.title);
  assert.ok(a.title.length > 0 && b.title.length > 0);
  assert.ok(!NEUTRAL_SUGGESTIONS.some((item) => item.title === a.title));
});

test("varias áreas: cada frase sale de un área distinta", () => {
  const hires = [hire("video"), hire("documentos")];
  for (let turn = 0; turn < 12; turn += 1) {
    const [, a, b] = chatSuggestions(hires, turn);
    assert.notEqual(a.title, b.title, `turno ${turn}`);
  }
});

test("la rotación cambia las frases de un turno a otro", () => {
  const hires = [hire("video"), hire("documentos"), hire("programacion")];
  const first = chatSuggestions(hires, 0).map((item) => item.title).join("|");
  const second = chatSuggestions(hires, 1).map((item) => item.title).join("|");
  assert.notEqual(first, second);
});

test("cada una de las nueve áreas del catálogo tiene su propia frase", () => {
  const seen = new Set();
  for (const area of AREAS) {
    const [, phrase] = chatSuggestions([hire(area)], DAY);
    assert.ok(!NEUTRAL_SUGGESTIONS.some((item) => item.title === phrase.title), area);
    seen.add(phrase.title);
  }
  assert.equal(seen.size, AREAS.length);
});

test("un área desconocida se ignora y se rellena con neutras", () => {
  const list = chatSuggestions([hire("area-inventada")], DAY);
  assert.equal(list[0].title, "¿Cómo va el equipo?");
  assert.equal(list[1].title, NEUTRAL_SUGGESTIONS[1].title);
  assert.equal(list[2].title, NEUTRAL_SUGGESTIONS[2].title);
});

test("nunca más de tres, y ninguna marca fuera de las dos previstas", () => {
  const cases = [[], [hire("video")], [hire("video"), hire("audio"), hire("marketing"), hire("otras")]];
  for (const hires of cases) {
    for (let turn = 0; turn < 20; turn += 1) {
      const list = chatSuggestions(hires, turn);
      assert.equal(list.length, 3);
      for (const item of list) assert.ok(["?", "+"].includes(item.mark), item.mark);
    }
  }
});

test("ningún texto lleva jerga ni nombres de agentes", () => {
  const cases = [[], [hire("video")], AREAS.map((area) => hire(area))];
  for (const hires of cases) {
    for (let turn = 0; turn < 20; turn += 1) {
      for (const text of texts(chatSuggestions(hires, turn))) {
        assert.doesNotMatch(text, BANNED, text);
      }
    }
  }
});

test("cada sugerencia trae un texto para escribir en la caja del chat", () => {
  for (const item of chatSuggestions([hire("video")], DAY)) {
    assert.ok(item.prompt.length > 0);
  }
});
