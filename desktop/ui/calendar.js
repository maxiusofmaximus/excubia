// «Calendario»: the screen and its entry points. The company's zone comes first: until it
// is fixed the list does not load and no event can be created. The list and the panel
// do the rest.

import { t } from "./i18n.js";
import { $, invoke } from "./core.js";
import { showView } from "./nav.js";
import { element } from "./chatdom.js";
import { reportWrite } from "./agenda-ui.js";
import { emptyState, errorState } from "./empty-state.js";
import { createCalendarList } from "./calendar-list.js";
import { createEventPanel } from "./calendar-form.js";
import { installIcs } from "./calendar-ics.js";
import { COMMON_ZONES, RANGES, calendarErrorText, listFailureText, zoneProblem } from "./calendar-model.js";

const OFFLINE_HINT = t("calendar.offline_hint");
const FAILED_HINT = t("calendar.failed_hint");
// Without a zone these do nothing (calendar-list.js returns early), so they stay off.
const ZONE_BUTTONS = [
  "btn-calendar-new",
  "btn-calendar-import",
  "btn-calendar-export",
  "calendar-prev",
  "calendar-next",
  ...RANGES.map((range) => `calendar-range-${range.id}`),
];

let list = null;
let panel = null;
let zone = null;

// The zone row stays folded under «Zona horaria: …» and the new-event and .ics buttons wait for
// a zone. The header hint is only for a read that failed.
function paintZone(known, hint = "") {
  $("calendar-zone").hidden = !known;
  // Once a zone is set the row folds again; with none it stays folded until the operator opens it.
  if (zone) $("calendar-zone").open = false;
  $("calendar-zone-summary").textContent = t("calendar.zone_summary", { zone: zone || t("calendar.zone_unset") });
  $("calendar-zone-hint").hidden = Boolean(zone) || !hint;
  $("calendar-zone-hint").textContent = hint;
  $("calendar-tools-hint").hidden = Boolean(zone);
  for (const id of ZONE_BUTTONS) $(id).disabled = !zone;
  // No range is chosen while the list cannot load; the 30-day default does not look pressed.
  if (!zone) for (const range of RANGES) $(`calendar-range-${range.id}`).setAttribute("aria-pressed", "false");
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
    ? t("calendar.idle_with_zone", { button: t("calendar.new") })
    : t("calendar.idle_no_zone");
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
  // A correction makes the empty-zone message stale, so it goes; no check runs until Save.
  $("calendar-zone-input").addEventListener("input", () => {
    $("calendar-zone-problem").hidden = true;
  });
}
