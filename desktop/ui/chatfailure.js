// The failure card of the chat: a typed refusal read as a title, a line and its next step.

import { t } from "./i18n.js";
import { action, element } from "./chatdom.js";

const FAILURE_TITLES = {
  offline: "failure.offline",
  unauthorized: "failure.unauthorized",
  forbidden: "failure.forbidden",
  seatUnavailable: "failure.seat_unavailable",
  delegationRefused: "failure.delegation_refused",
  agentBusy: "failure.agent_busy",
  archived: "failure.archived",
  notFound: "failure.not_found",
  invalid: "failure.invalid",
  host: "failure.host",
  protocol: "failure.protocol",
};

// The core writes «Concédelo en el host: <command>» (or the English words); a marker the host no
// longer writes is not a reason to show the command as plain text.
const GRANT_MARKERS = ["Concédelo en el host: ", "Concédelo en el PC: ", "Grant it on the host: "];

function grantCommand(text) {
  const marker = GRANT_MARKERS.find((item) => text && text.includes(item));
  if (!marker) return null;
  return text.slice(text.indexOf(marker) + marker.length).trim();
}

export function renderFailure(failure, context = {}) {
  const kind = failure && failure.kind ? failure.kind : "host";
  const card = element("div", "error-card");
  card.dataset.failure = kind;

  const icon = element("span", "error-icon", "!");
  icon.setAttribute("aria-hidden", "true");
  const body = element("div", "error-body");
  const title = element("h3", null, t(FAILURE_TITLES[kind] || FAILURE_TITLES.host));
  const message = element("p", null, failure && failure.message ? failure.message : t("failure.generic"));
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
    actions.append(action(t("failure.reconnect"), { action: "reconnect" }, "btn primary btn-small"));
  } else if (kind === "unauthorized") {
    actions.append(action(t("failure.open_settings"), { action: "settings" }, "btn secondary btn-small"));
  } else if (kind === "seatUnavailable") {
    actions.append(
      action(t("failure.check_device"), { action: "open-team" }, "btn secondary btn-small"),
      action(t("agents.retry"), { retryPending: context.pendingId || "" }, "btn primary btn-small"),
    );
  } else if (kind === "agentBusy") {
    const left = context.retrySeconds || 0;
    const retry = action(
      left > 0 ? t("failure.retry_in", { s: left }) : t("agents.retry"),
      { retryPending: context.pendingId || "" },
      "btn primary btn-small",
    );
    retry.disabled = left > 0;
    actions.append(retry, action(t("failure.view_queue"), { action: "open-team" }, "btn ghost btn-small"));
  } else if (kind === "archived") {
    actions.append(
      action(t("failure.unarchive"), { action: "unarchive" }, "btn secondary btn-small"),
    );
  } else if (kind === "invalid" || kind === "notFound") {
    actions.append(action(t("failure.history"), { action: "history" }, "btn ghost btn-small"));
  } else if (kind === "forbidden") {
    actions.append(action(t("agents.retry"), { retryPending: context.pendingId || "" }, "btn ghost btn-small"));
  } else {
    actions.append(
      action(t("agents.retry"), { retryPending: context.pendingId || "" }, "btn secondary btn-small"),
    );
  }
  if (actions.childElementCount) body.append(actions);

  if (context.meta) {
    body.append(element("p", "error-meta", context.meta));
  }
  card.append(icon, body);
  return card;
}
