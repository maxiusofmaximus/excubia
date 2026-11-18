// Delegation cards, their detail dialog and the human text of every typed
// refusal. The host owns the state; this module only renders and requests.

import { $, app, invoke, toast } from "./core.js";
import { errorMessage, stripTestMarkers } from "./format.js";
import { formatWhen } from "./chatlist.js";
import { openDetail } from "./detail.js";
import { saveRecipeFromDelegation } from "./recipes.js";
import { chat } from "./chatstate.js";
import {
  cardState,
  cardTask,
  detailKicker,
  finishedTasks,
  projectName,
  statusMeta,
  taskRepeatsTitle,
} from "./delegation-model.js";

const REASON_TEXT = {
  delegation_timeout: "El turno de la delegación agotó el tiempo de espera.",
  pane_gone: "El panel del asignado desapareció antes de terminar.",
  host_restart: "Excubia se reinició y la delegación quedó interrumpida.",
  conversation_cancelled: "Se canceló el equipo de esta conversación.",
  user_cancelled: "La delegación se canceló desde el escritorio.",
  delegation_cancelled: "La delegación ya había terminado o estaba cancelada.",
  seat_unavailable: "El asiento no tenía un panel vivo cuando se asignó.",
  depth_exceeded: "Alfred no puede delegar más veces en esta tarea.",
  pane_limit: "El PC ya tiene seis paneles abiertos; no se puede abrir otro.",
  cycle_detected: "La delegación repite un encargo reciente; se detuvo para evitar un ciclo.",
  turn_budget: "Este turno ya hizo doce delegaciones; no se permiten más.",
  hour_budget: "Este asiento ya hizo sesenta delegaciones en la última hora.",
  seat_limit: "El proyecto ya tiene ocho asientos; no cabe otro.",
};

// CORE7 stores a few assignment titles double-encoded (`asignÃ³`). Keep the
// text plain and repair the known UTF-8/Latin-1 pairs at the display edge.
function cleanText(text) {
  return stripTestMarkers(String(text || ""))
    .replace(/Ã³/g, "ó")
    .replace(/Ã¡/g, "á")
    .replace(/Ã©/g, "é")
    .replace(/Ã­/g, "í")
    .replace(/Ãº/g, "ú")
    .replace(/Ã±/g, "ñ")
    .replace(/Ã¼/g, "ü")
    .replace(/Ã/g, "");
}

export function reasonText(reason) {
  if (!reason) return "";
  return REASON_TEXT[reason] || reason;
}

export function delegationId(entry) {
  return entry.ref_id || entry.delegation_id || "";
}

