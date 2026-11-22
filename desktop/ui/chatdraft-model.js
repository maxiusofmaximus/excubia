// Pure text rules for the mail draft card in the chat: no DOM. The host already
// sends the card without the body or the sender; the shell only fits the lines.
import { addressLine } from "./mail-model.js";

export const SUBJECT_MAX = 120;
export const PREVIEW_MAX = 160;

// Collapses the spaces and cuts past `max` characters with an ellipsis; characters, not
// UTF-16 units, so an accent or an emoji is never split in half.
export function clip(text, max) {
  const characters = Array.from(String(text || "").replace(/\s+/g, " ").trim());
  if (characters.length <= max) return characters.join("");
  return `${characters.slice(0, max - 1).join("")}…`;
}

export function draftCardLines(card) {
  return {
    subject: clip(card.subject, SUBJECT_MAX) || "(sin asunto)",
    to: clip(addressLine(card.to), SUBJECT_MAX),
    preview: clip(card.preview, PREVIEW_MAX),
  };
}

// The button names the draft by its subject, so a screen reader says which one it opens.
export function reviewLabel(subject) {
  return `Revisar borrador: ${subject}`;
}
