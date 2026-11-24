import { $, invoke, toast } from "./core.js";
import { t } from "./i18n.js";
import { chipName, chipsFor, filterArgs } from "./chat-filter-model.js";

// The history filter: one discreet control that shows the filter in use, and a
// small menu with the three choices and their counts. The row is hidden when the
// host has no counts route. The menu is built once per render; its open state
// lives in the DOM so a redraw never closes it by surprise.
export function renderFilters(chat) {
  const bar = $("chat-filters");
  const chips = chipsFor(chat.counts);
  bar.classList.toggle("hidden", !chips.length);
  const current = chips.find((chip) => chip.id === chat.filter) || chips[0];
  $("chat-filter-label").textContent = current ? current.label : t("drawer.filter_all");
  $("chat-filter-toggle").classList.toggle("is-filtered", chat.filter !== "all");
  const menu = $("chat-filter-menu");
  menu.replaceChildren();
  if (!chips.length) setFilterMenu(false);
  for (const chip of chips) {
    const item = document.createElement("button");
    item.type = "button";
    item.className = "chat-filter-item";
    item.setAttribute("role", "menuitemradio");
    item.setAttribute("aria-checked", String(chat.filter === chip.id));
    item.dataset.filter = chip.id;
    item.setAttribute("aria-label", chipName(chip));
    const label = document.createElement("span");
    label.textContent = chip.label;
    item.append(label);
    if (chip.text) {
      const count = document.createElement("span");
      count.className = "chat-filter-count";
      count.setAttribute("aria-hidden", "true");
      count.textContent = chip.text;
      item.append(count);
    }
    menu.append(item);
  }
}

export function setFilterMenu(open) {
  $("chat-filter-menu").classList.toggle("hidden", !open);
  $("chat-filter-toggle").setAttribute("aria-expanded", String(open));
}

export function filterMenuOpen() {
  return !$("chat-filter-menu").classList.contains("hidden");
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
