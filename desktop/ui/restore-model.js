// «Copias del turno»: the rules of the restore screen, free of DOM so
// tooling/restore-model.test.mjs can run them. The host keeps a copy of the folder
// before each turn; the words here are the operator's, never the host's.

import { elapsedWords } from "./format.js";
import { friendlyError } from "./friendly-error.js";
import { joinLabels } from "./hires-model.js";

export const CONFIRM_WARNING =
  "Se compara la carpeta antes y después; los cambios hechos por comandos no se siguen uno a uno.";

const CHANGE_LABELS = new Map([
  ["added", "nuevo"],
  ["modified", "cambiado"],
  ["deleted", "borrado"],
]);

const EXCLUDED_LABELS = new Map([
  ["too_large", "archivos o carpetas muy grandes"],
  ["name", "claves o carpetas técnicas"],
  ["symlink", "enlaces"],
  ["special", "elementos especiales"],
  ["unsupported_name", "nombres que no se pueden guardar"],
]);

// A refusal the host words by its code. Anything else goes through friendlyError.
const RESTORE_CODES = new Map([
  ["restore_point_not_found", "Esa copia ya no existe. Vuelve a la lista."],
  ["restore_invalid", "La copia tiene rutas que salen de la carpeta, así que no se ha restaurado nada."],
  ["restore_failed", "No se pudo volver a esa copia. Revisa la carpeta y reinténtalo."],
]);

// Newest first. The list the host sent is left as it came.
export function sortPoints(points) {
  return [...points].sort((a, b) => b.created_ms - a.created_ms);
}

// «hace 5 min», «hace 3 h», «hace 2 días»: how long ago the copy was taken.
export function pointAge(createdMs, now) {
  return `hace ${elapsedWords(createdMs, now)}`;
}

// The local clock of the copy, `HH:MM`.
export function pointTime(createdMs) {
  const date = new Date(createdMs);
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}

export function pointLabel(point) {
  return point.kind === "before_undo" ? "Copia antes de deshacer" : "Copia del turno";
}

export function filesText(count) {
  return count === 1 ? "1 fichero" : `${count} ficheros`;
}

export function changeLabel(status) {
  if (!CHANGE_LABELS.has(status)) throw new Error(`cambio desconocido: ${status}`);
  return CHANGE_LABELS.get(status);
}

// «No se guardaron 3 elementos: a, b y c.» Reasons keep the order the host sent them.
export function excludedSentence(excluded) {
  if (excluded.length === 0) return null;
  const reasons = [...new Set(excluded.map((item) => EXCLUDED_LABELS.get(item.reason) || item.reason))];
  const count = excluded.length;
  const head = count === 1 ? "No se guardó 1 elemento" : `No se guardaron ${count} elementos`;
  return `${head}: ${joinLabels(reasons)}.`;
}

// What the confirmation names before anything is put back.
export function confirmText(count) {
  const what = count === 1 ? "Se va a restaurar 1 fichero." : `Se van a restaurar ${count} ficheros.`;
  return `${what} Antes se guarda una copia de la carpeta actual para poder deshacer.`;
}

// What a finished restore put back and what it took away.
export function doneText({ restored, removed }) {
  const back = `Se han restaurado ${filesText(restored)}.`;
  return removed === 0 ? back : `${back} Se han quitado ${filesText(removed)}.`;
}

export function restoreErrorText(error) {
  if (error && error.kind === "restoreRefused" && RESTORE_CODES.has(error.code)) {
    return RESTORE_CODES.get(error.code);
  }
  return friendlyError(error);
}
