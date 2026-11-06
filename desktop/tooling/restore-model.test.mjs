// Run with: node --test tooling/restore-model.test.mjs
// «Copias del turno»: orden, edad, etiquetas, exclusiones con frase llana y errores del host.

import { test } from "node:test";
import assert from "node:assert/strict";
import {
  CONFIRM_WARNING,
  changeLabel,
  confirmText,
  doneText,
  excludedSentence,
  filesText,
  pointAge,
  pointLabel,
  pointTime,
  restoreErrorText,
  sortPoints,
} from "../ui/restore-model.js";
import { OFFLINE_TEXT, REJECTED_TEXT } from "../ui/friendly-error.js";

const NOW = Date.UTC(2026, 10, 5, 12, 0, 0);
const MIN = 60_000;

test("los puntos salen del más nuevo al más antiguo sin tocar la lista original", () => {
  const points = [{ id: "a", created_ms: 1 }, { id: "b", created_ms: 3 }, { id: "c", created_ms: 2 }];
  assert.deepEqual(sortPoints(points).map((point) => point.id), ["b", "c", "a"]);
  assert.deepEqual(points.map((point) => point.id), ["a", "b", "c"]);
});

test("la edad se dice en lenguaje llano", () => {
  assert.equal(pointAge(NOW - 10_000, NOW), "hace un momento");
  assert.equal(pointAge(NOW - 5 * MIN, NOW), "hace 5 min");
  assert.equal(pointAge(NOW - 1 * MIN, NOW), "hace 1 min");
  assert.equal(pointAge(NOW - 3 * 60 * MIN, NOW), "hace 3 h");
  assert.equal(pointAge(NOW - 2 * 24 * 60 * MIN, NOW), "hace 2 días");
  assert.equal(pointAge(NOW - 1 * 24 * 60 * MIN, NOW), "hace 1 día");
});

test("la hora es HH:MM con dos cifras", () => {
  const at = new Date(2026, 10, 5, 9, 4).getTime();
  assert.equal(pointTime(at), "09:04");
});

test("cada copia se nombra por su tipo", () => {
  assert.equal(pointLabel({ kind: "turn" }), "Copia del turno");
  assert.equal(pointLabel({ kind: "before_undo" }), "Copia antes de deshacer");
});

test("el número de ficheros concuerda en singular y plural", () => {
  assert.equal(filesText(0), "0 ficheros");
  assert.equal(filesText(1), "1 fichero");
  assert.equal(filesText(3), "3 ficheros");
});

test("los estados de un cambio se traducen a nuevo, cambiado y borrado", () => {
  assert.equal(changeLabel("added"), "nuevo");
  assert.equal(changeLabel("modified"), "cambiado");
  assert.equal(changeLabel("deleted"), "borrado");
  assert.throws(() => changeLabel("moved"));
});

test("lo excluido se cuenta y se agrupa por motivo en una frase", () => {
  assert.equal(excludedSentence([]), null);
  assert.equal(
    excludedSentence([
      { path: "a", reason: "too_large" },
      { path: "b", reason: "name" },
      { path: "c", reason: "symlink" },
    ]),
    "No se guardaron 3 elementos: archivos o carpetas muy grandes, claves o carpetas técnicas y enlaces.",
  );
  assert.equal(
    excludedSentence([{ path: "x", reason: "symlink" }]),
    "No se guardó 1 elemento: enlaces.",
  );
});

test("la confirmación dice cuántos ficheros cambian y que antes se guarda una copia", () => {
  assert.equal(
    confirmText(5),
    "Se van a restaurar 5 ficheros. Antes se guarda una copia de la carpeta actual para poder deshacer.",
  );
  assert.equal(confirmText(1), "Se va a restaurar 1 fichero. Antes se guarda una copia de la carpeta actual para poder deshacer.");
  assert.equal(
    CONFIRM_WARNING,
    "Se compara la carpeta antes y después; los cambios hechos por comandos no se siguen uno a uno.",
  );
});

test("el resultado dice cuántos ficheros volvieron y cuántos se quitaron", () => {
  assert.equal(doneText({ restored: 3, removed: 1 }), "Se han restaurado 3 ficheros. Se han quitado 1 fichero.");
  assert.equal(doneText({ restored: 1, removed: 0 }), "Se han restaurado 1 fichero.");
});

test("los errores del host de restauración tienen su frase; el resto pasa por friendlyError", () => {
  assert.equal(
    restoreErrorText({ kind: "restoreRefused", code: "restore_point_not_found" }),
    "Esa copia ya no existe. Vuelve a la lista.",
  );
  assert.equal(
    restoreErrorText({ kind: "restoreRefused", code: "restore_invalid" }),
    "La copia tiene rutas que salen de la carpeta, así que no se ha restaurado nada.",
  );
  assert.equal(
    restoreErrorText({ kind: "restoreRefused", code: "restore_failed" }),
    "No se pudo volver a esa copia. Revisa la carpeta y reinténtalo.",
  );
  assert.equal(restoreErrorText({ kind: "offline" }), OFFLINE_TEXT);
  assert.equal(restoreErrorText({ kind: "restoreRefused", code: "otro" }), REJECTED_TEXT);
  assert.equal(restoreErrorText(new Error("boom")), "No se pudo cargar. Inténtalo de nuevo.");
});
