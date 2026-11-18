// The one sidebar. It owns the destinations, the map from every screen to the
// destination it belongs to, and showView. The screens that open a destination's
// data (contacts, calendar…) are passed in by app.js through installNav.
// No imports: tooling/nav.test.mjs loads this module in node.

// Four sidebar entries. Agenda holds calendar, notes and contacts; Ajustes holds
// alerts and devices as sections of its own screens.
export const DESTINATIONS = ["chat", "agents", "agenda", "settings"];

// Start screens (first run, pairing, palette, onboarding) show no sidebar.
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

export function showView(name, owner = ownerOf(name)) {
  for (const view of VIEW_NAMES) {
    document.getElementById(`view-${view}`).classList.toggle("hidden", view !== name);
  }
  const bar = showsNav(name);
  document.querySelector(".app").classList.toggle("nav-hidden", !bar);
  for (const item of document.querySelectorAll("#app-nav [data-nav]")) {
    const current = bar && item.dataset.nav === owner;
    item.classList.toggle("is-active", current);
    if (current) item.setAttribute("aria-current", "page");
    else item.removeAttribute("aria-current");
  }
}

// Every screen that a click or Ctrl+K can open, by its name. The sidebar shows
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
