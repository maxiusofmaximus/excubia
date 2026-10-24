import { app, $, invoke, showView, toast } from "./core.js";
import { formatTime, renderHistory, renderSide } from "./chatlist.js";
import { busySeconds, renderThread } from "./chatthread.js";
import { setBootstrap } from "./approval-model.js";
import { openDetail } from "./detail.js";
import { installDelegationHandlers, openDelegationDetail } from "./delegations.js";

import { LAST_KEY, loadCache, saveCache } from "./chatcache.js";
import { createChatMenu } from "./chatmenu.js";

let refreshTimer = null;
let homeTimer = null;
let searchTimer = null;
let busyTimer = null;
let markingRead = false;

export const chat = {
  home: null,
  conversation: null,
  entries: [],
  pending: [],
  nextBeforeSeq: null,
  selectedId: null,
  query: "",
  searchResults: null,
  showArchived: false,
  waiting: false,
  forceScroll: false,
  refreshing: false,
  refreshQueued: false,
  cacheOnly: false,
  lastSync: null,
  loadFailure: null,
  menuFor: null,
  openToken: 0,
  cancelTarget: null,
  delegationError: null,
};

function failure(error) {
  if (error && typeof error === "object" && error.kind) return error;
  if (typeof error === "string") return { kind: "gateway", message: error };
  if (error && typeof error.message === "string") return { kind: "gateway", message: error.message };
  return { kind: "gateway", message: "No se pudo completar la operación." };
}

function connection() {
  return app.connection || { kind: "idle" };
}

function offline() {
  return connection().kind === "offline" || connection().kind === "fatal";
}

function hostName() {
  if (app.boot && app.boot.mode === "local") return "el host local";
  try {
    return new URL(app.boot.gateway_url).hostname || "el host";
  } catch {
    return "el host";
  }
}

function lastSeq() {
  return chat.entries.reduce((max, entry) => Math.max(max, entry.seq || 0), 0);
}

function renderHeader() {
  const alfred =
    (chat.home && chat.home.bootstrap && chat.home.bootstrap.alfred.name) || "Alfred";
  const title = chat.conversation ? chat.conversation.title : alfred;
  $("chat-title").textContent = title;
  $("chat-avatar").textContent = alfred.slice(0, 1).toUpperCase();
  $("btn-chat-more").disabled = !chat.conversation;
  const state = connection();
  let subtitle = `${alfred} · CTO`;
  if (chat.conversation && chat.conversation.archived) subtitle = `${alfred} · archivada`;
  else if (chat.waiting) subtitle = `${alfred} · trabajando`;
  else if (state.kind === "connected") subtitle = `${alfred} · en vivo`;
  else if (state.kind === "offline" || state.kind === "fatal") subtitle = `${alfred} · sin conexión`;
  else if (state.kind === "connecting") subtitle = `${alfred} · conectando…`;
  $("chat-subtitle").textContent = subtitle;
}

function banner(actions) {
  const element = $("chat-banner");
  element.replaceChildren();
  element.className = "chat-banner";
  const icon = document.createElement("span");
  icon.className = "banner-icon";
  icon.setAttribute("aria-hidden", "true");
  icon.textContent = "!";
  const text = document.createElement("span");
  text.dataset.bannerText = "";
  element.append(icon, text);
  if (actions) {
    const group = document.createElement("span");
    group.className = "banner-actions";
    for (const [label, data, style] of actions) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = style || "btn ghost btn-small";
      button.textContent = label;
      for (const [name, value] of Object.entries(data)) button.dataset[name] = value;
      group.append(button);
    }
    element.append(group);
  }
  return { element, text };
}

