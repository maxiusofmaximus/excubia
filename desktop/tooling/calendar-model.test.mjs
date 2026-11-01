// Run with: node --test tooling/calendar-model.test.mjs
// «Calendario»: the day grouping, the clock in the company's zone (DST included), the
// ranges and their navigation, the local limits, and the words of each refusal.
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  EVENT_LIMITS,
  addDays,
  addMonths,
  calendarErrorText,
  deleteQuestion,
  emptyListText,
  eventProblem,
  eventRow,
  formValues,
  groupByDay,
  listFailureText,
  localClock,
  localDate,
  localDateTime,
  nextCursor,
  reminderChoices,
  reminderFromSelect,
  reminderLabel,
  shiftAnchor,
  whenText,
  windowLabel,
  windowOf,
  zoneProblem,
} from "../ui/calendar-model.js";

const MADRID = "Europe/Madrid";

function event(overrides) {
  return {
    id: "ev_1",
    title: "Llamada con Ana",
    start_utc: "2026-10-29T08:30:00.000Z",
    end_utc: "2026-10-29T09:00:00.000Z",
    all_day: false,
    timezone: MADRID,
    location: null,
    notes: null,
    reminder_minutes: null,
    ...overrides,
  };
}

function draft(overrides) {
  return {
    title: "Llamada con Ana",
    start: "2026-11-03T09:30",
    end: null,
    all_day: false,
    location: "",
    notes: "",
    reminder_minutes: null,
    ...overrides,
  };
}

test("the clock and the day are read in the company's zone, not in UTC", () => {
  // 23:30 UTC on the 29th is 00:30 on the 30th in Madrid.
  assert.equal(localDate("2026-10-29T23:30:00.000Z", MADRID), "2026-10-30");
  assert.equal(localClock("2026-10-29T23:30:00.000Z", MADRID), "00:30");
  assert.equal(localDateTime("2026-10-29T08:30:00.000Z", MADRID), "2026-10-29T09:30");
});

test("the clock follows the change of hour: the repeated hour and the one that moves", () => {
  // Madrid leaves summer time on 25 Oct 2026 at 01:00 UTC: 02:59 CEST is followed by 02:00 CET.
  assert.equal(localDateTime("2026-10-25T00:59:00.000Z", MADRID), "2026-10-25T02:59");
  assert.equal(localDateTime("2026-10-25T01:00:00.000Z", MADRID), "2026-10-25T02:00");
  assert.equal(localDateTime("2026-10-25T01:30:00.000Z", MADRID), "2026-10-25T02:30");
});

test("a whole day is a date, and its clock is not shown", () => {
  const day = event({
    all_day: true,
    start_utc: "2026-10-28T23:00:00.000Z",
    end_utc: "2026-10-31T23:00:00.000Z",
  });
  const row = eventRow(day, MADRID);
  assert.equal(row.time, "Todo el día");
  assert.equal(whenText(day, MADRID), "29 oct – 31 oct");
});

test("a timed row shows the local clock and marks the reminder", () => {
  const row = eventRow(event({ location: "Oficina", reminder_minutes: 15 }), MADRID);
  assert.deepEqual(row, {
    id: "ev_1",
    time: "09:30",
    title: "Llamada con Ana",
    place: "Oficina",
    reminder: true,
    repeats: false,
  });
});

test("an occurrence of a series shows its own day and time, and says that it repeats", () => {
  const occurrence = event({
    recurring: true,
    recurrence: "FREQ=WEEKLY;INTERVAL=1;BYDAY=WE",
    start_utc: "2026-10-28T08:30:00.000Z",
    occurrence_start: "2026-11-04T08:30:00.000Z",
    end_utc: null,
  });
  assert.equal(eventRow(occurrence, MADRID).time, "09:30");
  assert.equal(eventRow(occurrence, MADRID).repeats, true);
  assert.deepEqual(
    groupByDay([occurrence], MADRID).map((group) => group.day),
    ["2026-11-04"],
  );
});

