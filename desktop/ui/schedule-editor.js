// «Programar una tarea»: the one form of a scheduled task, new or edited. It reads the team's hires,
// the conversations with a project, the recipes of an oficio and the approved plans, then sends the
// rules the form means. The host still decides; its refusal is worded here, never shown raw.

import { $, invoke, setBusy } from "./core.js";
import { element } from "./chatdom.js";
import { DAYS, weekdayOf } from "./calendar-recurrence-model.js";
import { activeHires } from "./hires-model.js";
import { openPending } from "./plans.js";
import {
  KINDS,
  NO_PROJECTS_HELP,
  NO_PROJECTS_TEXT,
  PC_OFF_NOTE,
  PERMISSION_TEXT,
  PLAN_EXTRA_TEXT,
  PLAN_NEEDED_TEXT,
  conversationChoices,
  formProblems,
  needsPlan,
  payloadOf,
  scheduleFailureText,
  stepCount,
} from "./schedule-model.js";

// Which control each problem marks. A problem with no control (the plan) is only listed.
const FIELD_IDS = {
  role: "schedule-role",
  title: "schedule-title",
  body: "schedule-body",
  recipe: "schedule-recipe",
  conversation: "schedule-conversation",
  startDate: "schedule-date",
  time: "schedule-time",
  timezone: "schedule-zone",
  days: "schedule-days-wrap",
  plan: "schedule-plan",
};

// `view` exists only while the dialog is open. `ready` turns on once the choices have arrived.
let view = null;
let wired = false;

const field = (id) => $(id).value;
const closeEditor = () => $("schedule-editor").close();

function today() {
  const now = new Date();
  const pad = (number) => String(number).padStart(2, "0");
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}

// The rule of a schedule the host sent, read back into the form's words.
function ruleFields(rule) {
  return Object.fromEntries((rule ?? "").split(";").filter(Boolean).map((part) => part.split("=")));
}

// What the form shows: the schedule being edited, or the defaults of a new one.
function initialOf(schedule) {
  if (!schedule) {
    return {
      title: "",
      role: "",
      source: "text",
      body: "",
      recipeId: "",
      conversationId: "",
      kind: "once",
      days: [],
      startDate: today(),
      time: "09:00",
      timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
      planId: "",
    };
  }
  const rule = ruleFields(schedule.rule);
  const kinds = { DAILY: "daily", WEEKLY: "weekly", MONTHLY: "monthly" };
  return {
    title: schedule.title,
    role: schedule.role_id,
    source: schedule.recipe_id ? "recipe" : "text",
    body: schedule.body ?? "",
    recipeId: schedule.recipe_id ?? "",
    conversationId: schedule.conversation_id,
    kind: schedule.rule ? (kinds[rule.FREQ] ?? "daily") : "once",
    days: rule.BYDAY ? rule.BYDAY.split(",") : [],
    startDate: schedule.start_date,
    time: schedule.time_local,
    timezone: schedule.timezone,
    planId: schedule.plan_id ?? "",
  };
}

// The text the steps are counted from: the typed encargo, or the recipe chosen.
function textOf(form) {
  if (form.source === "text") return form.body;
  return view?.recipes.get(form.role)?.find((recipe) => recipe.id === form.recipeId)?.body ?? "";
}

function readForm() {
  const form = {
    title: field("schedule-title"),
    role: field("schedule-role"),
    source: document.querySelector('input[name="schedule-source"]:checked')?.value ?? "text",
    body: field("schedule-body"),
    recipeId: field("schedule-recipe"),
    conversationId: field("schedule-conversation"),
    kind: field("schedule-kind"),
    days: [...$("schedule-days").querySelectorAll("input:checked")].map((input) => input.value),
    startDate: field("schedule-date"),
    time: field("schedule-time"),
    timezone: field("schedule-zone"),
    planId: field("schedule-plan"),
  };
  form.steps = stepCount(textOf(form));
  return form;
}

function fill(initial) {
  $("schedule-title").value = initial.title;
  $("schedule-body").value = initial.body;
  $("schedule-kind").value = initial.kind;
  $("schedule-date").value = initial.startDate;
  $("schedule-time").value = initial.time;
  $("schedule-zone").value = initial.timezone;
  for (const input of $("schedule-days").querySelectorAll("input")) input.checked = initial.days.includes(input.value);
  $(`schedule-source-${initial.source}`).checked = true;
}

