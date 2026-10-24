// The conversation menu and the two small dialogs (rename, section). The
// factory keeps the chat state in one place instead of importing it back.

import { $, invoke, toast } from "./core.js";

export function createChatMenu({ chat, render, patch, failure }) {
  function menuItem(label, action, options = {}) {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = label;
    button.dataset.menuAction = action;
    if (options.danger) button.classList.add("danger");
    return button;
  }

  function openMenu(conversation, anchor) {
    const menu = $("chat-menu");
    menu.replaceChildren();
    menu.append(
      menuItem(conversation.pinned ? "☆ Dejar de fijar" : "★ Fijar", "pin"),
      menuItem("✎ Renombrar", "rename"),
      menuItem("▸ Mover a sección…", "section"),
    );
    menu.append(document.createElement("hr"));
    menu.append(
      menuItem(conversation.archived ? "↩ Desarchivar" : "⧉ Archivar", "archive", {
        danger: !conversation.archived,
      }),
      menuItem("⊘ Cancelar delegaciones activas", "cancel-delegations", { danger: true }),
    );
    chat.menuFor = conversation;
    menu.classList.remove("hidden");
    const rect = anchor.getBoundingClientRect();
    const width = 210;
    const left = Math.min(Math.max(8, rect.right - width), window.innerWidth - width - 8);
    const top = Math.min(rect.bottom + 5, window.innerHeight - 230);
    menu.style.left = `${left}px`;
    menu.style.top = `${Math.max(8, top)}px`;
    menu.querySelector("button").focus();
  }

  function closeMenu() {
    $("chat-menu").classList.add("hidden");
    chat.menuFor = null;
  }

  async function runMenuAction(action) {
    const conversation = chat.menuFor;
    closeMenu();
    if (!conversation) return;
    if (action === "pin") {
      await patch(
        conversation,
        { pinned: !conversation.pinned },
        conversation.pinned ? "Conversación desfijada" : "Conversación fijada",
      );
    } else if (action === "rename") {
      openRename(conversation);
    } else if (action === "section") {
      openSection(conversation);
    } else if (action === "archive") {
      await patch(
        conversation,
        { archived: !conversation.archived },
        conversation.archived ? "Conversación desarchivada" : "Conversación archivada",
      );
    } else if (action === "cancel-delegations") {
      $("delegation-kill").showModal();
    }
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
      $("chat-rename-error").textContent = "Escribe un título.";
      $("chat-rename-error").classList.remove("hidden");
      return;
    }
    const conversation = chat.home
      ? [...chat.home.conversations, ...chat.home.archived].find(
          (row) => row.id === $("chat-rename").dataset.id,
        )
      : null;
    $("chat-rename").close();
    if (conversation) patch(conversation, { title }, "Título actualizado");
  }

  function fillSections(selected) {
    const select = $("chat-section-select");
    select.replaceChildren();
    const none = document.createElement("option");
    none.value = "";
    none.textContent = "— Elegir sección —";
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
        $("chat-section-error").textContent = "Elige una sección o escribe un nombre.";
        $("chat-section-error").classList.remove("hidden");
        return;
      }
      $("chat-section").close();
      if (chat.sectionFor) {
        await patch(chat.sectionFor, { sectionId }, "Conversación movida de sección");
      } else {
        render();
        toast("Sección creada");
      }
    } catch (error) {
      $("chat-section-error").textContent = failure(error).message;
      $("chat-section-error").classList.remove("hidden");
    }
  }

  return { openMenu, closeMenu, runMenuAction, saveRename, saveSection, openSection };
}
