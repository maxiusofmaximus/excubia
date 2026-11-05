// Runs synchronously in <head>, after palette-data.js: applies the saved
// palette before the first paint so the window never flashes the wrong theme.

(function () {
  const model = globalThis.ExcubiaPalettes;
  if (!model) return;
  const prefs = model.loadPrefs();
  const id = prefs ? model.resolvePalette(prefs, model.systemIsDark()) : model.DEFAULT_PALETTE;
  model.applyPalette(id);
})();