function renderBanner() {
  const host = app.boot && app.boot.host;
  const state = connection();
  if (host && host.state === "failed") {
    const { element, text } = banner([
      ["Reiniciar el host", { action: "restart-host" }, "btn secondary btn-small"],
    ]);
    const strong = document.createElement("strong");
    strong.textContent = "El host local no arrancó. ";
    text.append(strong, document.createTextNode(host.error || "Revisa el log y vuelve a intentarlo."));
    element.classList.remove("hidden");
    return;
  }
  if (offline()) {
    const { element, text } = banner([
      ["Reconectar", { action: "reconnect" }, "btn primary btn-small"],
      ...(state.kind === "fatal"
        ? [["Volver a emparejar", { action: "settings" }, "btn ghost btn-small"]]
        : []),
    ]);
    element.classList.add(state.kind === "fatal" ? "fatal" : "offline");
    const when = chat.lastSync ? formatTime(new Date(chat.lastSync).toISOString()) : "—";
    const strong = document.createElement("strong");
    strong.textContent = `Sin conexión con ${hostName()}. `;
    text.append(
      strong,
      document.createTextNode(
        `Última sincronización ${when}. Lo que ya está cargado sigue legible; no se puede enviar.`,
      ),
    );
    element.classList.remove("hidden");
    return;
  }
  if (!chat.home && chat.loadFailure) {
    const { element, text } = banner([
      ["Abrir ajustes", { action: "settings" }, "btn secondary btn-small"],
      ["Reintentar", { action: "reconnect" }, "btn ghost btn-small"],
    ]);
    const strong = document.createElement("strong");
    strong.textContent = "No se pudo cargar el chat. ";
    text.append(strong, document.createTextNode(chat.loadFailure.message || ""));
    element.classList.remove("hidden");
    return;
  }
  if (chat.conversation && chat.conversation.archived) {
    const { element, text } = banner([
      ["Desarchivar", { action: "unarchive" }, "btn secondary btn-small"],
    ]);
    const strong = document.createElement("strong");
    strong.textContent = "Conversación archivada. ";
    text.append(
      strong,
      document.createTextNode(
        "Puedes leerla, fijarla o moverla; para escribir, desarchívala.",
      ),
    );
    element.classList.remove("hidden");
    return;
  }
  if (chat.cacheOnly) {
    const { element, text } = banner([]);
    text.textContent = "Mostrando la última copia legible; aún no hay conexión con el host.";
    element.classList.remove("hidden");
    return;
  }
  $("chat-banner").classList.add("hidden");
}

function renderComposer() {
  const input = $("chat-input");
  const send = $("btn-send");
  const archived = Boolean(chat.conversation && chat.conversation.archived);
  const blocked = archived || offline();
  input.disabled = blocked;
  send.disabled = blocked;
  $("chat-composer").classList.toggle("is-blocked", blocked);
  if (archived) {
    input.placeholder = "Conversación archivada";
    $("chat-hint").textContent = "Desarchívala para escribir a Alfred";
  } else if (offline()) {
    input.placeholder = "Reconecta para escribir a Alfred";
    $("chat-hint").textContent = "La copia local es de solo lectura";
  } else {
    input.placeholder = "Mensaje a Alfred";
    $("chat-hint").textContent = "Enter envía · Shift+Enter salto de línea";
  }
}

function ensureBusyTicker() {
  const busy = chat.pending.some(
    (pending) => pending.failure && pending.failure.kind === "agentBusy" && busySeconds(pending) > 0,
  );
  if (busy && !busyTimer) {
    busyTimer = setInterval(() => render(), 1000);
  } else if (!busy && busyTimer) {
    clearInterval(busyTimer);
    busyTimer = null;
  }
}

export function renderChat() {
  renderHeader();
  renderHistory(chat);
  renderThread(chat);
  renderSide(chat, app.state ? app.state.agents : []);
  renderBanner();
  renderComposer();
  ensureBusyTicker();
  $("btn-archived").textContent = `Archivados (${chat.home ? chat.home.archived.length : 0})`;
  $("btn-archived").classList.toggle("is-active", chat.showArchived);
}

function applyFeed(feed) {
  chat.entries = feed.entries || [];
  chat.approvals = feed.approvals || [];
  chat.nextBeforeSeq = feed.next_before_seq || null;
  if (chat.conversation) chat.conversation.last_read_seq = feed.last_read_seq;
  const last = chat.entries[chat.entries.length - 1];
  if (last && last.kind === "message" && last.message.author_kind !== "user") {
    chat.waiting = false;
  }
  chat.forceScroll = chat.forceScroll || !chat.entries.length;
}

async function markRead() {
  if (!chat.conversation || markingRead) return;
  const conversationId = chat.conversation.id;
  const seq = lastSeq();
  if (!seq || seq <= (chat.conversation.last_read_seq || 0)) return;
  markingRead = true;
  try {
    const value = await invoke("chat_mark_read", { conversationId, lastSeq: seq });
    if (chat.conversation && chat.conversation.id === conversationId) {
      chat.conversation.last_read_seq = value;
    }
    scheduleHome();
  } catch {
    // The cursor is a convenience: a failed mark must not block reading.
  } finally {
    markingRead = false;
  }
}

export async function refreshHome() {
  try {
    chat.home = await invoke("chat_home");
    setBootstrap(chat.home);
    chat.loadFailure = null;
    if (offline()) chat.cacheOnly = true;
    if (chat.conversation) {
      const fresh = chat.home.conversations.find((row) => row.id === chat.conversation.id);
      if (fresh) chat.conversation = { ...chat.conversation, ...fresh };
    }
    renderChat();
    return true;
  } catch (error) {
    const failureInfo = failure(error);
    chat.loadFailure = failureInfo;
    if (!chat.home) loadCache(chat);
    renderChat();
    return false;
  }
}

