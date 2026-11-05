// Ajustes: one screen with five plain sections. Palette, approvals and mail mount
// their own controls into their section and refresh on «settings:open»; this module
// opens the screen, fills the static parts and links to the connection screen.

import { $, tauri } from "./core.js";
import { showView } from "./nav.js";

export function openSettings() {
  showView("settings");
  document.dispatchEvent(new Event("settings:open"));
}

export function installSettings() {
  $("btn-settings-connect").addEventListener("click", () => showView("pair"));
  showVersion();
}

async function showVersion() {
  try {
    $("app-version").textContent = `Versión ${await tauri.app.getVersion()}`;
  } catch {
    $("app-version").textContent = "Versión no disponible";
  }
}
