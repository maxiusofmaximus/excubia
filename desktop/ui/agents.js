import { app, $, invoke, toast } from "./core.js";
import { agentTitle, errorMessage, statusLabel, subtitle } from "./format.js";
import { openDetail } from "./detail.js";
import { chat, chatAfterConnect, chatOnConnection } from "./chat.js";

const CONNECTION_LABELS = {
  idle: "Sin conectar",
  connecting: "Conectando…",
  connected: "En vivo",
  offline: "Sin conexión",
  fatal: "Requiere atención",
};

function paintChip(chip, connection) {
  if (!chip) return;
  chip.className = `chip ${connection.kind}`;
  chip.querySelector(".chip-label").textContent =
    CONNECTION_LABELS[connection.kind] || "Sin conectar";
  chip.title = connection.reason || "Estado de la conexión";
}

export function renderConnection() {
  const connection = app.connection || { kind: "idle" };
  paintChip($("connection-chip"), connection);
  paintChip($("chat-connection"), connection);

  const live = $("detail-live");
  if (live) {
    live.textContent =
      connection.kind === "connected"
        ? "En vivo"
        : connection.kind === "offline" || connection.kind === "fatal"
          ? "Sin conexión"
          : "";
  }

  const stale = $("stale-banner");
  const showStale =
    (connection.kind === "offline" || connection.kind === "fatal") && Boolean(app.state);
  stale.classList.toggle("hidden", !showStale);
  if (showStale) {
    $("stale-text").textContent =
      connection.reason || "Sin conexión con el gateway. Mostrando el último estado conocido.";
    $("btn-stale-retry").classList.toggle("hidden", connection.kind === "fatal");
    $("btn-stale-pair").classList.toggle("hidden", connection.kind !== "fatal");
  }
}

function hostFailed() {
  return Boolean(app.boot && app.boot.host && app.boot.host.state === "failed");
}

function renderEmpty() {
  const connection = app.connection || { kind: "idle" };
  const action = $("btn-empty-action");
  const pair = $("btn-empty-pair");
  const title = $("empty-title");
  const body = $("empty-body");
  const local = Boolean(app.boot && app.boot.mode === "local");
  const create = $("btn-empty-new");

  pair.classList.add("hidden");
  action.classList.add("hidden");
  create.classList.remove("hidden");
  action.dataset.action = "";

  if (local && hostFailed()) {
    title.textContent = "El host local no arrancó";
    body.textContent = app.boot.host.error || "Revisa el log del host y vuelve a intentarlo.";
    action.textContent = "Reiniciar el host";
    action.dataset.action = "restart-host";
    action.classList.remove("hidden");
    create.classList.add("hidden");
    return;
  }

  if (local && app.state && app.state.agents.length === 0) {
    title.textContent = "Aún no hay agentes";
    body.textContent = "El host local está en marcha. Los agentes aparecerán aquí en cuanto existan.";
    action.textContent = "Actualizar";
    action.dataset.action = "refresh";
    action.classList.remove("hidden");
    return;
  }

  if (app.state && app.state.agents.length === 0) {
    title.textContent = "Ningún agente en marcha";
    body.textContent = "Arranca un agente en herdr y aparecerá aquí en cuanto lo detecte.";
    action.textContent = "Actualizar";
    action.dataset.action = "refresh";
    action.classList.remove("hidden");
    return;
  }

  if (!app.state && connection.kind === "fatal") {
    title.textContent = "No se puede leer el estado";
    body.textContent =
      connection.reason || "El gateway no acepta este dispositivo. Vuelve a emparejarlo.";
    pair.classList.remove("hidden");
    return;
  }

  if (!app.state && connection.kind === "offline") {
    title.textContent = "Sin conexión con el gateway";
    body.textContent = connection.reason || "El gateway no responde. Se reintentará automáticamente.";
    action.textContent = "Reintentar";
    action.dataset.action = "retry";
    action.classList.remove("hidden");
    return;
  }

  title.textContent = "Cargando agentes…";
  body.textContent = "Conectando con el gateway.";
}

function projectLeadPane(project) {
  const projects =
    chat.home && chat.home.bootstrap ? chat.home.bootstrap.projects || [] : [];
  const key = String(project || "").toLowerCase();
  return (
    projects.find(
      (item) =>
        item.name.toLowerCase() === key ||
        (item.root_label || "").toLowerCase() === key,
    )?.lead?.pane_id || null
  );
}

