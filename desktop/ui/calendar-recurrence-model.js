// «Repetir»: the rules that need no DOM, so tooling/calendar-recurrence-model.test.mjs can run
// them. The form keeps a state of plain strings; a rule it cannot show is kept as it came.

import { locale, t } from "./i18n.js";
export const REPEATS = [
  { value: "none", label: t("calendar.repeat_none") },
  { value: "DAILY", label: t("calendar.repeat_daily") },
  { value: "WEEKLY", label: t("calendar.repeat_weekly") },
  { value: "MONTHLY", label: t("calendar.repeat_monthly") },
  { value: "YEARLY", label: t("calendar.repeat_yearly") },
];

// Monday first, as the week reads in the form.
export const DAYS = [
  { code: "MO", short: t("calendar.day_mo_short"), name: t("calendar.day_mo") },
  { code: "TU", short: t("calendar.day_tu_short"), name: t("calendar.day_tu") },
  { code: "WE", short: t("calendar.day_we_short"), name: t("calendar.day_we") },
  { code: "TH", short: t("calendar.day_th_short"), name: t("calendar.day_th") },
  { code: "FR", short: t("calendar.day_fr_short"), name: t("calendar.day_fr") },
  { code: "SA", short: t("calendar.day_sa_short"), name: t("calendar.day_sa") },
  { code: "SU", short: t("calendar.day_su_short"), name: t("calendar.day_su") },
];

const UNITS = { DAILY: "day", WEEKLY: "week", MONTHLY: "month", YEARLY: "year" };

const KEYS = ["FREQ", "INTERVAL", "BYDAY", "COUNT", "UNTIL"];
const DAY_PATTERN = /^\d{4}-\d{2}-\d{2}$/;
const UNTIL_PATTERN = /^(\d{4})(\d{2})(\d{2})$/;

export const unitText = (repeat, count) => t(`calendar.unit_${UNITS[repeat]}.${count === 1 ? "one" : "other"}`);

// The weekday code of a «YYYY-MM-DD» day, or of a local time that starts with one.
export function weekdayOf(start) {
  const day = start.slice(0, 10);
  if (!DAY_PATTERN.test(day)) return null;
  const [year, month, date] = day.split("-").map(Number);
  const index = (new Date(Date.UTC(year, month - 1, date)).getUTCDay() + 6) % 7;
  return DAYS[index].code;
}

// A whole number from 1 to `max`, or null.
function whole(text, max) {
  const value = Number(text);
  return /^\d+$/.test(text) && value >= 1 && value <= max ? value : null;
}

function endOf({ COUNT, UNTIL }) {
  if (COUNT !== undefined && UNTIL !== undefined) return null;
  if (COUNT !== undefined) {
    const count = whole(COUNT, 1000);
    return count === null ? null : { kind: "count", count };
  }
  if (UNTIL !== undefined) {
    const day = UNTIL_PATTERN.exec(UNTIL);
    return day ? { kind: "until", day: `${day[1]}-${day[2]}-${day[3]}` } : null;
  }
  return { kind: "never" };
}

// The parts of a rule the form can show, or null for a rule outside them.
export function readRule(rule) {
  const pairs = rule.replace(/^RRULE:/, "").split(";").map((part) => part.split("="));
  const fields = Object.fromEntries(pairs);
  if (Object.keys(fields).length !== pairs.length) return null;
  if (pairs.some(([key, value, extra]) => value === undefined || extra !== undefined || !KEYS.includes(key))) {
    return null;
  }
  const { FREQ, INTERVAL = "1", BYDAY = "" } = fields;
  if (!Object.hasOwn(UNITS, FREQ)) return null;
  const interval = whole(INTERVAL, 999);
  const days = BYDAY === "" ? [] : BYDAY.split(",");
  if (days.length && (FREQ !== "WEEKLY" || !days.every((code) => DAYS.some((day) => day.code === code)))) return null;
  const end = endOf(fields);
  if (interval === null || end === null) return null;
  return { freq: FREQ, interval, days, end };
}

const BLANK = { repeat: "none", every: "1", days: [], end: "never", count: "10", until: "", custom: null };

// The form's state for a rule (null: a new event) that starts on `start`.
export function formState(rule, start) {
  if (!rule) return { ...BLANK };
  const read = readRule(rule);
  if (!read) return { ...BLANK, repeat: "custom", custom: rule };
  const days = read.days.length ? read.days : [weekdayOf(start)].filter(Boolean);
  return {
    ...BLANK,
    repeat: read.freq,
    every: String(read.interval),
    days,
    end: read.end.kind,
    count: read.end.kind === "count" ? String(read.end.count) : BLANK.count,
    until: read.end.kind === "until" ? read.end.day : "",
  };
}

// Picking a repeat; a weekly one that has no day yet takes the day the event starts on.
export function setRepeat(state, repeat, start) {
  const day = weekdayOf(start);
  const days = state.days.length || !day ? state.days : [day];
  return { ...state, repeat, days };
}

// The rule the form means, or the first problem it has: `{rule}` (null for none) or `{problem}`.
export function ruleFromForm(state, start) {
  if (state.repeat === "none") return { rule: null };
  if (state.repeat === "custom") return { rule: state.custom };
  const every = whole(state.every, 999);
  if (every === null) return { problem: t("calendar.rec_interval_range") };
  const parts = [`FREQ=${state.repeat}`, `INTERVAL=${every}`];
  if (state.repeat === "WEEKLY") {
    if (!state.days.length) return { problem: t("calendar.rec_pick_day") };
    parts.push(`BYDAY=${DAYS.map((day) => day.code).filter((code) => state.days.includes(code)).join(",")}`);
  }
  if (state.end === "count") {
    const count = whole(state.count, 1000);
    if (count === null) return { problem: t("calendar.rec_count_range") };
    parts.push(`COUNT=${count}`);
  }
  if (state.end === "until") {
    if (!DAY_PATTERN.test(state.until)) return { problem: t("calendar.rec_until_pick") };
    if (state.until < start.slice(0, 10)) return { problem: t("calendar.rec_until_before") };
    parts.push(`UNTIL=${state.until.replaceAll("-", "")}`);
  }
  return { rule: parts.join(";") };
}

const longDate = (day) => {
  const [year, month, date] = day.split("-").map(Number);
  return new Date(Date.UTC(year, month - 1, date)).toLocaleDateString(locale(), {
    day: "numeric",
    month: "long",
    year: "numeric",
    timeZone: "UTC",
  });
};

// «un», «lunes y miércoles», «lunes, miércoles y viernes».
function listWords(words) {
  if (words.length < 2) return words.join("");
  return `${words.slice(0, -1).join(", ")}${t("calendar.rec_and")}${words[words.length - 1]}`;
}

// A rule in words: «Cada semana, lunes y miércoles, 10 veces». A rule the form cannot show
// reads as «Repetición personalizada».
export function ruleText(rule) {
  const read = readRule(rule);
  if (!read) return t("calendar.rec_custom");
  const count = read.interval === 1 ? "" : `${read.interval} `;
  const head = t("calendar.rec_head", { every: count, unit: unitText(read.freq, read.interval) });
  const days = DAYS.filter((day) => read.days.includes(day.code)).map((day) => day.name);
  const parts = [head];
  if (days.length) parts.push(listWords(days));
  if (read.end.kind === "count") parts.push(t(`calendar.rec_times.${read.end.count === 1 ? "one" : "other"}`, { n: read.end.count }));
  if (read.end.kind === "until") parts.push(t("calendar.rec_until", { date: longDate(read.end.day) }));
  return parts.join(", ");
}
