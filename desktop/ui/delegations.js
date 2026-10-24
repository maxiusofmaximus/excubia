// Delegation cards, their detail dialog and the human text of every typed
// refusal. The host owns the state; this module only renders and requests.

import { $, app, invoke, toast } from "./core.js";
import { errorMessage, stripTestMarkers } from "./format.js";
import { formatWhen } from "./chatlist.js";
import { openDetail } from "./detail.js";

const STATUS = {
  queued: { label: "En cola", tone: "queued" },
  running: { label: "En curso", tone: "running" },
  waiting: { label: "Esperando", tone: "waiting" },
  done: { label: "Hecho", tone: "done" },
  failed: { label: "Falló", tone: "failed" },
  cancelled: { label: "Cancelado", tone: "cancelled" },
  interrupted: { label: "Interrumpido", tone: "interrupted" },
};

const REASON_TEXT = {
  delegation_timeout: "El turno de la delegación agotó el tiempo de espera.",
  pane_gone: "El panel del asignado desapareció antes de terminar.",
  host_restart: "El host se reinició y la delegación quedó interrumpida.",
  conversation_cancelled: "Se canceló el equipo de esta conversación.",
  user_cancelled: "La delegación se canceló desde el escritorio.",
  delegation_cancelled: "La delegación ya había terminado o estaba cancelada.",
  seat_unavailable: "El asiento no tenía un panel vivo cuando se asignó.",
  depth_exceeded: "La delegación supera la profundidad permitida (Alfred → lead → especialista).",
  pane_limit: "El host ya tiene seis paneles vivos; no se puede abrir otro.",
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

export function statusMeta(status) {
  return STATUS[status] || { label: status || "Sin estado", tone: "unknown" };
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

function statusChip(status) {
  const meta = statusMeta(status);
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
  const meta = statusMeta(status);
  const card = element("article", `delegation-card tone-${meta.tone}`);
  card.dataset.delegation = id;

  const head = element("div", "delegation-head");
  const titleWrap = element("div", "delegation-title-wrap");
  titleWrap.append(
    element("p", "delegation-kicker", "DELEGACIÓN"),
    element("h3", "delegation-title", cleanText(entry.title) || "Delegación del equipo"),
  );
  head.append(titleWrap, statusChip(status));
  card.append(head);

  const summary = element("p", "delegation-summary");
  const text = ["failed", "cancelled", "interrupted"].includes(status)
    ? reasonText(entry.summary)
    : cleanText(entry.summary);
  summary.textContent = text || "Sin resumen todavía.";
  card.append(summary);

  if (status === "running" || status === "queued") {
    const progress = element("div", "delegation-progress");
    progress.setAttribute("aria-hidden", "true");
    progress.append(element("span"));
    card.append(progress);
  }

  const metaLine = element("div", "delegation-meta");
  metaLine.append(element("code", null, shortId(id)));
  if (entry.pane_id) metaLine.append(element("span", null, entry.pane_id));
  if (entry.attention >= 3) metaLine.append(element("span", "delegation-attention", "requiere atención"));
  metaLine.append(element("span", null, formatWhen(entry.created_at)));
  card.append(metaLine);

  const actions = element("div", "delegation-actions");
  if (entry.pane_id) {
    actions.append(action("Abrir agente", "agent", { pane: entry.pane_id }));
  }
  actions.append(
    action(
      status === "done" ? "Ver resultado completo" : "Ver detalle",
      "detail",
      { delegation: id },
      "delegation-link",
    ),
  );
  const active = ["queued", "running", "waiting"].includes(status);
  if (active) {
    actions.append(
      action("Cancelar", "cancel", { delegation: id }, "btn ghost btn-small delegation-cancel"),
    );
  }
  card.append(actions);

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

function renderDetail(view) {
  const detail = view.detail;
  const delegation = detail.delegation;
  const title = detail.card
    ? cleanText(detail.card.title)
    : `${delegation.from_name} pidió a ${delegation.to_name}`;
  $("delegation-detail-title").textContent = title;
  $("delegation-detail-kicker").textContent = `Delegación · profundidad ${delegation.depth}`;
  const status = $("delegation-detail-status");
  const chip = statusChip(delegation.state);
  chip.id = "delegation-detail-status";
  status.replaceWith(chip);

  const body = $("delegation-detail-body");
  body.replaceChildren();
  body.append(
    detailRow("De", delegation.from_name),
    detailRow("A", delegation.to_name),
    detailRow("Estado", statusMeta(delegation.state).label),
    detailRow("Proyecto", delegation.project_id || "sin proyecto"),
    detailRow("Panel", delegation.pane_id || "sin panel"),
    detailRow("Identificador", delegation.id),
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
      "Resultado completo",
      result
        ? element("pre", "delegation-result", result)
        : element("p", "delegation-muted", "El host no guardó un resultado para esta delegación."),
    ),
  );

  if (detail.children.length) {
    const list = element("div", "delegation-children");
    for (const child of detail.children) {
      const button = element("button", "delegation-child", `Delegación hija · profundidad ${child.depth}`);
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
    body.append(detailSection("Síntesis de Alfred", element("p", null, synthesis)));
  }

  const pane = delegation.pane_id || (detail.card ? detail.card.pane_id : null);
  const agentButton = $("delegation-detail-agent");
  agentButton.classList.toggle("hidden", !pane);
  agentButton.dataset.pane = pane || "";
  $("delegation-detail").dataset.delegation = delegation.id;
}

export async function openDelegationDetail(id) {
  if (!id) return;
  const dialog = $("delegation-detail");
  $("delegation-detail-title").textContent = "Cargando delegación…";
  $("delegation-detail-kicker").textContent = shortId(id);
  $("delegation-detail-body").replaceChildren(element("p", "delegation-muted", "Leyendo el host…"));
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
  $("delegation-detail-body").addEventListener("click", (event) => {
    const button = event.target.closest("[data-open-delegation]");
    if (button) openDelegationDetail(button.dataset.openDelegation);
  });
}
