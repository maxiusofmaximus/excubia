// «Calendario»: the rules that need no DOM, so tooling/calendar-model.test.mjs can run
// them. The host keeps instants in UTC; the screen shows and edits them in the company's
// zone. Local limits mirror the host's; the host still has the last word.

import { locale, t } from "./i18n.js";
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
  { minutes: null, label: t("calendar.reminder_none") },
  { minutes: 5, label: t("calendar.reminder_min", { n: 5 }) },
  { minutes: 15, label: t("calendar.reminder_min", { n: 15 }) },
  { minutes: 30, label: t("calendar.reminder_min", { n: 30 }) },
  { minutes: 60, label: t("calendar.reminder_min", { n: 60 }) },
  { minutes: 1440, label: t("calendar.reminder_day") },
];

// The reminder the form offers, plus the one the event already has when the list lacks it.
export function reminderChoices(current) {
  if (current === null || REMINDERS.some((choice) => choice.minutes === current)) return REMINDERS;
  const extra = { minutes: current, label: t("calendar.reminder_min", { n: current }) };
  return [...REMINDERS, extra].sort((a, b) => (a.minutes ?? -1) - (b.minutes ?? -1));
}

export function reminderLabel(minutes) {
  const choice = REMINDERS.find((entry) => entry.minutes === (minutes ?? null));
  return choice ? choice.label : t("calendar.reminder_min", { n: minutes });
}

// A select holds text; «» is no reminder.
export function reminderFromSelect(text) {
  return text === "" ? null : Number(text);
}

// The four ranges of the list. `days` is null for the calendar month.
export const RANGES = [
  { id: "day", label: t("calendar.range_day"), days: 1 },
  { id: "week", label: t("calendar.range_week"), days: 7 },
  { id: "thirty", label: t("calendar.range_thirty"), days: 30 },
  { id: "month", label: t("calendar.range_month"), days: null },
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

const shortDate = (day) => dayDate(day).toLocaleDateString(locale(), { day: "numeric", month: "short", timeZone: "UTC" });
const longDate = (day) =>
  dayDate(day).toLocaleDateString(locale(), { weekday: "long", day: "numeric", month: "long", timeZone: "UTC" });

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
    time: event.all_day ? t("calendar.all_day") : localClock(occurrenceOf(event), zone),
    title: event.title,
    place: event.location || "",
    reminder: typeof event.reminder_minutes === "number",
    repeats: Boolean(event.recurring),
  };
}

// The page after `page`: the host sends its cursor only while the page is truncated.
export const nextCursor = (page) => (page.truncated ? page.next : null);

export const emptyListText = (hasMore) => (hasMore ? t("calendar.empty_more") : t("calendar.empty_none"));

// A series goes whole: its delete says so, since no single repetition can be removed.
export const deleteQuestion = (event) => (event.recurrence ? t("calendar.delete_series") : t("calendar.delete_once"));

// «29 oct, 09:30 – 10:00», «29 oct, 22:00 – 30 oct, 01:30», «29 oct – 31 oct» for whole days.
export function whenText(event, zone) {
  const first = localDate(event.start_utc, zone);
  if (event.all_day) {
    const last = lastDay(event, zone);
    return last > first ? `${shortDate(first)} – ${shortDate(last)}` : t("calendar.when_all_day", { date: shortDate(first) });
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
  if (!title) return problem("title", t("calendar.title_required"));
  if ([...title].length > EVENT_LIMITS.title) {
    return problem("title", t("calendar.title_too_long", { max: EVENT_LIMITS.title }));
  }
  const pattern = draft.all_day ? DAY_PATTERN : TIME_PATTERN;
  if (!pattern.test(draft.start)) {
    return problem("start", t(draft.all_day ? "calendar.start_day_required" : "calendar.start_required"));
  }
  if (draft.end !== null) {
    if (!pattern.test(draft.end)) {
      return problem("end", t(draft.all_day ? "calendar.end_day_required" : "calendar.end_required"));
    }
    const late = draft.all_day ? draft.end < draft.start : draft.end <= draft.start;
    if (late) {
      return problem("end", t(draft.all_day ? "calendar.end_day_order" : "calendar.end_order"));
    }
  }
  if ([...draft.location].length > EVENT_LIMITS.location) {
    return problem("location", t("calendar.location_too_long", { max: EVENT_LIMITS.location }));
  }
  if (new TextEncoder().encode(draft.notes).length > EVENT_LIMITS.notesBytes) {
    return problem("notes", t("calendar.notes_too_long"));
  }
  return null;
}

export function zoneProblem(name) {
  return name.trim() ? null : t("calendar.zone_required");
}

const CALENDAR_REFUSALS = {
  timezone_unset: t("calendar.err_timezone_unset"),
  timezone_already_set: t("calendar.err_timezone_already_set"),
  invalid_zone: t("calendar.zone_unknown"),
  invalid_local_time: t("calendar.err_invalid_local_time"),
  unsupported_recurrence: t("calendar.err_unsupported_recurrence"),
  invalid_recurrence: t("calendar.err_invalid_recurrence"),
  calendar_limit: t("calendar.err_calendar_limit", { n: CALENDAR_LIMIT }),
};

const FIELD_REFUSALS = {
  start: t("calendar.err_start_invalid"),
  end: t("calendar.err_end_invalid"),
  to: t("calendar.err_period_invalid"),
  timezone: t("calendar.zone_unknown"),
  recurrence: t("calendar.err_invalid_recurrence"),
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
