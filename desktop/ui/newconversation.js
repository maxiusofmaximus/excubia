import { $, invoke, toast } from "./core.js";
import { errorMessage } from "./format.js";
import { openDetail } from "./detail.js";

const NO_AGENTS =
  "No encontré ningún agente instalado: instala uno de claude, codex, gemini u opencode y vuelve a intentarlo.";

let submitting = false;

function showError(message) {
  const box = $("nc-error");
  box.textContent = message;
  box.classList.toggle("hidden", !message);
}

function selectedAgent() {
  const checked = document.querySelector('input[name="nc-agent-option"]:checked');
  return checked && !checked.disabled ? checked.value : null;
}

function updateSubmit() {
  $("nc-submit").disabled = submitting || !selectedAgent();
}

async function loadCatalog() {
  const list = $("nc-agents");
  list.replaceChildren();
  updateSubmit();
  try {
    const agents = await invoke("agent_catalog");
    for (const agent of agents) {
      const label = document.createElement("label");
      label.className = agent.available ? "nc-agent" : "nc-agent is-unavailable";
      const radio = document.createElement("input");
      radio.type = "radio";
      radio.name = "nc-agent-option";
      radio.value = agent.name;
      radio.disabled = !agent.available;
      radio.addEventListener("change", updateSubmit);
      const name = document.createElement("span");
      name.className = "nc-agent-name";
      name.textContent = agent.label || agent.name;
      const note = document.createElement("span");
      note.className = "nc-agent-note";
      note.textContent = agent.available
        ? "disponible"
        : "no instalado en este equipo";
      label.append(radio, name, note);
      list.append(label);
    }
    const first = agents.find((agent) => agent.available);
    if (first) {
      const radio = list.querySelector(
        `input[value="${CSS.escape(first.name)}"]`,
      );
      if (radio) radio.checked = true;
    }
    updateSubmit();
    if (!first) showError(NO_AGENTS);
  } catch (error) {
    showError(errorMessage(error));
    updateSubmit();
  }
}

export async function openNewConversation(cwd = "") {
  const dialog = $("new-conversation");
  showError("");
  $("nc-cwd").value = cwd;
  $("nc-label").value = "";
  await loadCatalog();
  if (!dialog.open) dialog.showModal();
  if (cwd) $("nc-cwd").focus();
}

async function openCreated(paneId) {
  for (let attempt = 0; attempt < 10; attempt += 1) {
    try {
      await invoke("refresh_state");
    } catch {
      // The SSE frame may still deliver it; opening is best effort.
    }
    if (await openDetail(paneId)) return;
    await new Promise((resolve) => setTimeout(resolve, 300));
  }
  toast("Conversación creada; aparecerá en la lista");
}

async function submit(event) {
  event.preventDefault();
  if (submitting) return;
  const agent = selectedAgent();
  const cwd = $("nc-cwd").value.trim();
  const label = $("nc-label").value.trim();
  if (!agent) {
    showError(NO_AGENTS);
    return;
  }
  if (!cwd) {
    showError("Elige la carpeta de trabajo.");
    return;
  }

  submitting = true;
  const button = $("nc-submit");
  const original = button.textContent;
  button.disabled = true;
  button.textContent = "Creando…";
  showError("");
  try {
    const created = await invoke("create_conversation", {
      agent,
      cwd,
      label: label || null,
    });
    $("new-conversation").close();
    toast("Conversación creada");
    await openCreated(created.pane_id);
  } catch (error) {
    showError(errorMessage(error));
  } finally {
    submitting = false;
    button.textContent = original;
    updateSubmit();
  }
}

async function pickFolder() {
  try {
    const folder = await invoke("pick_folder");
    if (folder) $("nc-cwd").value = folder;
  } catch (error) {
    showError(errorMessage(error));
  }
}

export function installNewConversationHandlers() {
  $("new-conversation-form").addEventListener("submit", submit);
  $("nc-cancel").addEventListener("click", () => $("new-conversation").close());
  $("btn-pick-folder").addEventListener("click", pickFolder);
}
