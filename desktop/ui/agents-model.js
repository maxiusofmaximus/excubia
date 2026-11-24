// Whether an open «⋯» menu closes: Esc always, a click (key "click") only when it lands outside the menu.
export function shouldCloseMenu(key, insideMenu = false) {
  if (key === "Escape") return true;
  return key === "click" && !insideMenu;
}

// The host can list one agent twice (a lead that ran twice keeps its old pane): one row per pane.
// Agents without a pane id are never merged; the first row of a pane is the one that stays.
export function oneRowPerPane(agents) {
  const seen = new Set();
  return agents.filter((agent) => {
    if (!agent.pane_id) return true;
    if (seen.has(agent.pane_id)) return false;
    seen.add(agent.pane_id);
    return true;
  });
}