function element(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function shortId(id) {
  return id && id.length > 14 ? `${id.slice(0, 11)}…` : id || "sin id";
}

function statusChip(meta) {
  const chip = element("span", `delegation-status tone-${meta.tone}`);
  const dot = element("span", "delegation-dot");
  dot.setAttribute("aria-hidden", "true");
  chip.append(dot, element("span", null, meta.label));
  return chip;
}

function action(label, kind, data, className = "btn ghost btn-small") {
  const button = element("button", className, label);
  button.type = "button";
  button.dataset.delegationAction = kind;
  for (const [name, value] of Object.entries(data)) button.dataset[name] = value;
  return button;
}

export function renderDelegationCard(entry, options = {}) {
  const id = delegationId(entry);
  const status = entry.status || "queued";
  const state = cardState(entry);
  const active = ["queued", "running", "waiting"].includes(status);
  const card = element("article", `delegation-card tone-${state.tone}`);
  card.dataset.delegation = id;

  // One line: who, the short task, the state and «Ver detalle». The report, the ids and
  // «Guardar como receta» wait in the detail, never on the card.
  const line = element("div", "delegation-line");
  const title = cleanText(entry.title) || "Delegación del equipo";
  const titleNode = element("span", "delegation-title", title);
  titleNode.title = title;
  line.append(titleNode);
  const task = active ? cardTask(cleanText(entry.summary)) : "";
  if (task && !taskRepeatsTitle(task, title)) {
    const what = element("span", "delegation-what", task);
    what.title = task;
    line.append(what);
  }
  line.append(statusChip(state));
  line.append(action("Ver detalle", "detail", { delegation: id }, "delegation-link"));
  if (active) {
    line.append(action("Cancelar", "cancel", { delegation: id }, "delegation-link delegation-cancel"));
  }
  if (entry.attention >= 3) line.append(element("span", "delegation-attention", "Requiere atención"));
  card.append(line);

  if (options.cancelTarget === id) {
    const confirm = element("div", "delegation-confirm");
    confirm.setAttribute("role", "alert");
    confirm.append(
      element(
        "span",
        null,
        "¿Cancelar esta delegación? Se interrumpirá el panel del asignado; el historial se conserva.",
      ),
    );
    const buttons = element("span", "delegation-confirm-actions");
    buttons.append(
      action("Sí, cancelar", "cancel-confirm", { delegation: id }, "btn destructive btn-small"),
      action("No", "cancel-abort", { delegation: id }, "btn ghost btn-small"),
    );
    confirm.append(buttons);
    card.append(confirm);
  }

  return card;
}

function detailRow(label, value) {
  const row = element("div", "delegation-detail-row");
  row.append(element("span", "delegation-detail-label", label), element("span", null, value || "—"));
  return row;
}

function detailSection(title, body) {
  const section = element("section", "delegation-detail-section");
  section.append(element("h3", null, title), body);
  return section;
}

// The project, pane and id are for a support question, not for reading the task: they stay folded.
function internalDetails(delegation) {
  const box = element("details", "delegation-internal");
  box.append(element("summary", null, "Datos internos ▸"));
  const home = chat.home && chat.home.bootstrap ? chat.home.bootstrap : null;
  const project = projectName(home ? home.projects : [], delegation.project_id);
  if (project) box.append(detailRow("Proyecto", project));
  box.append(detailRow("Panel", delegation.pane_id || "sin panel"), detailRow("Identificador", delegation.id));
  return box;
}

function renderDetail(view) {
  const detail = view.detail;
  const delegation = detail.delegation;
  const title = detail.card
    ? cleanText(detail.card.title)
    : `${delegation.from_name} pidió a ${delegation.to_name}`;
  $("delegation-detail-title").textContent = title;
  $("delegation-detail-kicker").textContent = detailKicker();
  const status = $("delegation-detail-status");
  const chip = statusChip(statusMeta(delegation.state));
  chip.id = "delegation-detail-status";
  status.replaceWith(chip);

  const body = $("delegation-detail-body");
  body.replaceChildren();
  body.append(
    detailRow("De", delegation.from_name),
    detailRow("A", delegation.to_name),
    detailRow("Estado", statusMeta(delegation.state).label),
  );
  const reason = view.reason_text || reasonText(delegation.reason);
  if (reason) {
    body.append(detailSection("Motivo", element("p", null, cleanText(reason))));
  }
  body.append(
    detailSection(
      "Tarea",
      element("p", "delegation-task", cleanText(delegation.task) || "Sin tarea registrada."),
    ),
  );

  const result = detail.result_message ? cleanText(detail.result_message.body_md) : "";
  body.append(
    detailSection(
      "Resultado",
      result
        ? element("pre", "delegation-result", result)
        : element("p", "delegation-muted", "Sin resultado guardado todavía."),
    ),
  );

  if (detail.children.length) {
    const list = element("div", "delegation-children");
    for (const child of detail.children) {
      const button = element("button", "delegation-child", "Subtarea");
      button.type = "button";
      button.dataset.openDelegation = child.id;
      button.append(element("code", null, shortId(child.id)));
      list.append(button);
    }
    body.append(detailSection("Hijos", list));
  }

  if (detail.synthesis) {
    const synthesis =
      { pending: "pendiente", queued: "en cola", skipped: "omitida" }[
        detail.synthesis.state
      ] || detail.synthesis.state;
    body.append(detailSection("Resumen de Alfred", element("p", null, synthesis)));
  }
  body.append(internalDetails(delegation));

  const pane = delegation.pane_id || (detail.card ? detail.card.pane_id : null);
  const agentButton = $("delegation-detail-agent");
  agentButton.classList.toggle("hidden", !pane);
  agentButton.dataset.pane = pane || "";
  // «Guardar como receta» lives here, not on the card, and only a finished job can become one.
  const recipe = $("delegation-detail-recipe");
  recipe.classList.toggle("hidden", delegation.state !== "done");
  recipe.dataset.delegation = delegation.id;
  $("delegation-detail").dataset.delegation = delegation.id;
}

export async function openDelegationDetail(id) {
  if (!id) return;
  const dialog = $("delegation-detail");
  $("delegation-detail-title").textContent = "Cargando delegación…";
  $("delegation-detail-kicker").textContent = shortId(id);
  $("delegation-detail-body").replaceChildren(element("p", "delegation-muted", "Leyendo desde el PC…"));
  if (!dialog.open) dialog.showModal();
  try {
    const view = await invoke("chat_delegation", { delegationId: id });
    renderDetail(view);
  } catch (error) {
    if (dialog.open) dialog.close();
    toast(errorMessage(error));
  }
}

export function installDelegationHandlers() {
  $("delegation-detail-close").addEventListener("click", () => $("delegation-detail").close());
  $("delegation-detail-agent").addEventListener("click", () => {
    const pane = $("delegation-detail-agent").dataset.pane;
    $("delegation-detail").close();
    if (pane) {
      app.detailReturn = "chat";
      openDetail(pane);
    }
  });
  $("delegation-detail-recipe").addEventListener("click", () => {
    const id = $("delegation-detail-recipe").dataset.delegation;
    $("delegation-detail").close();
    if (id) saveRecipeFromDelegation(id);
  });
  $("delegation-detail-body").addEventListener("click", (event) => {
    const button = event.target.closest("[data-open-delegation]");
    if (button) openDelegationDetail(button.dataset.openDelegation);
  });
}
// The Equipo panel's finished tasks: each row is its title, its state (a denial reads «No se hizo»)
// and «Ver detalle», which the thread no longer shows once a task is done.
export function paintFinishedTasks(entries) {
  const items = finishedTasks(entries);
  $("side-finished").classList.toggle("hidden", !items.length);
  $("side-finished-list").replaceChildren(...items.map(finishedRow));
}

function finishedRow(entry) {
  const row = element("div", "side-finished-row");
  const title = cleanText(entry.title) || "Delegación del equipo";
  const name = element("span", "side-finished-title", title);
  name.title = title;
  row.append(
    name,
    statusChip(cardState(entry)),
    action("Ver detalle", "detail", { delegation: delegationId(entry) }, "delegation-link"),
  );
  return row;
}
