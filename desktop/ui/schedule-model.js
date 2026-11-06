// «Automatizar»: the rules of a scheduled task, free of DOM so tooling/schedule-model.test.mjs can
// run them. The host decides what is kept and when it runs; the words here are the operator's.
// Times are the schedule's own zone, so a «9:00» means 9:00 where the task was written.

import { DAYS } from "./calendar-recurrence-model.js";
import { friendlyError } from "./friendly-error.js";
import { BODY_MAX, TITLE_MAX, SECRET_TEXT, recipeFailureText } from "./recipe-model.js";
import { planFailureText } from "./plan-model.js";

export const PLAN_STEP_LIMIT = 3;
export const ACTIVE_LIMIT = 20;

export const PC_OFF_NOTE =
  "Solo se ejecuta con tu PC encendido. Si estaba apagado a la hora, no se lanza al encenderlo: te avisamos aquí y puedes lanzarla tú.";
export const PERMISSION_TEXT =
  "Este dispositivo no tiene el permiso APPROVE: puedes ver las tareas, pero no cambiarlas ni lanzarlas.";
export const PLAN_NEEDED_TEXT = "Este encargo es largo: antes de programarlo hay que aprobar un plan.";
export const NO_PROJECTS_TEXT = "Aún no tienes proyectos";
export const NO_PROJECTS_HELP = "Pídele a Alfred que cree un proyecto y vuelve aquí para programar la tarea.";

// Only a conversation with a project can carry a task; the others are not offered.
export function conversationChoices(conversations) {
  return conversations.filter((chat) => chat.project_id).map((chat) => [chat.id, chat.title]);
}
export const PLAN_EXTRA_TEXT =
  "Un plan que toca correo o internet nunca se aprueba solo: lo confirmas tú en «Planes propuestos».";
export const PLAN_NOT_APPROVED_TEXT =
  "Ese plan no sirve para este encargo: tiene que estar aprobado, ser de esta conversación y no usarlo otro programado.";
export const PLAN_EXPIRED_TEXT = "Cambiaste el texto: hay que aprobar un plan nuevo.";
const ARCHIVED_TEXT = "Esta conversación está archivada. Desarchívala para programar tareas en ella.";
const NOT_FOUND_TEXT = "Esa tarea ya no existe. Se quitará de la lista.";

export const KINDS = [
  { value: "once", label: "Una vez" },
  { value: "daily", label: "Cada día" },
  { value: "weekly", label: "Cada semana" },
  { value: "monthly", label: "Cada mes" },
];

const MONTHS = ["ene.", "feb.", "mar.", "abr.", "may.", "jun.", "jul.", "ago.", "sept.", "oct.", "nov.", "dic."];
const WEEKDAYS = ["dom", "lun", "mar", "mié", "jue", "vie", "sáb"];
const RANK = { active: 0, paused: 1, done: 2 };
const STATUS = { active: "Activa", paused: "En pausa", done: "Hecha" };
const OUTCOME = {
  started: "Se lanzó",
  skipped_overlap: "Omitida: ya hay una en marcha",
  skipped_cap: "Omitida: tope de ejecuciones",
  missed_offline: "Perdida: tu PC estaba apagado",
  failed: "Falló",
};
const REASON_TEXT = {
  rule: "La repetición no es válida. Elige una de la lista.",
  time: "La hora o la zona no son válidas.",
  steps: "El número de pasos no es válido.",
  recipe: "Esa receta ya no existe.",
  title: "Ponle un título a la tarea.",
  body: "Escribe el encargo.",
  content: "Escribe el encargo.",
};

// Characters, not UTF-16 units, as the rest of the app counts text.
const count = (text) => [...text].length;

export const statusLabel = (status) => STATUS[status] ?? "";
export const outcomeLabel = (outcome) => OUTCOME[outcome] ?? "";

function listWords(words) {
  if (words.length < 2) return words.join("");
  return `${words.slice(0, -1).join(", ")} y ${words[words.length - 1]}`;
}

// «9:00», «18:30»: the clock without its leading zero.
const clock = (time) => {
  const [hour, minute] = time.split(":");
  return `${Number(hour)}:${minute}`;
};

// «12 de nov.», «1 de sept.»
export function dateSentence(day) {
  const [, month, date] = day.split("-").map(Number);
  return `${date} de ${MONTHS[month - 1]}`;
}

