// Ctrl+K (or Ctrl+Mayús+P): one box to reach any action or conversation. The keys a
// browser would use to print are taken here, so they never open the print dialog.
import { t } from "./i18n.js";
import { $ } from "./core.js";
import { goTo } from "./nav.js";
import { openDrawer } from "./drawer.js";
import { paletteItems } from "./command-palette-model.js";

export function installCommandPalette({ conversations, selectConversation }) {
  const dialog = $("command-palette");
  const input = $("command-palette-input");
  const list = $("command-palette-list");
  let items = [];
  let active = 0;

  function paint() {
    items = paletteItems(input.value, conversations());
    active = Math.min(active, Math.max(items.length - 1, 0));
    list.replaceChildren(
      ...items.map((item, index) => {
        const row = document.createElement("li");
        row.className = `palette-item${index === active ? " is-active" : ""}`;
        row.setAttribute("role", "option");
        row.setAttribute("aria-selected", String(index === active));
        row.dataset.index = String(index);
        const label = document.createElement("span");
        label.textContent = item.label;
        row.append(label);
        if (item.hint) {
          const hint = document.createElement("span");
          hint.className = "palette-hint";
          hint.textContent = item.hint;
          row.append(hint);
        }
        return row;
      }),
    );
    if (!items.length) {
      const empty = document.createElement("li");
      empty.className = "palette-empty";
      empty.textContent = t("commandpalette.empty");
      list.append(empty);
    }
  }

  function open() {
    input.value = "";
    active = 0;
    paint();
    if (!dialog.open) dialog.showModal();
    input.focus();
  }

  function run(item) {
    if (!item) return;
    dialog.close();
    if (item.kind === "conversation") {
      document.querySelector('[data-nav="chat"]')?.click();
      selectConversation(item.id.slice("chat:".length));
    } else if (item.id === "new-chat") {
      document.querySelector('[data-nav="chat"]')?.click();
      $("btn-new-chat").click();
    } else if (item.id === "search") {
      document.querySelector('[data-nav="chat"]')?.click();
      // The history search lives in the drawer: open it, then put the caret in its field.
      openDrawer();
      $("chat-search").focus();
    } else if (item.id === "new-project") {
      document.querySelector('[data-nav="chat"]')?.click();
      $("btn-new-project").click();
    } else if (item.id.startsWith("nav:")) {
      goTo(item.id.slice(4));
    }
  }

  input.addEventListener("input", () => {
    active = 0;
    paint();
  });
  input.addEventListener("keydown", (event) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      active = Math.min(active + 1, items.length - 1);
      paint();
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      active = Math.max(active - 1, 0);
      paint();
    } else if (event.key === "Enter") {
      event.preventDefault();
      run(items[active]);
    }
  });
  list.addEventListener("click", (event) => {
    const row = event.target.closest("[data-index]");
    if (row) run(items[Number(row.dataset.index)]);
  });

  document.addEventListener(
    "keydown",
    (event) => {
      const ctrl = event.ctrlKey || event.metaKey;
      if (!ctrl) return;
      const key = event.key.toLowerCase();
      if (key === "k" || (event.shiftKey && key === "p")) {
        event.preventDefault();
        open();
      } else if (key === "p") {
        // A desktop app has nothing to print: the webview's print dialog stays shut.
        event.preventDefault();
      } else if (key === "n" && !event.shiftKey) {
        event.preventDefault();
        run({ id: "new-chat" });
      } else if (key === "f" && !event.shiftKey && !dialog.open) {
        event.preventDefault();
        run({ id: "search" });
      }
    },
    true,
  );
  return { open };
}
