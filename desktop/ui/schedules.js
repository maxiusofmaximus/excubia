// «Automatizar» in Equipo: the company's scheduled tasks, under the team's own cards. The list is
// read with the team's 5 s poll and after each change; it is repainted only when its text changes.
// Nothing runs by itself: «Lanzar ahora» asks the host once, and the wording says so.

import { $, invoke, toast } from "./core.js";
import { element, loadingBlock } from "./chatdom.js";
import { button, deleteConfirm } from "./agenda-ui.js";
import { emptyState, errorState } from "./empty-state.js";
import { statusMeta } from "./delegations.js";
import { openScheduleEditor } from "./schedule-editor.js";
import {
  PC_OFF_NOTE,
  PERMISSION_TEXT,
  lastRunText,
  nextRunText,
  outcomeLabel,
  scheduleFailureText,
  scheduleSentence,
  sortSchedules,
  statusLabel,
} from "./schedule-model.js";

const EMPTY_TEXT = "Pídele a tu equipo que haga algo cada semana o cada día.";
const LOAD_TEXT = "No se pudieron cargar las tareas. Inténtalo de nuevo.";

// `items` stays null until the first answer. `lines` is the last answer to «Lanzar ahora» by id;
// `runs` the history by id, present only while its panel is open.
const state = {
  items: null,
  roles: new Map(),
  error: null,
  notice: "",
  locked: false,
  busy: new Set(),
  lines: {},
  runs: {},
  confirm: null,
  inFlight: false,
  key: "",
};

function tagged(node, action, id) {
  node.dataset.scheduleAction = action;
  node.dataset.scheduleId = id;
  return node;
}

// Keeps keyboard focus on the same control after a repaint; the list is rebuilt when it changes.
function focusedControl() {
  const node = document.activeElement;
  return node?.dataset?.scheduleAction ? { action: node.dataset.scheduleAction, id: node.dataset.scheduleId } : null;
}

function restoreFocus(saved) {
  if (!saved) return;
  $("team-schedules-body")
    .querySelector(`[data-schedule-action="${saved.action}"][data-schedule-id="${saved.id}"]`)
    ?.focus();
}

// A panel is open from the moment it is asked for, even before its runs arrive (`null` then).
const isOpen = (id) => Object.hasOwn(state.runs, id);

function roleTitle(roleId) {
  return state.roles.get(roleId) || "Oficio";
}

function timesOf(item) {
  const times = element("dl", "schedule-times");
  for (const [label, value] of [
    ["Próxima", nextRunText(item.next_run_at, item.timezone)],
    ["Última", lastRunText(item.last_run_at, item.timezone)],
  ]) {
    const pair = element("div", "schedule-time");
    pair.append(element("dt", null, label), element("dd", null, value));
    times.append(pair);
  }
  return times;
}

// The switch that pauses or resumes; a finished task has none.
function switchOf(item) {
  const on = item.status === "active";
  const node = tagged(element("button", "schedule-switch", on ? "Activa" : "En pausa"), "switch", item.id);
  node.type = "button";
  node.setAttribute("role", "switch");
  node.setAttribute("aria-checked", String(on));
  node.disabled = state.locked || state.busy.has(item.id);
  node.addEventListener("click", () => setPaused(item, on));
  return node;
}

function actionsOf(item) {
  const row = element("div", "schedule-actions");
  const off = state.locked || state.busy.has(item.id);
  const run = tagged(button("Lanzar ahora", "btn secondary btn-small", () => runNow(item)), "run", item.id);
  const edit = tagged(button("Editar", "btn ghost btn-small", () => editSchedule(item)), "edit", item.id);
  const history = tagged(
    button(isOpen(item.id) ? "Ocultar historial" : "Historial", "btn ghost btn-small", () => toggleHistory(item)),
    "history",
    item.id,
  );
  const remove = tagged(button("Borrar", "btn ghost btn-small", () => askDelete(item)), "delete", item.id);
  for (const control of [run, edit, remove]) control.disabled = off;
  if (item.status === "done") {
    const done = element("span", "schedule-status", statusLabel("done"));
    row.append(done);
  } else {
    row.append(switchOf(item), run);
  }
  row.append(edit, history, remove);
  return row;
}

function runLine(id) {
  const line = state.lines[id];
  if (!line) return null;
  const node = element("p", line.error ? "schedule-line is-error" : "schedule-line", line.text);
  node.setAttribute("role", line.error ? "alert" : "status");
  return node;
}

function historyList(item) {
  const runs = state.runs[item.id];
  const box = element("div", "schedule-history");
  box.append(element("h4", "schedule-history-title", "Últimas ejecuciones"));
  if (!runs) {
    box.append(element("p", "schedule-muted", "Cargando el historial…"));
  } else if (runs.length === 0) {
    box.append(element("p", "schedule-muted", "Todavía no hay ejecuciones."));
  } else {
    const list = element("ul", "schedule-runs");
    list.setAttribute("role", "list");
    for (const run of runs) {
      list.append(element("li", null, `${nextRunText(run.due_at, item.timezone)} · ${outcomeLabel(run.outcome)}`));
    }
    box.append(list);
  }
  return box;
}

function rowFor(item) {
  const row = element("li", "schedule-row");
  const head = element("div", "schedule-head");
  head.append(element("h3", "schedule-name", item.title));
  row.append(head, element("p", "schedule-meta", `${roleTitle(item.role_id)} · ${scheduleSentence(item)}`));
  if (state.confirm?.id === item.id) {
    row.append(state.confirm.node);
    return row;
  }
  row.append(timesOf(item), actionsOf(item));
  const line = runLine(item.id);
  if (line) row.append(line);
  if (isOpen(item.id)) row.append(historyList(item));
  return row;
}