test("the list pages on while the host says it is truncated, and an empty truncated list asks for a narrower range", () => {
  assert.equal(nextCursor({ truncated: true, next: "ev_1@2026-11-04T08:30:00.000Z" }), "ev_1@2026-11-04T08:30:00.000Z");
  assert.equal(nextCursor({ truncated: false, next: null, events: [] }), null);
  assert.equal(emptyListText(false), "No hay eventos en este periodo.");
  assert.match(emptyListText(true), /acota las fechas/);
});

test("a delete of a series says that every repetition goes", () => {
  assert.equal(deleteQuestion(event({ recurrence: "FREQ=DAILY;INTERVAL=1" })), "Se borran todas las repeticiones. No se puede deshacer.");
  assert.equal(deleteQuestion(event({ recurrence: null })), "Se borra para siempre. No se puede deshacer.");
});

test("events are grouped by their day in the company's zone, earliest day first", () => {
  const groups = groupByDay(
    [
      event({ id: "a", start_utc: "2026-10-29T23:30:00.000Z", end_utc: null }),
      event({ id: "b", start_utc: "2026-10-29T08:00:00.000Z", end_utc: null }),
      event({ id: "c", start_utc: "2026-10-28T10:00:00.000Z", end_utc: null }),
    ],
    MADRID,
  );
  assert.deepEqual(
    groups.map((group) => [group.day, group.events.map((item) => item.id)]),
    [
      ["2026-10-28", ["c"]],
      ["2026-10-29", ["b"]],
      ["2026-10-30", ["a"]],
    ],
  );
  assert.match(groups[0].label, /miércoles/);
});

test("day arithmetic never moves across a zone or a month by accident", () => {
  assert.equal(addDays("2026-10-31", 1), "2026-11-01");
  assert.equal(addDays("2026-03-01", -1), "2026-02-28");
  assert.equal(addMonths("2026-01-15", 1), "2026-02-01");
});

test("each range covers its days, the month its calendar month", () => {
  assert.deepEqual(windowOf("day", "2026-10-29"), { from: "2026-10-29", to: "2026-10-30" });
  assert.deepEqual(windowOf("week", "2026-10-29"), { from: "2026-10-29", to: "2026-11-05" });
  assert.deepEqual(windowOf("thirty", "2026-10-29"), { from: "2026-10-29", to: "2026-11-28" });
  assert.deepEqual(windowOf("month", "2026-11-03"), { from: "2026-11-01", to: "2026-12-01" });
});

test("navigation steps a range by its length and a month by a month", () => {
  assert.equal(shiftAnchor("week", "2026-10-29", -1), "2026-10-22");
  assert.equal(shiftAnchor("thirty", "2026-10-29", 1), "2026-11-28");
  assert.equal(shiftAnchor("month", "2026-10-29", 1), "2026-11-01");
  assert.equal(windowLabel(windowOf("month", "2026-11-03")), "1 nov – 30 nov");
  assert.equal(windowLabel(windowOf("day", "2026-10-29")), "29 oct");
});

test("a form reads back the schedule it was saved with", () => {
  const day = event({
    all_day: true,
    start_utc: "2026-10-28T23:00:00.000Z",
    end_utc: "2026-10-31T23:00:00.000Z",
  });
  assert.deepEqual(formValues(day, MADRID), { allDay: true, start: "2026-10-29", end: "2026-10-31" });
  assert.deepEqual(formValues(event({ end_utc: null }), MADRID), {
    allDay: false,
    start: "2026-10-29T09:30",
    end: "",
  });
});

test("a timed event that ends on another day says so", () => {
  const night = event({
    start_utc: "2026-10-29T21:00:00.000Z",
    end_utc: "2026-10-30T00:30:00.000Z",
  });
  assert.equal(whenText(night, MADRID), "29 oct, 22:00 – 30 oct, 01:30");
});

