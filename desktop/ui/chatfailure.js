// The failure card of the chat: a typed refusal read as a title, a line and its next step.

import { action, element } from "./chatdom.js";

const FAILURE_TITLES = {
  offline: "Sin conexión con el host",
  unauthorized: "El host ya no confía en este equipo",
  forbidden: "Falta el permiso RESPOND",
  seatUnavailable: "Alfred no pudo arrancar",
  delegationRefused: "La delegación no se pudo completar",
  agentBusy: "La cola de Alfred está llena",
  archived: "La conversación está archivada",
  notFound: "La conversación ya no existe",
  invalid: "El mensaje no es válido",
  host: "El host rechazó la petición",
  protocol: "Respuesta inesperada del host",
};

function grantCommand(text) {
  const marker = "Concédelo en el host: ";
  if (!text || !text.includes(marker)) return null;
  return text.slice(text.indexOf(marker) + marker.length).trim();
}

export function renderFailure(failure, context = {}) {
  const kind = failure && failure.kind ? failure.kind : "host";
  const card = element("div", "error-card");
  card.dataset.failure = kind;

  const icon = element("span", "error-icon", "!");
  icon.setAttribute("aria-hidden", "true");
  const body = element("div", "error-body");
  const title = element("h3", null, FAILURE_TITLES[kind] || FAILURE_TITLES.host);
  const message = element("p", null, failure && failure.message ? failure.message : "No se pudo completar la operación.");
  body.append(title, message);

  const command = grantCommand(failure ? failure.action : "");
  if (command) {
    const code = element("code", null, command);
    body.append(code);
  } else if (failure && failure.action && kind !== "forbidden") {
    const hint = element("p", null, failure.action);
    body.append(hint);
  }
  if (failure && failure.code) {
    body.append(element("code", null, failure.code));
  }

  const actions = element("div", "error-actions");
  if (kind === "offline") {
    actions.append(action("Reconectar", { action: "reconnect" }, "btn primary btn-small"));
  } else if (kind === "unauthorized") {
    actions.append(action("Abrir ajustes", { action: "settings" }, "btn secondary btn-small"));
  } else if (kind === "seatUnavailable") {
    actions.append(
      action("Revisar equipo", { action: "open-team" }, "btn secondary btn-small"),
      action("Reintentar", { retryPending: context.pendingId || "" }, "btn primary btn-small"),
    );
  } else if (kind === "agentBusy") {
    const left = context.retrySeconds || 0;
    const retry = action(
      left > 0 ? `Reintentar en ${left} s` : "Reintentar",
      { retryPending: context.pendingId || "" },
      "btn primary btn-small",
    );
    retry.disabled = left > 0;
    actions.append(retry, action("Ver cola", { action: "open-team" }, "btn ghost btn-small"));
  } else if (kind === "archived") {
    actions.append(
      action("Desarchivar", { action: "unarchive" }, "btn secondary btn-small"),
    );
  } else if (kind === "invalid" || kind === "notFound") {
    actions.append(action("Ver historial", { action: "history" }, "btn ghost btn-small"));
  } else if (kind === "forbidden") {
    actions.append(action("Reintentar", { retryPending: context.pendingId || "" }, "btn ghost btn-small"));
  } else {
    actions.append(
      action("Reintentar", { retryPending: context.pendingId || "" }, "btn secondary btn-small"),
    );
  }
  if (actions.childElementCount) body.append(actions);

  if (context.meta) {
    body.append(element("p", "error-meta", context.meta));
  }
  card.append(icon, body);
  return card;
}
