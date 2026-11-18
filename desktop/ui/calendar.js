// «Calendario»: the screen and its entry points. The company's zone comes first: until it
// is fixed the list does not load and no event can be created. The list and the panel
// do the rest.

import { $, invoke } from "./core.js";
import { showView } from "./nav.js";
import { element } from "./chatdom.js";
import { reportWrite } from "./agenda-ui.js";
import { emptyState, errorState } from "./empty-state.js";
import { createCalendarList } from "./calendar-list.js";
import { createEventPanel } from "./calendar-form.js";
import { installIcs } from "./calendar-ics.js";
import { COMMON_ZONES, calendarErrorText, listFailureText, zoneProblem } from "./calendar-model.js";

const OFFLINE_HINT = "Sin conexión con tu PC: no puedes crear ni importar eventos todavía.";
const FAILED_HINT = "Sin zona horaria no puedes crear ni importar eventos todavía.";

let list = null;
let panel = null;
let zone = null;

// The zone row stays folded under «Zona horaria: …» and the new-event and .ics buttons wait for
// a zone. The header hint is only for a read that failed.
function paintZone(known, hint = "") {
  $("calendar-zone").hidden = !known;
  // Once a zone is set the row folds again; with none it stays folded until the operator opens it.
  if (zone) $("calendar-zone").open = false;
  $("calendar-zone-summary").textContent = `Zona horaria: ${zone || "sin fijar"} · Cambiar`;
  $("calendar-zone-hint").hidden = Boolean(zone) || !hint;
  $("calendar-zone-hint").textContent = hint;
  $("calendar-tools-hint").hidden = Boolean(zone);
  for (const id of ["btn-calendar-new", "btn-calendar-import", "btn-calendar-export"]) $(id).disabled = !zone;
  paintIdle();
}

// While no event is open, the right side says what comes next instead of staying empty.
function paintIdle() {
  const idle = $("calendar-idle");
  const open = !$("calendar-panel").hidden;
  idle.hidden = open;
  idle.replaceChildren(...(open ? [] : [idleContent()]));
}

// The explanation only: the list's own empty state carries the one action.
function idleContent() {
  const text = zone
    ? "Elige un evento de la lista para verlo aquí, o crea uno con «Nuevo evento»."
    : "Aquí verás cada evento al elegirlo.";
  return emptyState({ icon: "calendar", text });
}

async function loadZone() {
  try {
    zone = (await invoke("calendar_timezone")).timezone;
  } catch (failure) {
    zone = null;
    paintZone(false, failure?.kind === "offline" ? OFFLINE_HINT : FAILED_HINT);
    list.note(errorState(listFailureText(failure), loadZone));
    return;
  }
  paintZone(true);
  if (zone) list.reload(zone);
  else list.note(null);
}

async function saveZone() {
  const problem = $("calendar-zone-problem");
  const timezone = $("calendar-zone-input").value.trim();
  problem.hidden = true;
  const check = zoneProblem(timezone);
  if (check) {
    problem.textContent = check;
    problem.hidden = false;
    return;
  }
  const save = $("btn-calendar-zone");
  save.disabled = true;
  try {
    zone = (await invoke("calendar_set_timezone", { timezone })).timezone;
  } catch (failure) {
    save.disabled = false;
    reportWrite(failure, { fields: {}, problem, permission: "APPROVE", words: calendarErrorText });
    return;
  }
  paintZone(true);
  list.reload(zone);
}

export function openCalendar() {
  showView("calendar");
  panel.close();
  loadZone();
}

export function installCalendar() {
  for (const name of COMMON_ZONES) {
    const option = element("option");
    option.value = name;
    $("calendar-zones").append(option);
  }
  list = createCalendarList({
    onOpen: (event, companyZone) => {
      list.mark(event);
      panel.show(event, companyZone);
      paintIdle();
    },
  });
  panel = createEventPanel({
    onSaved: (event) => {
      list.upsert(event);
      list.mark(event);
    },
    onDeleted: (id) => list.remove(id),
    onClosed: () => {
      list.mark(null);
      paintIdle();
    },
  });
  installIcs({ visibleWindow: list.visibleWindow, refresh: list.refresh });
  $("btn-calendar-new").addEventListener("click", () => {
    panel.showNew(zone);
    paintIdle();
  });
  $("btn-calendar-zone").addEventListener("click", saveZone);
}
