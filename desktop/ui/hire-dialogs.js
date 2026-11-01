// The three actions on a hire: rename it, change its character or dismiss it. A
// row of «Tu equipo» opens the action menu; each action is its own dialog, and a
// refused call shows its message inside the dialog that made it.

import { $, invoke, toast } from "./core.js";
import { avatarElement } from "./avatar.js";
import { errorMessage } from "./format.js";
import { HIRE_CHARACTERS, characterLabel } from "./hires-model.js";

let current = null;
let pickedAvatar = null;
let onChange = null;

function showError(id, message) {
  const box = $(id);
  box.textContent = message;
  box.classList.toggle("hidden", !message);
}

export function openHireActions(hire) {
  current = hire;
  $("hire-actions-title").textContent = hire.name;
  $("hire-actions").showModal();
}

function openRename() {
  $("hire-actions").close();
  $("hire-rename-input").value = current.name;
  showError("hire-rename-error", "");
  $("hire-rename").showModal();
  $("hire-rename-input").focus();
  $("hire-rename-input").select();
}

async function saveRename(event) {
  event.preventDefault();
  const name = $("hire-rename-input").value.trim();
  if (!name) {
    showError("hire-rename-error", "Escribe un nombre.");
    return;
  }
  try {
    await invoke("team_hire_rename", { hireId: current.id, name });
    $("hire-rename").close();
    toast("Nombre guardado");
    await onChange();
  } catch (error) {
    showError("hire-rename-error", errorMessage(error));
  }
}

function paintAvatarGrid() {
  const options = [...HIRE_CHARACTERS, null].map((id) => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "avatar-option";
    button.setAttribute("aria-pressed", String(id === pickedAvatar));
    button.dataset.avatar = id || "";
    button.addEventListener("click", () => {
      pickedAvatar = id;
      paintAvatarGrid();
    });
    const face = avatarElement({ name: current.name, characterId: id, px: 48, faceFromName: false });
    const label = document.createElement("span");
    label.textContent = id ? characterLabel(id) : "Sin personaje";
    button.append(face, label);
    return button;
  });
  $("hire-avatar-grid").replaceChildren(...options);
}

function openAvatar() {
  $("hire-actions").close();
  pickedAvatar = current.avatar || null;
  showError("hire-avatar-error", "");
  paintAvatarGrid();
  $("hire-avatar").showModal();
}

async function saveAvatar(event) {
  event.preventDefault();
  try {
    await invoke("team_hire_avatar", { hireId: current.id, avatar: pickedAvatar });
    $("hire-avatar").close();
    toast("Personaje guardado");
    await onChange();
  } catch (error) {
    showError("hire-avatar-error", errorMessage(error));
  }
}

function openDismiss() {
  $("hire-actions").close();
  $("hire-dismiss-lede").textContent = `¿Despedir a ${current.name}? Dejará de estar en tu equipo.`;
  showError("hire-dismiss-error", "");
  $("hire-dismiss").showModal();
}

async function confirmDismiss(event) {
  event.preventDefault();
  try {
    await invoke("team_hire_dismiss", { hireId: current.id });
    $("hire-dismiss").close();
    toast(`${current.name} ya no está en tu equipo`);
    await onChange();
  } catch (error) {
    showError("hire-dismiss-error", errorMessage(error));
  }
}

export function installHireDialogs(refresh) {
  onChange = refresh;
  $("hire-actions-rename").addEventListener("click", openRename);
  $("hire-actions-avatar").addEventListener("click", openAvatar);
  $("hire-actions-dismiss").addEventListener("click", openDismiss);
  $("hire-actions-close").addEventListener("click", () => $("hire-actions").close());
  $("hire-rename-form").addEventListener("submit", saveRename);
  $("hire-rename-cancel").addEventListener("click", () => $("hire-rename").close());
  $("hire-avatar-form").addEventListener("submit", saveAvatar);
  $("hire-avatar-cancel").addEventListener("click", () => $("hire-avatar").close());
  $("hire-dismiss-form").addEventListener("submit", confirmDismiss);
  $("hire-dismiss-cancel").addEventListener("click", () => $("hire-dismiss").close());
}
