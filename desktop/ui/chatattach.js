// The attachment tray above the composer: pick, drop or paste up to four pictures or
// documents, watch each one prepare and upload, and send them with the next message.
import { $ } from "./core.js";
import { chat, failure } from "./chatstate.js";
import { element, action } from "./chatdom.js";
import { preparePicture } from "./chatattach-image.js";
import { documentBadgeNode } from "./chatattach-doc.js";
import { PICTURE_ICON, openPicture, pictureFallbackNode } from "./chatattach-view.js";
import { dropRemote, uploadPicture } from "./chatattach-upload.js";
import {
  ACCEPT_ATTR,
  blocksSend,
  canRetry,
  failureText,
  hasRoom,
  itemStatus,
  keepOutside,
  NOTICE_MS,
  outsideConversation,
  planAdds,
  progressPercent,
  readyItems,
  safeName,
  sizeLabel,
} from "./chatattach-model.js";

const items = [];
let ensureConversation = null;
let blocked = false;
let dragDepth = 0;
let dropZone = null;
let form = null;
let tray = null;
let status = null;
let attach = null;
let picker = null;
// The refusal line over the tray: its words, and the timer that clears it.
let notice = "";
let noticeTimer = null;
// The markup the tray last drew: a redraw that changes nothing keeps the buttons in place.
let shown = "";

const newId = () => crypto.randomUUID();

// `ensureConversation` creates the conversation a first picture belongs to.
export function installAttachments({ ensureConversation: create }) {
  ensureConversation = create;
  const css = element("link");
  css.rel = "stylesheet";
  css.href = "chatattach.css";
  document.head.append(css);

  form = $("chat-composer");
  attach = element("button", "attach-btn");
  attach.type = "button";
  attach.id = "btn-attach";
  attach.title = "Adjuntar imagen o archivo";
  attach.setAttribute("aria-label", "Adjuntar imagen o archivo");
  attach.innerHTML = PICTURE_ICON;
  $("btn-send").before(attach);

  tray = element("div", "attach-tray");
  tray.id = "chat-attach-tray";
  tray.hidden = true;
  status = element("p", "attach-status");
  status.setAttribute("aria-live", "polite");
  $("chat-reply").after(tray);

  picker = element("input");
  picker.type = "file";
  picker.id = "chat-file";
  picker.accept = ACCEPT_ATTR;
  picker.multiple = true;
  picker.hidden = true;
  form.append(picker);

  attach.addEventListener("click", () => picker.click());
  picker.addEventListener("change", () => {
    const files = [...picker.files];
    picker.value = "";
    addFiles(files);
  });
  tray.addEventListener("click", onTrayClick);
  $("chat-input").addEventListener("paste", onPaste);
  // Files dropped anywhere on the chat (thread and composer) are added; the window never opens them.
  dropZone = $("view-chat");
  dropZone.addEventListener("dragenter", onDragEnter);
  dropZone.addEventListener("dragover", onDragOver);
  dropZone.addEventListener("dragleave", onDragLeave);
  dropZone.addEventListener("drop", onDrop);
  document.addEventListener("dragover", keepFileInWindow);
  document.addEventListener("drop", keepFileInWindow);
}

// The composer redraws the tray. Drawing gives no picture up: see dropAttachmentsOutside.
export function renderAttachments(isBlocked) {
  blocked = isBlocked;
  renderTray();
}

// Called where the conversation changes: a tray picture of another one is given up.
export function dropAttachmentsOutside(conversationId) {
  drop((item) => outsideConversation(item, conversationId));
  renderTray();
}

export function hasPendingAttachments() {
  return blocksSend(items);
}

export function readyAttachments() {
  return readyItems(items);
}

// The pictures a send took leave the tray; they stay on the host.
export function takeAttachments(taken) {
  items.splice(0, items.length, ...keepOutside(items, taken));
  for (const item of taken) releasePreview(item);
  renderTray();
}

