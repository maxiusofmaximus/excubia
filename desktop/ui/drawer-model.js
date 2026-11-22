// The drawer's rules, with no DOM (tooling/drawer-model.test.mjs loads this in node).
// The drawer is the one menu of the app: ☰ or Ctrl+B opens it; Esc, a click on the veil,
// a choice in it or a change of section closes it. ui/drawer.js does the DOM work.

// What a key press asks for: Esc closes, Ctrl+B (or Cmd+B) toggles. Anything else is no drawer action.
export function keyAction(key, event = {}) {
  if (key === "Escape") return "close";
  const modified = (event.ctrlKey || event.metaKey) && !event.shiftKey && !event.altKey;
  return modified && String(key).toLowerCase() === "b" ? "toggle" : null;
}

// Where the drawer stands after an action.
export function drawerNext(open, action) {
  if (action === "open") return true;
  if (action === "toggle") return !open;
  if (action === "close" || action === "veil" || action === "pick") return false;
  return open;
}

// A plain click on a choice closes the drawer. Ctrl, Shift or Cmd picks a row of the history
// for several chats at once, so the drawer stays open for it.
export function pickAction(event = {}) {
  return event.ctrlKey || event.metaKey || event.shiftKey ? null : "pick";
}

// Tab inside the open drawer moves to the next item and wraps at both ends. With nothing
// focused in it, Tab takes the first item and Shift+Tab the last.
export function cycleIndex(count, index, backwards = false) {
  if (count <= 0) return -1;
  if (index < 0 || index >= count) return backwards ? count - 1 : 0;
  return (index + (backwards ? count - 1 : 1)) % count;
}