function bodyNodes() {
  if (state.items === null) {
    return state.error ? [errorState(LOAD_TEXT, refreshSchedules)] : [loadingBlock("Cargando tareas")];
  }
  if (state.items.length === 0) {
    return [emptyState({ icon: "clock", text: EMPTY_TEXT, action: "Programar una tarea" }, () => newSchedule())];
  }
  const list = element("ul", "schedule-list");
  list.setAttribute("role", "list");
  list.append(...sortSchedules(state.items).map(rowFor));
  return [list];
}

function paintNotice() {
  const text = state.notice || (state.locked ? PERMISSION_TEXT : "");
  const notice = $("team-schedules-notice");
  notice.textContent = text;
  notice.hidden = !text;
  $("team-schedules-new").disabled = state.locked;
}

// The view as text: a repaint that would show the same thing is skipped, so polling does not flicker.
function signature() {
  return JSON.stringify([state.items, state.error, state.notice, state.locked, [...state.busy], state.lines, state.runs, state.confirm?.id]);
}

function paint() {
  const key = signature();
  if (key === state.key) return;
  const saved = focusedControl();
  state.key = key;
  paintNotice();
  $("team-schedules-body").replaceChildren(...bodyNodes());
  restoreFocus(saved);
  $("team-schedules-count").textContent = state.items?.length ? `${state.items.length} en tu equipo` : "";
}

// Reads the list, and the oficio titles once; a failed read keeps the list already on screen.
// Called by the team's poll (team-summary.js) and after each change.
export async function refreshSchedules() {
  if (state.inFlight || !invoke) return;
  state.inFlight = true;
  try {
    if (state.roles.size === 0) {
      try {
        const catalog = await invoke("team_roles");
        state.roles = new Map(catalog.roles.map((role) => [role.id, role.title]));
      } catch {
        // The titles are cosmetic: a failed read must not hide the list. roleTitle() falls back until the next poll.
      }
    }
    state.items = await invoke("schedules");
    state.error = null;
    state.notice = "";
  } catch (error) {
    if (state.items) state.notice = scheduleFailureText(error);
    else state.error = error;
  } finally {
    state.inFlight = false;
    paint();
  }
}

// A write the host refused: a device without APPROVE locks the buttons; anything else is said above the list.
function writeFailed(error) {
  if (error && error.kind === "forbidden") state.locked = true;
  state.notice = state.locked ? "" : scheduleFailureText(error);
  paint();
}

function replaceItem(updated) {
  state.items = state.items.map((item) => (item.id === updated.id ? updated : item));
}

async function setPaused(item, on) {
  state.busy.add(item.id);
  try {
    replaceItem(await invoke("schedule_set_paused", { id: item.id, paused: on }));
    state.notice = "";
    toast(on ? "Tarea en pausa" : "Tarea activa");
  } catch (error) {
    writeFailed(error);
  } finally {
    state.busy.delete(item.id);
    paint();
  }
}

// «Lanzar ahora»: one run on the host's own rules. A skipped run is said as such, never queued.
async function runNow(item) {
  state.busy.add(item.id);
  state.lines[item.id] = { text: "Lanzando la tarea…" };
  paint();
  try {
    const launch = await invoke("schedule_run_now", { id: item.id });
    state.lines[item.id] = launch.run
      ? { text: outcomeLabel(launch.run.outcome), error: launch.run.outcome === "failed" }
      : { text: `Encargo lanzado · ${statusMeta(launch.status ?? "queued").label}` };
    if (isOpen(item.id) && state.runs[item.id]) state.runs[item.id] = await invoke("schedule_runs", { id: item.id });
  } catch (error) {
    if (error && error.kind === "forbidden") {
      state.locked = true;
      delete state.lines[item.id];
    } else {
      state.lines[item.id] = { text: scheduleFailureText(error), error: true };
    }
  } finally {
    state.busy.delete(item.id);
    paint();
  }
  refreshSchedules();
}

async function toggleHistory(item) {
  if (isOpen(item.id)) {
    delete state.runs[item.id];
    paint();
    return;
  }
  state.runs[item.id] = null;
  paint();
  try {
    state.runs[item.id] = await invoke("schedule_runs", { id: item.id });
  } catch (error) {
    delete state.runs[item.id];
    state.lines[item.id] = { text: scheduleFailureText(error), error: true };
  }
  paint();
}

function askDelete(item) {
  const { node, problem } = deleteConfirm({
    onConfirm: () => remove(item),
    onCancel: () => {
      state.confirm = null;
      paint();
    },
  });
  state.confirm = { id: item.id, node, problem };
  paint();
}

async function remove(item) {
  const { problem } = state.confirm;
  try {
    await invoke("schedule_delete", { id: item.id });
    state.confirm = null;
    state.items = state.items.filter((other) => other.id !== item.id);
    toast("Tarea borrada");
    paint();
  } catch (error) {
    if (error && error.kind === "forbidden") state.locked = true;
    problem.textContent = scheduleFailureText(error);
    problem.hidden = false;
    paint();
  }
}

function newSchedule() {
  openScheduleEditor({ schedule: null, onSaved: refreshSchedules });
}

function editSchedule(item) {
  openScheduleEditor({ schedule: item, onSaved: refreshSchedules });
}

export function installSchedules() {
  if (!$("team-schedules-body")) return;
  $("team-schedules-body").replaceChildren(loadingBlock("Cargando tareas"));
  $("team-schedules-new").addEventListener("click", newSchedule);
}