export async function selectConversation(id, options = {}) {
  if (!id) return;
  menu.closeMenu();
  const token = ++chat.openToken;
  chat.selectedId = id;
  chat.forceScroll = true;
  try {
    const opened = await invoke("chat_open", { conversationId: id });
    if (token !== chat.openToken) return;
    chat.conversation = opened.conversation;
    chat.pending = [];
    chat.waiting = false;
    chat.cacheOnly = false;
    chat.cancelTarget = null;
    chat.delegationError = null;
    applyFeed(opened.feed);
    chat.lastSync = Date.now();
    localStorage.setItem(LAST_KEY, id);
    renderChat();
    saveCache(chat);
    await markRead();
    if (options.focus !== false) $("chat-input").focus();
  } catch (error) {
    if (token !== chat.openToken) return;
    const failureInfo = failure(error);
    chat.loadFailure = failureInfo;
    const restored = loadCache(chat, id);
    if (!restored || !chat.conversation || chat.conversation.id !== id) {
      const cached = chat.home && chat.home.conversations.find((row) => row.id === id);
      chat.conversation = cached || { id, title: "Conversación", archived: false, last_read_seq: 0 };
      chat.entries = [];
    }
    renderChat();
    if (!offline()) toast(failureInfo.message);
  }
}

export async function refreshActive() {
  if (!chat.conversation || chat.refreshing) {
    if (chat.refreshing) chat.refreshQueued = true;
    return;
  }
  const conversationId = chat.conversation.id;
  chat.refreshing = true;
  try {
    const feed = await invoke("chat_refresh", {
      conversationId,
      afterSeq: lastSeq(),
    });
    if (!chat.conversation || chat.conversation.id !== conversationId) return;
    applyFeed(feed);
    chat.lastSync = Date.now();
    chat.cacheOnly = false;
    renderChat();
    saveCache(chat);
    await markRead();
  } catch (error) {
    chat.loadFailure = failure(error);
    renderChat();
  } finally {
    chat.refreshing = false;
    if (chat.refreshQueued) {
      chat.refreshQueued = false;
      scheduleRefresh(0);
    }
  }
}

function scheduleRefresh(delay = 180) {
  clearTimeout(refreshTimer);
  refreshTimer = setTimeout(refreshActive, delay);
}

export function scheduleHome(delay = 400) {
  clearTimeout(homeTimer);
  homeTimer = setTimeout(refreshHome, delay);
}

export function chatOnEvent(event) {
  if (!event) return;
  if (event.conversation_id && chat.conversation && event.conversation_id === chat.conversation.id) {
    const refreshes = ["chat.message", "chat.conversation", "chat.delegation", "chat.activity", "chat.approval"];
    if (refreshes.includes(event.event)) scheduleRefresh();
  }
  if (event.conversation_id) scheduleHome();
}

export function chatOnConnection() {
  renderChat();
  if (connection().kind === "connected") {
    if (!chat.home) refreshHome();
    if (chat.conversation) scheduleRefresh(0);
  }
}

export function chatOnStateChange() {
  renderSide(chat, app.state ? app.state.agents : []);
  renderHeader();
}

