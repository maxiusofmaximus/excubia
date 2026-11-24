// Ajustes: one screen with plain sections. Palette, approvals and mail mount
// their own controls into their section and refresh on «settings:open»; this module
// opens the screen, fills the static parts, the language card and links to the connection screen.

import { $, tauri } from "./core.js";
import { showView } from "./nav.js";
import { openAlerts } from "./alerts.js";
import { openDevices } from "./devices.js";
import { mountVaultSettings } from "./vault-settings.js";
import { changeLang, lang, langPref, t } from "./i18n.js";

const LANG_CHOICES = [
  ["auto", "settings.lang_auto"],
  ["es", "settings.lang_es"],
  ["en", "settings.lang_en"],
];

// undefined: todavía cargando; null: no disponible.
let version;

export function openSettings() {
  showView("settings");
  document.dispatchEvent(new Event("settings:open"));
}

export function installSettings() {
  $("btn-settings-connect").addEventListener("click", () => showView("pair"));
  $("btn-settings-alerts").addEventListener("click", () => openAlerts());
  $("btn-settings-devices").addEventListener("click", () => openDevices());
  mountVaultSettings();
  mountLanguage();
  document.addEventListener("excubia:lang", paintVersion);
  loadVersion();
}

// «Idioma / Language»: changeLang stores the choice. It reloads only when the effective language
// changed and the choice was saved, so the reload shows the new language (and syncs the backend
// at boot). A choice that could not be saved keeps the previous language, and the selector
// keeps showing the saved preference, so it always matches what the app shows.
function mountLanguage() {
  const card = document.createElement("section");
  card.className = "settings-card";
  card.setAttribute("aria-labelledby", "settings-language-title");
  const title = document.createElement("h2");
  title.id = "settings-language-title";
  title.dataset.i18n = "settings.language_title";
  title.textContent = t("settings.language_title");
  const hint = document.createElement("p");
  hint.className = "hint";
  hint.dataset.i18n = "settings.language_hint";
  hint.textContent = t("settings.language_hint");
  const row = document.createElement("div");
  row.className = "lang-choices";
  row.setAttribute("role", "radiogroup");
  row.setAttribute("aria-labelledby", "settings-language-title");
  for (const [pref, key] of LANG_CHOICES) {
    const button = document.createElement("button");
    button.type = "button";
    button.setAttribute("role", "radio");
    button.dataset.langPref = pref;
    button.dataset.i18n = key;
    button.textContent = t(key);
    row.append(button);
  }
  row.addEventListener("click", (event) => {
    const button = event.target.closest("[data-lang-pref]");
    if (!button) return;
    const before = lang();
    const result = changeLang(button.dataset.langPref);
    if (result.saved && result.lang !== before) {
      globalThis.location.reload();
      return;
    }
    paintLanguage(row);
  });
  card.append(title, hint, row);
  $("settings-appearance").after(card);
  paintLanguage(row);
}

function paintLanguage(row) {
  const current = langPref();
  for (const button of row.querySelectorAll("[data-lang-pref]")) {
    const on = button.dataset.langPref === current;
    button.className = on ? "btn primary btn-small" : "btn secondary btn-small";
    button.setAttribute("aria-checked", String(on));
  }
}

async function loadVersion() {
  try {
    version = await tauri.app.getVersion();
  } catch {
    version = null;
  }
  paintVersion();
}

// Runs after applyDom on a language change, so the version line never shows the loading text.
function paintVersion() {
  if (version === undefined) $("app-version").textContent = t("settings.version_loading");
  else if (version === null) $("app-version").textContent = t("settings.version_missing");
  else $("app-version").textContent = t("settings.version", { v: version });
}
