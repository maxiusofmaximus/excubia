import { $, invoke, toast } from "./core.js";
import { errorMessage } from "./format.js";
import { openDetail } from "./detail.js";

const NO_AGENTS =
  "No encontré ningún agente instalado: instala uno de claude, codex, commandcode, gemini u opencode y vuelve a intentarlo.";

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

/// Saves `name` as the PC's default for new conversations, then reads the list again.
async function makeDefault(name) {
  showError("");
  try {
    await invoke("set_default_agent", { name });
    await loadCatalog();
  } catch (error) {
    showError(errorMessage(error));
  }
}

async function loadCatalog() {
  const list = $("nc-agents");
  list.replaceChildren();
  updateSubmit();
  try {
    const agents = await invoke("agent_catalog");
    for (const agent of agents) {
      const label = document.createElement("label");
      label.className = agent.runnable ? "nc-agent" : "nc-agent is-unavailable";
      const radio = document.createElement("input");
      radio.type = "radio";
      radio.name = "nc-agent-option";
      radio.value = agent.name;
      radio.disabled = !agent.runnable;
      radio.addEventListener("change", updateSubmit);
      const name = document.createElement("span");
      name.className = "nc-agent-name";
      name.textContent = agent.label || agent.name;
      const note = document.createElement("span");
      note.className = "nc-agent-note";
      note.textContent = !agent.runnable
        ? agent.problem || "no se puede usar en este equipo"
        : agent.is_default
          ? "por defecto"
          : "disponible";
      label.append(radio, name, note);
      if (agent.runnable && !agent.is_default) {
        const make = document.createElement("button");
        make.type = "button";
        make.className = "nc-agent-make-default";
        make.textContent = "Usar por defecto";
        make.addEventListener("click", (event) => {
          // The button sits inside the label: it must not also pick the radio.
          event.preventDefault();
          makeDefault(agent.name);
        });
        label.append(make);
      }
      list.append(label);
    }
    const chosen =
      agents.find((agent) => agent.runnable && agent.is_default) ||
      agents.find((agent) => agent.runnable);
    if (chosen) {
      const radio = list.querySelector(
        `input[value="${CSS.escape(chosen.name)}"]`,
      );
      if (radio) radio.checked = true;
    }
    updateSubmit();
    if (!chosen) showError(NO_AGENTS);
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
