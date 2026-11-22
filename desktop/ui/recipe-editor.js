// «Revisar receta»: the one editor of a saved or drafted text. It shows what the operator is about
// to store and stores it only on «Guardar». A refused save is worded here, by reviewNotice; a
// secret blocks the save until the text changes, and a risk waits for the operator's tick.

import { $, setBusy } from "./core.js";
import { element } from "./chatdom.js";
import { BODY_MAX, TITLE_MAX, bodyError, canSave, limitCount, reviewNotice, titleError } from "./recipe-model.js";

// `save({ title, body, confirmRisks })` resolves once the host stores the text. `phase` is "ready",
// "loading" (a draft on its way) or "error" (the draft failed; `retry` asks again).
let view = null;

function field(id) {
  return $(id).value;
}

function paintCounts() {
  $("recipe-title-count").textContent = limitCount(field("recipe-title"), TITLE_MAX);
  $("recipe-body-count").textContent = limitCount(field("recipe-body"), BODY_MAX);
}

function showNotice(notice) {
  const box = $("recipe-notice");
  box.hidden = !notice;
  box.textContent = notice?.text ?? "";
  box.dataset.tone = notice?.tone ?? "";
  const list = $("recipe-reasons");
  const reasons = notice?.reasons ?? [];
  list.replaceChildren(...reasons.map((reason) => element("li", null, reason)));
  list.hidden = reasons.length === 0;
}

// Everything the operator can do right now, computed from the one state the screen holds.
function paint() {
  if (!view) return;
  const frozen = view.phase !== "ready" || view.locked || view.busy;
  for (const id of ["recipe-title", "recipe-body", "recipe-risk"]) $(id).disabled = frozen;
  const riskOk = !view.risk || $("recipe-risk").checked;
  const saveOff = frozen || view.secret || !riskOk || !canSave(field("recipe-title"), field("recipe-body"));
  $("recipe-save").disabled = saveOff;
  $("recipe-body").setAttribute("aria-invalid", String(view.secret));
  $("recipe-retry").hidden = !(view.phase === "error" && view.retry);
  $("recipe-risk-wrap").hidden = !view.risk;
  paintCounts();
}

function setRisk(on) {
  view.risk = on;
  $("recipe-risk").checked = false;
}

// A text that changed is a new text: the host's last verdict on it no longer holds.
function onEdit() {
  view.secret = false;
  setRisk(false);
  showNotice(null);
  paint();
}

// Opens the editor, or refills it when it is already open (the draft arrives after the editor opened).
export function openRecipeEditor({ heading, lede, title = "", body = "", phase = "ready", errorText = "", retry = null, save, onSaved }) {
  view = { save, onSaved, retry, phase, risk: false, secret: false, locked: false, busy: false };
  $("recipe-editor-title").textContent = heading;
  $("recipe-editor-lede").textContent = lede;
  $("recipe-title").value = title;
  $("recipe-body").value = body;
  setRisk(false);
  showNotice(phase === "error" ? { tone: "error", text: errorText, reasons: [] } : null);
  const dialog = $("recipe-editor");
  if (!dialog.open) dialog.showModal();
  paint();
}

function closeEditor() {
  view = null;
  $("recipe-editor").close();
}

// A refused save: a secret blocks, a risk asks for the tick, a denied device loses «Guardar».
function refused(error) {
  const notice = reviewNotice(error);
  if (error && error.kind === "forbidden") view.locked = true;
  view.secret = notice.tone === "secret";
  setRisk(notice.tone === "risk");
  showNotice(notice);
  paint();
}

async function submit(event) {
  event.preventDefault();
  if (!view) return;
  const title = field("recipe-title").trim();
  const body = field("recipe-body").trim();
  const problem = titleError(title) ?? bodyError(body);
  if (problem) {
    showNotice({ tone: "error", text: problem, reasons: [] });
    return;
  }
  const { save, onSaved } = view;
  view.busy = true;
  setBusy($("recipe-save"), true, "Guardando…");
  paint();
  try {
    const saved = await save({ title, body, confirmRisks: view.risk && $("recipe-risk").checked });
    closeEditor();
    onSaved(saved);
  } catch (error) {
    if (!view) return;
    view.busy = false;
    setBusy($("recipe-save"), false);
    refused(error);
  }
}

// Wired once, when the screens start.
export function installRecipeEditor() {
  $("recipe-form").addEventListener("submit", submit);
  $("recipe-cancel").addEventListener("click", closeEditor);
  $("recipe-retry").addEventListener("click", () => view?.retry?.());
  $("recipe-title").addEventListener("input", onEdit);
  $("recipe-body").addEventListener("input", onEdit);
  $("recipe-risk").addEventListener("change", paint);
  $("recipe-editor").addEventListener("close", () => {
    view = null;
  });
}
