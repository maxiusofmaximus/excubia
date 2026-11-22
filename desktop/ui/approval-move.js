// The checklist of a `mail.move` approval: one checkbox per mail while it waits, the
// host's own marks once it is decided. Sender, subject and reason are text only.

import { element } from "./chatdom.js";
import { destLabel, moveItems, subjectText, triageLabel } from "./approval-model.js";

function itemText(item) {
  const text = element("span", "move-text");
  text.append(
    element("span", "move-from", item.from_address || "Remitente desconocido"),
    element("span", "move-subject", subjectText(item.subject)),
  );
  return text;
}

function itemMeta(item) {
  const meta = element("span", "move-meta");
  meta.append(element("span", "approval-chip", destLabel(item.dest)));
  if (item.reason) meta.append(element("span", "move-reason", item.reason));
  meta.append(element("span", "approval-chip level", triageLabel(item.level)));
  return meta;
}

function checkbox(item, unchecked) {
  const box = element("input");
  box.type = "checkbox";
  box.checked = !unchecked.has(item.message_id);
  box.dataset.moveId = item.message_id;
  return box;
}

function moveRow(item, pending, unchecked) {
  const row = element("li", item.excluded ? "move-item excluded" : "move-item");
  const parts = [itemText(item), itemMeta(item)];
  if (pending && !item.excluded) {
    const label = element("label", "move-row");
    label.append(checkbox(item, unchecked), ...parts);
    row.append(label);
  } else {
    if (item.excluded) parts.push(element("span", "move-note", "no se mueve"));
    row.append(...parts);
  }
  return row;
}

export function moveBatch(entry, status, unchecked = new Set()) {
  const list = element("ul", "move-batch");
  list.setAttribute("aria-label", "Correos a mover");
  for (const item of moveItems(entry)) {
    list.append(moveRow(item, status === "pending", unchecked));
  }
  return list;
}
