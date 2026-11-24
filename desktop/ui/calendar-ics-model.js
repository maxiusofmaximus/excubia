// «Calendario» in .ics: the words of an import's result and of each refusal of an import or
// an export. The file never reaches the screen: the shell opens it, reads it and writes it.

import { calendarErrorText } from "./calendar-model.js";
import { t } from "./i18n.js";

const REASONS = {
  exists: t("calendar.ics_reason_exists"),
  unsupported_recurrence: t("calendar.ics_reason_unsupported_recurrence"),
  exception_instance: t("calendar.ics_reason_exception_instance"),
  no_start: t("calendar.ics_reason_no_start"),
  invalid_time: t("calendar.ics_reason_invalid_time"),
  invalid_text: t("calendar.ics_reason_invalid_text"),
  invalid_recurrence: t("calendar.ics_reason_invalid_recurrence"),
  calendar_limit: t("calendar.ics_reason_calendar_limit"),
};

export const reasonText = (reason) => REASONS[reason] ?? t("calendar.ics_reason_other");

const plural = (count, key) => t(`${key}.${count === 1 ? "one" : "other"}`, { n: count });

export function importSummary(outcome) {
  return [
    plural(outcome.imported, "calendar.ics_imported"),
    plural(outcome.skipped_existing, "calendar.ics_existing"),
    plural(outcome.skipped_unsupported, "calendar.ics_skipped"),
  ].join(" · ");
}

// The host counts each event from 0; the screen names them from 1, as the file's first event.
export const skippedLines = (outcome) =>
  outcome.details.map((skip) => t("calendar.ics_event_line", { n: skip.index + 1, reason: reasonText(skip.reason) }));

// The host names up to 50 skipped events; the ones it only counted are said so.
export function skippedNote(outcome) {
  const rest = outcome.skipped_existing + outcome.skipped_unsupported - outcome.details.length;
  return rest > 0 ? t("calendar.ics_more", { n: rest }) : null;
}

export const savedText = (name) => t("calendar.ics_saved", { name });

const FILE_REFUSALS = {
  too_large: t("calendar.ics_refuse_too_large"),
  read_failed: t("calendar.ics_refuse_read_failed"),
  write_failed: t("calendar.ics_refuse_write_failed"),
};

const SHARED_REFUSALS = {
  invalid_ics: t("calendar.ics_refuse_invalid"),
  timezone_unset: t("calendar.ics_refuse_timezone"),
};

const TOO_MANY_EVENTS = {
  import: t("calendar.ics_refuse_too_many_import"),
  export: t("calendar.ics_refuse_too_many_export"),
};

// The operator's words for a failure of an import (`"import"`) or of an export (`"export"`).
export function icsErrorText(failure, operation) {
  if (failure.kind === "fileRefused" && FILE_REFUSALS[failure.code]) return FILE_REFUSALS[failure.code];
  if (failure.kind === "host" && failure.status === 413) return FILE_REFUSALS.too_large;
  if (failure.kind === "agendaRefused" && failure.code === "too_many_events") return TOO_MANY_EVENTS[operation];
  if (failure.kind === "agendaRefused" && SHARED_REFUSALS[failure.code]) return SHARED_REFUSALS[failure.code];
  return calendarErrorText(failure);
}
