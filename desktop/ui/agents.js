import { app, $, invoke, toast } from "./core.js";
import { avatarElement } from "./avatar.js";
import { element } from "./chatdom.js";
import { agentTitle, readableState, statusLabel } from "./format.js";
import { friendlyError, OFFLINE_TEXT } from "./friendly-error.js";
import { openDetail } from "./detail.js";
import { chat, chatAfterConnect, chatOnConnection } from "./chat.js";
import { teamSubtitle } from "./hires-model.js";
import { teamHired } from "./hires.js";
import { openRestore } from "./restore-panel.js";

const CONNECTION_LABELS = {
  idle: "Sin conectar",
  connecting: "Conectando…",
  connected: "En vivo",
  offline: "Sin conexión en vivo",
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
  // The team page says «En vivo» only when that is not so: while live, its chip stays hidden.
  $("connection-chip").classList.toggle("hidden", connection.kind === "connected");
  // The header chip says only what is not normal: while live it stays hidden.
  $("chat-connection").classList.toggle("hidden", connection.kind === "connected");

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
      connection.reason || "Sin conexión con el PC. Mostrando el último estado conocido.";
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

  // One action per state: the pairing button and the refresh never show together.
  pair.classList.add("hidden");
  action.classList.add("hidden");
  action.dataset.action = "";

  if (local && hostFailed()) {
    title.textContent = "Excubia no arrancó en este equipo";
    body.textContent = app.boot.host.error || "Vuelve a intentarlo. Si sigue igual, reinicia el equipo.";
    action.textContent = "Reiniciar Excubia";
    action.dataset.action = "restart-host";
    action.classList.remove("hidden");
    return;
  }

  if (local && app.state && app.state.agents.length === 0) {
    title.textContent = "Aún no hay agentes";
    body.textContent = "Excubia está en marcha en este equipo. Los agentes aparecerán aquí en cuanto existan.";
    action.textContent = "Actualizar";
    action.dataset.action = "refresh";
    action.classList.remove("hidden");
    return;
  }

  if (app.state && app.state.agents.length === 0) {
    title.textContent = "Ningún agente en marcha";
    body.textContent = "Cuando arranques un agente aparecerá aquí en cuanto lo detecte.";
    action.textContent = "Actualizar";
    action.dataset.action = "refresh";
    action.classList.remove("hidden");
    return;
  }

  if (!app.state && connection.kind === "fatal") {
    title.textContent = "No se puede leer el estado";
    body.textContent =
      connection.reason || "El PC no acepta este dispositivo. Vuelve a emparejarlo.";
    pair.classList.remove("hidden");
    return;
  }

  if (!app.state && connection.kind === "offline") {
    title.textContent = "Sin conexión con el PC";
    body.textContent = connection.reason || "El PC no responde. Se reintentará automáticamente.";
    action.textContent = "Reintentar";
    action.dataset.action = "retry";
    action.classList.remove("hidden");
    return;
  }

  title.textContent = "Cargando agentes…";
  body.textContent = "Conectando con el PC.";
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

// The project a lead answers for. Its pane sits in the herdr workspace «Excubia», so the row
// names the lead's project instead: two leads then no longer read the same.
function leadProjectName(agent) {
  const projects =
    chat.home && chat.home.bootstrap ? chat.home.bootstrap.projects || [] : [];
  // The pane id can be stale (a lead that ran twice): the folder it works in names the project too.
  const cwd = (agent.cwd || "").replace(/\\/g, "/").toLowerCase();
  const isLead = /(^|\s|-)lead(\s|$)/i.test(agent.title || "");
  return (
    projects.find(
      (item) =>
        (item.lead && item.lead.pane_id === agent.pane_id) ||
        (isLead && item.root_label && cwd.endsWith(`/${item.root_label.toLowerCase()}`)),
    )?.name || ""
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

  const face = avatarElement({
    name: agentTitle(agent),
    characterId: agent.characterId,
    status: agent.status,
    px: 44,
  });
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
    lead.textContent = "Líder";
    project.appendChild(lead);
  }
  if (agent.focused) {
    const focus = document.createElement("span");
    focus.className = "focus-note";
    focus.textContent = "· en foco";
    project.appendChild(focus);
  }
  const title = document.createElement("span");
  title.className = "row-title";
  title.textContent = leadProjectName(agent) || agent.project || "Sin proyecto";
  text.append(project, title);

  // A state the host could not classify says nothing to the operator: it waits in the menu.
  const known = statusLabel(agent.status) !== statusLabel("unknown");
  const chevron = document.createElement("span");
  chevron.className = "row-chevron";
  chevron.setAttribute("aria-hidden", "true");
  chevron.textContent = "›";

  // A gray dot without a known state tells nothing: the row keeps its name and drops the dot.
  if (known) {
    const status = document.createElement("span");
    status.className = "row-status";
    status.textContent = statusLabel(agent.status);
    button.append(face, dot, text, status, chevron);
  } else {
    button.append(face, text, chevron);
  }

  const copies = document.createElement("button");
  copies.type = "button";
  copies.className = "linkish agent-copies";
  copies.textContent = "Copias del turno";
  copies.addEventListener("click", () => openRestore(agent.pane_id, agentTitle(agent)));
  const menu = document.createElement("details");
  menu.className = "agent-menu";
  const toggle = document.createElement("summary");
  toggle.className = "agent-menu-toggle";
  toggle.textContent = "⋯";
  toggle.title = "Más acciones";
  toggle.setAttribute("aria-label", "Más acciones");
  const panel = document.createElement("div");
  panel.className = "agent-menu-panel";
  if (agent.restored) panel.append(element("p", "agent-menu-note", "Restaurado"));
  if (!known) panel.append(element("p", "agent-menu-note", statusLabel(agent.status)));
  panel.append(copies);
  menu.append(toggle, panel);
  const foot = document.createElement("div");
  foot.className = "agent-foot";
  foot.append(menu);

  item.append(button, foot);
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
    header.append(name);
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
  // A contracted team fills the view: the empty state gives way to «Tu equipo».
  const hired = teamHired();
  $("agents-empty").classList.toggle("hidden", loading || hasAgents || hired > 0);
  $("agents-subtitle").textContent = hasState
    ? teamSubtitle(hired, app.state.agents.length)
    : loading
      ? "Cargando estado…"
      : "Sin estado";

  if (hasAgents) renderList();
  if (!loading && !hasAgents) renderEmpty();
}

export function setState(state) {
  app.state = readableState(state);
  renderAgents();
}

export async function connectNow() {
  try {
    await invoke("connect");
    await chatAfterConnect();
  } catch {
    app.connection = { kind: "offline", reason: OFFLINE_TEXT };
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
    toast(friendlyError(error));
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
