// «Calendario»: the screen and its entry points. The company's zone comes first: until it
// is fixed the list does not load and no event can be created. The list and the panel
// do the rest.

import { $, invoke, showView } from "./core.js";
import { element } from "./chatdom.js";
import { reportWrite } from "./agenda-ui.js";
import { createCalendarList } from "./calendar-list.js";
import { createEventPanel } from "./calendar-form.js";
import { installIcs } from "./calendar-ics.js";
import { COMMON_ZONES, calendarErrorText, listFailureText, zoneProblem } from "./calendar-model.js";

const ZONE_NOTE = "Fija la zona horaria de la empresa para ver los eventos.";

let list = null;
let panel = null;
let zone = null;

// The zone form shows while the company has none; the new-event and .ics buttons wait for one.
function paintZone(known) {
  $("calendar-zone").hidden = !known || Boolean(zone);
  $("calendar-zone-hint").hidden = !known || Boolean(zone);
  for (const id of ["btn-calendar-new", "btn-calendar-import", "btn-calendar-export"]) $(id).disabled = !zone;
}

async function loadZone() {
  try {
    zone = (await invoke("calendar_timezone")).timezone;
  } catch (failure) {
    zone = null;
    paintZone(false);
    list.note(listFailureText(failure), loadZone);
    return;
  }
  paintZone(true);
  if (zone) list.reload(zone);
  else list.note(ZONE_NOTE);
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
    },
  });
  panel = createEventPanel({
    onSaved: (event) => {
      list.upsert(event);
      list.mark(event);
    },
    onDeleted: (id) => list.remove(id),
    onClosed: () => list.mark(null),
  });
  installIcs({ visibleWindow: list.visibleWindow, refresh: list.refresh });
  $("btn-calendar-new").addEventListener("click", () => panel.showNew(zone));
  $("btn-calendar-zone").addEventListener("click", saveZone);
}
