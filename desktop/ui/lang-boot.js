// Runs synchronously in <head>, before the first paint: when the interface starts in English,
// the static Spanish markup stays hidden until i18n.applyDom translates it (it removes
// i18n-pending). Fallback: if the module graph fails or stalls, the page shows anyway.
// The language rule mirrors resolve() in i18n.js.

(function () {
  let pref = null;
  try {
    pref = globalThis.localStorage?.getItem("excubia.lang");
  } catch {
    pref = null;
  }
  const english = pref === "en" || (pref !== "es" && !String(navigator.language ?? "").toLowerCase().startsWith("es"));
  if (!english) return;
  const root = document.documentElement;
  const reveal = () => root.classList.remove("i18n-pending");
  root.classList.add("i18n-pending");
  setTimeout(reveal, 5000);
  globalThis.addEventListener("error", reveal);
})();
