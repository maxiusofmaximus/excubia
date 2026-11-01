// «Calendario»: the rules that need no DOM, so tooling/calendar-model.test.mjs can run
// them. The host keeps instants in UTC; the screen shows and edits them in the company's
// zone. Local limits mirror the host's; the host still has the last word.

import { agendaErrorText, permissionText } from "./agenda-model.js";

export const EVENT_LIMITS = { title: 120, location: 200, notesBytes: 4 * 1024 };
const CALENDAR_LIMIT = 5000;

export const COMMON_ZONES = [
  "Europe/Madrid",
  "Europe/London",
  "Europe/Paris",
  "Europe/Berlin",
  "America/New_York",
  "America/Los_Angeles",
  "America/Mexico_City",
  "America/Bogota",
  "America/Argentina/Buenos_Aires",
  "America/Santiago",
  "UTC",
];

const REMINDERS = [
  { minutes: null, label: "Ninguno" },
  { minutes: 5, label: "5 min antes" },
  { minutes: 15, label: "15 min antes" },
  { minutes: 30, label: "30 min antes" },
  { minutes: 60, label: "60 min antes" },
  { minutes: 1440, label: "1 día antes" },
];

// The reminder the form offers, plus the one the event already has when the list lacks it.
export function reminderChoices(current) {
  if (current === null || REMINDERS.some((choice) => choice.minutes === current)) return REMINDERS;
  const extra = { minutes: current, label: `${current} min antes` };
  return [...REMINDERS, extra].sort((a, b) => (a.minutes ?? -1) - (b.minutes ?? -1));
}

export function reminderLabel(minutes) {
  const choice = REMINDERS.find((entry) => entry.minutes === (minutes ?? null));
  return choice ? choice.label : `${minutes} min antes`;
}

// A select holds text; «» is no reminder.
export function reminderFromSelect(text) {
  return text === "" ? null : Number(text);
}

// The four ranges of the list. `days` is null for the calendar month.
export const RANGES = [
  { id: "day", label: "Hoy", days: 1 },
  { id: "week", label: "7 días", days: 7 },
  { id: "thirty", label: "30 días", days: 30 },
  { id: "month", label: "Mes", days: null },
];

const rangeOf = (id) => RANGES.find((range) => range.id === id);

// Zone reads go through one formatter per zone, built on first use.
const formatters = new Map();

function formatterFor(zone) {
  if (!formatters.has(zone)) {
    formatters.set(
      zone,
      new Intl.DateTimeFormat("en-CA", {
        timeZone: zone,
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
        hour: "2-digit",
        minute: "2-digit",
        hourCycle: "h23",
      }),
    );
  }
  return formatters.get(zone);
}

function zoneClock(instant, zone) {
  const parts = {};
  for (const { type, value } of formatterFor(zone).formatToParts(new Date(instant))) parts[type] = value;
  return { date: `${parts.year}-${parts.month}-${parts.day}`, clock: `${parts.hour}:${parts.minute}` };
}

export const localDate = (instant, zone) => zoneClock(instant, zone).date;
export const localClock = (instant, zone) => zoneClock(instant, zone).clock;
export const localDateTime = (instant, zone) => {
  const { date, clock } = zoneClock(instant, zone);
  return `${date}T${clock}`;
};
export const todayIn = (zone, now = Date.now()) => localDate(now, zone);

// Calendar days are «YYYY-MM-DD» strings. The arithmetic runs in UTC, so no zone moves them.
const dayDate = (day) => {
  const [year, month, date] = day.split("-").map(Number);
  return new Date(Date.UTC(year, month - 1, date));
};
const dayString = (date) => date.toISOString().slice(0, 10);

export function addDays(day, count) {
  const date = dayDate(day);
  date.setUTCDate(date.getUTCDate() + count);
  return dayString(date);
}

function firstOfMonth(day) {
  return `${day.slice(0, 8)}01`;
}

export function addMonths(day, count) {
  const date = dayDate(firstOfMonth(day));
  date.setUTCMonth(date.getUTCMonth() + count);
  return dayString(date);
}

// The days a range covers, as [from, to): `to` is not one of them.
export function windowOf(id, anchor) {
  if (id === "month") {
    const from = firstOfMonth(anchor);
    return { from, to: addMonths(from, 1) };
  }
  return { from: anchor, to: addDays(anchor, rangeOf(id).days) };
}

export function shiftAnchor(id, anchor, step) {
  if (id === "month") return addMonths(anchor, step);
  return addDays(anchor, step * rangeOf(id).days);
}

const shortDate = (day) => dayDate(day).toLocaleDateString("es-ES", { day: "numeric", month: "short", timeZone: "UTC" });
const longDate = (day) =>
  dayDate(day).toLocaleDateString("es-ES", { weekday: "long", day: "numeric", month: "long", timeZone: "UTC" });

export function windowLabel({ from, to }) {
  const last = addDays(to, -1);
  return last === from ? shortDate(from) : `${shortDate(from)} – ${shortDate(last)}`;
}

// An all-day event ends at the midnight after its last day, so the last day is one earlier.
function lastDay(event, zone) {
  return event.end_utc ? addDays(localDate(event.end_utc, zone), -1) : localDate(event.start_utc, zone);
}

// The instant a list row stands for: its occurrence in a series, else the event's start.
export const occurrenceOf = (event) => event.occurrence_start ?? event.start_utc;

// The events grouped by their day in the company's zone, earliest day first.
export function groupByDay(events, zone) {
  const groups = new Map();
  for (const event of events) {
    const day = localDate(occurrenceOf(event), zone);
    if (!groups.has(day)) groups.set(day, { day, label: longDate(day), events: [] });
    groups.get(day).events.push(event);
  }
  return [...groups.values()].sort((a, b) => a.day.localeCompare(b.day));
}

