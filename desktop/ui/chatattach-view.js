// Pictures in the thread: each one loads its bytes once from the host and opens in a
// plain full-size view that Esc or the close button dismisses.
import { invoke } from "./core.js";
import { element } from "./chatdom.js";

// Attachment id → the data URL of its bytes, or the request that will bring them.
const loaded = new Map();
let viewer = null;
let returnFocus = null;

export function attachmentsNode(conversationId, attachments) {
  const list = element("div", "msg-attachments");
  for (const attachment of attachments) {
    const button = element("button", "msg-attachment");
    button.type = "button";
    button.dataset.attachmentId = attachment.id;
    button.dataset.conversationId = conversationId;
    const image = element("img");
    image.alt = attachment.name || "Imagen adjunta";
    button.append(image);
    button.addEventListener("click", () => openViewer(button));
    list.append(button);
  }
  return list;
}

// Fills the thumbnails the thread has just drawn; a loaded one keeps its source.
export function hydrateAttachments() {
  for (const button of document.querySelectorAll(".msg-attachment[data-attachment-id]")) {
    loadInto(button);
  }
}

function fetchDataUrl(conversationId, attachmentId) {
  return invoke("chat_attachment_content", { conversationId, attachmentId }).then(
    ({ mime, data }) => `data:${mime};base64,${data}`,
  );
}

function loadInto(button) {
  const image = button.querySelector("img");
  if (image.getAttribute("src")) return;
  const attachmentId = button.dataset.attachmentId;
  if (!loaded.has(attachmentId)) {
    loaded.set(attachmentId, fetchDataUrl(button.dataset.conversationId, attachmentId));
  }
  loaded.get(attachmentId).then(
    (url) => {
      image.src = url;
      button.classList.add("is-loaded");
    },
    () => {
      loaded.delete(attachmentId);
      button.classList.add("is-failed");
    },
  );
}

function openViewer(button) {
  const image = button.querySelector("img");
  if (!image.getAttribute("src")) return;
  if (!viewer) viewer = buildViewer();
  returnFocus = button;
  viewer.picture.src = image.src;
  viewer.picture.alt = image.alt;
  viewer.node.hidden = false;
  viewer.close.focus();
  document.addEventListener("keydown", onKey);
}

function buildViewer() {
  const node = element("div", "attach-viewer");
  node.hidden = true;
  node.setAttribute("role", "dialog");
  node.setAttribute("aria-modal", "true");
  node.setAttribute("aria-label", "Imagen adjunta");
  const close = element("button", "btn secondary btn-small", "Cerrar");
  close.type = "button";
  const picture = element("img", "attach-viewer-image");
  node.append(close, picture);
  // A click outside the picture (the close button included) dismisses the view.
  node.addEventListener("click", (event) => {
    if (event.target !== picture) closeViewer();
  });
  document.body.append(node);
  return { node, close, picture };
}

function closeViewer() {
  viewer.node.hidden = true;
  viewer.picture.removeAttribute("src");
  document.removeEventListener("keydown", onKey);
  if (returnFocus && returnFocus.isConnected) returnFocus.focus();
  returnFocus = null;
}

function onKey(event) {
  if (event.key !== "Escape") return;
  event.preventDefault();
  closeViewer();
}
