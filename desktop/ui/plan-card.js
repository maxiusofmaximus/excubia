// One «Plan propuesto» card: the host's sentence, its numbered steps and what they touch, then
// the answers. Each answer opens one step inside the card; Esc goes back to the three answers.
// `decide(action, args)` resolves with the host's answer or rejects with its failure; the card
// words a refusal in its own slot.

import { button } from "./agenda-ui.js";
import { element } from "./chatdom.js";
import { setBusy } from "./core.js";
import { confirmApproveText, needsExtraConfirm, noteCount, noteError, planFailureText } from "./plan-model.js";

const BUSY = {
  approve: "Aprobando…",
  reject: "Rechazando…",
  request_changes: "Enviando…",
};

function actionRow(nodes) {
  const row = element("div", "plan-actions");
  row.append(...nodes);
  return row;
}

function stepNode(step) {
  const item = element("li", "plan-step");
  item.append(element("span", "plan-step-title", step.title), element("span", "plan-step-touch", step.labels.join(" · ")));
  return item;
}

function headNodes(view) {
  const nodes = [element("p", "plan-title", view.summary), element("p", "plan-lede", view.lede)];
  if (view.decided) nodes.push(element("p", "plan-status", view.statusLabel));
  if (view.note) nodes.push(element("p", "plan-note", `Tu nota: ${view.note}`));
  return nodes;
}

export function planCard(plan, view, { readOnly, decide }) {
  const card = element("li", view.decided ? "plan-card is-decided" : "plan-card");
  card.dataset.plan = view.id;
  const steps = element("ol", "plan-steps");
  steps.append(...view.steps.map(stepNode));
  const slot = element("div", "plan-slot");
  slot.setAttribute("role", "alert");
  slot.hidden = true;
  const form = element("div", "plan-form");
  card.append(...headNodes(view), steps, form, slot);
  if (view.decided) return card;

  let controls = [];

  // The form holds either the three answers or one step; a step replaces the answers and back again.
  const mount = (nodes, focus) => {
    form.replaceChildren(...nodes);
    controls = [...form.querySelectorAll("button, textarea")];
    for (const control of controls) control.disabled = readOnly;
    slot.hidden = true;
    (focus ?? controls[0])?.focus();
  };
  const showText = (text) => {
    slot.textContent = text;
    slot.hidden = false;
  };
  const failWith = (error) => {
    setBusy(busyControl, false);
    for (const control of controls) control.disabled = readOnly;
    showText(planFailureText(error));
  };
  let busyControl = null;
  const send = async (name, args, control) => {
    busyControl = control;
    slot.hidden = true;
    setBusy(control, true, BUSY[name]);
    for (const item of controls) item.disabled = true;
    try {
      await decide(name, args);
    } catch (error) {
      failWith(error);
    }
  };

  const answers = () => {
    const approve = button("Aprobar", "btn primary", () =>
      needsExtraConfirm(plan) ? confirmStep() : send("approve", { id: plan.id }, approve),
    );
    const change = button("Cambiar…", "btn secondary", () => changesStep());
    const reject = button("Rechazar", "btn ghost plan-danger", () => rejectStep());
    mount([actionRow([approve, change, reject])], approve);
  };

  const confirmStep = () => {
    const yes = button("Aprobar igualmente", "btn primary", () => send("approve", { id: plan.id }, yes));
    const back = button("Volver", "btn ghost", answers);
    mount([element("p", "plan-ask", confirmApproveText()), actionRow([yes, back])], yes);
  };

  const changesStep = () => {
    const area = element("textarea", "plan-note-field");
    area.rows = 3;
    area.setAttribute("aria-label", "Qué cambiarías del plan");
    const count = element("p", "hint plan-count", noteCount(""));
    area.addEventListener("input", () => {
      count.textContent = noteCount(area.value);
    });
    const submit = button("Enviar cambios", "btn primary", () => {
      const error = noteError(area.value);
      if (error) return showText(error);
      return send("request_changes", { id: plan.id, note: area.value.trim() }, submit);
    });
    const back = button("Cancelar", "btn ghost", answers);
    mount([element("p", "plan-ask", "¿Qué cambiarías?"), area, count, actionRow([submit, back])], area);
  };

  const rejectStep = () => {
    const yes = button("Rechazar plan", "btn secondary plan-danger", () => send("reject", { id: plan.id }, yes));
    const back = button("Volver", "btn ghost", answers);
    mount([element("p", "plan-ask", "¿Rechazar este plan? No se hará nada de lo propuesto."), actionRow([yes, back])], yes);
  };

  card.addEventListener("keydown", (event) => {
    if (event.key !== "Escape" || form.querySelector(".plan-ask") === null) return;
    event.preventDefault();
    answers();
  });

  answers();
  return card;
}
