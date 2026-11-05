// The palette screen and its twin inside Ajustes share one picker: six cards
// with a tiny preview, plus the «follow the system» switch. The first-run view
// only adds the step footer and the «Continuar» button (in index.html).

import { $ } from "./core.js";
import { showView } from "./nav.js";
import {
  DEFAULT_PALETTE,
  PALETTES,
  applyPalette,
  initialSetup,
  loadPrefs,
  resolvePalette,
  savePrefs,
  systemIsDark,
  watchSystem,
} from "./palettes.js";

const PREVIEW_TOKENS = ["bg", "surface", "line", "text", "muted", "accent"];
const state = { prefs: loadPrefs(), systemDark: systemIsDark(), pickers: [], firstRun: null, onContinue: null };

function currentPrefs() {
  return state.prefs || { id: DEFAULT_PALETTE, followSystem: false };
}

function shownId() {
  return resolvePalette(currentPrefs(), state.systemDark);
}

function updatePrefs(next) {
  const resolved = resolvePalette(next, state.systemDark);
  state.prefs = savePrefs({ id: resolved, followSystem: next.followSystem });
  applyPalette(resolved);
  syncPickers();
}

function syncPickers() {
  for (const picker of state.pickers) picker.render(currentPrefs());
}

function cardFor(palette, onChange) {
  const card = document.createElement("button");
  card.type = "button";
  card.className = "palette-card";
  card.dataset.palette = palette.id;
  card.setAttribute("aria-pressed", "false");

  const preview = document.createElement("span");
  preview.className = "palette-preview";
  preview.setAttribute("aria-hidden", "true");
  for (const name of PREVIEW_TOKENS) {
    preview.style.setProperty(`--p-${name}`, palette.tokens[name]);
  }
  const side = document.createElement("span");
  side.className = "palette-preview-side";
  const main = document.createElement("span");
  main.className = "palette-preview-main";
  side.append(bar(), bar(), bar());
  main.append(bar("is-text"), bar(), chip());
  preview.append(side, main);

  const text = document.createElement("span");
  text.className = "palette-card-text";
  const name = document.createElement("span");
  name.className = "palette-card-name";
  name.textContent = palette.name;
  const note = document.createElement("span");
  note.className = "palette-card-note";
  note.textContent = palette.note;
  text.append(name, note);
  card.append(preview, text);

  card.addEventListener("click", () => onChange({ id: palette.id, followSystem: false }));
  return card;
}

function bar(extra) {
  const element = document.createElement("span");
  element.className = "palette-bar";
  if (extra) element.classList.add(extra);
  return element;
}

function chip() {
  const element = document.createElement("span");
  element.className = "palette-chip";
  return element;
}

function focusCard(cards, index, event) {
  const step = event.key === "ArrowLeft" || event.key === "ArrowUp" ? -1 : 1;
  const next = (index + step + cards.length) % cards.length;
  cards[next].focus();
  event.preventDefault();
}

function toggle(onChange) {
  const label = document.createElement("label");
  label.className = "switch palette-system";
  const input = document.createElement("input");
  input.type = "checkbox";
  input.setAttribute("role", "switch");
  const text = document.createElement("span");
  text.className = "palette-system-text";
  text.textContent = "Seguir el tema del sistema (claro de día, oscuro de noche)";
  label.append(input, text);
  input.addEventListener("change", () => {
    onChange({ id: shownId(), followSystem: input.checked });
  });
  return { label, input };
}

function createPicker(onChange) {
  const root = document.createElement("div");
  root.className = "palette-picker";
  const grid = document.createElement("div");
  grid.className = "palette-grid";
  grid.setAttribute("role", "group");
  grid.setAttribute("aria-label", "Paletas");
  const cards = Object.values(PALETTES).map((palette) => cardFor(palette, onChange));
  cards.forEach((card, index) => {
    card.addEventListener("keydown", (event) => {
      if (["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) {
        focusCard(cards, index, event);
      }
    });
  });
  grid.append(...cards);

  const system = toggle(onChange);
  const hint = document.createElement("p");
  hint.className = "palette-system-hint";
  hint.setAttribute("aria-live", "polite");
  root.append(grid, system.label, hint);

  function render(prefs) {
    const selected = resolvePalette(prefs, state.systemDark);
    for (const card of cards) {
      card.setAttribute("aria-pressed", String(card.dataset.palette === selected));
      card.disabled = prefs.followSystem;
    }
    system.input.checked = prefs.followSystem;
    hint.textContent = prefs.followSystem
      ? `La decide el sistema. Ahora: ${PALETTES[selected].name}.`
      : "Se guarda en este equipo y se aplica al instante.";
  }

  function focusSelected() {
    cards.find((card) => card.getAttribute("aria-pressed") === "true")?.focus();
  }

  return { root, render, focusSelected };
}

function mountSettings() {
  const section = document.createElement("section");
  section.id = "palette-settings";
  section.className = "palette-settings";
  section.setAttribute("aria-labelledby", "palette-settings-title");
  const title = document.createElement("h2");
  title.id = "palette-settings-title";
  title.textContent = "Apariencia";
  const lede = document.createElement("p");
  lede.className = "hint";
  lede.textContent = "Elige la paleta de la aplicación. Se aplica al instante.";
  const picker = createPicker((next) => updatePrefs(next));
  state.pickers.push(picker);
  section.append(title, lede, picker.root);
  $("settings-appearance").append(section);
}

export function installPalette() {
  mountSettings();
  const picker = createPicker((next) => updatePrefs(next));
  state.pickers.push(picker);
  state.firstRun = picker;
  $("first-run-palette").append(picker.root);
  syncPickers();
  applyPalette(shownId());
  $("palette-continue").addEventListener("click", continueFirstRun);
  watchSystem((isDark) => {
    state.systemDark = isDark;
    if (currentPrefs().followSystem) {
      applyPalette(resolvePalette(currentPrefs(), isDark));
      syncPickers();
    }
  });
}

export function offerFirstRunPalette({ onboardingDone, onContinue }) {
  const setup = initialSetup({ stored: state.prefs, onboardingDone, systemIsDark: state.systemDark });
  if (!setup.showPicker) {
    if (!state.prefs && setup.prefs) {
      state.prefs = savePrefs(setup.prefs);
      applyPalette(resolvePalette(state.prefs, state.systemDark));
      syncPickers();
    }
    return false;
  }
  state.onContinue = onContinue;
  syncPickers();
  showView("palette");
  state.firstRun?.focusSelected();
  return true;
}

function continueFirstRun() {
  const prefs = currentPrefs();
  const resolved = resolvePalette(prefs, state.systemDark);
  state.prefs = savePrefs({ id: resolved, followSystem: prefs.followSystem });
  applyPalette(resolved);
  syncPickers();
  const next = state.onContinue;
  state.onContinue = null;
  if (next) next();
}
