import { app, $, invoke, showView, toast } from "./core.js";
import { agentTitle, errorMessage, statusLabel, stripAnsi } from "./format.js";

let stopping = false;

function setResult(text, ok) {
  const element = $("composer-result");
  element.textContent = text || "";
  element.classList.toggle("is-ok", ok === true);
  element.classList.toggle("is-error", ok === false);
}

export function renderComposer(agent) {
  const restored = Boolean(agent.restored);
  const ready = agent.status === "blocked" && !restored;
  $("composer").classList.toggle("is-ready", ready);
  $("composer-hint").textContent = restored
    ? "El agente terminó; inicia una nueva conversación."
    : ready
      ? "El agente está esperando; tu respuesta llega al terminal."
      : `Disponible cuando el agente está bloqueado (ahora: ${statusLabel(agent.status).toLowerCase()}).`;
  $("respond-text").disabled = !ready;
  $("btn-respond").disabled = !ready;
  for (const button of document.querySelectorAll(".quick-keys .key")) {
    button.disabled = !ready;
  }
}

function composerControls() {
  return [
    $("btn-respond"),
    $("respond-text"),
    ...document.querySelectorAll(".quick-keys .key"),
  ];
}

/// Runs one write with the double-send guard and the visible result. Returns
/// whether it succeeded, so the caller can clear the draft.
async function write(label, action) {
  if (app.sending) return false;
  app.sending = true;
  for (const control of composerControls()) control.disabled = true;
  setResult(label, null);
  let ok = true;
  try {
    await action();
    setResult("Enviado. El agente lo recibirá en su terminal.", true);
  } catch (error) {
    ok = false;
    setResult(errorMessage(error), false);
  } finally {
    app.sending = false;
    if (app.current) renderComposer(app.current);
  }
  return ok;
}

async function sendRespond() {
  const text = $("respond-text").value.trim();
  if (!text) {
    setResult("Escribe un mensaje antes de enviar.", false);
    return;
  }
  const sent = await write("Enviando…", () =>
    invoke("respond", { paneId: app.current.pane_id, text }),
  );
  if (sent) $("respond-text").value = "";
}

function sendKey(key) {
  return write("Enviando tecla…", () =>
    invoke("send_keys", { paneId: app.current.pane_id, keys: [key], confirm: false }),
  );
}

async function markSeenIfDone(agent) {
  if (agent.status !== "done") return;
  const key = `${agent.pane_id}:${app.state ? app.state.revision : 0}`;
  if (app.seenKeys.has(key)) return;
  app.seenKeys.add(key);
  try {
    await invoke("mark_seen", { paneId: agent.pane_id });
    setResult("Marcado como visto.", true);
  } catch (error) {
    setResult(errorMessage(error), false);
  }
}

function hydrateDetailHeader(agent) {
  $("detail-project").textContent = agent.project || "?";
  $("detail-task").textContent = agentTitle(agent);
  $("detail-dot").className = `status-dot status-${agent.status}`;
  $("detail-status").className = `status-label status-${agent.status}`;
  $("detail-status").textContent = statusLabel(agent.status);
  $("detail-restored").classList.toggle("hidden", !agent.restored);
  $("btn-new-here").classList.toggle("hidden", !agent.restored);
  $("btn-stop").classList.toggle("hidden", Boolean(agent.restored));
  const meta = [
    agent.agent,
    agent.pane_id,
    agent.workspace_id,
    agent.cwd,
    agent.can_resume ? "reanudable" : null,
  ]
    .filter(Boolean)
    .join(" · ");
  $("detail-meta").textContent = meta;
}

