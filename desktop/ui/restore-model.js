// «Copias del turno»: the rules of the restore screen, free of DOM so
// tooling/restore-model.test.mjs can run them. The host keeps a copy of the folder
// before each turn; the words here are the operator's, never the host's.

import { t } from "./i18n.js";
import { elapsedWords } from "./format.js";
import { friendlyError } from "./friendly-error.js";
import { joinLabels } from "./hires-model.js";

export const CONFIRM_WARNING = t("restore.confirm_warning");

const CHANGE_LABELS = new Map([
  ["added", "restore.change_added"],
  ["modified", "restore.change_modified"],
  ["deleted", "restore.change_deleted"],
]);

const EXCLUDED_LABELS = new Map([
  ["too_large", "restore.excluded_too_large"],
  ["name", "restore.excluded_name"],
  ["symlink", "restore.excluded_symlink"],
  ["special", "restore.excluded_special"],
  ["unsupported_name", "restore.excluded_unsupported_name"],
]);

// A refusal the host words by its code. Anything else goes through friendlyError.
const RESTORE_CODES = new Map([
  ["restore_point_not_found", "restore.code_not_found"],
  ["restore_invalid", "restore.code_invalid"],
  ["restore_failed", "restore.code_failed"],
]);

// Newest first. The list the host sent is left as it came.
export function sortPoints(points) {
  return [...points].sort((a, b) => b.created_ms - a.created_ms);
}

// «hace 5 min», «hace 3 h», «hace 2 días»: how long ago the copy was taken.
export function pointAge(createdMs, now) {
  return t("restore.age", { when: elapsedWords(createdMs, now) });
}

// The local clock of the copy, `HH:MM`.
export function pointTime(createdMs) {
  const date = new Date(createdMs);
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}

export function pointLabel(point) {
  return point.kind === "before_undo" ? t("restore.point_before_undo") : t("restore.point_turn");
}

export function filesText(count) {
  return t(count === 1 ? "restore.files.one" : "restore.files.other", { n: count });
}

export function changeLabel(status) {
  if (!CHANGE_LABELS.has(status)) throw new Error(`cambio desconocido: ${status}`);
  return t(CHANGE_LABELS.get(status));
}

// «No se guardaron 3 elementos: a, b y c.» Reasons keep the order the host sent them.
export function excludedSentence(excluded) {
  if (excluded.length === 0) return null;
  const reasons = [
    ...new Set(excluded.map((item) => (EXCLUDED_LABELS.has(item.reason) ? t(EXCLUDED_LABELS.get(item.reason)) : item.reason))),
  ];
  const count = excluded.length;
  const head = t(count === 1 ? "restore.excluded_head.one" : "restore.excluded_head.other", { n: count });
  return t("restore.excluded_sentence", { head, reasons: joinLabels(reasons) });
}

// What the confirmation names before anything is put back.
export function confirmText(count) {
  const what = t(count === 1 ? "restore.confirm_one" : "restore.confirm_other", { n: count });
  return `${what} ${t("restore.confirm_backup")}`;
}

// What a finished restore put back and what it took away.
export function doneText({ restored, removed }) {
  const back = t("restore.done_restored", { files: filesText(restored) });
  return removed === 0 ? back : `${back} ${t("restore.done_removed", { files: filesText(removed) })}`;
}

export function restoreErrorText(error) {
  if (error && error.kind === "restoreRefused" && RESTORE_CODES.has(error.code)) {
    return t(RESTORE_CODES.get(error.code));
  }
  return friendlyError(error);
}
