// «Copias del turno»: the dialog of one agent. It lists the copies the host kept, shows
// what a copy would change, and puts the folder back only after a confirmation. Each step
// is one view of the same dialog; a refused call words its message inside the step that made it.

import { $, invoke, setBusy } from "./core.js";
import { element } from "./chatdom.js";
import { emptyState, errorState } from "./empty-state.js";
import { grantNote } from "./hires.js";
import { readStateText } from "./hires-model.js";
import {
  CONFIRM_WARNING,
  changeLabel,
  confirmText,
  doneText,
  excludedSentence,
  filesText,
  pointAge,
  pointLabel,
  pointTime,
  restoreErrorText,
  sortPoints,
} from "./restore-model.js";

const RESTORE_GRANT_LEDE =
  "Para volver a una copia, este dispositivo necesita el permiso APPROVE. Concédelo desde el PC:";

const session = { agent: "", changes: null, point: null, undo: null, slot: null };

function show(nodes) {
  $("restore-body").replaceChildren(...nodes);
}

// A step opens on its own title, focused, so a screen reader announces where it is.
function step(title, nodes) {
  const heading = element("h3", "restore-step-title", title);
  heading.tabIndex = -1;
  show([heading, ...nodes]);
  heading.focus();
}

function button(text, className, onClick) {
  const node = element("button", className, text);
  node.type = "button";
  node.addEventListener("click", onClick);
  return node;
}

function actionRow(buttons) {
  const row = element("div", "dialog-actions restore-actions");
  row.append(...buttons);
  return row;
}

function loadingBlock() {
  const block = element("div", "loading-list");
  block.setAttribute("aria-label", "Cargando copias");
  block.append(element("div", "skeleton-row"), element("div", "skeleton-row"));
  return block;
}

// A read the host refused: no permission yet is a wait, anything else can be read again.
function readFailure(error, retry) {
  const waiting = readStateText(error);
  if (waiting) return element("p", "hint", waiting);
  return errorState(restoreErrorText(error), retry);
}

function pointButton(point) {
  const item = element("li");
  const row = element("button", "restore-point");
  row.type = "button";
  const text = element("span", "restore-point-text");
  text.append(
    element("span", "restore-point-title", `${pointLabel(point)} · ${pointTime(point.created_ms)}`),
    element("span", "restore-point-meta", `${pointAge(point.created_ms, Date.now())} · ${filesText(point.files)}`),
  );
  const chevron = element("span", "restore-chevron", "›");
  chevron.setAttribute("aria-hidden", "true");
  row.append(text, chevron);
  row.addEventListener("click", () => openPoint(point));
  item.append(row);
  return item;
}

function listNodes(points) {
  if (points.length === 0) {
    return [emptyState({ icon: "restore", text: "Aún no hay copias de este agente. Se guarda una al empezar cada turno.", action: "Actualizar" }, loadList)];
  }
  const list = element("ul", "restore-list");
  list.setAttribute("role", "list");
  list.append(...sortPoints(points).map(pointButton));
  return [list];
}

async function loadList() {
  show([loadingBlock()]);
  try {
    const { points } = await invoke("restore_points", { agent: session.agent });
    show(listNodes(points));
  } catch (error) {
    show([readFailure(error, loadList)]);
  }
}

function changeItem(change) {
  const item = element("li", "restore-change");
  const tag = element("span", `restore-tag is-${change.status}`, changeLabel(change.status));
  item.append(element("span", "restore-path", change.path), tag);
  return item;
}

function detailNodes(point, changes) {
  const nodes = [element("p", "restore-meta", `${pointLabel(point)} · ${pointTime(point.created_ms)} · ${pointAge(point.created_ms, Date.now())}`)];
  if (changes.changes.length === 0) {
    nodes.push(element("p", "hint", "Esta copia coincide con la carpeta de ahora: no hay nada que volver a poner."));
  } else {
    const list = element("ul", "restore-changes");
    list.setAttribute("role", "list");
    list.append(...changes.changes.map(changeItem));
    nodes.push(list);
  }
  const excluded = excludedSentence(changes.excluded);
  if (excluded) nodes.push(element("p", "hint", excluded));
  const buttons = [button("Volver a la lista", "btn ghost", loadList)];
  if (changes.changes.length > 0) buttons.push(button("Volver a este punto", "btn primary", confirmStep));
  nodes.push(actionRow(buttons));
  return nodes;
}

async function openPoint(point) {
  session.point = point;
  show([loadingBlock()]);
  try {
    session.changes = await invoke("restore_changes", { point: point.id });
    step("Cambios de esta copia", detailNodes(point, session.changes));
  } catch (error) {
    show([readFailure(error, () => openPoint(point))]);
  }
}

// The slot is where a refused call says why; it stays empty until one does.
function confirmStep() {
  const count = session.changes.changes.length;
  session.slot = element("div", "restore-slot");
  step("Volver a este punto", [
    element("p", "dialog-lede", confirmText(count)),
    element("p", "hint", CONFIRM_WARNING),
    session.slot,
    actionRow([
      button("Volver", "btn ghost", () => openPoint(session.point)),
      button("Volver a este punto", "btn primary", (event) => runRestore(event.currentTarget)),
    ]),
  ]);
}

// What a refused restore shows: the APPROVE grant for a device without it, else the words for the code.
function failureNodes(error) {
  if (error && error.kind === "forbidden") return [grantNote(RESTORE_GRANT_LEDE)];
  const text = element("p", "form-error", restoreErrorText(error));
  text.setAttribute("role", "alert");
  return [text];
}

async function runRestore(control) {
  setBusy(control, true, "Volviendo…");
  try {
    const done = await invoke("restore_point", { point: session.point.id });
    session.undo = done.undo_point;
    doneStep(done);
  } catch (error) {
    setBusy(control, false);
    session.slot.replaceChildren(...failureNodes(error));
  }
}

function doneStep(done) {
  session.slot = element("div", "restore-slot");
  step("Hecho.", [
    element("p", "dialog-lede", doneText(done)),
    element("p", "hint", "Si no era lo que querías:"),
    session.slot,
    actionRow([
      button("Deshacer", "btn primary", (event) => runUndo(event.currentTarget)),
      button("Cerrar", "btn ghost", closeDialog),
    ]),
  ]);
}

// Undo is the restore of the copy the first restore kept: the folder goes back as it was before it.
async function runUndo(control) {
  setBusy(control, true, "Deshaciendo…");
  try {
    await invoke("restore_point", { point: session.undo });
    session.undo = null;
    step("Deshecho.", [
      element("p", "dialog-lede", "La carpeta vuelve a estar como estaba antes de volver a esa copia."),
      actionRow([button("Cerrar", "btn ghost", closeDialog)]),
    ]);
  } catch (error) {
    setBusy(control, false);
    session.slot.replaceChildren(...failureNodes(error));
  }
}

function closeDialog() {
  $("restore-dialog").close();
}

export function openRestore(pane, title) {
  session.agent = pane;
  $("restore-agent").textContent = title;
  $("restore-dialog").showModal();
  loadList();
}

export function installRestore() {
  $("restore-close").addEventListener("click", closeDialog);
  $("restore-dialog").addEventListener("close", () => {
    session.point = null;
    session.changes = null;
    session.undo = null;
  });
}
