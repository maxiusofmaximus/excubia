// The page viewer of a document in the chat: a dialog that shows one page as an image, with
// «Anterior» and «Siguiente», and the document's own actions. Focus stays inside it and Esc closes it.
// The state and the words of each refusal are in chatattach-preview-model.js.
import { invoke } from "./core.js";
import { element } from "./chatdom.js";
import { failure } from "./chatstate.js";
import {
  PREVIEW_CACHE_SIZE,
  canGoNext,
  canGoPrevious,
  createPageCache,
  keyAction,
  markLastPage,
  nextFocus,
  nextPage,
  pageKey,
  pageLabel,
  previewOutcome,
  previousPage,
  startPreview,
} from "./chatattach-preview-model.js";

const cache = createPageCache(PREVIEW_CACHE_SIZE);
let viewer = null;
// The document on screen: its ids, the card that opened it, the old-style opener for a
// document without pages, the page state and whether a page is being fetched.
let session = null;

// Opens the viewer for one document. `fallback` opens the file the old way when the host has no page.
export function openPreview({ conversationId, attachment, returnTo, fallback }) {
  if (!viewer) viewer = buildViewer();
  session = { conversationId, attachment, returnTo, fallback, state: startPreview(), loading: false };
  const name = attachment.name || "Documento";
  viewer.title.textContent = name;
  viewer.node.setAttribute("aria-label", name);
  viewer.status.textContent = "";
  viewer.node.hidden = false;
  document.addEventListener("keydown", onKey);
  viewer.close.focus();
  showPage();
}

function buildViewer() {
  const node = element("div", "doc-preview");
  node.hidden = true;
  node.setAttribute("role", "dialog");
  node.setAttribute("aria-modal", "true");
  const title = element("span", "doc-preview-title");
  const close = button("Cerrar", "btn secondary btn-small");
  const picture = element("img", "doc-preview-page");
  picture.hidden = true;
  picture.alt = "";
  const message = element("p", "doc-preview-message");
  message.setAttribute("aria-live", "polite");
  const retry = button("Reintentar", "btn secondary btn-small");
  const previous = button("Anterior", "btn secondary btn-small");
  const label = element("span", "doc-preview-label");
  const next = button("Siguiente", "btn secondary btn-small");
  const open = button("Abrir con el programa del sistema", "btn secondary btn-small");
  const save = button("Guardar", "btn secondary btn-small");
  const status = element("p", "doc-preview-status");
  status.setAttribute("aria-live", "polite");

  const bar = element("div", "doc-preview-bar");
  const stage = element("div", "doc-preview-stage");
  const nav = element("div", "doc-preview-nav");
  const actions = element("div", "doc-preview-actions");
  bar.append(title, close);
  stage.append(picture, message, retry);
  nav.append(previous, label, next);
  actions.append(open, save);
  node.append(bar, stage, nav, actions, status);

  const built = { node, title, close, picture, message, retry, previous, label, next, open, save, status };
  close.addEventListener("click", closeViewer);
  previous.addEventListener("click", () => go("previous"));
  next.addEventListener("click", () => go("next"));
  retry.addEventListener("click", () => showPage());
  open.addEventListener("click", () => openWithSystem());
  save.addEventListener("click", () => saveCopy());
  // A click on the dimmed backdrop, outside the dialog's contents, closes the viewer.
  node.addEventListener("click", (event) => {
    if (event.target === node) closeViewer();
  });
  document.body.append(node);
  return built;
}

function button(text, className) {
  const made = element("button", className, text);
  made.type = "button";
  return made;
}

// Shows the page of the current state: from the cache when it is there, else from the host.
function showPage() {
  const current = session;
  if (!current) return;
  const { page } = current.state;
  const key = pageKey(current.attachment.id, page);
  if (cache.has(key)) {
    showImage(current, cache.get(key));
    return;
  }
  current.loading = true;
  showMessage(current, "Cargando la página…", false);
  invoke("chat_attachment_preview", {
    conversationId: current.conversationId,
    attachmentId: current.attachment.id,
    page,
  }).then(
    ({ mime, data }) => {
      const url = `data:${mime};base64,${data}`;
      cache.set(key, url);
      if (session === current) showImage(current, url);
    },
    (error) => {
      if (session === current) showError(current, error, page);
    },
  );
}

function showImage(current, url) {
  if (session !== current) return;
  current.loading = false;
  viewer.picture.src = url;
  viewer.picture.alt = `${pageLabel(current.state.page)} de ${current.attachment.name || "documento"}`;
  viewer.picture.hidden = false;
  viewer.message.hidden = true;
  viewer.retry.hidden = true;
  renderNav(current);
}

function showMessage(current, text, retry) {
  viewer.picture.removeAttribute("src");
  viewer.picture.hidden = true;
  viewer.message.textContent = text;
  viewer.message.hidden = false;
  viewer.retry.hidden = !retry;
  renderNav(current);
}

// A refusal of one page: the end of the document, the system's program, or a message.
function showError(current, error, requested) {
  const outcome = previewOutcome(error, requested);
  if (outcome.kind === "end") {
    current.state = markLastPage(current.state, requested);
    showPage();
    return;
  }
  if (outcome.kind === "fallback") {
    const { fallback } = current;
    closeViewer();
    fallback();
    return;
  }
  current.loading = false;
  showMessage(current, outcome.message, outcome.retry);
}

function renderNav(current) {
  const { state, loading } = current;
  viewer.previous.disabled = loading || !canGoPrevious(state);
  viewer.next.disabled = loading || !canGoNext(state);
  viewer.label.textContent = pageLabel(state.page);
}

function go(direction) {
  if (!session || session.loading) return;
  const before = session.state;
  session.state = direction === "next" ? nextPage(before) : previousPage(before);
  if (session.state.page === before.page) return;
  showPage();
}

async function openWithSystem() {
  const current = session;
  if (!current) return;
  viewer.status.textContent = "Abriendo…";
  try {
    await invoke("chat_attachment_open", {
      conversationId: current.conversationId,
      attachmentId: current.attachment.id,
    });
    if (session === current) viewer.status.textContent = "";
  } catch (error) {
    if (session === current) viewer.status.textContent = failure(error).message;
  }
}

async function saveCopy() {
  const current = session;
  if (!current) return;
  try {
    const saved = await invoke("chat_attachment_save", {
      conversationId: current.conversationId,
      attachmentId: current.attachment.id,
    });
    if (session === current) viewer.status.textContent = saved ? "Archivo guardado." : "";
  } catch (error) {
    if (session === current) viewer.status.textContent = failure(error).message;
  }
}

function closeViewer() {
  const { returnTo } = session || {};
  viewer.node.hidden = true;
  viewer.picture.removeAttribute("src");
  document.removeEventListener("keydown", onKey);
  session = null;
  if (returnTo && returnTo.isConnected) returnTo.focus();
}

function onKey(event) {
  const action = keyAction(event.key);
  if (event.key === "Tab") {
    trapFocus(event);
    return;
  }
  if (!action) return;
  event.preventDefault();
  if (action === "close") closeViewer();
  else go(action);
}

// Tab cycles through the buttons the viewer shows; focus never leaves the dialog.
function trapFocus(event) {
  const items = [...viewer.node.querySelectorAll("button")].filter((item) => !item.hidden && !item.disabled);
  const target = nextFocus(items.indexOf(document.activeElement), items.length, event.shiftKey);
  if (target < 0) return;
  event.preventDefault();
  items[target].focus();
}
