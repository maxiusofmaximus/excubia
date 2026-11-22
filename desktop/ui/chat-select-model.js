// The multi-selection of the chat history, free of the DOM so the tests can run it. As in
// the Windows Explorer: a click opens a chat and clears the selection, Ctrl+click toggles
// one row, Shift+click takes the range from the anchor. `order` is the ids of the visible
// rows, top to bottom.

export const DELETE_BATCH = 100;
// shortcut: the host allows 30 writes a minute, so archiving goes 25 per pause of a minute
// through the per-conversation patch; one bulk command would need no pause.
export const ARCHIVE_BATCH = 25;
export const ARCHIVE_PAUSE_MS = 61_000;

export function createSelection() {
  return { ids: new Set(), anchor: null };
}

export function only(selection, id) {
  selection.ids = new Set([id]);
  selection.anchor = id;
  return selection;
}

export function clickRow(selection, order, id, { ctrl = false, shift = false } = {}) {
  if (shift && selection.anchor !== null && order.includes(selection.anchor) && order.includes(id)) {
    const from = order.indexOf(selection.anchor);
    const to = order.indexOf(id);
    const [start, end] = from <= to ? [from, to] : [to, from];
    selection.ids = new Set(order.slice(start, end + 1));
  } else if (ctrl) {
    if (selection.ids.has(id)) selection.ids.delete(id);
    else selection.ids.add(id);
    selection.anchor = id;
  } else {
    selection.ids = new Set();
    selection.anchor = id;
  }
  return selection;
}

export function selectAll(selection, order) {
  selection.ids = new Set(order);
  if (selection.anchor === null && order.length) selection.anchor = order[0];
  return selection;
}

export function clear(selection) {
  selection.ids = new Set();
  selection.anchor = null;
  return selection;
}

// Drops the ids whose rows left the list (deleted, moved to another filter, searched away).
export function prune(selection, order) {
  const visible = new Set(order);
  for (const id of selection.ids) {
    if (!visible.has(id)) selection.ids.delete(id);
  }
  if (selection.anchor !== null && !visible.has(selection.anchor)) selection.anchor = null;
  return selection;
}

// The selected ids in the order they show, so a batch keeps the list's order.
export function inOrder(selection, order) {
  return order.filter((id) => selection.ids.has(id));
}

export function deleteBatches(ids, size = DELETE_BATCH) {
  const list = [...ids];
  const batches = [];
  for (let index = 0; index < list.length; index += size) {
    batches.push(list.slice(index, index + size));
  }
  return batches;
}
