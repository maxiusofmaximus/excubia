// The mail draft card in the chat timeline: subject, recipients and a preview, and the
// button that opens the draft for review. The click is routed by chatinstall.js.
import { element, action } from "./chatdom.js";
import { draftCardLines, reviewLabel } from "./chatdraft-model.js";

// The card's styles load with the chat, as the picture tray's do: thread.css is over its size limit.
export function installDraftCards() {
  const css = element("link");
  css.rel = "stylesheet";
  css.href = "chatdraft.css";
  document.head.append(css);
}

export function draftCardNode(card) {
  const lines = draftCardLines(card);
  const node = element("article", "draft-card");
  const head = element("div", "draft-card-head");
  head.append(element("span", "draft-card-mark", "BORRADOR"), element("strong", null, lines.subject));
  const open = action(
    "Revisar borrador",
    { draftId: card.draft_id, draftAccount: card.account },
    "btn secondary btn-small",
  );
  open.setAttribute("aria-label", reviewLabel(lines.subject));
  node.append(
    head,
    element("p", "draft-card-meta", `Para: ${lines.to}`),
    element("p", "draft-card-preview", lines.preview),
    open,
  );
  return node;
}