export function eventRow(event, zone) {
  return {
    id: event.id,
    time: event.all_day ? "Todo el día" : localClock(occurrenceOf(event), zone),
    title: event.title,
    place: event.location || "",
    reminder: typeof event.reminder_minutes === "number",
    repeats: Boolean(event.recurring),
  };
}

// The page after `page`: the host sends its cursor only while the page is truncated.
export const nextCursor = (page) => (page.truncated ? page.next : null);

export const emptyListText = (hasMore) =>
  hasMore ? "Hay demasiados eventos en este periodo, acota las fechas." : "No hay eventos en este periodo.";

// A series goes whole: its delete says so, since no single repetition can be removed.
export const deleteQuestion = (event) =>
  event.recurrence ? "Se borran todas las repeticiones. No se puede deshacer." : "Se borra para siempre. No se puede deshacer.";

// «29 oct, 09:30 – 10:00», «29 oct, 22:00 – 30 oct, 01:30», «29 oct – 31 oct» for whole days.
export function whenText(event, zone) {
  const first = localDate(event.start_utc, zone);
  if (event.all_day) {
    const last = lastDay(event, zone);
    return last > first ? `${shortDate(first)} – ${shortDate(last)}` : `${shortDate(first)}, todo el día`;
  }
  const start = `${shortDate(first)}, ${localClock(event.start_utc, zone)}`;
  if (!event.end_utc) return start;
  const endDay = localDate(event.end_utc, zone);
  const end = endDay === first ? localClock(event.end_utc, zone) : `${shortDate(endDay)}, ${localClock(event.end_utc, zone)}`;
  return `${start} – ${end}`;
}

// The form's dates for an event: a day for a whole-day one, a local time otherwise.
export function formValues(event, zone) {
  if (event.all_day) {
    const first = localDate(event.start_utc, zone);
    const last = lastDay(event, zone);
    return { allDay: true, start: first, end: last > first ? last : "" };
  }
  return {
    allDay: false,
    start: localDateTime(event.start_utc, zone),
    end: event.end_utc ? localDateTime(event.end_utc, zone) : "",
  };
}

function problem(field, message) {
  return { field, message };
}

const DAY_PATTERN = /^\d{4}-\d{2}-\d{2}$/;
const TIME_PATTERN = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}$/;

// The first problem of an event draft, or null. Times compare as text: both shapes sort right.
export function eventProblem(draft) {
  const title = draft.title.trim();
  if (!title) return problem("title", "Escribe un título.");
  if ([...title].length > EVENT_LIMITS.title) {
    return problem("title", `El título admite ${EVENT_LIMITS.title} caracteres como máximo.`);
  }
  const pattern = draft.all_day ? DAY_PATTERN : TIME_PATTERN;
  if (!pattern.test(draft.start)) {
    return problem("start", draft.all_day ? "Elige el día." : "Elige la fecha y la hora.");
  }
  if (draft.end !== null) {
    if (!pattern.test(draft.end)) {
      return problem("end", draft.all_day ? "Elige el último día." : "Elige la fecha y la hora del fin.");
    }
    const late = draft.all_day ? draft.end < draft.start : draft.end <= draft.start;
    if (late) {
      return problem("end", draft.all_day ? "El último día no puede ser anterior al primero." : "El fin debe ser posterior al inicio.");
    }
  }
  if ([...draft.location].length > EVENT_LIMITS.location) {
    return problem("location", `El lugar admite ${EVENT_LIMITS.location} caracteres como máximo.`);
  }
  if (new TextEncoder().encode(draft.notes).length > EVENT_LIMITS.notesBytes) {
    return problem("notes", "Las notas admiten 4 KiB como máximo.");
  }
  return null;
}

export function zoneProblem(name) {
  return name.trim() ? null : "Escribe una zona, como Europe/Madrid.";
}

const CALENDAR_REFUSALS = {
  timezone_unset: "Falta la zona horaria de la empresa. Fíjala antes de crear eventos.",
  timezone_already_set: "La empresa ya tiene zona horaria y no se puede cambiar.",
  invalid_zone: "Esa zona no existe. Usa un nombre como Europe/Madrid.",
  invalid_local_time: "Esa hora no existe por el cambio de horario.",
  unsupported_recurrence: "Esa repetición todavía no está soportada.",
  invalid_recurrence: "La repetición no es válida.",
  calendar_limit: `Ya tienes ${CALENDAR_LIMIT} eventos. Borra alguno antes de crear otro.`,
};

const FIELD_REFUSALS = {
  start: "La fecha de inicio no es válida.",
  end: "El fin no es válido: debe ser posterior al inicio.",
  to: "El periodo no es válido.",
  timezone: "Esa zona no existe. Usa un nombre como Europe/Madrid.",
  recurrence: "La repetición no es válida.",
};

// The operator's words for a failure of the calendar; the agenda's own for the rest.
export function calendarErrorText(failure) {
  if (failure && failure.kind === "agendaRefused" && CALENDAR_REFUSALS[failure.code]) {
    return CALENDAR_REFUSALS[failure.code];
  }
  if (failure && failure.kind === "invalid" && FIELD_REFUSALS[failure.field]) {
    return FIELD_REFUSALS[failure.field];
  }
  return agendaErrorText(failure);
}

// A refused read: the device lacks READ_STATE, or the host could not be reached.
export function listFailureText(failure) {
  if (failure && failure.kind === "forbidden") return permissionText("READ_STATE");
  return calendarErrorText(failure);
}