// The rule the form means, as RFC 5545 for the host. A one-time task has none.
export function ruleFor({ kind, days }) {
  if (kind === "once") return null;
  if (kind === "daily") return "FREQ=DAILY;INTERVAL=1";
  if (kind === "monthly") return "FREQ=MONTHLY;INTERVAL=1";
  const picked = DAYS.map((day) => day.code).filter((code) => days.includes(code));
  return `FREQ=WEEKLY;INTERVAL=1;BYDAY=${picked.join(",")}`;
}

// The sentence of a schedule that the form and the list both read the same way.
export function ruleSentence({ kind, startDate, time, days }) {
  const at = `a las ${clock(time)}`;
  if (kind === "once") return `El ${dateSentence(startDate)} ${at}`;
  if (kind === "daily") return `Todos los días ${at}`;
  if (kind === "monthly") return `El ${Number(startDate.slice(8, 10))} de cada mes ${at}`;
  const names = DAYS.filter((day) => days.includes(day.code)).map((day) => day.name);
  return names.length ? `Cada ${listWords(names)} ${at}` : `Cada semana ${at}`;
}

// The same sentence for a schedule the host sent, read back from its rule text.
export function scheduleSentence(item) {
  if (!item.rule) return ruleSentence({ kind: "once", startDate: item.start_date, time: item.time_local, days: [] });
  const fields = Object.fromEntries(item.rule.split(";").map((part) => part.split("=")));
  const kinds = { DAILY: "daily", WEEKLY: "weekly", MONTHLY: "monthly" };
  return ruleSentence({
    kind: kinds[fields.FREQ] ?? "daily",
    startDate: item.start_date,
    time: item.time_local,
    days: fields.BYDAY ? fields.BYDAY.split(",") : [],
  });
}

// The steps an encargo names: its numbered or bulleted lines. A text with none is one step.
export function stepCount(text) {
  const lines = text.split("\n").filter((line) => /^\s*(?:[-*•]|\d+[.)])\s+\S/.test(line));
  return Math.max(1, lines.length);
}

export const needsPlan = (steps) => steps > PLAN_STEP_LIMIT;

// The offset of `zone` at `instant`, in milliseconds, so a wall-clock time can be compared with now.
function zoneOffsetMs(instant, zone) {
  const parts = Object.fromEntries(
    new Intl.DateTimeFormat("en-US", {
      timeZone: zone,
      hourCycle: "h23",
      year: "numeric",
      month: "numeric",
      day: "numeric",
      hour: "numeric",
      minute: "numeric",
      second: "numeric",
    })
      .formatToParts(new Date(instant))
      .map((part) => [part.type, Number(part.value)]),
  );
  const wall = Date.UTC(parts.year, parts.month - 1, parts.day, parts.hour, parts.minute, parts.second);
  return wall - Math.floor(instant / 1000) * 1000;
}

function instantOf(day, time, zone) {
  const wall = Date.parse(`${day}T${time}:00Z`);
  const first = wall - zoneOffsetMs(wall, zone);
  return wall - zoneOffsetMs(first, zone);
}

function zoneProblem(zone) {
  const name = zone.trim();
  if (!name) return "Escribe una zona, como Europe/Madrid.";
  try {
    new Intl.DateTimeFormat("es", { timeZone: name });
    return null;
  } catch {
    return "Esa zona no existe. Usa un nombre como Europe/Madrid.";
  }
}

const DAY_PATTERN = /^\d{4}-\d{2}-\d{2}$/;
const TIME_PATTERN = /^([01]\d|2[0-3]):[0-5]\d$/;

function validDay(day) {
  if (!DAY_PATTERN.test(day)) return false;
  const [year, month, date] = day.split("-").map(Number);
  const real = new Date(Date.UTC(year, month - 1, date));
  return real.getUTCMonth() === month - 1 && real.getUTCDate() === date;
}

