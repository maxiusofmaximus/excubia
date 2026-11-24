// Pictures in the thread and in the tray: each one loads its bytes once from the host and
// opens in a plain full-size view that Esc or a click outside dismisses. Documents get a card.
import { invoke } from "./core.js";
import { t } from "./i18n.js";
import { element } from "./chatdom.js";
import { attachmentKind } from "./chatattach-model.js";
import { documentCardNode } from "./chatattach-doc.js";

// The picture icon, drawn where a picture is not (yet, or any more) on screen.
export const PICTURE_ICON =
  '<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" ' +
  'stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' +
  '<rect x="3" y="3" width="18" height="18" rx="3"></rect><circle cx="9" cy="9" r="2"></circle>' +
  '<path d="m21 15-5-5L5 21"></path></svg>';

// Attachment id → the data URL of its bytes, or the request that will bring them.
const loaded = new Map();
// The view on screen: its node, the element focus returns to and what runs when it closes.
let viewer = null;
let session = null;

export function pictureFallbackNode() {
  const node = element("span", "attach-fallback");
  node.setAttribute("aria-hidden", "true");
  node.innerHTML = PICTURE_ICON;
  return node;
}

export function attachmentsNode(conversationId, attachments) {
  const list = element("div", "msg-attachments");
  for (const attachment of attachments) {
    list.append(
      attachmentKind(attachment) === "document"
        ? documentCardNode(conversationId, attachment)
        : pictureNode(conversationId, attachment),
    );
  }
  return list;
}

function pictureNode(conversationId, attachment) {
  const button = element("button", "msg-attachment");
  button.type = "button";
  button.dataset.attachmentId = attachment.id;
  button.dataset.conversationId = conversationId;
  const image = element("img");
  image.alt = attachment.name || t("chatattach.picture_alt");
  // The picture shows only once its bytes are in; until then the neutral box stands in for it.
  button.append(image, pictureFallbackNode());
  button.addEventListener("click", () => openThumb(button));
  return button;
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

function openThumb(button) {
  const image = button.querySelector("img");
  if (image.getAttribute("src")) openPicture({ src: image.src, alt: image.alt, returnTo: button });
}

// The full-size view of one picture. `release` runs when the view closes: it frees a URL made for this view only.
export function openPicture({ src, alt, returnTo, release = null }) {
  if (session) closeViewer();
  if (!viewer) viewer = buildViewer();
  session = { returnTo, release };
  const label = alt || t("chatattach.picture_alt");
  viewer.node.setAttribute("aria-label", label);
  viewer.picture.src = src;
  viewer.picture.alt = label;
  viewer.node.hidden = false;
  viewer.close.focus();
  document.addEventListener("keydown", onKey);
}

function buildViewer() {
  const node = element("div", "attach-viewer");
  node.hidden = true;
  node.setAttribute("role", "dialog");
  node.setAttribute("aria-modal", "true");
  const close = element("button", "btn secondary btn-small", t("common.close"));
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
  if (!session) return;
  const { returnTo, release } = session;
  viewer.node.hidden = true;
  viewer.picture.removeAttribute("src");
  document.removeEventListener("keydown", onKey);
  session = null;
  if (release) release();
  if (returnTo && returnTo.isConnected) returnTo.focus();
}

function onKey(event) {
  if (event.key !== "Escape") return;
  event.preventDefault();
  closeViewer();
}