function agentItem(agent, leadPane) {
  const item = document.createElement("li");
  const button = document.createElement("button");
  button.type = "button";
  button.className = `agent-row status-${agent.status}`;
  button.dataset.pane = agent.pane_id;
  button.setAttribute(
    "aria-label",
    `${agent.project || "Proyecto"}, ${statusLabel(agent.status)}${agent.title ? `, ${agent.title}` : ""}`,
  );
  button.addEventListener("click", () => openDetail(agent.pane_id));

  const dot = document.createElement("span");
  dot.className = "status-dot";
  dot.setAttribute("aria-hidden", "true");

  const text = document.createElement("span");
  text.className = "row-text";
  const project = document.createElement("span");
  project.className = "row-project";
  project.textContent = agentTitle(agent);
  if (agent.pane_id === leadPane || /(^|\s|-)(lead)(\s|$)/i.test(agent.title || "")) {
    const lead = document.createElement("span");
    lead.className = "row-restored";
    lead.textContent = "lead";
    project.appendChild(lead);
  }
  if (agent.focused) {
    const focus = document.createElement("span");
    focus.className = "focus-note";
    focus.textContent = "· en foco";
    project.appendChild(focus);
  }
  if (agent.restored) {
    const restored = document.createElement("span");
    restored.className = "row-restored";
    restored.textContent = "restaurado";
    project.appendChild(restored);
  }
  const title = document.createElement("span");
  title.className = "row-title";
  title.textContent = agent.project || "Sin proyecto";
  text.append(project, title);

  const status = document.createElement("span");
  status.className = "row-status";
  status.textContent = statusLabel(agent.status);

  const chevron = document.createElement("span");
  chevron.className = "row-chevron";
  chevron.setAttribute("aria-hidden", "true");
  chevron.textContent = "›";

  button.append(dot, text, status, chevron);
  item.appendChild(button);
  return item;
}

function renderList() {
  const list = $("agent-list");
  const focusedPane =
    document.activeElement && document.activeElement.dataset
      ? document.activeElement.dataset.pane
      : null;
  list.replaceChildren();

  const groups = new Map();
  for (const agent of app.state.agents) {
    const key = agent.project || "Sin proyecto";
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(agent);
  }

  for (const [project, rows] of groups) {
    const header = document.createElement("li");
    header.className = "team-group";
    const name = document.createElement("span");
    name.className = "team-group-name";
    name.textContent = project;
    const count = document.createElement("span");
    count.className = "team-group-count";
    count.textContent = `${rows.length}`;
    header.append(name, count);
    list.appendChild(header);
    const leadPane = projectLeadPane(project);
    for (const agent of rows) {
      list.appendChild(agentItem(agent, leadPane));
    }
  }

  if (focusedPane) {
    const next = [...list.querySelectorAll(".agent-row")].find(
      (row) => row.dataset.pane === focusedPane,
    );
    if (next) next.focus({ preventScroll: true });
  }
}

export function renderAgents() {
  const hasState = Boolean(app.state);
  const hasAgents = hasState && app.state.agents.length > 0;
  const connection = app.connection || { kind: "idle" };
  const loading =
    !hasState &&
    !hostFailed() &&
    (connection.kind === "idle" || connection.kind === "connecting");

  $("agents-loading").classList.toggle("hidden", !loading);
  $("agent-list").classList.toggle("hidden", !hasAgents);
  $("agents-empty").classList.toggle("hidden", loading || hasAgents);
  $("agents-subtitle").textContent = hasState
    ? subtitle(app.state)
    : loading
      ? "Cargando estado…"
      : "Sin estado";

  if (hasAgents) renderList();
  if (!loading && !hasAgents) renderEmpty();
}

export function setState(state) {
  app.state = state;
  renderAgents();
}

export async function connectNow() {
  try {
    await invoke("connect");
    await chatAfterConnect();
  } catch (error) {
    app.connection = { kind: "offline", reason: errorMessage(error) };
    renderConnection();
    renderAgents();
    chatOnConnection();
  }
}

async function refreshNow() {
  const button = $("btn-refresh");
  button.disabled = true;
  try {
    setState(await invoke("refresh_state"));
    toast("Estado actualizado");
  } catch (error) {
    toast(errorMessage(error));
  } finally {
    button.disabled = false;
  }
}

export function installAgentHandlers() {
  $("btn-refresh").addEventListener("click", refreshNow);
  $("btn-stale-retry").addEventListener("click", connectNow);
  $("btn-empty-action").addEventListener("click", () => {
    const action = $("btn-empty-action").dataset.action;
    if (action === "retry") connectNow();
    else if (action === "refresh") refreshNow();
    else if (action === "restart-host") invoke("restart_host");
  });
}