// Fills a select with its choices; a value that is no longer among them stays, labelled as gone.
function choose(select, pairs, selected, placeholder) {
  const shown = !selected || pairs.some(([value]) => value === selected) ? pairs : [...pairs, [selected, "Ya no disponible"]];
  const first = placeholder ? [new Option(placeholder, "")] : [];
  select.replaceChildren(...first, ...shown.map(([value, label]) => new Option(label, value)));
  select.value = selected ?? "";
}

function showNotice(text) {
  const box = $("schedule-notice");
  box.hidden = !text;
  box.textContent = text ?? "";
}

function showProblems(problems) {
  const list = $("schedule-problems");
  // The plan's reason is already the hint under its select; the list would only repeat it.
  const messages = Object.entries(problems).filter(([key]) => key !== "plan").map(([, text]) => text);
  list.replaceChildren(...messages.map((text) => element("li", null, text)));
  list.hidden = messages.length === 0;
  for (const [key, id] of Object.entries(FIELD_IDS)) $(id).setAttribute("aria-invalid", String(key in problems));
}

function paintSteps(form) {
  const plural = form.steps === 1 ? "paso" : "pasos";
  $("schedule-steps").textContent = `${form.steps} ${plural}`;
  $("schedule-plan-wrap").hidden = !needsPlan(form.steps);
}

// Everything the operator can do right now, from the one state the form holds.
function paint() {
  if (!view) return;
  const form = readForm();
  const recipe = form.source === "recipe";
  $("schedule-body-wrap").hidden = recipe;
  $("schedule-recipe-wrap").hidden = !recipe;
  $("schedule-days-wrap").hidden = form.kind !== "weekly";
  paintSteps(form);
  const frozen = !view.ready || view.locked || view.busy;
  for (const id of [...Object.values(FIELD_IDS), "schedule-kind", "schedule-source-text", "schedule-source-recipe"]) {
    $(id).disabled = frozen;
  }
  // Without a project there is nothing to pick, so the task cannot be saved yet.
  $("schedule-conversation").disabled = frozen || view.noProjects;
  $("schedule-save").disabled = frozen || view.noProjects;
  $("schedule-conversation-hint").hidden = !view.noProjects;
  $("schedule-permission").hidden = !view.locked;
}

// A new weekly task starts on the day of its first date, so the operator only changes what differs.
function preselectDay() {
  if (field("schedule-kind") !== "weekly") return;
  const boxes = [...$("schedule-days").querySelectorAll("input")];
  if (boxes.some((box) => box.checked)) return;
  const code = weekdayOf(field("schedule-date"));
  for (const box of boxes) box.checked = box.value === code;
}

async function loadRecipes(role, selected) {
  const current = view;
  const recipes = view.recipes;
  if (!role) {
    choose($("schedule-recipe"), [], "", "Elige una receta");
    return paint();
  }
  try {
    if (!recipes.has(role)) recipes.set(role, await invoke("recipes", { role }));
    if (view !== current) return;
    choose(
      $("schedule-recipe"),
      recipes.get(role).map((recipe) => [recipe.id, recipe.title]),
      selected,
      "Elige una receta",
    );
  } catch (error) {
    if (view === current) showNotice(scheduleFailureText(error));
  }
  paint();
}

