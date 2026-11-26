import { locale, t } from "./i18n.js";
import { $, invoke } from "./core.js";
import { paintAvatar } from "./avatar.js";
import { installLinkOpener, renderMarkdown } from "./markdown.js";
import { formatTime } from "./chatlist.js";
import { renderApprovalEntry, renderApprovalLine } from "./approval-card.js";
import { conversationContext, isOperatorLevel, repeatsCard, rememberAction, supersededIndexes } from "./approval-model.js";
import { setThreadApprovals } from "./approval-inbox.js";
import { renderDelegationCard } from "./delegations.js";
import { showInThread } from "./delegation-model.js";
import { stripTestMarkers } from "./format.js";
import { STATUS_LABELS } from "./plan-model.js";
import { answeredAfter, delegationResultIndexes, isOwn, turnOf, turnStatusLine } from "./chat-turn-model.js";
import { answerModel, failedTitle } from "./chat-card-model.js";
import { quickRepliesDisabled, quoteLabel, suggestionsAt } from "./chat-reply-model.js";
import { announcement, openStep } from "./chat-progress-model.js";
import { action, element } from "./chatdom.js";
import { renderFailure } from "./chatfailure.js";
import { bubbleBody } from "./chatattach-model.js";
import { attachmentsNode } from "./chatattach-view.js";
import { draftCardNode } from "./chatdraft.js";
import { chatSuggestions } from "./chat-suggestions.js";
import { teamSnapshot } from "./hires.js";
import { chat as chatState } from "./chatstate.js";

// The highest message seq each conversation showed at the last paint. A message
// above it is new and slams in once (ui/comic.css); a repaint replays nothing.
const shownSeq = new Map();
let newAfter = Infinity;

