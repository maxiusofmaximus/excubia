export const tauri = window.__TAURI__;
export const invoke = tauri && tauri.core ? tauri.core.invoke : null;
export const listen = tauri && tauri.event ? tauri.event.listen : null;

export const app = {
  boot: null,
  state: null,
  connection: { kind: "idle" },
  current: null,
  detailToken: 0,
  detailReturn: null,
  sending: false,
  seenKeys: new Set(),
};

export const $ = (id) => document.getElementById(id);

export function showView(name) {
  for (const view of ["pair", "recovery", "lan", "chat", "agents", "detail"]) {
    $(`view-${view}`).classList.toggle("hidden", view !== name);
  }
}

export function setBusy(button, busy, label) {
  if (!button) return;
  if (!button.dataset.label) button.dataset.label = button.textContent;
  button.disabled = busy;
  button.textContent = busy ? label : button.dataset.label;
}

let toastTimer = null;

export function toast(message) {
  const element = $("toast");
  element.textContent = message;
  element.classList.remove("hidden");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => element.classList.add("hidden"), 2600);
}

export async function copyText(text, label) {
  if (!text) return;
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const area = document.createElement("textarea");
    area.value = text;
    area.setAttribute("readonly", "");
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.appendChild(area);
    area.select();
    document.execCommand("copy");
    area.remove();
  }
  toast(label || "Copiado");
}