// What is wrong with the form, field by field. An empty object means it can be sent.
export function formProblems(form, nowMs) {
  const problems = {};
  if (!form.title.trim()) problems.title = "Ponle un título a la tarea.";
  else if (count(form.title.trim()) > TITLE_MAX) problems.title = `El título admite ${TITLE_MAX} caracteres como máximo.`;
  if (form.source === "recipe") {
    if (!form.recipeId) problems.recipe = "Elige una receta.";
  } else if (!form.body.trim()) {
    problems.body = "Escribe el encargo.";
  } else if (count(form.body) > BODY_MAX) {
    problems.body = `El encargo admite ${BODY_MAX} caracteres como máximo.`;
  }
  if (!form.conversationId) problems.conversation = "Elige una conversación con proyecto.";
  if (!validDay(form.startDate)) problems.startDate = "Elige la fecha de inicio.";
  if (!TIME_PATTERN.test(form.time)) problems.time = "Escribe la hora como 09:00.";
  const zone = zoneProblem(form.timezone);
  if (zone) problems.timezone = zone;
  if (form.kind === "weekly" && form.days.length === 0) problems.days = "Elige al menos un día.";
  if (form.kind === "once" && !problems.startDate && !problems.time && !zone) {
    if (instantOf(form.startDate, form.time, form.timezone) <= nowMs) {
      problems.startDate = "Esa fecha y hora ya pasaron. Elige una futura.";
    }
  }
  return problems;
}

// The body the host takes for a create or an edit. Only the fields the form means are sent.
export function payloadOf(form) {
  const body = {
    title: form.title.trim(),
    conversation_id: form.conversationId,
    role: form.role,
    start_date: form.startDate,
    time: form.time,
    timezone: form.timezone.trim(),
    steps: form.steps,
  };
  if (form.source === "recipe") body.recipe_id = form.recipeId;
  else body.body = form.body.trim();
  const rule = ruleFor(form);
  if (rule) body.rule = rule;
  if (form.planId) body.plan_id = form.planId;
  return body;
}

// «jue 12 nov., 9:00» in the schedule's zone, or what the list says when there is no such run.
function zoneStamp(iso, zone) {
  const parts = Object.fromEntries(
    new Intl.DateTimeFormat("en-US", {
      timeZone: zone,
      hourCycle: "h23",
      year: "numeric",
      month: "numeric",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit",
    })
      .formatToParts(new Date(iso))
      .map((part) => [part.type, part.value]),
  );
  const weekday = WEEKDAYS[new Date(Date.UTC(parts.year, parts.month - 1, parts.day)).getUTCDay()];
  return `${weekday} ${Number(parts.day)} ${MONTHS[Number(parts.month) - 1]}, ${Number(parts.hour)}:${parts.minute}`;
}

export function nextRunText(iso, zone) {
  return iso ? zoneStamp(iso, zone) : "Sin próxima ejecución";
}

export function lastRunText(iso, zone) {
  return iso ? zoneStamp(iso, zone) : "Todavía no se ha ejecutado";
}

// Active first, then paused, then done; among the active, the soonest run first.
export function sortSchedules(list) {
  const when = (item) => Date.parse(item.next_run_at) || Number.MAX_SAFE_INTEGER;
  const rank = (item) => RANK[item.status] ?? 3;
  return [...list].sort((a, b) => rank(a) - rank(b) || when(a) - when(b));
}

// What the operator reads for a failed schedule call. The host's words are never shown raw.
export function scheduleFailureText(error) {
  if (error && error.kind === "scheduleRefused") return scheduleCodeText(error);
  if (error && error.kind === "planRefused") {
    if (error.code === "plan_not_approved") return PLAN_NOT_APPROVED_TEXT;
    if (error.code === "plan_expired") return PLAN_EXPIRED_TEXT;
    return planFailureText(error);
  }
  if (error && error.kind === "recipeRefused" && error.code === "secret_detected") return SECRET_TEXT;
  if (error && error.kind === "forbidden") return PERMISSION_TEXT;
  if (error && error.kind === "notFound") return NOT_FOUND_TEXT;
  if (error && error.kind === "archived") return ARCHIVED_TEXT;
  if (error && error.kind === "agentBusy") return recipeFailureText(error);
  return friendlyError(error);
}

function scheduleCodeText({ code, reason }) {
  if (code === "invalid_schedule") return REASON_TEXT[reason] ?? "Revisa los datos de la tarea.";
  if (code === "schedule_limit") return `Ya tienes ${ACTIVE_LIMIT} tareas activas. Pausa o borra una para programar otra.`;
  if (code === "schedule_done") return "Esta tarea ya se hizo y no se puede cambiar.";
  if (code === "schedule_no_future_run") return "Esa fecha y hora ya pasaron. Elige una futura.";
  if (code === "no_such_schedule") return NOT_FOUND_TEXT;
  return friendlyError({ kind: "scheduleRefused" });
}
