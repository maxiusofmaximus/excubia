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
  leaderCount,
  showsHireButton,
  readStateText,
  showsHireSection,
  teamSubtitle,
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

test("el subtítulo cuenta lo que lista la vista: contratados y sesiones de agentes", () => {
  assert.equal(teamSubtitle(0, 1), "1 sesión");
  assert.equal(teamSubtitle(1, 0), "1 contratado");
  assert.equal(teamSubtitle(2, 0), "2 contratados");
  assert.equal(teamSubtitle(0, 3), "3 sesiones");
  assert.equal(teamSubtitle(2, 1), "2 contratados · 1 sesión");
  assert.equal(teamSubtitle(0, 0), "Sin nadie contratado todavía");
});

test("un proyecto con líder cuenta como equipo; sin líder no cuenta", () => {
  assert.equal(leaderCount([{ lead: { agent_id: "ag_lead" } }, { lead: null }, {}]), 1);
  assert.equal(leaderCount(undefined), 0);
  assert.equal(leaderCount([{ lead: { agent_id: "a" } }, { lead: { agent_id: "a" } }, { lead: { agent_id: "b" } }]), 2);
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

test("«Contratados» solo aparece con algo que mostrar: un contratado, una lectura fallida o el permiso pendiente", () => {
  const none = { hires: [], canHire: true, readError: null };
  assert.equal(showsHireSection(none), false, "sin nadie contratado y con permiso no hay sección");
  assert.equal(showsHireSection({ ...none, hires: [hire("bob", "programacion", "removed")] }), false, "un despedido no cuenta");
  assert.equal(showsHireSection({ ...none, hires: [hire("bob", "programacion")] }), true);
  assert.equal(showsHireSection({ ...none, readError: { text: "x", retry: true } }), true, "una lectura fallida se explica");
  assert.equal(showsHireSection({ ...none, canHire: false }), true, "sin permiso aparece la nota para concederlo");
});

test("«Contratar» está en la cabecera estable de Equipo mientras el dispositivo pueda contratar y la lectura no falle", () => {
  assert.equal(showsHireButton({ canHire: true, readError: null }), true, "sin contratados también hay botón");
  assert.equal(showsHireButton({ canHire: true, readError: null, hires: [hire("bob", "programacion")] }), true);
  assert.equal(showsHireButton({ canHire: true, readError: { text: "x", retry: true } }), false, "con lectura fallida no se contrata");
  assert.equal(showsHireButton({ canHire: false, readError: null }), false, "sin permiso no hay botón");
});