// The pictures of a refused message return to the front of the tray, in the order they were sent.
export function restoreAttachments(rows) {
  for (const item of rows) {
    item.preview = previewOf(item);
    item.broken = false;
  }
  items.unshift(...rows);
  renderTray();
}

function onPaste(event) {
  const files = [...(event.clipboardData ? event.clipboardData.files : [])];
  if (!files.length) return;
  event.preventDefault();
  addFiles(files);
}

const carriesFiles = (event) => Boolean(event.dataTransfer) && [...event.dataTransfer.types].includes("Files");

function onDragEnter(event) {
  if (!carriesFiles(event)) return;
  dragDepth += 1;
  dropZone.classList.add("is-dragging");
}

function onDragOver(event) {
  if (carriesFiles(event)) event.preventDefault();
}

function onDragLeave(event) {
  if (!carriesFiles(event)) return;
  dragDepth = Math.max(0, dragDepth - 1);
  if (!dragDepth) dropZone.classList.remove("is-dragging");
}

function onDrop(event) {
  if (!carriesFiles(event)) return;
  event.preventDefault();
  dragDepth = 0;
  dropZone.classList.remove("is-dragging");
  addFiles([...event.dataTransfer.files]);
}

// A file released outside the chat must not open in the window: the browser's own drop is cancelled.
function keepFileInWindow(event) {
  if (carriesFiles(event)) event.preventDefault();
}

function addFiles(files) {
  if (blocked) return;
  const plan = planAdds(files, items.length);
  showNotice(plan.code ? failureText(plan.code) : "");
  for (const { file, kind } of plan.accepted) startItem(file, kind);
  renderTray();
}

// A refusal stays until NOTICE_MS passes, a file is added or removed, or its × is pressed.
function showNotice(text) {
  clearNotice();
  notice = text;
  if (text) noticeTimer = setTimeout(() => { clearNotice(); renderTray(); }, NOTICE_MS);
}

function clearNotice() {
  clearTimeout(noticeTimer);
  noticeTimer = null;
  notice = "";
}

function noticeNodes() {
  if (!notice) return [];
  const close = action("×", { attachAction: "dismiss" }, "attach-notice-close");
  close.setAttribute("aria-label", "Cerrar aviso");
  return [element("span", null, notice), close];
}

// A picture is re-encoded before it leaves; a document is sent as it was picked.
function startItem(file, kind) {
  const item = {
    localId: newId(),
    clientId: newId(),
    kind,
    name: safeName(file.name),
    file,
    state: "preparing",
    sent: 0,
    size: 0,
    blob: null,
    preview: null,
    broken: false,
    id: null,
    conversationId: null,
    failure: null,
    removed: false,
  };
  items.push(item);
  renderTray();
  run(item, file);
}

// Prepares and uploads one picture. A failure stays on its row until the operator retries or removes it.
async function run(item, file) {
  try {
    const conversation = chat.conversation || (await ensureConversation());
    item.conversationId = conversation.id;
    item.blob = item.kind === "image" ? await preparePicture(file) : file;
    if (item.removed) return;
    item.size = item.blob.size;
    item.preview = previewOf(item);
    renderTray();
    await uploadPicture(conversation.id, item);
  } catch (error) {
    if (item.id) dropRemote(item.conversationId, item.id);
    item.id = null;
    item.state = "failed";
    item.failure = failure(error);
  }
  renderTray();
}

// «Reintentar» uploads the same file again under a new client id: the host dropped the failed upload.
function retry(item) {
  if (!canRetry(item) || !items.includes(item)) return;
  releasePreview(item);
  Object.assign(item, {
    clientId: newId(),
    state: "preparing",
    failure: null,
    sent: 0,
    size: 0,
    blob: null,
    broken: false,
  });
  renderTray();
  run(item, item.file);
}

function discard(item) {
  item.removed = true;
  releasePreview(item);
  if (item.id) dropRemote(item.conversationId, item.id);
}

