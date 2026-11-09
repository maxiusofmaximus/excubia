// Documents in the chat: the badge a tray row shows for one, and the card a thread
// message shows. A click opens the file with the system's own program; the host's
// words for a failure are read as plain text.
import { invoke } from "./core.js";
import { element } from "./chatdom.js";
import { failure } from "./chatstate.js";
import { documentBadge, documentCard } from "./chatattach-model.js";
import { opensPreview } from "./chatattach-preview-model.js";
import { openPreview } from "./chatattach-preview.js";

// The coloured square a tray row or a card shows where a picture would be.
export function documentBadgeNode(name) {
  const { family, label } = documentBadge(name);
  const badge = element("span", `doc-badge is-${family}`, label);
  badge.setAttribute("aria-hidden", "true");
  return badge;
}

export function documentCardNode(conversationId, attachment) {
  const card = element("button", "msg-document");
  card.type = "button";
  card.dataset.attachmentId = attachment.id;
  card.dataset.conversationId = conversationId;
  const { name, size } = documentCard(attachment);
  const text = element("span", "doc-text");
  text.append(element("span", "doc-name", name), element("span", "doc-size", size));
  const status = element("span", "doc-status");
  status.setAttribute("aria-live", "polite");
  card.append(documentBadgeNode(name), text, status);
  card.addEventListener("click", () =>
    opensPreview(attachment.name)
      ? previewDocument(card, attachment)
      : openDocument(card),
  );
  return card;
}

// xlsx, docx and pdf show their pages; a failed page hands the file to the system as before.
function previewDocument(card, attachment) {
  openPreview({
    conversationId: card.dataset.conversationId,
    attachment: { id: attachment.id, name: attachment.name },
    returnTo: card,
    fallback: () => openDocument(card),
  });
}

async function openDocument(card) {
  const status = card.querySelector(".doc-status");
  card.disabled = true;
  card.classList.remove("is-failed");
  status.textContent = "Abriendo…";
  try {
    await invoke("chat_attachment_open", {
      conversationId: card.dataset.conversationId,
      attachmentId: card.dataset.attachmentId,
    });
    status.textContent = "";
  } catch (error) {
    card.classList.add("is-failed");
    status.textContent = failure(error).message;
  } finally {
    card.disabled = false;
  }
}
