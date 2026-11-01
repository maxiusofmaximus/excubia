// The list of «Calendario»: the events of one range, grouped by day in the company's zone.
// «‹» and «›» move the range; «Cargar más» pages with `after`. A stale page never
// replaces a newer one, and a row written here updates the list without a new read.

import { $, invoke } from "./core.js";
import { element } from "./chatdom.js";
import { button } from "./agenda-ui.js";
import {
  RANGES,
  emptyListText,
  eventRow,
  groupByDay,
  listFailureText,
  localDate,
  nextCursor,
  occurrenceOf,
  shiftAnchor,
  todayIn,
  windowLabel,
  windowOf,
} from "./calendar-model.js";

// An occurrence is one row; the series id alone would mark all of its rows.
const rowKey = (event) => `${event.id}@${occurrenceOf(event)}`;

export function createCalendarList({ onOpen }) {
  const list = $("calendar-list");
  const status = $("calendar-status");
  const more = $("calendar-more");
  let zone = null;
  let range = "thirty";
  let anchor = null;
  let items = [];
  let next = null;
  let token = 0;
  let selectedKey = null;

  function setStatus(text, again = null) {
    status.replaceChildren();
    status.hidden = !text;
    if (!text) return;
    status.append(text);
    if (again) status.append(" ", button("Reintentar", "btn secondary btn-small", () => again()));
  }

  function eventItem(event) {
    const data = eventRow(event, zone);
    const item = element("li");
    const row = button("", "agenda-row", () => onOpen(event, zone));
    if (rowKey(event) === selectedKey) row.setAttribute("aria-current", "true");
    const meta = element("span", "agenda-row-meta", [data.time, data.place].filter(Boolean).join(" · "));
    if (data.reminder) meta.append(" ", element("span", "agenda-badge", "Aviso"));
    if (data.repeats) meta.append(" ", element("span", "agenda-badge", "Se repite"));
    row.append(element("span", "agenda-row-title", data.title), meta);
    item.append(row);
    return item;
  }

  function rows() {
    return groupByDay(items, zone).flatMap((group) => [
      element("li", "agenda-day", group.label),
      ...group.events.map(eventItem),
    ]);
  }

  function render() {
    list.replaceChildren(...rows());
    more.hidden = !next;
    setStatus(items.length ? "" : emptyListText(Boolean(next)));
  }

  function drawRange() {
    $("calendar-range-label").textContent = windowLabel(windowOf(range, anchor));
    for (const entry of RANGES) {
      $(`calendar-range-${entry.id}`).setAttribute("aria-pressed", String(entry.id === range));
    }
  }

  function fetchPage(after) {
    const { from, to } = windowOf(range, anchor);
    return invoke("calendar_events", { from, to, after });
  }

  function note(text, again) {
    items = [];
    next = null;
    list.replaceChildren();
    more.hidden = true;
    setStatus(text, again);
  }

  async function load() {
    const current = ++token;
    note("Cargando eventos…");
    drawRange();
    try {
      const page = await fetchPage(null);
      if (current !== token) return;
      items = page.events;
      next = nextCursor(page);
      render();
    } catch (failure) {
      if (current === token) note(listFailureText(failure), load);
    }
  }

  async function loadMore() {
    if (!next) return;
    const current = token;
    more.disabled = true;
    try {
      const page = await fetchPage(next);
      if (current !== token) return;
      items = [...items, ...page.events];
      next = nextCursor(page);
      render();
    } catch (failure) {
      if (current === token) setStatus(listFailureText(failure), loadMore);
    } finally {
      more.disabled = false;
    }
  }

  function pick(id) {
    if (!zone) return;
    range = id;
    anchor = todayIn(zone);
    load();
  }

  function step(count) {
    if (!zone) return;
    anchor = shiftAnchor(range, anchor, count);
    load();
  }

  // Opens the list on the range that holds today, in the company's zone.
  function reload(companyZone) {
    zone = companyZone;
    anchor = todayIn(zone);
    load();
  }

  // A saved series comes back as one event, so its rows are read again instead.
  function upsert(event) {
    if (event.recurrence || items.some((item) => item.id === event.id && item.recurring)) {
      load();
      return;
    }
    const { from, to } = windowOf(range, anchor);
    const day = localDate(occurrenceOf(event), zone);
    items = items.filter((item) => item.id !== event.id);
    if (day >= from && day < to) {
      items = [...items, event].sort((a, b) => occurrenceOf(a).localeCompare(occurrenceOf(b)));
    }
    render();
  }

  function remove(id) {
    items = items.filter((item) => item.id !== id);
    render();
  }

  // Marks the open row (null: none); the status line keeps whatever it says.
  function mark(event) {
    selectedKey = event ? rowKey(event) : null;
    list.replaceChildren(...rows());
  }

  for (const entry of RANGES) {
    $(`calendar-range-${entry.id}`).addEventListener("click", () => pick(entry.id));
  }
  $("calendar-prev").addEventListener("click", () => step(-1));
  $("calendar-next").addEventListener("click", () => step(1));
  more.addEventListener("click", loadMore);

  return {
    reload,
    note,
    upsert,
    remove,
    mark,
    refresh: load,
    visibleWindow: () => windowOf(range, anchor),
  };
}