function releasePreview(item) {
  if (item.preview) URL.revokeObjectURL(item.preview);
  item.preview = null;
}

// Only a picture has a thumbnail; a document row shows its badge instead.
function previewOf(item) {
  return item.kind === "image" && item.blob ? URL.createObjectURL(item.blob) : null;
}

// Removes, from the last row back, every row the predicate names.
function drop(predicate) {
  for (let index = items.length - 1; index >= 0; index -= 1) {
    if (!predicate(items[index])) continue;
    discard(items[index]);
    items.splice(index, 1);
  }
}

// The row buttons: «Quitar» in every state, «Reintentar» after a failure, the picture itself to open it.
function onTrayClick(event) {
  const control = event.target.closest("[data-attach-action]");
  if (control && control.dataset.attachAction === "dismiss") {
    clearNotice();
    renderTray();
    return;
  }
  const item = control && items.find((row) => row.localId === control.dataset.attachId);
  if (!item) return;
  if (control.dataset.attachAction === "remove") {
    clearNotice();
    drop((row) => row === item);
  } else if (control.dataset.attachAction === "retry") retry(item);
  else openItem(item, control);
  renderTray();
}

// The view gets its own object URL, freed when the view closes.
function openItem(item, control) {
  if (!item.blob) return;
  const url = URL.createObjectURL(item.blob);
  openPicture({ src: url, alt: item.name, returnTo: control, release: () => URL.revokeObjectURL(url) });
}

function renderTray() {
  attach.disabled = blocked || !hasRoom(items.length);
  status.replaceChildren(...noticeNodes());
  tray.hidden = !items.length && !notice;
  const nodes = [status, ...items.map(rowNode)];
  // The chat redraws on every feed event and each second while an agent is busy. A tray that
  // looks the same stays as it is, so a click that began on one of its buttons still lands on it.
  const html = nodes.map((node) => node.outerHTML).join("");
  if (html === shown) return;
  shown = html;
  tray.replaceChildren(...nodes);
}

function rowNode(item) {
  const row = element("div", `attach-item is-${item.state}`);
  const thumb = item.kind === "image" ? pictureThumb(item) : documentBadgeNode(item.name);

  const text = element("div", "attach-text");
  const detail = element(
    "span",
    "attach-detail",
    item.size ? `${sizeLabel(item.size)} · ${itemStatus(item)}` : itemStatus(item),
  );
  text.append(element("span", "attach-name", item.name), detail);
  if (item.state === "preparing" || item.state === "uploading") {
    const bar = element("progress", "attach-progress");
    bar.max = 100;
    if (item.state === "uploading") bar.setAttribute("value", String(progressPercent(item.sent, item.size)));
    bar.setAttribute("aria-label", `Progreso de ${item.name}`);
    text.append(bar);
  }

  const actions = element("div", "attach-actions");
  if (canRetry(item)) actions.append(rowButton("Reintentar", "retry", item));
  actions.append(rowButton("Quitar", "remove", item));
  row.append(thumb, text, actions);
  return row;
}

function rowButton(label, verb, item) {
  const button = action(label, { attachAction: verb, attachId: item.localId }, "btn ghost btn-small attach-action");
  button.setAttribute("aria-label", `${label} ${item.name}`);
  return button;
}

// The thumbnail of a picture: the neutral box until the picture draws, and a button that opens it once its bytes are here.
function pictureThumb(item) {
  const picture = item.preview && !item.broken ? previewImage(item) : pictureFallbackNode();
  if (!item.blob) return picture;
  const open = action("", { attachAction: "open", attachId: item.localId }, "attach-open");
  open.setAttribute("aria-label", `Ver ${item.name}`);
  open.append(picture);
  return open;
}

function previewImage(item) {
  const image = element("img", "attach-picture");
  image.alt = item.name;
  // A picture the browser cannot draw shows the neutral box instead of a broken image.
  image.addEventListener("error", () => {
    item.broken = true;
    renderTray();
  });
  image.src = item.preview;
  return image;
}
