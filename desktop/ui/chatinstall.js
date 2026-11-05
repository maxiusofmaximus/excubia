import { app, $, invoke, toast } from "./core.js";
import { showView } from "./nav.js";
import { openSettings } from "./settings.js";
import { openDetail } from "./detail.js";
import { installDelegationHandlers } from "./delegations.js";
import { messageById } from "./chat-reply-model.js";
import { bubbleMenuOpen, closeBubbleMenu, openBubbleMenu } from "./chatreply.js";
import { autoGrow, stopTurn } from "./chatcomposer.js";
import { jumpToBottom, renderJump } from "./chatjump.js";
import { createSender } from "./chatsend.js";
import { hasPendingAttachments, installAttachments, readyAttachments } from "./chatattach.js";
import { installVoice } from "./chatvoice.js";
import { openDraft } from "./mail-draft.js";
import { installDraftCards } from "./chatdraft.js";
import { createDelegations } from "./chatdelegations.js";
import { chat, applyFeed, failure, offline } from "./chatstate.js";
import {
  debounceSearch,
  filters,
  menu,
  patchConversation,
  refreshActive,
  refreshHome,
  renderChat,
  scheduleHome,
  scheduleRefresh,
  selectConversation,
} from "./chat.js";

const sender = createSender({ render: renderChat, home: scheduleHome });
const delegations = createDelegations({ render: renderChat, refresh: refreshActive });

function stop() {
  return stopTurn({ render: renderChat, refresh: scheduleRefresh });
}

function toggleSide() {
  const shell = $("chat-shell");
  const collapsed = shell.classList.toggle("side-collapsed");
  $("btn-side-toggle").setAttribute("aria-expanded", String(!collapsed));
  $("btn-side-toggle").setAttribute("aria-label", collapsed ? "Desplegar el panel del equipo" : "Plegar el panel del equipo");
  try {
    localStorage.setItem("hn-chat-side-collapsed", collapsed ? "1" : "0");
  } catch {
    // Preference only.
  }
}

async function newConversation() {
  try {
    const conversation = await sender.create(null);
    await refreshHome();
    if (conversation) selectConversation(conversation.id);
    $("chat-input").focus();
  } catch (error) {
    toast(failure(error).message);
  }
}

async function reconnect() {
  try {
    await invoke("connect");
    await refreshHome();
    if (chat.conversation) await refreshActive();
  } catch (error) {
    toast(failure(error).message);
  }
}

async function loadOlder() {
  if (!chat.conversation || !chat.nextBeforeSeq) return;
  try {
    const feed = await invoke("chat_history", {
      conversationId: chat.conversation.id,
      beforeSeq: chat.nextBeforeSeq,
    });
    const previousHeight = $("chat-timeline").scrollHeight;
    applyFeed(feed);
    renderChat();
    const timeline = $("chat-timeline");
    timeline.scrollTop = timeline.scrollHeight - previousHeight;
  } catch (error) {
    toast(failure(error).message);
  }
}

function clearReply() {
  chat.reply = null;
  renderChat();
}

function startReply(id) {
  const message = messageById(chat.entries, id);
  if (!message) return;
  chat.reply = { id, author_kind: message.author_kind, preview: message.body_md };
  renderChat();
  $("chat-input").focus();
}

// The quoted original scrolls into view when the thread has it loaded.
function jumpToMessage(id) {
  const row = $("chat-timeline").querySelector(`[data-message-id="${CSS.escape(id)}"]`);
  if (!row) return;
  row.scrollIntoView({ block: "center" });
  row.classList.add("is-flash");
  setTimeout(() => row.classList.remove("is-flash"), 1200);
}

function userTextBefore(seq) {
  for (let index = chat.entries.length - 1; index >= 0; index -= 1) {
    const entry = chat.entries[index];
    if (entry.kind !== "message" || entry.seq >= seq) continue;
    if (entry.message.author_kind === "user" && entry.message.body_md.trim()) {
      return entry.message.body_md;
    }
  }
  return null;
}

