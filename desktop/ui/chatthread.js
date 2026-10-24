import { $ } from "./core.js";
import { formatTime } from "./chatlist.js";
import { renderApprovalEntry, renderApprovalLine } from "./approval-card.js";
import { conversationContext, isOperatorLevel, supersededIndexes } from "./approval-model.js";
import { setThreadApprovals } from "./approval-inbox.js";
import { renderDelegationCard } from "./delegations.js";
import { stripTestMarkers } from "./format.js";

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
  gateway: "El host rechazó la petición",
  protocol: "Respuesta inesperada del host",
};

function element(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function action(label, data, className = "btn ghost btn-small") {
  const button = element("button", className, label);
  button.type = "button";
  for (const [name, value] of Object.entries(data)) {
    button.dataset[name] = value;
  }
  return button;
}

function grantCommand(text) {
  const marker = "Concédelo en el host: ";
  if (!text || !text.includes(marker)) return null;
  return text.slice(text.indexOf(marker) + marker.length).trim();
}

export function renderFailure(failure, context = {}) {
  const kind = failure && failure.kind ? failure.kind : "gateway";
  const card = element("div", "error-card");
  card.dataset.failure = kind;

  const icon = element("span", "error-icon", "!");
  icon.setAttribute("aria-hidden", "true");
  const body = element("div", "error-body");
  const title = element("h3", null, FAILURE_TITLES[kind] || FAILURE_TITLES.gateway);
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

function messageNode(message, alfredName) {
  const user = message.author_kind === "user";
  const wrapper = element("article", `msg ${user ? "user" : "agent"}`);
  wrapper.dataset.seq = String(message.seq);

  if (!user) {
    const avatar = element("span", "msg-avatar", (alfredName || "A").slice(0, 1).toUpperCase());
    avatar.setAttribute("aria-hidden", "true");
    wrapper.append(avatar);
  }

  const column = element("div", "msg-column");
  const author = element("span", "msg-author");
  if (user) {
    author.append(element("span", "you", "TÚ"));
  } else {
    author.textContent = alfredName || "Alfred";
  }

  const body = stripTestMarkers(message.body_md);
  const bubble = element("div", "msg-bubble", body);
  if (!body) bubble.classList.add("is-empty");
  const meta = element("div", "msg-meta");
  meta.append(element("span", null, formatTime(message.created_at)));
  if (message.state === "failed" || message.state === "interrupted") {
    meta.append(element("span", "failed", message.state === "interrupted" ? "interrumpido" : "falló"));
  } else if (user && message.state === "complete") {
    meta.append(element("span", "ok", "enviado"));
  }
  column.append(author, bubble, meta);
  wrapper.append(column);
  return wrapper;
}

function failedTurnNode(message, alfredName) {
  const card = element("div", "error-card");
  const icon = element("span", "error-icon", "!");
  icon.setAttribute("aria-hidden", "true");
  const body = element("div", "error-body");
  const who = alfredName || "Alfred";
  body.append(
    element("h3", null, `${who} terminó sin responder`),
    element(
      "p",
      null,
      "El turno falló antes de llamar a chat.reply; no se inventó una respuesta. El proyecto, las tarjetas y el historial se conservan.",
    ),
  );
  const actions = element("div", "error-actions");
  actions.append(
    action("Reintentar", { retrySeq: String(message.seq) }, "btn primary btn-small"),
    action("Abrir equipo", { action: "open-team" }, "btn secondary btn-small"),
  );
  body.append(actions);
  body.append(
    element("p", "error-meta", `${message.id} · estado ${message.state} · ${formatTime(message.created_at)}`),
  );
  card.append(icon, body);
  return card;
}

function noteNode(entry) {
  const note = element("div", "timeline-note");
  const mark = element("span", "note-mark", entry.kind === "card" ? "DELEG" : "HOST");
  const text = element("span");
  const title = entry.title || entry.summary || entry.preview || "Entrada del timeline";
  text.append(element("strong", null, title));
  if (entry.summary && entry.title) text.append(document.createTextNode(` · ${entry.summary}`));
  if (entry.status) text.append(document.createTextNode(` · ${entry.status}`));
  note.append(mark, text);
  return note;
}

function pendingNode(pending, alfredName) {
  const wrapper = messageNode(
    {
      author_kind: "user",
      body_md: pending.text,
      state: pending.state === "sent" ? "complete" : "",
      created_at: new Date(pending.createdAt).toISOString(),
      seq: 0,
    },
    alfredName,
  );
  const meta = wrapper.querySelector(".msg-meta");
  meta.replaceChildren();
  meta.append(element("span", null, formatTime(new Date(pending.createdAt).toISOString())));
  if (pending.state === "sending") {
    meta.append(element("span", null, "enviando…"));
  } else if (pending.state === "failed") {
    meta.append(element("span", "failed", "falló"));
    const retry = element("button", "inline-retry", "Reintentar");
    retry.type = "button";
    retry.dataset.retryPending = pending.clientId;
    meta.append(retry);
  } else {
    meta.append(element("span", "ok", "enviado"));
  }
  return wrapper;
}

export function busySeconds(pending) {
  if (!pending.failure || pending.failure.kind !== "agentBusy") return 0;
  const retryAt = pending.failureAt + (pending.failure.retry_after_secs || 8) * 1000;
  return Math.max(0, Math.ceil((retryAt - Date.now()) / 1000));
}

export function renderThread(chat) {
  const timeline = $("chat-timeline");
  const empty = $("chat-empty");
  const alfredName = chat.home && chat.home.bootstrap ? chat.home.bootstrap.alfred.name : "Alfred";
  const previousScroll = timeline.scrollTop;
  const nearBottom =
    timeline.scrollHeight - timeline.scrollTop - timeline.clientHeight < 140 || chat.forceScroll;
  timeline.replaceChildren();

  if (!chat.conversation) {
    timeline.classList.add("hidden");
    empty.classList.remove("hidden");
    $("empty-chat-title").textContent = "¿Qué movemos hoy?";
    $("empty-chat-body").textContent =
      "Dime un objetivo y reparto el trabajo entre Ernesto y su equipo. Yo te traigo el resultado.";
    $("chat-title").textContent = alfredName;
    // No thread is open, so the strip shows every approval of the operator.
    setThreadApprovals([]);
    return;
  }

  if (!chat.entries.length && !chat.pending.length) {
    timeline.classList.add("hidden");
    empty.classList.remove("hidden");
    $("empty-chat-title").textContent = "¿Qué movemos hoy?";
    $("empty-chat-body").textContent =
      "Dime un objetivo y reparto el trabajo entre Ernesto y su equipo. Yo te traigo el resultado.";
  } else {
    empty.classList.add("hidden");
    timeline.classList.remove("hidden");
  }

  if (chat.nextBeforeSeq) {
    const tools = element("div", "timeline-tools");
    tools.append(action("Cargar mensajes anteriores", { action: "load-more" }, "btn ghost btn-small"));
    timeline.append(tools);
  }

  const cards = (chat.approvals || []).filter(isOperatorLevel);
  const superseded = supersededIndexes(chat.entries, new Set(cards.map((approval) => approval.id)));
  for (const [index, entry] of chat.entries.entries()) {
    if (superseded.has(index)) continue;
    if (entry.kind === "message") {
      const message = entry.message;
      if (
        message.author_kind !== "user" &&
        (message.state === "failed" || message.state === "interrupted") &&
        !message.body_md.trim()
      ) {
        timeline.append(failedTurnNode(message, alfredName));
      } else {
        timeline.append(messageNode(message, alfredName));
      }
    } else if (entry.kind === "card") {
      timeline.append(
        renderDelegationCard(entry.entry, { cancelTarget: chat.cancelTarget }),
      );
    } else if (entry.kind === "approval") {
      const node = renderApprovalLine(entry.entry);
      if (node) timeline.append(node);
    } else {
      timeline.append(noteNode(entry.entry));
    }
  }

  // Requests of the operator's level are cards below the conversation; the
  // strip above the thread leaves them out.
  setThreadApprovals(cards.map((approval) => approval.id));
  for (const approval of cards) {
    timeline.append(renderApprovalEntry(approval, Date.now(), conversationContext(chat.conversation.id)));
  }

  for (const pending of chat.pending) {
    timeline.append(pendingNode(pending, alfredName));
    if (pending.state === "failed" && pending.failure) {
      timeline.append(
        renderFailure(pending.failure, {
          pendingId: pending.clientId,
          retrySeconds: busySeconds(pending),
          meta: `intento ${new Date(pending.createdAt).toLocaleTimeString("es-ES", {
            hour: "2-digit",
            minute: "2-digit",
          })}`,
        }),
      );
    }
  }

  if (chat.delegationError) {
    const card = element("div", "error-card");
    const icon = element("span", "error-icon", "!");
    icon.setAttribute("aria-hidden", "true");
    const body = element("div", "error-body");
    body.append(
      element("h3", null, "No se pudo completar la delegación"),
      element("p", null, chat.delegationError.message || "El host rechazó la operación."),
    );
    if (chat.delegationError.code) {
      body.append(element("code", null, chat.delegationError.code));
    }
    const actions = element("div", "error-actions");
    const dismiss = element("button", "btn ghost btn-small", "Cerrar aviso");
    dismiss.type = "button";
    dismiss.dataset.action = "dismiss-delegation-error";
    actions.append(dismiss);
    body.append(actions);
    card.append(icon, body);
    timeline.append(card);
  }

  if (chat.waiting) {
    const waiting = element("div", "waiting");
    const dots = element("span", "dots");
    for (let index = 0; index < 3; index += 1) dots.append(element("span"));
    waiting.append(dots, element("span", null, `${alfredName} está trabajando…`));
    timeline.append(waiting);
  }

  if (chat.conversation && chat.conversation.archived) {
    timeline.append(
      noteNode({
        kind: "host",
        title: "Conversación archivada",
        summary: "solo lectura hasta que la desarchives",
      }),
    );
  }

  if (nearBottom) {
    timeline.scrollTop = timeline.scrollHeight;
  } else {
    timeline.scrollTop = previousScroll;
  }
  chat.forceScroll = false;
}
