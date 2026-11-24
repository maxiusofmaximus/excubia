// The conversation menu, the menu of several picked rows and the small dialogs (rename,
// section, delete). The factory keeps the chat state in one place instead of importing it back.

import { t } from "./i18n.js";
import { $, invoke, toast } from "./core.js";
import { conversationTitle } from "./chat-row-model.js";
import {
  ARCHIVE_BATCH,
  ARCHIVE_PAUSE_MS,
  DELETE_BATCH,
  createSelection,
  deleteBatches,
} from "./chat-select-model.js";

const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

export function createChatMenu({ chat, render, patch, failure, refresh }) {
  // The ids the open delete dialog removes, and the ids the open menu of several acts on.
  let deleting = [];
  let menuIds = [];

  function menuItem(label, action, options = {}) {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = label;
    button.dataset.menuAction = action;
    if (options.danger) button.classList.add("danger");
    return button;
  }

  function picked(ids) {
    if (!chat.home) return [];
    return [...chat.home.conversations, ...chat.home.archived].filter((row) => ids.includes(row.id));
  }

  // `rect` is the anchor's box: a button's, or a point's (a right click) as a zero-size box.
  function show(rect) {
    const menu = $("chat-menu");
    const width = 210;
    const left = Math.min(Math.max(8, rect.right - width), window.innerWidth - width - 8);
    const top = Math.min(rect.bottom + 5, window.innerHeight - 230);
    menu.classList.remove("hidden");
    menu.style.left = `${left}px`;
    menu.style.top = `${Math.max(8, top)}px`;
    menu.querySelector("button").focus();
  }

  function openMenu(conversation, anchor) {
    const menu = $("chat-menu");
    menu.replaceChildren();
    menu.append(
      menuItem(conversation.pinned ? t("chatmenu.unpin") : t("chatmenu.pin"), "pin"),
      menuItem(t("chatmenu.rename"), "rename"),
      menuItem(t("chatmenu.section"), "section"),
    );
    menu.append(document.createElement("hr"));
    menu.append(
      menuItem(conversation.archived ? t("chatmenu.unarchive") : t("chatmenu.archive"), "archive", {
        danger: !conversation.archived,
      }),
      menuItem(t("chatmenu.cancel_delegations"), "cancel-delegations", { danger: true }),
      menuItem(t("chatmenu.delete"), "delete", { danger: true }),
    );
    chat.menuFor = conversation;
    show(anchor.getBoundingClientRect());
  }

  // The menu of a right click on one row, at the pointer.
  function openMenuAt(conversation, point) {
    openMenu(conversation, { getBoundingClientRect: () => ({ right: point.x, bottom: point.y }) });
  }

  // The menu of several picked rows: only the actions that make sense for all of them.
  function openMany(ids, point) {
    const allArchived = picked(ids).every((row) => row.archived);
    const menu = $("chat-menu");
    menu.replaceChildren();
    menu.append(
      allArchived
        ? menuItem(t("chatmenu.unarchive_many", { n: ids.length }), "many-unarchive")
        : menuItem(t("chatmenu.archive_many", { n: ids.length }), "many-archive"),
      menuItem(t("chatmenu.delete_many", { n: ids.length }), "many-delete", { danger: true }),
    );
    menu.append(document.createElement("hr"));
    menu.append(menuItem(t("chatmenu.clear_selection"), "many-clear"));
    menuIds = ids;
    chat.menuFor = null;
    show({ right: point.x, bottom: point.y });
  }

  function closeMenu() {
    $("chat-menu").classList.add("hidden");
    chat.menuFor = null;
  }

  async function runMenuAction(action) {
    if (action.startsWith("many-")) {
      const ids = menuIds;
      closeMenu();
      if (action === "many-clear") {
        chat.selection = createSelection();
        render();
      } else if (action === "many-delete") {
        deleteIds(ids);
      } else {
        await archiveMany(ids, action === "many-archive");
      }
      return;
    }
    const conversation = chat.menuFor;
    closeMenu();
    if (!conversation) return;
    if (action === "pin") {
      await patch(
        conversation,
        { pinned: !conversation.pinned },
        conversation.pinned ? t("chatmenu.unpinned") : t("chatmenu.pinned"),
      );
    } else if (action === "rename") {
      openRename(conversation);
    } else if (action === "section") {
      openSection(conversation);
    } else if (action === "archive") {
      await patch(
        conversation,
        { archived: !conversation.archived },
        conversation.archived ? t("chatmenu.unarchived") : t("chatmenu.archived"),
      );
    } else if (action === "cancel-delegations") {
      $("delegation-kill").showModal();
    } else if (action === "delete") {
      deleteIds([conversation.id]);
    }
  }

  function deleteIds(ids) {
    if (!ids.length) return;
    deleting = ids;
    const only = ids.length === 1 ? picked(ids)[0] : null;
    $("chat-delete-title").textContent =
      ids.length > 1 ? t("chatmenu.delete_title_many", { n: ids.length }) : t("chatdelete.title");
    $("chat-delete-lede").textContent = only
      ? t("chatmenu.delete_lede_named", { title: conversationTitle(only) })
      : t("chatdelete.lede");
    $("chat-delete").showModal();
  }

  // The host takes up to 100 ids per call: one call per batch, never one per conversation.
  async function confirmDelete(event) {
    event.preventDefault();
    const ids = deleting;
    deleting = [];
    $("chat-delete").close();
    if (!ids.length || !invoke) return;
    const deleted = new Set();
    const failed = [];
    let error = null;
    try {
      for (const batch of deleteBatches(ids, DELETE_BATCH)) {
        const result = await invoke("chat_delete_conversations", { conversationIds: batch });
        for (const id of result.deleted) deleted.add(id);
        failed.push(...result.failed);
      }
    } catch (caught) {
      // A failed batch stops the rest; what was deleted before stays deleted.
      error = caught;
    }
    if (chat.conversation && deleted.has(chat.conversation.id)) {
      chat.conversation = null;
      chat.entries = [];
      chat.pending = [];
    }
    if (chat.home && deleted.size) {
      chat.home.conversations = chat.home.conversations.filter((row) => !deleted.has(row.id));
      chat.home.archived = chat.home.archived.filter((row) => !deleted.has(row.id));
    }
    chat.selection = createSelection();
    render();
    // Open work blocks a delete: the host says so and nothing is lost.
    if (error) toast(failure(error).message);
    else if (failed.length) {
      toast(
        failed.length === 1
          ? t("chatmenu.delete_failed.one")
          : t("chatmenu.delete_failed.other", { n: failed.length }),
      );
    } else if (deleted.size) {
      toast(deleted.size === 1 ? t("chatmenu.deleted.one") : t("chatmenu.deleted.other", { n: deleted.size }));
    }
    await refresh();
    render();
  }

  // Archive or unarchive the rows that are not in that state yet. Over 25 rows the host's
  // write limit (30 a minute) needs a pause: 25 per minute, shown in the bar as progress.
  async function archiveMany(ids, archived) {
    const targets = picked(ids).filter((row) => Boolean(row.archived) !== archived);
    chat.selection = createSelection();
    if (!targets.length) {
      render();
      return;
    }
    if (targets.length > ARCHIVE_BATCH) toast(t("chatmenu.archive_batches", { n: ARCHIVE_BATCH }));
    let done = 0;
    chat.bulkProgress = { done, total: targets.length, archived };
    render();
    for (const [index, row] of targets.entries()) {
      if (index > 0 && index % ARCHIVE_BATCH === 0) await pause(ARCHIVE_PAUSE_MS);
      if (await patch(row, { archived })) done += 1;
      chat.bulkProgress = { ...chat.bulkProgress, done: index + 1 };
      render();
    }
    chat.bulkProgress = null;
    render();
    toast(archived ? t("chatmenu.archived_count", { n: done }) : t("chatmenu.unarchived_count", { n: done }));
  }

  function openRename(conversation) {
    $("chat-rename-input").value = conversation.title;
    $("chat-rename-error").classList.add("hidden");
    $("chat-rename").showModal();
    $("chat-rename-input").focus();
    $("chat-rename-input").select();
    $("chat-rename").dataset.id = conversation.id;
  }

  function saveRename(event) {
    event.preventDefault();
    const title = $("chat-rename-input").value.trim();
    if (!title) {
      $("chat-rename-error").textContent = t("chatmenu.title_required");
      $("chat-rename-error").classList.remove("hidden");
      return;
    }
    const conversation = chat.home
      ? [...chat.home.conversations, ...chat.home.archived].find(
          (row) => row.id === $("chat-rename").dataset.id,
        )
      : null;
    $("chat-rename").close();
    if (conversation) patch(conversation, { title }, t("chatmenu.title_saved"));
  }

  function fillSections(selected) {
    const select = $("chat-section-select");
    select.replaceChildren();
    const none = document.createElement("option");
    none.value = "";
    none.textContent = t("chatmenu.pick_section");
    select.append(none);
    for (const section of chat.home ? chat.home.sections : []) {
      const option = document.createElement("option");
      option.value = section.id;
      option.textContent = section.name;
      if (section.id === selected) option.selected = true;
      select.append(option);
    }
    $("chat-section-name").value = "";
    $("chat-section-error").classList.add("hidden");
  }

  function openSection(conversation) {
    chat.sectionFor = conversation;
    fillSections(conversation ? conversation.section_id : null);
    $("chat-section").showModal();
    $("chat-section-select").focus();
  }

  async function saveSection(event) {
    event.preventDefault();
    const name = $("chat-section-name").value.trim();
    let sectionId = $("chat-section-select").value;
    try {
      if (name) {
        const section = await invoke("chat_save_section", { name });
        sectionId = section.id;
        chat.home.sections.push(section);
      }
      if (!sectionId) {
        $("chat-section-error").textContent = t("chatmenu.section_required");
        $("chat-section-error").classList.remove("hidden");
        return;
      }
      $("chat-section").close();
      if (chat.sectionFor) {
        await patch(chat.sectionFor, { sectionId }, t("chatmenu.section_moved"));
      } else {
        render();
        toast(t("chatmenu.section_created"));
      }
    } catch (error) {
      $("chat-section-error").textContent = failure(error).message;
      $("chat-section-error").classList.remove("hidden");
    }
  }

  return {
    openMenu,
    openMenuAt,
    openMany,
    closeMenu,
    runMenuAction,
    saveRename,
    saveSection,
    openSection,
    deleteIds,
    confirmDelete,
  };
}