function newClientId() {
  if (window.crypto && window.crypto.randomUUID) return window.crypto.randomUUID();
  return `c-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

async function createConversation(title) {
  const conversation = await invoke("chat_create", { title: title || null });
  if (!chat.home) chat.home = { conversations: [], archived: [], sections: [], bootstrap: null };
  chat.home.conversations = [conversation, ...chat.home.conversations.filter((row) => row.id !== conversation.id)];
  chat.conversation = conversation;
  chat.selectedId = conversation.id;
  chat.entries = [];
  chat.pending = [];
  chat.nextBeforeSeq = null;
  chat.forceScroll = true;
  localStorage.setItem(LAST_KEY, conversation.id);
  renderChat();
  return conversation;
}

export async function sendMessage(text, reuseClientId) {
  const body = String(text || "").replace(/\s+$/g, "");
  if (!body.trim()) return;
  if (!chat.conversation) {
    try {
      await createConversation(null);
    } catch (error) {
      toast(failure(error).message);
      return;
    }
  }
  if (chat.conversation.archived) {
    toast("La conversación está archivada.");
    return;
  }
  const conversationId = chat.conversation.id;
  const clientId = reuseClientId || newClientId();
  chat.pending.push({
    clientId,
    text: body,
    state: "sending",
    failure: null,
    failureAt: 0,
    createdAt: Date.now(),
  });
  chat.forceScroll = true;
  renderChat();
  try {
    const result = await invoke("chat_send", { conversationId, clientId, text: body });
    chat.pending = chat.pending.filter((pending) => pending.clientId !== clientId);
    if (!chat.conversation || chat.conversation.id !== conversationId) {
      scheduleHome();
      renderChat();
      return;
    }
    applyFeed(result.feed);
    chat.conversation = { ...chat.conversation, last_seq: result.message.seq };
    chat.waiting = true;
    chat.lastSync = Date.now();
    renderChat();
    saveCache(chat);
    scheduleHome();
  } catch (error) {
    const pending = chat.pending.find((item) => item.clientId === clientId);
    if (pending) {
      pending.state = "failed";
      pending.failure = failure(error);
      pending.failureAt = Date.now();
    }
    chat.waiting = false;
    renderChat();
    if (pending && pending.failure.kind === "agentBusy") ensureBusyTicker();
  }
}

function reuseFor(pending) {
  if (!pending || !pending.failure) return null;
  return ["offline", "gateway", "protocol"].includes(pending.failure.kind)
    ? pending.clientId
    : null;
}

function retryPending(clientId) {
  const pending = chat.pending.find((item) => item.clientId === clientId);
  if (!pending) return;
  if (pending.failure && pending.failure.kind === "agentBusy" && busySeconds(pending) > 0) return;
  const reuse = reuseFor(pending);
  chat.pending = chat.pending.filter((item) => item !== pending);
  sendMessage(pending.text, reuse);
  renderChat();
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

async function cancelDelegation(id) {
  chat.cancelTarget = null;
  try {
    await invoke("chat_cancel_delegation", { delegationId: id });
    chat.delegationError = null;
    toast("Delegación cancelada");
    await refreshActive();
  } catch (error) {
    const failureInfo = failure(error);
    chat.delegationError = failureInfo;
    toast(failureInfo.message);
    renderChat();
  }
}

async function cancelAllDelegations(event) {
  event.preventDefault();
  $("delegation-kill").close();
  if (!chat.conversation) return;
  try {
    const outcome = await invoke("chat_cancel_conversation", {
      conversationId: chat.conversation.id,
    });
    chat.delegationError = null;
    toast(
      outcome.cancelled.length
        ? `${outcome.cancelled.length} delegación(es) cancelada(s)`
        : "No había delegaciones activas",
    );
    await refreshActive();
  } catch (error) {
    const failureInfo = failure(error);
    chat.delegationError = failureInfo;
    toast(failureInfo.message);
    renderChat();
  }
}

function handleDelegationAction(dataset) {
  const action = dataset.delegationAction;
  if (action === "agent" && dataset.pane) {
    app.detailReturn = "chat";
    openDetail(dataset.pane).then((found) => {
      if (!found) toast("Ese panel ya no está disponible.");
    });
  } else if (action === "detail") {
    openDelegationDetail(dataset.delegation);
  } else if (action === "cancel") {
    chat.cancelTarget = dataset.delegation;
    renderChat();
  } else if (action === "cancel-abort") {
    chat.cancelTarget = null;
    renderChat();
  } else if (action === "cancel-confirm") {
    cancelDelegation(dataset.delegation);
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

function updateLocal(conversation) {
  if (!chat.home) return;
  const replace = (rows) => rows.map((row) => (row.id === conversation.id ? { ...row, ...conversation } : row));
  chat.home.conversations = replace(chat.home.conversations);
  chat.home.archived = replace(chat.home.archived);
  if (chat.conversation && chat.conversation.id === conversation.id) {
    chat.conversation = { ...chat.conversation, ...conversation };
  }
}

async function patchConversation(conversation, args, message) {
  try {
    const updated = await invoke("chat_update", { conversationId: conversation.id, ...args });
    updateLocal(updated);
    renderChat();
    saveCache(chat);
    scheduleHome(150);
    if (message) toast(message);
    return updated;
  } catch (error) {
    toast(failure(error).message);
    return null;
  }
}

const menu = createChatMenu({
  chat,
  render: renderChat,
  patch: patchConversation,
  failure,
});

// -- installation --------------------------------------------------------

function debounceSearch() {
  clearTimeout(searchTimer);
  const value = $("chat-search").value.trim();
  chat.query = value;
  if (!value) {
    chat.searchResults = null;
    renderChat();
    return;
  }
  searchTimer = setTimeout(async () => {
    try {
      chat.searchResults = await invoke("chat_search", { q: value });
      renderChat();
    } catch (error) {
      toast(failure(error).message);
    }
  }, 250);
}

function composerKeydown(event) {
  if (event.key === "Enter" && !event.shiftKey) {
    event.preventDefault();
    $("chat-composer").requestSubmit();
  }
}

function autoGrow() {
  const input = $("chat-input");
  input.style.height = "auto";
  input.style.height = `${Math.min(input.scrollHeight, 150)}px`;
}

function submitComposer(event) {
  event.preventDefault();
  const input = $("chat-input");
  const text = input.value;
  if (!text.trim()) return;
  input.value = "";
  autoGrow();
  sendMessage(text);
}

export function installChatHandlers() {
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
  $("tab-chat").addEventListener("click", () => showView("chat"));
  $("tab-team").addEventListener("click", () => showView("agents"));
  $("team-tab-chat").addEventListener("click", () => showView("chat"));
  $("team-tab-team").addEventListener("click", () => showView("agents"));
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
  $("btn-history-toggle").addEventListener("click", () => {
    $("chat-shell").classList.toggle("history-open");
  });
  for (const button of document.querySelectorAll("[data-suggestion]")) {
    button.addEventListener("click", () => {
      $("chat-input").value = button.dataset.suggestion;
      autoGrow();
      $("chat-input").focus();
    });
  }

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

  $("chat-timeline").addEventListener("click", (event) => {
    const target = event.target.closest("button");
    if (!target) return;
    if (target.dataset.delegationAction) {
      handleDelegationAction(target.dataset);
    } else if (target.dataset.action === "dismiss-delegation-error") {
      chat.delegationError = null;
      renderChat();
    } else if (target.dataset.retryPending !== undefined) retryPending(target.dataset.retryPending);
    else if (target.dataset.retrySeq) {
      const text = userTextBefore(Number(target.dataset.retrySeq));
      if (text) sendMessage(text);
    } else if (target.dataset.action === "load-more") loadOlder();
    else if (target.dataset.action === "open-team") showView("agents");
    else if (target.dataset.action === "unarchive" && chat.conversation) {
      patchConversation(chat.conversation, { archived: false }, "Conversación desarchivada");
    } else if (target.dataset.action === "reconnect") reconnect();
    else if (target.dataset.action === "settings") showView("pair");
    else if (target.dataset.action === "history") {
      if (chat.conversation) selectConversation(chat.conversation.id);
    }
  });

  $("chat-banner").addEventListener("click", (event) => {
    const target = event.target.closest("button");
    if (!target) return;
    if (target.dataset.action === "reconnect") reconnect();
    else if (target.dataset.action === "settings") showView("pair");
    else if (target.dataset.action === "unarchive" && chat.conversation) {
      patchConversation(chat.conversation, { archived: false }, "Conversación desarchivada");
    } else if (target.dataset.action === "restart-host") {
      invoke("restart_host").catch((error) => toast(failure(error).message));
    }
  });

  $("delegation-kill-form").addEventListener("submit", cancelAllDelegations);
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
  });
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape") menu.closeMenu();
  });
}

async function newConversation() {
  try {
    const conversation = await createConversation(null);
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

export async function chatBoot() {
  if (offline()) loadCache(chat);
  renderChat();
  const loaded = await refreshHome();
  if (!loaded && !chat.home) {
    renderChat();
    return;
  }
  if (!chat.home || !chat.home.conversations.length) {
    renderChat();
    return;
  }
  let last = null;
  try {
    last = localStorage.getItem(LAST_KEY);
  } catch {
    last = null;
  }
  const exists = chat.home.conversations.some((row) => row.id === last);
  const target = exists ? last : chat.home.conversations[0].id;
  await selectConversation(target, { focus: false });
}

export async function chatAfterConnect() {
  await refreshHome();
  if (chat.home && !chat.conversation && chat.home.conversations.length) {
    await selectConversation(chat.home.conversations[0].id, { focus: false });
  } else if (chat.conversation) {
    scheduleRefresh(0);
  }
}

export function chatReset() {
  clearTimeout(refreshTimer);
  clearTimeout(homeTimer);
  clearTimeout(searchTimer);
  clearInterval(busyTimer);
  busyTimer = null;
  chat.openToken += 1;
  chat.home = null;
  chat.conversation = null;
  chat.entries = [];
  chat.pending = [];
  chat.nextBeforeSeq = null;
  chat.selectedId = null;
  chat.searchResults = null;
  chat.showArchived = false;
  chat.waiting = false;
  chat.cacheOnly = false;
  chat.loadFailure = null;
  chat.cancelTarget = null;
  chat.delegationError = null;
  renderChat();
}

