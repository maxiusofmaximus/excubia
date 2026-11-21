// The destinations, the map from every screen to the destination it belongs to, and
// showView. The destinations sit in the drawer (ui/drawer.js), which closes on every change
// of screen. The screens that open a destination's data (contacts, calendar…) are passed in
// by app.js through installNav. tooling/nav.test.mjs loads this module in node, so nothing
// here touches the DOM at load time.
import { closeDrawer } from "./drawer.js";

// Four entries in the drawer. Agenda holds calendar, notes and contacts; Ajustes holds
// alerts and devices as sections of its own screens.
export const DESTINATIONS = ["chat", "agents", "agenda", "settings"];

// Start screens (first run, pairing, palette, onboarding) have no drawer and no ☰.
const START_VIEWS = new Set(["pair", "recovery", "lan", "palette", "onboarding"]);

// The destination each screen belongs to. Child screens highlight their parent.
const OWNER = {
  chat: "chat",
  agents: "agents",
  calendar: "agenda",
  notes: "agenda",
  contacts: "agenda",
  alerts: "settings",
  devices: "settings",
  settings: "settings",
  mail: "chat",
  detail: "agents",
};

export const VIEW_NAMES = [...Object.keys(OWNER), ...START_VIEWS];

export function ownerOf(view) {
  return OWNER[view] ?? null;
}

export function showsNav(view) {
  return !START_VIEWS.has(view);
}

let shownOwner = null;
const enteredViews = new Set();

export function showView(name, owner = ownerOf(name)) {
  for (const view of VIEW_NAMES) {
    document.getElementById(`view-${view}`).classList.toggle("hidden", view !== name);
  }
  // A screen comes in once, the first time it is shown (ui/comic.css). The class is
  // taken off after the animation, so showing it again does not replay it.
  const screen = document.getElementById(`view-${name}`);
  if (screen && !enteredViews.has(name)) {
    enteredViews.add(name);
    screen.classList.add("comic-enter");
    setTimeout(() => screen.classList.remove("comic-enter"), 450);
  }
  const bar = showsNav(name);
  // The diagonal curtain (ui/comic.css) runs when the section changes. It is
  // pointer-events: none and does not move focus; a quick second change restarts it.
  if (bar && owner && owner !== shownOwner && shownOwner !== null) {
    const curtain = document.getElementById("wipe");
    curtain?.classList.remove("go");
    void curtain?.offsetWidth;
    curtain?.classList.add("go");
  }
  if (bar) shownOwner = owner;
  for (const item of document.querySelectorAll("#app-nav [data-nav]")) {
    const current = bar && item.dataset.nav === owner;
    item.classList.toggle("is-active", current);
    if (current) item.setAttribute("aria-current", "page");
    else item.removeAttribute("aria-current");
  }
  // A new screen is a choice made: the drawer closes, and the focus goes to the ☰ of that screen.
  closeDrawer();
}

// Every screen that a click or Ctrl+K can open, by its name. The drawer shows
// only DESTINATIONS; the rest are reached from inside them.
let openers = {};

export function installNav(map) {
  openers = map;
  for (const item of document.querySelectorAll("#app-nav [data-nav]")) {
    const open = openers[item.dataset.nav];
    item.addEventListener("click", () => open());
  }
}

export function goTo(name) {
  openers[name]?.();
}

// The chat badge: the unread count the chat home already holds.
export function setNavBadge(count) {
  const badge = document.getElementById("nav-badge-chat");
  badge.textContent = count > 0 ? String(count) : "";
  badge.classList.toggle("hidden", !(count > 0));
}