test("the local limits match the host's, and the first problem is named", () => {
  assert.deepEqual(eventProblem(draft({ title: "   " })), {
    field: "title",
    message: "Escribe un título.",
  });
  assert.equal(eventProblem(draft({ title: "x".repeat(EVENT_LIMITS.title + 1) })).field, "title");
  assert.equal(eventProblem(draft({ title: "x".repeat(EVENT_LIMITS.title) })), null);
  assert.equal(eventProblem(draft({ location: "y".repeat(EVENT_LIMITS.location + 1) })).field, "location");
  assert.equal(eventProblem(draft({ notes: "ñ".repeat(2049) })).field, "notes");
  assert.equal(eventProblem(draft({ notes: "ñ".repeat(2048) })), null);
});

test("an end must come after the start, and an all-day end may be the same day", () => {
  assert.equal(eventProblem(draft({ end: "2026-11-03T09:30" })).field, "end");
  assert.equal(eventProblem(draft({ end: "2026-11-03T09:00" })).field, "end");
  assert.equal(eventProblem(draft({ end: "2026-11-03T10:00" })), null);
  const day = { all_day: true, start: "2026-11-03", end: "2026-11-03" };
  assert.equal(eventProblem(draft({ ...day, end: "2026-11-02" })).field, "end");
  assert.equal(eventProblem(draft(day)), null);
});

test("a start in the wrong shape is refused before it leaves the device", () => {
  assert.equal(eventProblem(draft({ start: "" })).field, "start");
  assert.equal(eventProblem(draft({ start: "2026-11-03" })).field, "start");
  assert.equal(eventProblem(draft({ all_day: true, start: "2026-11-03T09:30" })).field, "start");
});

test("reminders read back from the select and keep a value the list does not offer", () => {
  assert.equal(reminderFromSelect(""), null);
  assert.equal(reminderFromSelect("1440"), 1440);
  assert.equal(reminderLabel(1440), "1 día antes");
  assert.equal(reminderLabel(null), "Ninguno");
  assert.equal(reminderChoices(null).length, 6);
  const extra = reminderChoices(10);
  assert.equal(extra.find((choice) => choice.minutes === 10).label, "10 min antes");
  assert.equal(extra[0].minutes, null);
});

test("the zone must have a name before the host sees it", () => {
  assert.match(zoneProblem("  "), /Europe\/Madrid/);
  assert.equal(zoneProblem("Europe/Madrid"), null);
});

test("each refusal of the calendar reads in Spanish, the field ones by their field", () => {
  const refusal = (code, field) => ({ kind: "agendaRefused", code, field, message: "english" });
  assert.equal(
    calendarErrorText(refusal("invalid_local_time", "start")),
    "Esa hora no existe por el cambio de horario.",
  );
  assert.equal(
    calendarErrorText(refusal("timezone_unset", "timezone")),
    "Falta la zona horaria de la empresa. Fíjala antes de crear eventos.",
  );
  assert.equal(
    calendarErrorText(refusal("timezone_already_set")),
    "La empresa ya tiene zona horaria y no se puede cambiar.",
  );
  assert.match(calendarErrorText(refusal("calendar_limit")), /5000 eventos/);
  assert.equal(
    calendarErrorText({ kind: "invalid", field: "end", message: "english" }),
    "El fin no es válido: debe ser posterior al inicio.",
  );
  assert.equal(calendarErrorText(refusal("not_found")), "Eso ya no existe. Recarga la lista.");
  assert.equal(calendarErrorText(refusal("unsupported_recurrence")), "Esa repetición todavía no está soportada.");
  assert.equal(calendarErrorText(refusal("invalid_recurrence")), "La repetición no es válida.");
  assert.equal(
    calendarErrorText({ kind: "invalid", field: "recurrence", message: "english" }),
    "La repetición no es válida.",
  );
  assert.equal(calendarErrorText({ kind: "offline" }), "Sin conexión con el host. Lo cargado sigue visible.");
});

test("a list that cannot be read names the missing grant", () => {
  assert.equal(
    listFailureText({ kind: "forbidden" }),
    "Este dispositivo no tiene el permiso READ_STATE. Pídeselo al host.",
  );
});
