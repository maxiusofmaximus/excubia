// Run with: node --test tooling/calendar-recurrence-model.test.mjs
// «Repetir»: the form's state, the rule it builds and reads back, and the words of a rule.
import { test } from "node:test";
import assert from "node:assert/strict";

import { formState, readRule, ruleFromForm, ruleText, setRepeat, weekdayOf } from "../ui/calendar-recurrence-model.js";

const MONDAY = "2026-11-02T09:30";
const blank = formState(null, MONDAY);

test("a new event does not repeat, and choosing weekly picks the day it starts on", () => {
  assert.equal(blank.repeat, "none");
  assert.deepEqual(ruleFromForm(blank, MONDAY), { rule: null });
  const weekly = setRepeat(blank, "WEEKLY", MONDAY);
  assert.deepEqual(weekly.days, ["MO"]);
  assert.deepEqual(ruleFromForm(weekly, MONDAY), { rule: "FREQ=WEEKLY;INTERVAL=1;BYDAY=MO" });
});

test("the form builds the normalized rule for each end", () => {
  const counted = { ...setRepeat(blank, "WEEKLY", MONDAY), days: ["WE", "MO"], end: "count", count: "10" };
  assert.deepEqual(ruleFromForm(counted, MONDAY), { rule: "FREQ=WEEKLY;INTERVAL=1;BYDAY=MO,WE;COUNT=10" });
  const monthly = { ...blank, repeat: "MONTHLY", every: "2", end: "until", until: "2027-03-31" };
  assert.deepEqual(ruleFromForm(monthly, MONDAY), { rule: "FREQ=MONTHLY;INTERVAL=2;UNTIL=20270331" });
});

test("a weekly rule without BYDAY reads the day it starts on", () => {
  assert.deepEqual(formState("FREQ=WEEKLY;INTERVAL=1", MONDAY).days, ["MO"]);
});

test("a rule the form can show reads back into its fields", () => {
  const daily = formState("RRULE:FREQ=DAILY;INTERVAL=3;UNTIL=20261231", MONDAY);
  assert.deepEqual([daily.repeat, daily.every, daily.end, daily.until], ["DAILY", "3", "until", "2026-12-31"]);
  assert.equal(formState("FREQ=YEARLY;COUNT=5", MONDAY).count, "5");
});

test("a rule outside the form stays custom, is kept as it is and reads as text", () => {
  const outside = [
    "FREQ=HOURLY",
    "FREQ=DAILY;BYDAY=MO",
    "FREQ=MONTHLY;BYDAY=1MO",
    "FREQ=WEEKLY;INTERVAL=0",
    "FREQ=DAILY;COUNT=0",
    "FREQ=DAILY;COUNT=5;UNTIL=20261231",
    "FREQ=DAILY;UNTIL=20261231T120000Z",
    "FREQ=DAILY;WKST=MO",
    "FREQ=DAILY;FREQ=WEEKLY",
    "FREQ=WEEKLY;BYDAY=MO=TU",
    "INTERVAL=2",
  ];
  for (const rule of outside) {
    assert.equal(readRule(rule), null, rule);
    assert.equal(formState(rule, MONDAY).repeat, "custom", rule);
    assert.deepEqual(ruleFromForm(formState(rule, MONDAY), MONDAY), { rule });
    assert.equal(ruleText(rule), "Repetición personalizada");
  }
});

test("a custom rule changes only when the person picks another repeat", () => {
  const state = setRepeat(formState("FREQ=HOURLY", MONDAY), "DAILY", MONDAY);
  assert.deepEqual(ruleFromForm(state, MONDAY), { rule: "FREQ=DAILY;INTERVAL=1" });
});

test("the form names what is missing before the host does", () => {
  const weekly = setRepeat(blank, "WEEKLY", MONDAY);
  assert.match(ruleFromForm({ ...weekly, days: [] }, MONDAY).problem, /al menos un día/);
  assert.match(ruleFromForm({ ...blank, repeat: "DAILY", every: "0" }, MONDAY).problem, /1 a 999/);
  assert.match(ruleFromForm({ ...blank, repeat: "DAILY", every: "1000" }, MONDAY).problem, /1 a 999/);
  assert.match(ruleFromForm({ ...blank, repeat: "DAILY", end: "count", count: "1001" }, MONDAY).problem, /1 a 1000/);
  assert.match(ruleFromForm({ ...blank, repeat: "DAILY", end: "until", until: "" }, MONDAY).problem, /Elige la fecha/);
  assert.match(
    ruleFromForm({ ...blank, repeat: "DAILY", end: "until", until: "2026-10-01" }, MONDAY).problem,
    /anterior al inicio/,
  );
});

test("a rule reads back in words for the detail", () => {
  assert.equal(ruleText("FREQ=WEEKLY;INTERVAL=1;BYDAY=MO,WE;COUNT=10"), "Cada semana, lunes y miércoles, 10 veces");
  assert.equal(ruleText("FREQ=WEEKLY;INTERVAL=1;BYDAY=SU,MO"), "Cada semana, lunes y domingo");
  assert.equal(ruleText("FREQ=WEEKLY;INTERVAL=1;BYDAY=FR,MO,WE"), "Cada semana, lunes, miércoles y viernes");
  assert.equal(ruleText("FREQ=DAILY;INTERVAL=2;UNTIL=20261231"), "Cada 2 días, hasta el 31 de diciembre de 2026");
  assert.equal(ruleText("FREQ=MONTHLY;INTERVAL=1"), "Cada mes");
  assert.equal(ruleText("FREQ=YEARLY;INTERVAL=1;COUNT=1"), "Cada año, 1 vez");
});

test("the weekday of a day or of a local time, and nothing for an empty start", () => {
  assert.equal(weekdayOf("2026-11-02T09:30"), "MO");
  assert.equal(weekdayOf("2026-11-08"), "SU");
  assert.equal(weekdayOf(""), null);
});
