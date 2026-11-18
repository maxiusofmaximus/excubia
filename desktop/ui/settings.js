// Ajustes: one screen with five plain sections. Palette, approvals and mail mount
// their own controls into their section and refresh on «settings:open»; this module
// opens the screen, fills the static parts and links to the connection screen.

import { $, tauri } from "./core.js";
import { showView } from "./nav.js";
import { openAlerts } from "./alerts.js";
import { openDevices } from "./devices.js";

export function openSettings() {
  showView("settings");
  document.dispatchEvent(new Event("settings:open"));
}

export function installSettings() {
  $("btn-settings-connect").addEventListener("click", () => showView("pair"));
  $("btn-settings-alerts").addEventListener("click", () => openAlerts());
  $("btn-settings-devices").addEventListener("click", () => openDevices());
  showVersion();
}

async function showVersion() {
  try {
    $("app-version").textContent = `Versión ${await tauri.app.getVersion()}`;
  } catch {
    $("app-version").textContent = "Versión no disponible";
  }
}
