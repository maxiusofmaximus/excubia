// «Avisos»: el modo y las reglas que decide el host. La pantalla lee, cambia y borra lo que el host
// confirma; nada se pinta como hecho antes de su respuesta, y un cambio que falla devuelve el control a
// como estaba.

import { $, invoke, setBusy } from "./core.js";
import { showView } from "./nav.js";
import { element } from "./chatdom.js";
import { reportWrite } from "./agenda-ui.js";
import { emptyState, errorState } from "./empty-state.js";
import { permissionText } from "./agenda-model.js";
import {
  alertErrorText,
  draftFor,
  EMPTY_RULES_TEXT,
  GUARANTEE_TEXT,
  hiredNames,
  modeWords,
  ruleSentence,
} from "./alerts-model.js";

let rules = [];
let mode = "all";

const modeInputs = () => [...document.getElementsByName("alerts-mode")];

function paintMode(next) {
  mode = next;
  for (const input of modeInputs()) input.checked = input.value === next;
  $("alerts-mode-phrase").textContent = modeWords(next).phrase;
}

function paintRules() {
  const list = $("alerts-list");
  list.replaceChildren(...rules.map(ruleRow));
  $("alerts-empty").hidden = rules.length > 0;
  list.hidden = rules.length === 0;
}

// One rule: its sentence, its switch and «Borrar», which asks once before it goes.
function ruleRow(rule) {
  const row = element("li", "alert-rule");
  const sentence = ruleSentence(rule);
  const toggle = element("input");
  toggle.type = "checkbox";
  toggle.checked = rule.enabled;
  toggle.setAttribute("aria-label", `Regla activa: ${sentence}`);
  toggle.addEventListener("change", () => changeEnabled(rule, toggle));
  const switchLabel = element("label", "alert-switch");
  switchLabel.append(toggle, element("span", null, rule.enabled ? "Activa" : "Pausada"));
  const remove = element("button", "btn ghost btn-small", "Borrar");
  remove.type = "button";
  remove.addEventListener("click", () => askDelete(row, rule));
  row.append(element("span", "alert-sentence", sentence), switchLabel, remove);
  return row;
}

// A refusal in words, under the field that failed; a forbidden read or write names the grant.
function report(failure, problem, permission) {
  reportWrite(failure, { fields: {}, problem, permission, words: alertErrorText });
}

async function changeMode(input) {
  const problem = $("alerts-mode-problem");
  problem.hidden = true;
  try {
    paintMode(await invoke("alert_mode_set", { mode: input.value }));
  } catch (failure) {
    paintMode(mode);
    report(failure, problem, "RESPOND");
  }
}

async function changeEnabled(rule, toggle) {
  $("alerts-problem").hidden = true;
  try {
    const updated = await invoke("alert_rule_update", { id: rule.id, enabled: toggle.checked });
    rules = rules.map((existing) => (existing.id === rule.id ? updated : existing));
    paintRules();
  } catch (failure) {
    toggle.checked = rule.enabled;
    report(failure, $("alerts-problem"), "RESPOND");
  }
}

// The row asks in place: «¿Borrar esta regla?» with «Sí, borrar» and «No».
function askDelete(row, rule) {
  const ask = element("p", "alert-ask", "¿Borrar esta regla?");
  const yes = element("button", "btn primary btn-small", "Sí, borrar");
  const no = element("button", "btn ghost btn-small", "No");
  yes.type = "button";
  no.type = "button";
  yes.addEventListener("click", () => deleteRule(rule));
  no.addEventListener("click", () => ask.remove());
  ask.append(" ", yes, " ", no);
  row.append(ask);
}

async function deleteRule(rule) {
  $("alerts-problem").hidden = true;
  try {
    await invoke("alert_rule_delete", { id: rule.id });
    rules = rules.filter((existing) => existing.id !== rule.id);
    paintRules();
  } catch (failure) {
    report(failure, $("alerts-problem"), "RESPOND");
    paintRules();
  }
}

// The form's choices: a reminder has no agent and no status, so those two fields wait.
function paintKind() {
  const reminder = $("alerts-kind").value === "reminder";
  $("alerts-agent-field").hidden = reminder;
  $("alerts-when-field").hidden = reminder;
}

function formDraft() {
  return draftFor({
    kind: $("alerts-kind").value,
    agent: $("alerts-agent").value || null,
    when: $("alerts-when").value,
    action: $("alerts-action").value,
  });
}

async function createRule(event) {
  event.preventDefault();
  const problem = $("alerts-form-problem");
  const button = $("btn-alert-create");
  problem.hidden = true;
  setBusy(button, true, "Creando…");
  try {
    const rule = await invoke("alert_rule_create", { draft: formDraft() });
    rules = [...rules, rule];
    paintRules();
    $("alerts-form").reset();
    paintKind();
  } catch (failure) {
    report(failure, problem, "RESPOND");
  } finally {
    setBusy(button, false);
  }
}

// The team's names fill the agent list; without them the form still offers «Cualquiera».
async function fillAgents() {
  const select = $("alerts-agent");
  select.replaceChildren(new Option("Cualquiera", ""));
  try {
    const hires = await invoke("team_hires");
    for (const name of hiredNames(hires.hires)) select.append(new Option(name, name));
  } catch {
    // The list of agents is an aid: the rules still work with «Cualquiera».
  }
}

async function loadAll() {
  const status = $("alerts-status");
  $("alerts-problem").hidden = true;
  status.hidden = false;
  status.textContent = "Cargando avisos…";
  try {
    const [nextMode, nextRules] = await Promise.all([invoke("alert_mode"), invoke("alert_rules")]);
    paintMode(nextMode);
    rules = nextRules;
    status.hidden = true;
    paintRules();
  } catch (failure) {
    const text = failure?.kind === "forbidden" ? permissionText("READ_STATE") : alertErrorText(failure);
    status.replaceChildren(errorState(text, loadAll));
    status.hidden = false;
  }
}

export function openAlerts() {
  showView("alerts");
  loadAll();
  fillAgents();
}

export function installAlerts() {
  $("alerts-empty").replaceChildren(
    emptyState({ icon: "alerts", text: EMPTY_RULES_TEXT, action: "Nueva regla" }, () => $("alerts-kind").focus()),
  );
  $("alerts-guarantee").textContent = GUARANTEE_TEXT;
  for (const input of modeInputs()) input.addEventListener("change", () => changeMode(input));
  $("alerts-kind").addEventListener("change", paintKind);
  $("alerts-form").addEventListener("submit", createRule);
  paintKind();
}
