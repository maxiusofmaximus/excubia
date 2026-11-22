// The pieces both agenda screens share: a labelled field that shows its own problem,
// the panel beside the list, the buttons, the delete that asks first, and the grant
// that switches delete off when the device lacks APPROVE.

import { $, invoke } from "./core.js";
import { element } from "./chatdom.js";
import { agendaErrorText, permissionText } from "./agenda-model.js";

// Once the host refuses APPROVE, the screen keeps delete off until it is reloaded.
export const grants = { canApprove: true };

export function field({ id, label, value = "", multiline = false, rows = 3, type = "text" }) {
  const control = element(multiline ? "textarea" : "input");
  if (multiline) control.rows = rows;
  else control.type = type;
  control.id = id;
  control.value = value;
  control.autocomplete = "off";
  control.spellcheck = false;

  const caption = element("label", "agenda-label", label);
  caption.htmlFor = id;
  const error = element("p", "agenda-error");
  error.id = `${id}-error`;
  error.hidden = true;
  control.setAttribute("aria-describedby", error.id);

  const node = element("div", "agenda-field");
  node.append(caption, control, error);
  return {
    node,
    control,
    setError(text) {
      error.textContent = text || "";
      error.hidden = !text;
      control.setAttribute("aria-invalid", text ? "true" : "false");
    },
  };
}

export function button(label, className, onClick) {
  const node = element("button", className, label);
  node.type = "button";
  node.addEventListener("click", onClick);
  return node;
}

// A line that says what happened; hidden while there is nothing to say.
export function notice(text, className = "agenda-error") {
  const node = element("p", className, text || "");
  node.hidden = !text;
  node.setAttribute("role", "status");
  return node;
}

// A label and its value on one line of a panel.
export function detailLine(label, value) {
  const line = element("p", "agenda-detail-line");
  line.append(element("span", "agenda-label", label), " ", value);
  return line;
}

export function actionRow(...nodes) {
  const row = element("div", "agenda-actions");
  row.append(...nodes);
  return row;
}

// The delete button of a row, off with its reason when the device lacks APPROVE.
export function deleteAction(onClick) {
  const remove = button("Borrar", "btn ghost btn-small", onClick);
  remove.disabled = !grants.canApprove;
  return remove;
}

export function approveReason() {
  return notice(grants.canApprove ? "" : permissionText("APPROVE"), "agenda-hint");
}

// The delete asks first; «Se borra para siempre» is the sentence the operator reads by default.
export function deleteConfirm({ onConfirm, onCancel, question = "Se borra para siempre. No se puede deshacer." }) {
  const box = element("div", "agenda-confirm");
  box.setAttribute("role", "alertdialog");
  const text = element("p", null, question);
  const problem = notice("");
  const confirm = button("Borrar", "btn danger btn-small", () => onConfirm(problem));
  const cancel = button("Cancelar", "btn ghost btn-small", onCancel);
  box.append(text, actionRow(cancel, confirm), problem);
  return { node: box, problem };
}

// Runs a delete command. A refused grant turns delete off and calls `onDenied`; any
// other refusal is written into the confirmation's notice.
export async function removeRow({ command, id, problem, onDone, onDenied }) {
  try {
    await invoke(command, { id });
    onDone();
  } catch (failure) {
    if (failure.kind === "forbidden") {
      grants.canApprove = false;
      onDenied();
      return;
    }
    problem.textContent = agendaErrorText(failure);
    problem.hidden = false;
  }
}

// The panel of a screen: `<prefix>-panel`, its heading and its body, all in the page.
export function panelParts(prefix) {
  return {
    section: $(`${prefix}-panel`),
    heading: $(`${prefix}-panel-title`),
    body: $(`${prefix}-panel-body`),
  };
}

export function fillPanel(parts, title, nodes) {
  parts.heading.textContent = title;
  parts.body.replaceChildren(...nodes);
  parts.section.hidden = false;
}

export function emptyPanel(parts) {
  parts.section.hidden = true;
  parts.body.replaceChildren();
}

// Escape closes the panel while its screen is the one on show.
export function closeOnEscape(viewId, parts, close) {
  document.addEventListener("keydown", (event) => {
    const open = !parts.section.hidden && !$(viewId).classList.contains("hidden");
    if (event.key === "Escape" && open) close();
  });
}

// Runs `run` once the typing stops for a moment.
export function debounced(run, ms = 250) {
  let timer = null;
  return () => {
    clearTimeout(timer);
    timer = setTimeout(run, ms);
  };
}

// Shows a refused write where the operator can read it: on the field the host
// named, or in the panel's notice. A missing grant names the permission.
export function reportWrite(failure, { fields, problem, permission, words = agendaErrorText }) {
  problem.hidden = true;
  Object.values(fields).forEach((entry) => entry.setError(""));
  if (failure.kind === "forbidden") {
    problem.textContent = permissionText(permission);
    problem.hidden = false;
    return;
  }
  const text = words(failure);
  const target = failure.field && fields[failure.field];
  if (target && failure.kind === "agendaRefused") {
    target.setError(text);
    return;
  }
  problem.textContent = text;
  problem.hidden = false;
}
