import { $ } from "./core.js";
import { paintAvatar } from "./avatar.js";
import { formatTime } from "./chatlist.js";
import { renderApprovalEntry, renderApprovalLine } from "./approval-card.js";
import { conversationContext, isOperatorLevel, supersededIndexes } from "./approval-model.js";
import { setThreadApprovals } from "./approval-inbox.js";
import { renderDelegationCard } from "./delegations.js";
import { stripTestMarkers } from "./format.js";
import { isOwn, turnOf, turnStatusLine } from "./chat-turn-model.js";
import { quickRepliesDisabled, quoteLabel, suggestionsAt } from "./chat-reply-model.js";
import { announcement, openStep } from "./chat-progress-model.js";
import { action, element } from "./chatdom.js";
import { renderFailure } from "./chatfailure.js";
import { bubbleBody } from "./chatattach-model.js";
import { attachmentsNode } from "./chatattach-view.js";
import { draftCardNode } from "./chatdraft.js";
import { chatSuggestions } from "./chat-suggestions.js";
import { teamSnapshot } from "./hires.js";

function messageNode(message, alfredName, line = null, detail = null) {
  const user = message.author_kind === "user";
  const wrapper = element("article", `msg ${user ? "user" : "agent"}`);
  wrapper.dataset.seq = String(message.seq);
  if (message.id) wrapper.dataset.messageId = message.id;

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

  const body = stripTestMarkers(bubbleBody(message));
  const bubble = element("div", "msg-bubble", body);
  if (!body) bubble.classList.add("is-empty");
  if (message.quote) {
    // The quoted line leads the bubble; a click scrolls to the original when it is loaded.
    const quote = element("button", "msg-quote", quoteLabel(message.quote, alfredName));
    quote.type = "button";
    quote.dataset.quoteId = message.quote.id;
    bubble.prepend(quote);
  }
  const meta = element("div", "msg-meta");
  meta.append(element("span", null, formatTime(message.created_at)));
  if (!user && (message.state === "failed" || message.state === "interrupted")) {
    meta.append(element("span", "failed", message.state === "interrupted" ? "interrumpido" : "falló"));
  }
  if (message.id) {
    const reply = element("button", "msg-reply", "Responder");
    reply.type = "button";
    reply.dataset.replyId = message.id;
    meta.append(reply);
  }
  const parts = [author, bubble];
  if (message.attachments && message.attachments.length) {
    parts.push(attachmentsNode(message.conversation_id, message.attachments));
  }
  if (line) parts.push(element("div", line.failed ? "msg-status is-failed" : "msg-status", line.text));
  if (detail) {
    const step = element("div", "msg-progress", detail.text);
    step.setAttribute("aria-live", detail.live);
    parts.push(step);
  }
  parts.push(meta);
  column.append(...parts);
  wrapper.append(column);
  return wrapper;
}

// Plain text only: the host's words are never read as markup.
function quickRepliesNode(items, disabled) {
  const row = element("div", "quick-replies");
  row.setAttribute("role", "group");
  row.setAttribute("aria-label", "Respuestas rápidas");
  for (const text of items) {
    const button = element("button", "quick-reply", text);
    button.type = "button";
    button.dataset.quickReply = text;
    button.setAttribute("aria-label", `Responder: ${text}`);
    button.disabled = disabled;
    row.append(button);
  }
  return row;
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
    meta.append(element("span", null, "Enviando…"));
  } else if (pending.state === "failed") {
    meta.append(element("span", "failed", "Falló"));
    const retry = element("button", "inline-retry", "Reintentar");
    retry.type = "button";
    retry.dataset.retryPending = pending.clientId;
    meta.append(retry);
  }
  return wrapper;
}

// The step under the last own message, only while its turn is in flight. The live
// reading is decided here, once per render that shows the line.
function stepDetail(chat) {
  const text = openStep(chat);
  if (!text) return null;
  return { text, live: announcement(chat.progress[chat.conversation.id], Date.now()) };
}

export function busySeconds(pending) {
  if (!pending.failure || pending.failure.kind !== "agentBusy") return 0;
  const retryAt = pending.failureAt + (pending.failure.retry_after_secs || 8) * 1000;
  return Math.max(0, Math.ceil((retryAt - Date.now()) / 1000));
}

const DAY_MS = 86400000;

// The suggestions of the empty chat follow the team contracted here; the day
// changes which phrase of each trade comes up.
export function paintChatSuggestions() {
  const hires = teamSnapshot().hires;
  $("chat-suggestions").replaceChildren(
    ...chatSuggestions(hires, Math.floor(Date.now() / DAY_MS)).map(suggestionNode),
  );
}

function suggestionNode(item) {
  const button = element("button", "suggestion");
  button.type = "button";
  button.setAttribute("role", "listitem");
  button.dataset.suggestion = item.prompt;
  const mark = element("span", "suggestion-mark", item.mark);
  mark.setAttribute("aria-hidden", "true");
  const text = element("span");
  text.append(element("strong", "", item.title));
  if (item.detail) text.append(element("small", "", item.detail));
  button.append(mark, text);
  return button;
}

export function renderThread(chat) {
  const timeline = $("chat-timeline");
  const empty = $("chat-empty");
  paintChatSuggestions();
  const alfredName = chat.home && chat.home.bootstrap ? chat.home.bootstrap.alfred.name : "Alfred";
  paintAvatar($("empty-alfred"), { name: alfredName, characterId: "alfred", px: 68 });
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
  // Only the operator's last message shows the step of its turn.
  let lastOwn = -1;
  chat.entries.forEach((entry, index) => {
    if (entry.kind === "message" && isOwn(entry)) lastOwn = index;
  });
  const quick = suggestionsAt(chat.entries);
  const sending = quickRepliesDisabled(chat.pending);
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
        const turn = isOwn(entry) ? turnOf(message, chat.turns) : null;
        const isLast = index === lastOwn;
        timeline.append(
          messageNode(
            message,
            alfredName,
            turnStatusLine(turn, alfredName, isLast),
            isLast ? stepDetail(chat) : null,
          ),
        );
      }
      if (quick && quick.index === index) timeline.append(quickRepliesNode(quick.items, sending));
    } else if (entry.kind === "card") {
      timeline.append(
        renderDelegationCard(entry.entry, { cancelTarget: chat.cancelTarget }),
      );
    } else if (entry.kind === "approval") {
      const node = renderApprovalLine(entry.entry);
      if (node) timeline.append(node);
    } else if (entry.kind === "mailDraft") {
      timeline.append(draftCardNode(entry.entry));
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
      element("p", null, chat.delegationError.message || "El PC rechazó la operación."),
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
    const step = openStep(chat);
    waiting.append(dots, element("span", null, step ? `${alfredName} · ${step}` : `${alfredName} está trabajando…`));
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
