// The palette model. The data and the pure decisions live in palette-data.js
// because the boot script must run as a classic script; this module adds the
// storage and media-query helpers and is what the app and the tests import.

import "./palette-data.js";

const model = globalThis.ExcubiaPalettes;

export const PALETTES = model.PALETTES;
export const DEFAULT_PALETTE = model.DEFAULT_PALETTE;
export const STORAGE_KEY = model.STORAGE_KEY;
export const loadPrefs = model.loadPrefs;
export const resolvePalette = model.resolvePalette;
export const initialSetup = model.initialSetup;
export const applyPalette = model.applyPalette;
export const systemIsDark = model.systemIsDark;
export const isPaletteId = model.isPaletteId;

export function savePrefs(prefs) {
  const safe = {
    id: prefs && isPaletteId(prefs.id) ? prefs.id : DEFAULT_PALETTE,
    followSystem: Boolean(prefs && prefs.followSystem),
  };
  try {
    globalThis.localStorage.setItem(STORAGE_KEY, JSON.stringify(safe));
  } catch {
    // A full or disabled store must not break the screen: the choice still
    // applies until the app closes.
  }
  return safe;
}

export function watchSystem(callback) {
  try {
    const media = globalThis.matchMedia("(prefers-color-scheme: dark)");
    if (!media || typeof media.addEventListener !== "function") return () => {};
    const listener = (event) => callback(event.matches);
    media.addEventListener("change", listener);
    return () => media.removeEventListener("change", listener);
  } catch {
    return () => {};
  }
}
