import { $, invoke, toast } from "./core.js";
import { chipName, chipsFor, filterArgs } from "./chat-filter-model.js";

// The chips above the history: one button per filter, pressed when it is the
// one shown. The row is hidden when the host has no counts route.
export function renderFilters(chat) {
  const bar = $("chat-filters");
  const chips = chipsFor(chat.counts);
  bar.classList.toggle("hidden", !chips.length);
  bar.replaceChildren();
  for (const chip of chips) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "chat-chip";
    button.dataset.filter = chip.id;
    button.setAttribute("aria-pressed", String(chat.filter === chip.id));
    button.setAttribute("aria-label", chipName(chip));
    const label = document.createElement("span");
    label.textContent = chip.label;
    button.append(label);
    if (chip.text) {
      const count = document.createElement("span");
      count.className = "chat-chip-count";
      count.setAttribute("aria-hidden", "true");
      count.textContent = chip.text;
      button.append(count);
    }
    bar.append(button);
  }
}

// The numbers of the chips and the rows of the chosen filter come from the
// host. One timer folds a burst of chat events into one request.
export function createHistoryFilters({ chat, render, failure }) {
  let countsTimer = null;

  async function refreshFiltered() {
    const args = filterArgs(chat.filter);
    if (!args) return;
    const token = ++chat.filterToken;
    try {
      const rows = await invoke("chat_filter_conversations", args);
      if (token === chat.filterToken) chat.filtered = rows;
    } catch (error) {
      if (token === chat.filterToken) {
        chat.filtered = [];
        toast(failure(error).message);
      }
    }
  }

  async function refreshCounts() {
    try {
      chat.counts = await invoke("chat_counts");
    } catch {
      // The chips keep their last numbers; a failed count never hides the list.
      chat.counts = chat.counts || {};
    }
    // An older host has no filtered list either: back to «Todos».
    if (chat.counts === null) chat.filter = "all";
    await refreshFiltered();
    render();
  }

  return {
    scheduleCounts(delay = 180) {
      clearTimeout(countsTimer);
      countsTimer = setTimeout(refreshCounts, delay);
    },
    cancel() {
      clearTimeout(countsTimer);
    },
    async setFilter(filter) {
      chat.filter = filter;
      chat.showArchived = false;
      chat.filtered = [];
      render();
      await refreshFiltered();
      render();
    },
  };
}