// `continued`: Alfred's message right after another one of his. Only the first of a run
// shows his face and tail; the rest are indented to line up with the first bubble.
function messageNode(message, alfredName, line = null, detail = null, continued = false) {
  const user = message.author_kind === "user";
  const wrapper = element("article", `msg ${user ? "user" : "agent"}`);
  wrapper.dataset.seq = String(message.seq);
  if (message.seq > newAfter) wrapper.classList.add("is-new");
  if (message.id) wrapper.dataset.messageId = message.id;

  if (!user && continued) {
    wrapper.classList.add("is-cont");
  } else if (!user) {
    const avatar = element("span", "msg-avatar avatar");
    paintAvatar(avatar, { name: alfredName, characterId: "alfred", px: 44 });
    wrapper.append(avatar);
  }

  const column = element("div", "msg-column");
  const author = element("span", "msg-author");
  if (user) {
    author.append(element("span", "you", t("chat.thread.you")));
  } else {
    author.textContent = alfredName || "Alfred";
  }

  const body = stripTestMarkers(bubbleBody(message));
  // The user's words stay as typed; a reply is read as Markdown, built node by node.
  const bubble = user || !body ? element("div", "msg-bubble", body) : element("div", "msg-bubble is-md");
  if (!user && body) bubble.append(renderMarkdown(body));
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
  if (!user && answerModel(message)) meta.append(element("span", "msg-model", answerModel(message)));
  if (!user && (message.state === "failed" || message.state === "interrupted")) {
    meta.append(element("span", "failed", message.state === "interrupted" ? t("chat.thread.interrupted") : t("chat.thread.failed")));
  }
  if (message.id) {
    const reply = element("button", "msg-reply", t("chat.thread.reply"));
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
  row.setAttribute("aria-label", t("chat.thread.quick_replies_aria"));
  for (const text of items) {
    const button = element("button", "quick-reply", text);
    button.type = "button";
    button.dataset.quickReply = text;
    button.setAttribute("aria-label", t("chat.thread.quick_reply_aria", { text }));
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
    element("h3", null, failedTitle(message, t("chat.thread.failed_title", { who }))),
    element(
      "p",
      null,
      t("chat.thread.failed_body"),
    ),
  );
  const actions = element("div", "error-actions");
  actions.append(
    action(t("chat.thread.retry"), { retrySeq: String(message.seq) }, "btn primary btn-small"),
    action(t("chat.thread.open_team"), { action: "open-team" }, "btn secondary btn-small"),
  );
  body.append(actions);
  body.append(
    element(
      "p",
      "error-meta",
      t("chat.thread.error_meta", { id: message.id, state: message.state, time: formatTime(message.created_at) }),
    ),
  );
  card.append(icon, body);
  return card;
}

function noteNode(entry) {
  const note = element("div", "timeline-note");
  const mark = element("span", "note-mark", entry.kind === "card" ? t("chat.note.delegation") : t("chat.note.host"));
  const text = element("span");
  const title = entry.title || entry.summary || entry.preview || t("chat.thread.entry_fallback");
  text.append(element("strong", null, title));
  if (entry.summary && entry.title) text.append(document.createTextNode(` · ${entry.summary}`));
  if (entry.status) {
    const plan = entry.kind === "plan";
    text.append(document.createTextNode(` · ${plan ? STATUS_LABELS[entry.status] ?? entry.status : entry.status}`));
  }
  note.append(mark, text);
  // The decision is made on the plan's card under Equipo: the line only opens it.
  if (entry.kind === "plan" && entry.status === "proposed") {
    note.append(action(t("plan.review"), { action: "review-plan", planId: entry.ref_id }, "btn secondary btn-small"));
  }
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
    meta.append(element("span", null, t("chat.thread.sending")));
  } else if (pending.state === "failed") {
    meta.append(element("span", "failed", t("chat.thread.failed_cap")));
    const retry = element("button", "inline-retry", t("chat.thread.retry"));
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
// The starter prompts are for the first chat only; a returning operator sees none.
export function paintChatSuggestions() {
  const home = chatState.home;
  const hasHistory = home ? (home.conversations || []).length + (home.archived || []).length > 0 : false;
  const hires = teamSnapshot().hires;
  $("chat-suggestions").replaceChildren(
    ...(hasHistory ? [] : chatSuggestions(hires, Math.floor(Date.now() / DAY_MS)).map(suggestionNode)),
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

let linksInstalled = false;

export function renderThread(chat) {
  const timeline = $("chat-timeline");
  if (!linksInstalled && invoke) {
    installLinkOpener(timeline, (url) => invoke("open_link", { url }).catch(() => {}));
    linksInstalled = true;
  }
  const empty = $("chat-empty");
  paintChatSuggestions();
  const alfredName = chat.home && chat.home.bootstrap ? chat.home.bootstrap.alfred.name : "Alfred";
  paintAvatar($("empty-alfred"), { name: alfredName, characterId: "alfred", px: 224 });
  $("empty-alfred-name").textContent = alfredName;
  const previousScroll = timeline.scrollTop;
  const nearBottom =
    timeline.scrollHeight - timeline.scrollTop - timeline.clientHeight < 140 || chat.forceScroll;
  timeline.replaceChildren();
  const convId = chat.conversation ? chat.conversation.id : null;
  newAfter = shownSeq.has(convId) ? shownSeq.get(convId) : Infinity;
  shownSeq.set(
    convId,
    (chat.entries || []).reduce((top, entry) => (entry.kind === "message" ? Math.max(top, entry.message.seq) : top), 0),
  );

  if (!chat.conversation) {
    timeline.classList.add("hidden");
    empty.classList.remove("hidden");
    $("empty-chat-title").textContent = t("chat.empty.title");
    $("empty-chat-body").textContent =
      t("chat.empty.body");
    $("chat-title").textContent = alfredName;
    // No thread is open, so the strip shows every approval of the operator.
    setThreadApprovals([]);
    return;
  }

  if (!chat.entries.length && !chat.pending.length) {
    timeline.classList.add("hidden");
    empty.classList.remove("hidden");
    $("empty-chat-title").textContent = t("chat.empty.title");
    $("empty-chat-body").textContent =
      t("chat.empty.body");
  } else {
    empty.classList.add("hidden");
    timeline.classList.remove("hidden");
  }

  if (chat.nextBeforeSeq) {
    const tools = element("div", "timeline-tools");
    tools.append(action(t("chat.thread.load_more"), { action: "load-more" }, "btn ghost btn-small"));
    timeline.append(tools);
  }

  const cards = (chat.approvals || []).filter(isOperatorLevel);
  // Before the lines: a line names the action its card names, and the cards draw below them.
  cards.forEach(rememberAction);
  const cardIds = new Set(cards.map((approval) => approval.id));
  const superseded = supersededIndexes(chat.entries, cardIds);
  // Only the operator's last message shows the step of its turn.
  let lastOwn = -1;
  chat.entries.forEach((entry, index) => {
    if (entry.kind === "message" && isOwn(entry)) lastOwn = index;
  });
  const quick = suggestionsAt(chat.entries);
  const sending = quickRepliesDisabled(chat.pending);
  // A finished delegation's report lives on its card; its quick replies stay in place.
  const reported = delegationResultIndexes(chat.entries, chat.home?.bootstrap?.alfred?.agent_id);
  for (const [index, entry] of chat.entries.entries()) {
    if (superseded.has(index)) continue;
    if (reported.has(index)) {
      if (quick && quick.index === index) timeline.append(quickRepliesNode(quick.items, sending));
      continue;
    }
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
        const before = chat.entries[index - 1];
        const continued =
          message.author_kind !== "user" &&
          Boolean(before && before.kind === "message" && before.message.author_kind !== "user");
        timeline.append(
          messageNode(
            message,
            alfredName,
            turnStatusLine(turn, alfredName, isLast, answeredAfter(chat.entries, index)),
            isLast ? stepDetail(chat) : null,
            continued,
          ),
        );
      }
      if (quick && quick.index === index) timeline.append(quickRepliesNode(quick.items, sending));
    } else if (entry.kind === "card") {
      // A finished task is closed by Alfred's line; its card waits under Equipo.
      if (showInThread(entry.entry)) {
        timeline.append(renderDelegationCard(entry.entry, { cancelTarget: chat.cancelTarget }));
      }
    } else if (entry.kind === "approval") {
      // A denial its card already settles («✕ Denegado») is not said twice.
      const node = repeatsCard(entry.entry, cardIds) ? null : renderApprovalLine(entry.entry);
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
          meta: t("chat.thread.attempt", {
            time: new Date(pending.createdAt).toLocaleTimeString(locale(), {
              hour: "2-digit",
              minute: "2-digit",
            }),
          }),
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
      element("h3", null, t("chat.thread.delegation_failed")),
      element("p", null, chat.delegationError.message || t("chat.thread.pc_rejected")),
    );
    if (chat.delegationError.code) {
      body.append(element("code", null, chat.delegationError.code));
    }
    const actions = element("div", "error-actions");
    const dismiss = element("button", "btn ghost btn-small", t("chat.thread.dismiss"));
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
    waiting.append(dots, element("span", null, step ? `${alfredName} · ${step}` : t("chat.thread.working", { name: alfredName })));
    timeline.append(waiting);
  }

  if (chat.conversation && chat.conversation.archived) {
    timeline.append(
      noteNode({
        kind: "host",
        title: t("chat.thread.archived"),
        summary: t("chat.thread.archived_hint"),
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