// The choices the form needs, read once per opening. A failed read says so and keeps the save off.
async function loadChoices(initial) {
  const current = view;
  try {
    const [catalog, team, home, plans] = await Promise.all([
      invoke("team_roles"),
      invoke("team_hires"),
      invoke("chat_home"),
      invoke("plans"),
    ]);
    if (view !== current) return;
    const titles = new Map(catalog.roles.map((role) => [role.id, role.title]));
    const oficios = new Map();
    for (const hire of activeHires(team.hires)) {
      if (!oficios.has(hire.role)) oficios.set(hire.role, `${hire.name} · ${titles.get(hire.role) ?? "Oficio"}`);
    }
    const role = initial.role || (oficios.keys().next().value ?? "");
    choose($("schedule-role"), [...oficios], role, "Elige un oficio");
    const chats = conversationChoices(home.conversations);
    view.noProjects = chats.length === 0;
    $("schedule-conversation-hint").textContent = view.noProjects ? NO_PROJECTS_HELP : "";
    choose(
      $("schedule-conversation"),
      chats,
      initial.conversationId,
      view.noProjects ? NO_PROJECTS_TEXT : "Elige una conversación con proyecto",
    );
    const approved = plans.filter((plan) => plan.status === "approved").map((plan) => [plan.id, plan.summary]);
    choose($("schedule-plan"), approved, initial.planId, approved.length ? "Elige un plan aprobado" : "No hay planes aprobados");
    view.ready = true;
    paint();
    await loadRecipes(role, initial.recipeId);
  } catch (error) {
    if (view === current) {
      showNotice(scheduleFailureText(error));
      paint();
    }
  }
}

// A refused save is worded above the form; a device without APPROVE loses the save button.
function refused(error) {
  if (error && error.kind === "forbidden") view.locked = true;
  showNotice(view.locked ? null : scheduleFailureText(error));
  paint();
}

async function submit(event) {
  event.preventDefault();
  if (!view?.ready) return;
  const form = readForm();
  const problems = formProblems(form, Date.now());
  if (!form.role) problems.role = "Elige un oficio del equipo.";
  if (needsPlan(form.steps) && !form.planId) problems.plan = PLAN_NEEDED_TEXT;
  if (Object.keys(problems).length) {
    showNotice(null);
    showProblems(problems);
    return;
  }
  const { schedule, onSaved } = view;
  const draft = payloadOf(form);
  view.busy = true;
  setBusy($("schedule-save"), true, "Guardando…");
  paint();
  try {
    const saved = schedule
      ? await invoke("schedule_update", { id: schedule.id, draft })
      : await invoke("schedule_create", { draft });
    closeEditor();
    onSaved(saved);
  } catch (error) {
    if (!view) return;
    view.busy = false;
    setBusy($("schedule-save"), false);
    refused(error);
  }
}

// Wired once, the first time the form opens.
function wire() {
  if (wired) return;
  wired = true;
  $("schedule-editor-lede").textContent = PC_OFF_NOTE;
  $("schedule-plan-hint").textContent = `${PLAN_NEEDED_TEXT} ${PLAN_EXTRA_TEXT}`;
  $("schedule-permission").textContent = PERMISSION_TEXT;
  $("schedule-kind").replaceChildren(...KINDS.map((kind) => new Option(kind.label, kind.value)));
  $("schedule-days").replaceChildren(...DAYS.map(dayBox));
  $("schedule-form").addEventListener("submit", submit);
  $("schedule-form").addEventListener("input", paint);
  $("schedule-form").addEventListener("change", paint);
  $("schedule-cancel").addEventListener("click", closeEditor);
  $("schedule-plans-link").addEventListener("click", () => {
    closeEditor();
    openPending();
  });
  $("schedule-role").addEventListener("change", () => loadRecipes(field("schedule-role"), ""));
  $("schedule-kind").addEventListener("change", preselectDay);
  $("schedule-date").addEventListener("change", preselectDay);
  $("schedule-editor").addEventListener("close", () => {
    view = null;
  });
}

function dayBox(day) {
  const label = element("label", "schedule-day");
  const input = document.createElement("input");
  input.type = "checkbox";
  input.value = day.code;
  label.append(input, element("span", null, day.name));
  return label;
}

// Opens the form for a new task, or for `schedule` to edit. `onSaved` gets the host's answer.
export function openScheduleEditor({ schedule, onSaved }) {
  wire();
  const initial = initialOf(schedule);
  view = { schedule, onSaved, ready: false, locked: false, busy: false, noProjects: false, recipes: new Map() };
  $("schedule-editor-title").textContent = schedule ? "Editar tarea" : "Programar una tarea";
  setBusy($("schedule-save"), false);
  fill(initial);
  showNotice(null);
  showProblems({});
  if (!$("schedule-editor").open) $("schedule-editor").showModal();
  paint();
  loadChoices(initial);
}
