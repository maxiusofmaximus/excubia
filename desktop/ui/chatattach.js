// The picture tray above the composer: pick, drop or paste up to four images, watch
// each one prepare and upload, and send them with the next message.
import { $ } from "./core.js";
import { chat, failure } from "./chatstate.js";
import { element, action } from "./chatdom.js";
import { preparePicture } from "./chatattach-image.js";
import { dropRemote, uploadPicture } from "./chatattach-upload.js";
import {
  ACCEPT_ATTR,
  acceptsType,
  blocksSend,
  failureText,
  hasRoom,
  itemStatus,
  keepOutside,
  outsideConversation,
  progressPercent,
  readyItems,
  safeName,
  sizeLabel,
} from "./chatattach-model.js";

const items = [];
let ensureConversation = null;
let blocked = false;
let dragDepth = 0;
let form = null;
let tray = null;
let status = null;
let attach = null;
let picker = null;

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
  attach.title = "Adjuntar imagen";
  attach.setAttribute("aria-label", "Adjuntar imagen");
  attach.innerHTML =
    '<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" ' +
    'stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' +
    '<rect x="3" y="3" width="18" height="18" rx="3"></rect><circle cx="9" cy="9" r="2"></circle>' +
    '<path d="m21 15-5-5L5 21"></path></svg>';
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
  tray.addEventListener("click", (event) => {
    const button = event.target.closest("[data-attach-remove]");
    if (button) drop((item) => item.localId === button.dataset.attachRemove);
  });
  $("chat-input").addEventListener("paste", onPaste);
  form.addEventListener("dragenter", onDragEnter);
  form.addEventListener("dragover", onDragOver);
  form.addEventListener("dragleave", onDragLeave);
  form.addEventListener("drop", onDrop);
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
  for (const item of rows) item.preview = item.blob ? URL.createObjectURL(item.blob) : null;
  items.unshift(...rows);
  renderTray();
}

function onPaste(event) {
  const pictures = [...(event.clipboardData ? event.clipboardData.files : [])].filter((file) =>
    file.type.startsWith("image/"),
  );
  if (!pictures.length) return;
  event.preventDefault();
  addFiles(pictures);
}

const carriesFiles = (event) => Boolean(event.dataTransfer) && [...event.dataTransfer.types].includes("Files");

function onDragEnter(event) {
  if (!carriesFiles(event)) return;
  dragDepth += 1;
  form.classList.add("is-dragging");
}

function onDragOver(event) {
  if (carriesFiles(event)) event.preventDefault();
}

function onDragLeave(event) {
  if (!carriesFiles(event)) return;
  dragDepth = Math.max(0, dragDepth - 1);
  if (!dragDepth) form.classList.remove("is-dragging");
}

function onDrop(event) {
  if (!carriesFiles(event)) return;
  event.preventDefault();
  dragDepth = 0;
  form.classList.remove("is-dragging");
  addFiles([...event.dataTransfer.files]);
}

function addFiles(files) {
  if (blocked) return;
  status.textContent = "";
  for (const file of files) {
    if (!acceptsType(file.type)) {
      status.textContent = failureText("unsupported_type");
    } else if (!hasRoom(items.length)) {
      status.textContent = failureText("too_many_attachments");
      break;
    } else {
      startItem(file);
    }
  }
  renderTray();
}

function startItem(file) {
  const item = {
    localId: newId(),
    clientId: newId(),
    name: safeName(file.name),
    state: "preparing",
    sent: 0,
    size: 0,
    blob: null,
    preview: null,
    id: null,
    conversationId: null,
    failure: null,
    removed: false,
  };
  items.push(item);
  renderTray();
  run(item, file);
}

// Prepares and uploads one picture. A failure stays on its row until the operator removes it.
async function run(item, file) {
  try {
    const conversation = chat.conversation || (await ensureConversation());
    item.conversationId = conversation.id;
    item.blob = await preparePicture(file);
    if (item.removed) return;
    item.size = item.blob.size;
    item.preview = URL.createObjectURL(item.blob);
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

function discard(item) {
  item.removed = true;
  releasePreview(item);
  if (item.id) dropRemote(item.conversationId, item.id);
}

function releasePreview(item) {
  if (item.preview) URL.revokeObjectURL(item.preview);
  item.preview = null;
}

// Removes, from the last row back, every row the predicate names.
function drop(predicate) {
  for (let index = items.length - 1; index >= 0; index -= 1) {
    if (!predicate(items[index])) continue;
    discard(items[index]);
    items.splice(index, 1);
  }
}

function renderTray() {
  attach.disabled = blocked || !hasRoom(items.length);
  tray.hidden = !items.length && !status.textContent;
  tray.replaceChildren(status, ...items.map(rowNode));
}

function rowNode(item) {
  const row = element("div", `attach-item is-${item.state}`);
  const thumb = element("img", "attach-thumb");
  thumb.alt = item.name;
  if (item.preview) thumb.src = item.preview;

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
    if (item.state === "uploading") bar.value = progressPercent(item.sent, item.size);
    bar.setAttribute("aria-label", `Progreso de ${item.name}`);
    text.append(bar);
  }

  const remove = action("Quitar", { attachRemove: item.localId }, "btn ghost btn-small attach-remove");
  remove.setAttribute("aria-label", `Quitar ${item.name}`);
  row.append(thumb, text, remove);
  return row;
}