function composerKeydown(event) {
  if (event.key === "Escape" && chat.reply) {
    event.preventDefault();
    clearReply();
  } else if (event.key === "Enter" && !event.shiftKey) {
    event.preventDefault();
    $("chat-composer").requestSubmit();
  }
}

function submitComposer(event) {
  event.preventDefault();
  const input = $("chat-input");
  const text = input.value;
  if (hasPendingAttachments()) {
    toast("Espera a que terminen de subirse las imágenes.");
    return;
  }
  if (!text.trim() && !readyAttachments().length) return;
  input.value = "";
  autoGrow();
  sender.send(text, null, chat.reply);
}

export function installChatHandlers() {
  installAttachments({ ensureConversation: () => sender.create(null) });
  installVoice();
  installDraftCards();
  try {
    const collapsed = localStorage.getItem("hn-chat-side-collapsed") === "1";
    if (collapsed) {
      $("chat-shell").classList.add("side-collapsed");
      $("btn-side-toggle").setAttribute("aria-expanded", "false");
      $("btn-side-toggle").setAttribute("aria-label", "Desplegar el panel del equipo");
      $("btn-side-toggle").title = "Desplegar el panel";
    }
  } catch {
    // Preference only.
  }
  $("btn-team-foot").addEventListener("click", () => showView("agents"));
  $("btn-open-team").addEventListener("click", () => showView("agents"));
  $("btn-new-chat").addEventListener("click", () => newConversation());
  $("btn-chat-new").addEventListener("click", () => newConversation());
  $("btn-new-section").addEventListener("click", () => menu.openSection(null));
  $("btn-chat-more").addEventListener("click", () => {
    const conversation = chat.conversation;
    if (conversation) menu.openMenu(conversation, $("btn-chat-more"));
  });
  $("chat-search").addEventListener("input", debounceSearch);
  $("btn-archived").addEventListener("click", () => {
    chat.showArchived = !chat.showArchived;
    chat.searchResults = null;
    $("chat-search").value = "";
    chat.query = "";
    renderChat();
  });
  $("btn-side-toggle").addEventListener("click", toggleSide);
  $("chat-search").addEventListener("search", debounceSearch);
  $("chat-composer").addEventListener("submit", submitComposer);
  $("chat-input").addEventListener("keydown", composerKeydown);
  $("chat-input").addEventListener("input", autoGrow);
  $("btn-reply-cancel").addEventListener("click", clearReply);
  $("chat-filters").addEventListener("click", (event) => {
    const target = event.target.closest("[data-filter]");
    if (target) filters.setFilter(target.dataset.filter);
  });
  $("chat-timeline").addEventListener("contextmenu", (event) => {
    const bubble = event.target.closest(".msg[data-message-id]");
    if (!bubble) return;
    event.preventDefault();
    openBubbleMenu(event.clientX, event.clientY, bubble.dataset.messageId);
  });
  $("bubble-menu").addEventListener("click", (event) => {
    const item = event.target.closest("[data-reply-id]");
    if (!item) return;
    closeBubbleMenu();
    startReply(item.dataset.replyId);
  });
  $("btn-history-toggle").addEventListener("click", () => {
    $("chat-shell").classList.toggle("history-open");
  });
  $("chat-suggestions").addEventListener("click", (event) => {
    const button = event.target.closest("[data-suggestion]");
    if (!button) return;
    $("chat-input").value = button.dataset.suggestion;
    autoGrow();
    $("chat-input").focus();
  });

  $("chat-groups").addEventListener("click", (event) => {
    const target = event.target.closest("button");
    if (!target) return;
    if (target.dataset.select) {
      $("chat-shell").classList.remove("history-open");
      selectConversation(target.dataset.select);
    } else if (target.dataset.menu) {
      const id = target.dataset.menu;
      const conversation = [...(chat.home.conversations || []), ...(chat.home.archived || [])].find(
        (row) => row.id === id,
      );
      if (conversation) menu.openMenu(conversation, target);
    } else if (target.dataset.action === "new-section") {
      menu.openSection(null);
    } else if (target.dataset.action === "toggle-archived") {
      chat.showArchived = true;
      renderChat();
    }
  });

  $("side-body").addEventListener("click", (event) => {
    const row = event.target.closest("[data-pane]");
    if (!row) return;
    app.detailReturn = "chat";
    openDetail(row.dataset.pane).then((found) => {
      if (!found) toast("Ese panel ya no está disponible.");
    });
  });

  $("chat-menu").addEventListener("click", (event) => {
    const item = event.target.closest("[data-menu-action]");
    if (item) menu.runMenuAction(item.dataset.menuAction);
  });

  $("chat-timeline").addEventListener("scroll", renderJump);
  $("chat-jump").addEventListener("click", jumpToBottom);
  $("btn-send").addEventListener("click", (event) => {
    if (chat.waiting && !offline()) {
      event.preventDefault();
      stop();
    }
  });

  $("chat-timeline").addEventListener("click", (event) => {
    const target = event.target.closest("button");
    if (!target) return;
    if (target.dataset.replyId) startReply(target.dataset.replyId);
    else if (target.dataset.quoteId) jumpToMessage(target.dataset.quoteId);
    else if (target.dataset.quickReply !== undefined) sender.send(target.dataset.quickReply);
    else if (target.dataset.delegationAction) {
      delegations.handleAction(target.dataset);
    } else if (target.dataset.draftId) {
      openDraft(target.dataset.draftAccount, target.dataset.draftId, { alfredCanRevise: true });
    } else if (target.dataset.action === "dismiss-delegation-error") {
      chat.delegationError = null;
      renderChat();
    } else if (target.dataset.retryPending !== undefined) sender.retry(target.dataset.retryPending);
    else if (target.dataset.retrySeq) {
      const text = userTextBefore(Number(target.dataset.retrySeq));
      if (text) sender.send(text);
    } else if (target.dataset.action === "load-more") loadOlder();
    else if (target.dataset.action === "open-team") showView("agents");
    else if (target.dataset.action === "unarchive" && chat.conversation) {
      patchConversation(chat.conversation, { archived: false }, "Conversación desarchivada");
    } else if (target.dataset.action === "reconnect") reconnect();
    else if (target.dataset.action === "settings") openSettings();
    else if (target.dataset.action === "history") {
      if (chat.conversation) selectConversation(chat.conversation.id);
    }
  });

  $("chat-banner").addEventListener("click", (event) => {
    const target = event.target.closest("button");
    if (!target) return;
    if (target.dataset.action === "reconnect") reconnect();
    else if (target.dataset.action === "settings") openSettings();
    else if (target.dataset.action === "unarchive" && chat.conversation) {
      patchConversation(chat.conversation, { archived: false }, "Conversación desarchivada");
    } else if (target.dataset.action === "restart-host") {
      invoke("restart_host").catch((error) => toast(failure(error).message));
    }
  });

  $("delegation-kill-form").addEventListener("submit", delegations.cancelAll);
  $("delegation-kill-cancel").addEventListener("click", () => $("delegation-kill").close());
  installDelegationHandlers();

  $("chat-rename-form").addEventListener("submit", menu.saveRename);
  $("chat-rename-cancel").addEventListener("click", () => $("chat-rename").close());
  $("chat-section-form").addEventListener("submit", menu.saveSection);
  $("chat-section-cancel").addEventListener("click", () => $("chat-section").close());
  $("chat-section-clear").addEventListener("click", () => {
    const conversation = chat.sectionFor;
    $("chat-section").close();
    if (conversation) patchConversation(conversation, { clearSection: true }, "Sacada de la sección");
  });

  document.addEventListener("click", (event) => {
    if (!$("chat-menu").contains(event.target) && !event.target.closest("[data-menu]")) menu.closeMenu();
    if (bubbleMenuOpen() && !$("bubble-menu").contains(event.target)) closeBubbleMenu();
  });
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      menu.closeMenu();
      closeBubbleMenu();
    }
  });
}
