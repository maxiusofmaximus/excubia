// The drawer: the one menu of the app. ☰ or Ctrl+B slides it in from the left over a veil.
// Esc, a click on the veil, a choice in it or a change of section closes it, and the focus
// goes back to the ☰ on screen. The rules are in drawer-model.js; this file only touches the DOM.
import { cycleIndex, drawerNext, keyAction, pickAction } from "./drawer-model.js";

const FOCUSABLE =
  "button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), a[href], [tabindex]:not([tabindex='-1'])";

let open = false;

const drawer = () => document.getElementById("app-drawer");
const veil = () => document.getElementById("drawer-veil");

// A dialog (the palette, a confirmation) keeps the keys while it is open.
function dialogOpen() {
  return document.querySelector("dialog[open]") !== null;
}

// The ☰ on screen now. The drawer opens only where one shows: the start screens have none.
function visibleToggle() {
  return [...document.querySelectorAll("[data-drawer-toggle]")].find((button) => button.getClientRects().length > 0) ?? null;
}

function focusables() {
  return [...drawer().querySelectorAll(FOCUSABLE)].filter((element) => element.getClientRects().length > 0);
}

function paint(value) {
  open = value;
  drawer().hidden = !value;
  veil().hidden = !value;
  // The screen behind the drawer takes no focus and no click while the drawer is open.
  document.querySelector(".app").inert = value;
  for (const button of document.querySelectorAll("[data-drawer-toggle]")) {
    button.setAttribute("aria-expanded", String(value));
  }
}

export function drawerOpen() {
  return open;
}

export function openDrawer() {
  if (open || !visibleToggle()) return;
  paint(true);
  const current = drawer().querySelector("[data-nav].is-active") ?? focusables()[0];
  current?.focus();
}

export function closeDrawer({ focus = true } = {}) {
  if (!open) return;
  paint(false);
  if (focus && !dialogOpen()) visibleToggle()?.focus();
}

function run(action) {
  if (drawerNext(open, action)) openDrawer();
  else closeDrawer();
}

function trap(event) {
  const items = focusables();
  event.preventDefault();
  items[cycleIndex(items.length, items.indexOf(document.activeElement), event.shiftKey)]?.focus();
}

function onKey(event) {
  if (dialogOpen()) return;
  if (open && event.key === "Tab") {
    trap(event);
    return;
  }
  const action = keyAction(event.key, event);
  if (!action || (action === "close" && !open)) return;
  // The drawer takes the key, so Esc does not also clear the picked rows of the history.
  event.preventDefault();
  event.stopPropagation();
  run(action);
}

function onToggle(event) {
  if (event.target.closest("[data-drawer-toggle]")) run("toggle");
}

function onDrawerClick(event) {
  const choice = event.target.closest("[data-nav], #btn-new-chat, [data-select]");
  if (!choice) return;
  const action = choice.matches("[data-select]") ? pickAction(event) : "pick";
  if (action) run(action);
}

export function installDrawer() {
  document.addEventListener("keydown", onKey, true);
  document.addEventListener("click", onToggle);
  drawer().addEventListener("click", onDrawerClick);
  veil().addEventListener("click", () => run("veil"));
}
