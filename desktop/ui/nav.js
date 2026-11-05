// The one sidebar. It owns the destinations, the map from every screen to the
// destination it belongs to, and showView. The screens that open a destination's
// data (contacts, calendar…) are passed in by app.js through installNav.
// No imports: tooling/nav.test.mjs loads this module in node.

export const DESTINATIONS = ["chat", "agents", "calendar", "notes", "contacts", "alerts", "devices", "settings"];

// Start screens (first run, pairing, palette, onboarding) show no sidebar.
const START_VIEWS = new Set(["pair", "recovery", "lan", "palette", "onboarding"]);

// The destination each screen belongs to. Child screens highlight their parent.
const OWNER = {
  chat: "chat",
  agents: "agents",
  calendar: "calendar",
  notes: "notes",
  contacts: "contacts",
  alerts: "alerts",
  devices: "devices",
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

// Each destination gets its opener: the one place that says what a click does.
export function installNav(openers) {
  for (const item of document.querySelectorAll("#app-nav [data-nav]")) {
    const open = openers[item.dataset.nav];
    item.addEventListener("click", () => open());
  }
}

// The chat badge: the unread count the chat home already holds.
export function setNavBadge(count) {
  const badge = document.getElementById("nav-badge-chat");
  badge.textContent = count > 0 ? String(count) : "";
  badge.classList.toggle("hidden", !(count > 0));
}
