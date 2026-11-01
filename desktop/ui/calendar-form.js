// The panel of «Calendario»: one event read, edited or created. Times are shown and typed
// in the company's zone; the host turns them into instants. Delete asks first and stays
// off, with its reason, when the device lacks APPROVE.

import { $, invoke } from "./core.js";
import { element } from "./chatdom.js";
import {
  actionRow,
  approveReason,
  button,
  closeOnEscape,
  deleteAction,
  deleteConfirm,
  detailLine,
  emptyPanel,
  field,
  fillPanel,
  notice,
  panelParts,
  removeRow,
  reportWrite,
} from "./agenda-ui.js";
import {
  calendarErrorText,
  deleteQuestion,
  eventProblem,
  formValues,
  reminderChoices,
  reminderFromSelect,
  reminderLabel,
  whenText,
} from "./calendar-model.js";
import { ruleText } from "./calendar-recurrence-model.js";
import { recurrenceBlock } from "./calendar-recurrence.js";

// A day becomes the start of that day when the event turns timed; a time keeps its day.
function inputValue(value, allDay) {
  if (!value) return "";
  if (allDay) return value.slice(0, 10);
  return value.length === 10 ? `${value}T09:00` : value;
}

// The whole-day switch retypes the two dates: a day picker or a date and time.
function retype(fields, allDay) {
  for (const entry of [fields.start, fields.end]) {
    entry.control.type = allDay ? "date" : "datetime-local";
    entry.control.value = inputValue(entry.control.value, allDay);
  }
}

function reminderSelect(current) {
  const select = element("select", "agenda-select");
  select.id = "calendar-reminder";
  for (const choice of reminderChoices(current)) {
    const option = element("option", null, choice.label);
    option.value = choice.minutes === null ? "" : String(choice.minutes);
    select.append(option);
  }
  select.value = current === null ? "" : String(current);
  return select;
}

// The inputs of the form, their captions in reading order, and the controls a save reads.
function formControls(event, schedule) {
  const fields = {
    title: field({ id: "calendar-title", label: "Título", value: event?.title || "" }),
    start: field({
      id: "calendar-start",
      label: "Inicio",
      type: schedule.allDay ? "date" : "datetime-local",
      value: schedule.start,
    }),
    end: field({
      id: "calendar-end",
      label: "Fin (opcional)",
      type: schedule.allDay ? "date" : "datetime-local",
      value: schedule.end,
    }),
    location: field({ id: "calendar-location", label: "Lugar", value: event?.location || "" }),
    notes: field({ id: "calendar-notes", label: "Notas", value: event?.notes || "", multiline: true, rows: 4 }),
  };
  fields.recurrence = recurrenceBlock({ rule: event?.recurrence ?? null, startOf: () => fields.start.control.value });
  const allDay = element("input");
  allDay.type = "checkbox";
  allDay.id = "calendar-allday";
  allDay.checked = schedule.allDay;
  allDay.addEventListener("change", () => retype(fields, allDay.checked));
  const allDayLabel = element("label", "agenda-check");
  allDayLabel.htmlFor = allDay.id;
  allDayLabel.append(allDay, " Todo el día");

  const reminder = reminderSelect(event?.reminder_minutes ?? null);
  const reminderCaption = element("label", "agenda-label", "Aviso");
  reminderCaption.htmlFor = reminder.id;
  const reminderBox = element("div", "agenda-field");
  reminderBox.append(reminderCaption, reminder);

  const nodes = [
    fields.title.node,
    allDayLabel,
    fields.start.node,
    fields.end.node,
    fields.location.node,
    reminderBox,
    fields.recurrence.node,
    fields.notes.node,
  ];
  return { fields, allDay, reminder, nodes };
}

export function createEventPanel({ onSaved, onDeleted, onClosed }) {
  const parts = panelParts("calendar");
  let zone = null;

  function close() {
    emptyPanel(parts);
    onClosed();
  }

  function show(event, companyZone = zone) {
    zone = companyZone;
    const edit = button("Editar", "btn secondary btn-small", () => editEvent(event));
    const notes = event.notes ? [element("p", "agenda-notes", event.notes)] : [];
    const repeats = event.recurrence ? [detailLine("Repite", ruleText(event.recurrence))] : [];
    fillPanel(parts, event.title, [
      detailLine("Cuándo", whenText(event, zone)),
      ...repeats,
      ...(event.location ? [detailLine("Lugar", event.location)] : []),
      detailLine("Aviso", reminderLabel(event.reminder_minutes)),
      ...notes,
      actionRow(edit, deleteAction(() => showDelete(event))),
      approveReason(),
    ]);
    edit.focus();
  }

  // An occurrence of a series edits the series: its start is the series', not the occurrence's.
  async function editEvent(event) {
    if (!event.recurring) return showForm(event);
    try {
      showForm(await invoke("calendar_get", { id: event.id }));
    } catch (failure) {
      const back = button("Volver", "btn ghost btn-small", () => show(event));
      fillPanel(parts, event.title, [notice(calendarErrorText(failure)), actionRow(back)]);
    }
  }

  function showNew(companyZone) {
    zone = companyZone;
    showForm(null);
  }

  function showForm(event) {
    const schedule = event ? formValues(event, zone) : { allDay: false, start: "", end: "" };
    const form = formControls(event, schedule);
    const problem = notice("");
    const save = button(event ? "Guardar" : "Crear evento", "btn primary btn-small", () =>
      submit(event, form, problem, save),
    );
    const cancel = button("Cancelar", "btn ghost btn-small", () => (event ? show(event) : close()));
    fillPanel(parts, event ? "Editar evento" : "Nuevo evento", [...form.nodes, problem, actionRow(cancel, save)]);
    form.fields.title.control.focus();
  }

  async function submit(event, form, problem, save) {
    const repeat = form.fields.recurrence.read();
    const draft = {
      title: form.fields.title.control.value.trim(),
      start: form.fields.start.control.value,
      end: form.fields.end.control.value || null,
      all_day: form.allDay.checked,
      location: form.fields.location.control.value.trim(),
      notes: form.fields.notes.control.value.trim(),
      reminder_minutes: reminderFromSelect(form.reminder.value),
      recurrence: repeat.rule ?? null,
    };
    const check = repeat.problem ? { field: "recurrence", message: repeat.problem } : eventProblem(draft);
    Object.values(form.fields).forEach((entry) => entry.setError(""));
    if (check) {
      form.fields[check.field].setError(check.message);
      return;
    }
    save.disabled = true;
    try {
      const saved = event
        ? await invoke("calendar_update", { id: event.id, draft })
        : await invoke("calendar_create", { draft });
      onSaved(saved);
      show(saved);
    } catch (failure) {
      save.disabled = false;
      reportWrite(failure, { fields: form.fields, problem, permission: "RESPOND", words: calendarErrorText });
    }
  }

  function showDelete(event) {
    const confirm = deleteConfirm({
      question: deleteQuestion(event),
      onConfirm: (problem) =>
        removeRow({
          command: "calendar_delete",
          id: event.id,
          problem,
          onDone: () => {
            onDeleted(event.id);
            close();
          },
          onDenied: () => show(event),
        }),
      onCancel: () => show(event),
    });
    fillPanel(parts, `Borrar «${event.title}»`, [confirm.node]);
  }

  $("calendar-panel-close").addEventListener("click", close);
  closeOnEscape("view-calendar", parts, close);

  return { show, showNew, close };
}
