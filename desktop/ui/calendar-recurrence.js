// The «Repetir» block of the event form: how often, every how many, which days of the week
// and when it ends. The rules live in calendar-recurrence-model.js; this file draws them and
// keeps the state the form reads back on save.

import { element } from "./chatdom.js";
import { notice } from "./agenda-ui.js";
import { DAYS, REPEATS, formState, ruleFromForm, ruleText, setRepeat, unitText } from "./calendar-recurrence-model.js";

const END_OPTIONS = [
  { value: "never", label: "Nunca" },
  { value: "count", label: "Después de N veces" },
  { value: "until", label: "Hasta una fecha" },
];

function labelled(text, control) {
  const caption = element("label", "agenda-label", text);
  caption.htmlFor = control.id;
  const box = element("div", "agenda-field");
  box.append(caption, control);
  return box;
}

function select(id, options) {
  const node = element("select", "agenda-select");
  node.id = id;
  for (const choice of options) {
    const option = element("option", null, choice.label);
    option.value = choice.value;
    node.append(option);
  }
  return node;
}

function numberInput(id, max) {
  const input = element("input");
  input.type = "number";
  input.id = id;
  input.min = "1";
  input.max = String(max);
  input.step = "1";
  return input;
}

function dayChecks() {
  const node = element("div", "agenda-days");
  const boxes = new Map();
  for (const day of DAYS) {
    const box = element("input");
    box.type = "checkbox";
    box.id = `calendar-day-${day.code}`;
    box.setAttribute("aria-label", day.name);
    const label = element("label", "agenda-check", day.short);
    label.htmlFor = box.id;
    label.prepend(box);
    node.append(label);
    boxes.set(day.code, box);
  }
  return { node, boxes };
}

// A custom rule keeps its option, so picking it again brings the rule back.
const repeatOptions = (state) =>
  state.repeat === "custom" ? [...REPEATS, { value: "custom", label: "Repetición personalizada" }] : REPEATS;

// `startOf` gives the form's start as it is typed, so a weekly repeat can take its day.
export function recurrenceBlock({ rule, startOf }) {
  let state = formState(rule, startOf());
  const repeat = select("calendar-repeat", repeatOptions(state));
  const readOnly = element("p", "agenda-muted", "Repetición personalizada");
  const every = numberInput("calendar-every", 999);
  const unit = element("span", "agenda-muted");
  const everyBox = labelled("Cada", every);
  everyBox.append(unit);
  const days = dayChecks();
  const daysBox = element("div", "agenda-field");
  daysBox.append(element("span", "agenda-label", "Días"), days.node);
  const end = select("calendar-end", END_OPTIONS);
  const endBox = labelled("Termina", end);
  const count = numberInput("calendar-count", 1000);
  const countBox = labelled("Número de veces", count);
  const until = element("input");
  until.type = "date";
  until.id = "calendar-until";
  const untilBox = labelled("Fecha de fin", until);
  const summary = element("p", "agenda-muted");
  const hint = element("p", "agenda-hint", "Al editar o borrar se cambia toda la serie.");
  const problem = notice("");
  const node = element("div", "agenda-recurrence");
  node.append(labelled("Repetir", repeat), readOnly, everyBox, daysBox, endBox, countBox, untilBox, summary, hint, problem);

  function paint() {
    const repeating = state.repeat !== "none" && state.repeat !== "custom";
    repeat.value = state.repeat;
    readOnly.hidden = state.repeat !== "custom";
    everyBox.hidden = !repeating;
    endBox.hidden = !repeating;
    summary.hidden = !repeating;
    hint.hidden = !repeating;
    daysBox.hidden = state.repeat !== "WEEKLY";
    countBox.hidden = !repeating || state.end !== "count";
    untilBox.hidden = !repeating || state.end !== "until";
    unit.textContent = repeating ? unitText(state.repeat, Number(state.every)) : "";
    every.value = state.every;
    end.value = state.end;
    count.value = state.count;
    until.value = state.until;
    for (const day of DAYS) days.boxes.get(day.code).checked = state.days.includes(day.code);
    const built = ruleFromForm(state, startOf());
    summary.textContent = built.rule ? ruleText(built.rule) : "";
  }

  const update = (patch) => {
    state = { ...state, ...patch };
    paint();
  };
  repeat.addEventListener("change", () => {
    state = setRepeat(state, repeat.value, startOf());
    paint();
  });
  every.addEventListener("change", () => update({ every: every.value }));
  end.addEventListener("change", () => update({ end: end.value }));
  count.addEventListener("change", () => update({ count: count.value }));
  until.addEventListener("change", () => update({ until: until.value }));
  for (const day of DAYS) {
    days.boxes.get(day.code).addEventListener("change", () =>
      update({ days: DAYS.map((item) => item.code).filter((code) => days.boxes.get(code).checked) }),
    );
  }
  paint();

  return {
    node,
    read: () => ruleFromForm(state, startOf()),
    setError(text) {
      problem.textContent = text || "";
      problem.hidden = !text;
    },
  };
}
