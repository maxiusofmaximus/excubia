// Ajustes → Aprobaciones: who decides by default, and the projects that
// override it. The host stores the mode; these controls only ask for a change.
// Without APPROVE on this device they stay visible, disabled, with the grant.

import { $, invoke } from "./core.js";
import { element } from "./approval-card.js";
import { errorMessage } from "./format.js";
import { grantCommand, setDeviceId } from "./approval-model.js";

let settings = null;
let notice = "";

const LEVEL_HELP =
  "Política → puntuador → lead → Alfred → tú: cada nivel decide una vez y las aprobaciones solo suben.";
const DELEGATED_HELP =
  "En modo delegado solo te llega lo que la lista de nunca delegar o la falta de información te reserva.";

export function mountApprovalSettings() {
  const section = element("section", "approval-settings");
  section.id = "approval-settings";
  section.setAttribute("aria-labelledby", "approval-settings-title");
  $("view-pair").append(section);
  for (const id of ["btn-settings", "btn-settings-team"]) {
    $(id).addEventListener("click", loadSettings);
  }
  section.addEventListener("change", changeMode);
  render();
}

async function loadSettings() {
  try {
    settings = await invoke("chat_approval_settings");
    setDeviceId(settings.device_id);
    notice = "";
  } catch (error) {
    settings = null;
    notice = errorMessage(error);
  }
  render();
}

async function changeMode(event) {
  const input = event.target;
  if (!input.matches("input[type='checkbox'][data-approval-scope]")) return;
  const projectId = input.dataset.approvalScope || null;
  const mode = input.checked ? "always_user" : "delegated";
  input.disabled = true;
  try {
    const result = await invoke("chat_set_approval_mode", {
      clientId: crypto.randomUUID(),
      mode,
      projectId,
    });
    // The mode result carries no device id: keep the one loaded before.
    settings = { ...settings, ...result.settings };
    notice = result.escalated
      ? `Guardado. ${result.escalated} aprobaciones pasaron a tu bandeja.`
      : "Guardado.";
  } catch (error) {
    input.checked = !input.checked;
    const forbidden = error && error.kind === "forbidden";
    if (forbidden) settings = { ...settings, can_approve: false };
    // The grant note below already names APPROVE; a generic RESPOND notice would mislead.
    notice = forbidden ? "" : errorMessage(error);
  }
  render();
}

function render() {
  $("approval-settings").replaceChildren(...buildContent());
}

function buildContent() {
  const title = element("h2", "approval-settings-title", "Aprobaciones");
  title.id = "approval-settings-title";
  const content = [title, element("p", "hint", DELEGATED_HELP), element("p", "hint", LEVEL_HELP)];
  if (!settings) {
    content.push(element("p", "hint", notice || "Cargando ajustes…"));
    return content;
  }

  const canChange = settings.can_approve !== false;
  content.push(switchRow("Pedirme siempre a mí", "", settings.global === "always_user", canChange));
  for (const project of settings.projects || []) {
    content.push(
      switchRow(
        `Proyecto ${project.project_id}`,
        project.project_id,
        project.mode === "always_user",
        canChange,
      ),
    );
  }
  if (!canChange) {
    const note = element("div", "approval-error");
    note.append(
      element("p", null, "Este equipo no tiene el permiso APPROVE: los ajustes se ven pero no se cambian."),
      element("code", "approval-grant", grantCommand(settings.device_id)),
    );
    content.push(note);
  }
  content.push(element("p", "hint", notice));
  return content;
}

function switchRow(label, projectId, checked, enabled) {
  const row = element("label", "approval-switch");
  const input = element("input");
  input.type = "checkbox";
  input.checked = checked;
  input.disabled = !enabled;
  input.dataset.approvalScope = projectId;
  row.append(input, element("span", null, label));
  return row;
}
