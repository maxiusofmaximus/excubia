// Run with: node --test tooling/delegation-model.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  cardState,
  cardTask,
  detailKicker,
  finishedTasks,
  projectName,
  showInThread,
  statusMeta,
  taskRepeatsTitle,
} from "../ui/delegation-model.js";

test("the detail's kicker says Tarea at every depth; the child button alone says Subtarea", () => {
  assert.equal(detailKicker(), "Tarea");
});

test("a project shows its name, and an id the UI does not know shows nothing", () => {
  const projects = [{ id: "prj_01", name: "bench-proj" }, { id: "prj_02", name: "" }];
  assert.equal(projectName(projects, "prj_01"), "bench-proj");
  assert.equal(projectName(projects, "prj_02"), "");
  assert.equal(projectName(projects, "prj_99"), "");
  assert.equal(projectName(undefined, "prj_01"), "");
});

test("a task that only repeats the card's title is left out of the card", () => {
  assert.equal(taskRepeatsTitle("Borra el archivo", "Borra el archivo"), true);
  assert.equal(taskRepeatsTitle("Ernesto: borra el archivo", "Ernesto: borra el archivo"), true);
  assert.equal(taskRepeatsTitle("Borra el archivo README", "Borra el archivo"), true);
  assert.equal(taskRepeatsTitle("Revisa los tests", "Borra el archivo"), false);
  assert.equal(taskRepeatsTitle("", "Borra el archivo"), false);
});

test("el panel de Equipo lista las últimas cinco tareas terminadas, la más nueva primero", () => {
  const card = (id, status, extra = {}) => ({ kind: "card", entry: { ref_id: id, status, ...extra } });
  const entries = [
    { kind: "message", message: {} },
    card("a", "done"),
    card("b", "running"),
    card("c", "failed"),
    card("d", "queued", { reason: "operator_denied" }),
    card("e", "cancelled"),
    card("f", "done"),
    card("g", "waiting"),
    card("h", "interrupted"),
  ];
  assert.deepEqual(
    finishedTasks(entries).map((entry) => entry.ref_id),
    ["h", "f", "e", "d", "c"],
  );
  assert.deepEqual(finishedTasks([card("x", "running")]), []);
  assert.deepEqual(finishedTasks(undefined), []);
});

test("una tarjeta terminada no se pinta en el hilo; las activas, fallidas o canceladas sí", () => {
  assert.equal(showInThread({ status: "done" }), false);
  // A denial is done too: Alfred's line already closes it.
  assert.equal(showInThread({ status: "done", reason: "operator_denied" }), false);
  for (const status of ["queued", "running", "waiting", "failed", "cancelled", "interrupted"]) {
    assert.equal(showInThread({ status }), true, status);
  }
  assert.equal(showInThread(null), false);
});

test("una denegación del operador no se lee como hecha", () => {
  assert.deepEqual(cardState({ status: "done", reason: "operator_denied" }), { label: "No se hizo", tone: "cancelled" });
  assert.deepEqual(cardState({ status: "done" }), { label: "Hecho", tone: "done" });
  // Without the host's reason field the plain status still shows.
  assert.deepEqual(cardState({ status: "failed" }), { label: "Falló", tone: "failed" });
  assert.equal(cardState({}).label, "En cola");
  assert.equal(statusMeta("raro").tone, "unknown");
});

test("la tarea de la línea es una frase corta, sin rutas ni markdown", () => {
  assert.equal(cardTask("Crear redi-cinco.txt con el texto uno"), "Crear redi-cinco.txt con el texto uno");
  assert.equal(cardTask("He creado el archivo `redi-cinco.txt` en C:\Users\Max\bench-proj"), "");
  assert.equal(cardTask("Borrar\nel resto"), "Borrar");
  assert.equal(cardTask("**Listo** el informe"), "Listo el informe");
  assert.equal(cardTask(""), "");
  // Never cut in script, and never cut by the line either: a task that does not fit waits in «Ver detalle».
  assert.equal(cardTask("a".repeat(70)), "a".repeat(70));
  assert.equal(cardTask("a".repeat(71)), "");
});
