// Run with: node --test tooling/hires-model.test.mjs
// «Tu equipo»: who is active, the grouping by area, the header counter, the
// wording of a read the host refuses, and the characters a hire can wear.
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  AREA_CAP,
  HIRE_CHARACTERS,
  activeHires,
  groupByArea,
  hiredIn,
  isHired,
  joinLabels,
  readStateText,
  teamCountLabel,
} from "../ui/hires-model.js";

const areas = [
  { id: "programacion", label: "Programación" },
  { id: "video", label: "Vídeo y postproducción" },
];

const hire = (id, area, status = "hired", extra = {}) => ({
  id: `hire_${id}`,
  role: id,
  area,
  name: id,
  avatar: null,
  status,
  ...extra,
});

test("solo cuentan los contratados activos; los despedidos no entran en el equipo", () => {
  const hires = [hire("bob", "programacion"), hire("vera", "video", "removed")];

  assert.deepEqual(
    activeHires(hires).map((item) => item.id),
    ["hire_bob"],
  );
  assert.equal(hiredIn(hires, "video"), 0);
  assert.equal(hiredIn(hires, "programacion"), 1);
});

test("un rol ya contratado y activo cuenta como contratado; uno despedido, no", () => {
  const hires = [hire("bob", "programacion"), hire("ines", "programacion", "removed")];

  assert.equal(isHired(hires, "bob"), true);
  assert.equal(isHired(hires, "ines"), false);
  assert.equal(isHired(hires, "conan"), false);
});

test("el equipo se agrupa en el orden del catálogo y sin áreas vacías", () => {
  const hires = [
    hire("vera", "video"),
    hire("bob", "programacion"),
    hire("ines", "programacion"),
  ];

  const groups = groupByArea(hires, areas);

  assert.deepEqual(
    groups.map((group) => [group.label, group.hires.length]),
    [
      ["Programación", 2],
      ["Vídeo y postproducción", 1],
    ],
  );
});

test("una contratación en un área que el catálogo no conoce conserva su grupo", () => {
  const groups = groupByArea([hire("ana", "legal")], areas);

  assert.equal(groups.length, 1);
  assert.equal(groups[0].id, "legal");
  assert.equal(groups[0].label, "legal");
});

test("el contador dice «N en el equipo · M activos» con singular y plural", () => {
  assert.equal(teamCountLabel(12, 0), "12 en el equipo · 0 activos");
  assert.equal(teamCountLabel(3, 1), "3 en el equipo · 1 activo");
  assert.equal(teamCountLabel(3, 2), "3 en el equipo · 2 activos");
});

test("una lectura que el host rechaza con 403 es esperar aprobación, no un fallo de conexión", () => {
  assert.equal(readStateText({ kind: "forbidden" }), "Esperando que aprueben este dispositivo.");
  assert.equal(readStateText({ kind: "offline" }), null);
  assert.equal(readStateText(null), null);
});

test("un personaje de contratado es uno de los 8 que el host acepta; Alfred no", () => {
  assert.equal(HIRE_CHARACTERS.length, 8);
  assert.ok(!HIRE_CHARACTERS.includes("alfred"));
  assert.ok(HIRE_CHARACTERS.includes("bob"));
});

test("el tope por área es 8", () => {
  assert.equal(AREA_CAP, 8);
});

test("la unión de etiquetas suena a español: «a», «a y b», «a, b y c»", () => {
  assert.equal(joinLabels(["Programación"]), "Programación");
  assert.equal(joinLabels(["Programación", "Vídeo"]), "Programación y Vídeo");
  assert.equal(joinLabels(["A", "B", "C"]), "A, B y C");
  assert.equal(joinLabels([]), "");
});
