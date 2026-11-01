// Run with: node --test tooling/onboarding-model.test.mjs
// The first-run assistant: the level presets, the toggle under the cap of one
// area, the chosen areas in catalogue order, and the closing sentence.
import { test } from "node:test";
import assert from "node:assert/strict";

import { areaTotal, chosenAreas, confirmation, presetRoles, toggleRole } from "../ui/onboarding-model.js";

const programacion = {
  id: "programacion",
  roles: ["bob", "ines", "conan", "nicole"],
  levels: {
    personal: ["bob"],
    profesional: ["bob", "ines", "conan"],
    equipo: ["bob", "ines", "conan", "nicole"],
  },
};
const video = {
  id: "video",
  roles: ["vera", "jade"],
  levels: { personal: ["vera"], profesional: ["vera", "jade"], equipo: ["vera", "jade"] },
};
const catalog = [programacion, video];

const hired = (count, area) =>
  Array.from({ length: count }, (_, index) => ({
    id: `hire_${area}${index}`,
    area,
    status: "hired",
  }));

test("cada nivel preselecciona su parte del área: personal, profesional o equipo", () => {
  assert.deepEqual(presetRoles([programacion], "personal"), ["bob"]);
  assert.deepEqual(presetRoles([programacion], "profesional"), ["bob", "ines", "conan"]);
  assert.deepEqual(presetRoles([programacion], "equipo"), ["bob", "ines", "conan", "nicole"]);
});

test("un nivel que el catálogo no trae cae a profesional, como en el host", () => {
  const sinNivel = { ...video, levels: { profesional: ["vera"] } };

  assert.deepEqual(presetRoles([sinNivel], "equipo"), ["vera"]);
});

test("varias áreas suman sus preselecciones sin repetir un rol", () => {
  const roles = presetRoles([programacion, video], "personal");

  assert.deepEqual(roles, ["bob", "vera"]);
});

test("un rol que no pertenece al área se descarta aunque el nivel lo nombre", () => {
  const raro = { id: "video", roles: ["vera"], levels: { personal: ["vera", "bob"] } };

  assert.deepEqual(presetRoles([raro], "personal"), ["vera"]);
});

test("pulsar un rol lo quita o lo pone, sin aviso", () => {
  const off = toggleRole({ selected: ["bob"], role: "bob", area: programacion, hires: [] });
  const on = toggleRole({ selected: [], role: "ines", area: programacion, hires: [] });

  assert.deepEqual(off, { selected: [], notice: null });
  assert.deepEqual(on, { selected: ["ines"], notice: null });
});

test("el noveno de un área no se marca y avisa «Máximo 8 por área»", () => {
  const selected = ["a1", "a2", "a3", "a4", "a5", "a6", "a7", "a8"];
  const area = { id: "programacion", roles: [...selected, "nueve"] };

  const result = toggleRole({ selected, role: "nueve", area, hires: [] });

  assert.deepEqual(result.selected, selected);
  assert.equal(result.notice, "Máximo 8 por área.");
});

test("el contador del área suma contratados y elegidos, sin contar los de otras áreas", () => {
  const selected = ["bob", "ines", "vera"];

  assert.equal(areaTotal({ selected, area: programacion, hires: hired(3, "programacion") }), 5);
  assert.equal(areaTotal({ selected, area: video, hires: hired(3, "programacion") }), 1);
  assert.equal(areaTotal({ selected: [], area: programacion, hires: hired(8, "video") }), 0);
});

test("los contratados ya existentes cuentan para el tope del área", () => {
  const area = { id: "programacion", roles: ["bob", "ines"] };

  const result = toggleRole({ selected: [], role: "bob", area, hires: hired(8, "programacion") });

  assert.deepEqual(result.selected, []);
  assert.equal(result.notice, "Máximo 8 por área.");
});

test("las áreas elegidas salen en el orden del catálogo, no en el de los toques", () => {
  assert.deepEqual(
    chosenAreas(catalog, ["video", "programacion"]).map((area) => area.id),
    ["programacion", "video"],
  );
});

test("la confirmación dice el número en singular o plural y las áreas en español", () => {
  assert.equal(
    confirmation(1, ["Programación"]),
    "He preparado 1 agente para Programación. Puedes ajustarlos en cualquier momento.",
  );
  assert.equal(
    confirmation(12, ["Programación", "Vídeo y postproducción"]),
    "He preparado 12 agentes para Programación y Vídeo y postproducción. Puedes ajustarlos en cualquier momento.",
  );
});