export async function openDetail(paneId) {
  const agent = app.state
    ? app.state.agents.find((candidate) => candidate.pane_id === paneId)
    : null;
  if (!agent) return false;
  if (!app.detailReturn) app.detailReturn = "agents";
  app.current = agent;
  hydrateDetailHeader(agent);
  renderComposer(agent);
  setResult("", null);
  $("detail-error").classList.add("hidden");
  $("stop-confirm").classList.add("hidden");
  const stopResult = $("stop-result");
  stopResult.classList.add("hidden");
  stopResult.classList.remove("is-error", "is-ok");
  showView("detail");
  await loadOutput();
  await markSeenIfDone(app.current);
  return true;
}

async function loadOutput() {
  if (!app.current) return;
  const token = ++app.detailToken;
  const paneId = app.current.pane_id;
  $("detail-error").classList.add("hidden");
  $("detail-output").classList.add("is-loading");
  $("detail-output").textContent = "Leyendo salida…";
  $("output-meta").textContent = "Leyendo…";

  try {
    const detail = await invoke("agent_detail", { paneId, lines: 120 });
    if (token !== app.detailToken) return;
    app.current = detail.agent;
    hydrateDetailHeader(detail.agent);
    renderComposer(detail.agent);
    const clean = stripAnsi(detail.output || "").trimEnd();
    $("detail-output").classList.remove("is-loading");
    $("detail-output").textContent = clean || "El agente no ha producido salida todavía.";
    const time = new Date(detail.fetched_at * 1000).toLocaleTimeString("es-ES", {
      hour: "2-digit",
      minute: "2-digit",
    });
    $("output-meta").textContent = `Solo lectura · leído a las ${time}`;
  } catch (error) {
    if (token !== app.detailToken) return;
    $("detail-output").classList.remove("is-loading");
    $("detail-output").textContent = "";
    $("output-meta").textContent = "No disponible";
    const box = $("detail-error");
    box.textContent = errorMessage(error);
    box.classList.remove("hidden");
  }
}

export function syncDetailFromState() {
  if (!app.current || $("view-detail").classList.contains("hidden") || !app.state) return;
  const updated = app.state.agents.find((agent) => agent.pane_id === app.current.pane_id);
  if (updated) {
    app.current = updated;
    hydrateDetailHeader(updated);
    renderComposer(updated);
  }
}

async function stopCurrent() {
  if (stopping || !app.current) return;
  stopping = true;
  const button = $("btn-stop-confirm");
  const original = button.textContent;
  button.disabled = true;
  button.textContent = "Deteniendo…";
  const result = $("stop-result");
  result.classList.add("hidden");
  result.classList.remove("is-error", "is-ok");
  try {
    const outcome = await invoke("stop_agent", { paneId: app.current.pane_id });
    $("stop-confirm").classList.add("hidden");
    leaveDetail();
    toast(
      outcome.already_gone ? "El agente ya había terminado" : "Agente detenido",
    );
  } catch (error) {
    result.textContent = errorMessage(error);
    result.classList.remove("hidden");
    result.classList.add("is-error");
  } finally {
    stopping = false;
    button.disabled = false;
    button.textContent = original;
  }
}

function leaveDetail() {
  app.detailToken += 1;
  const target = app.detailReturn === "chat" ? "chat" : "agents";
  app.detailReturn = null;
  showView(target);
}

export function installDetailHandlers() {
  $("btn-back").addEventListener("click", leaveDetail);
  $("btn-read").addEventListener("click", loadOutput);
  $("btn-stop").addEventListener("click", () => {
    $("stop-confirm").classList.remove("hidden");
    $("btn-stop-confirm").focus();
  });
  $("btn-stop-cancel").addEventListener("click", () => {
    $("stop-confirm").classList.add("hidden");
  });
  $("btn-stop-confirm").addEventListener("click", stopCurrent);
  $("btn-respond").addEventListener("click", sendRespond);
  for (const button of document.querySelectorAll(".quick-keys .key")) {
    button.addEventListener("click", () => sendKey(button.dataset.key));
  }
  $("respond-text").addEventListener("keydown", (event) => {
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      sendRespond();
    }
  });
}
