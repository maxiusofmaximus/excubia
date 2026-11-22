// «Calendario» in .ics: the words of an import's result and of each refusal of an import or
// an export. The file never reaches the screen: the shell opens it, reads it and writes it.

import { calendarErrorText } from "./calendar-model.js";

const REASONS = {
  exists: "Ya existía",
  unsupported_recurrence: "Repetición no soportada",
  exception_instance: "Ocurrencia suelta de una serie",
  no_start: "Sin fecha de inicio",
  invalid_time: "Hora no válida",
  invalid_text: "Texto no válido",
  invalid_recurrence: "Repetición no válida",
  calendar_limit: "Límite de eventos alcanzado",
};

export const reasonText = (reason) => REASONS[reason] ?? "Omitido por el PC";

const plural = (count, one, many) => `${count} ${count === 1 ? one : many}`;

export function importSummary(outcome) {
  return [
    plural(outcome.imported, "importado", "importados"),
    plural(outcome.skipped_existing, "ya existía", "ya existían"),
    plural(outcome.skipped_unsupported, "omitido", "omitidos"),
  ].join(" · ");
}

// The host counts each event from 0; the screen names them from 1, as the file's first event.
export const skippedLines = (outcome) =>
  outcome.details.map((skip) => `Evento ${skip.index + 1}: ${reasonText(skip.reason)}`);

// The host names up to 50 skipped events; the ones it only counted are said so.
export function skippedNote(outcome) {
  const rest = outcome.skipped_existing + outcome.skipped_unsupported - outcome.details.length;
  return rest > 0 ? `Hay ${rest} más sin detalle.` : null;
}

export const savedText = (name) => `Guardado «${name}».`;

const FILE_REFUSALS = {
  too_large: "El archivo es demasiado grande (máximo 512 KiB).",
  read_failed: "No se pudo leer el archivo.",
  write_failed: "No se pudo guardar el archivo.",
};

const SHARED_REFUSALS = {
  invalid_ics: "El archivo no es un calendario .ics válido.",
  timezone_unset: "Fija antes la zona horaria de la empresa para importar o exportar.",
};

const TOO_MANY_EVENTS = {
  import: "Tiene más de 500 eventos: divide el archivo.",
  export: "Hay demasiados eventos: elige un rango más corto.",
};

// The operator's words for a failure of an import (`"import"`) or of an export (`"export"`).
export function icsErrorText(failure, operation) {
  if (failure.kind === "fileRefused" && FILE_REFUSALS[failure.code]) return FILE_REFUSALS[failure.code];
  if (failure.kind === "host" && failure.status === 413) return FILE_REFUSALS.too_large;
  if (failure.kind === "agendaRefused" && failure.code === "too_many_events") return TOO_MANY_EVENTS[operation];
  if (failure.kind === "agendaRefused" && SHARED_REFUSALS[failure.code]) return SHARED_REFUSALS[failure.code];
  return calendarErrorText(failure);
}
