// Idioma de la interfaz: «es» (por defecto) o «en». Las cadenas viven en ui/i18n/{es,en}*.js:
// el diccionario base y sus fragmentos B1–B4 y C2 se unen aquí, antes de que cualquier módulo llame a t().
// El HTML estático usa data-i18n / data-i18n-title|placeholder|aria-label; el JS usa t().
// Sin DOM (tests en Node) el idioma automático es «es», como hasta ahora.

import esBase from "./i18n/es.js";
import esB1 from "./i18n/es.b1.js";
import esB2 from "./i18n/es.b2.js";
import esB3 from "./i18n/es.b3.js";
import esB4 from "./i18n/es.b4.js";
import esC2 from "./i18n/es.c2.js";
import esC3 from "./i18n/es.c3.js";
import esC4 from "./i18n/es.c4.js";
import esGh from "./i18n/es.gh.js";
import enBase from "./i18n/en.js";
import enB1 from "./i18n/en.b1.js";
import enB2 from "./i18n/en.b2.js";
import enB3 from "./i18n/en.b3.js";
import enB4 from "./i18n/en.b4.js";
import enC2 from "./i18n/en.c2.js";
import enC3 from "./i18n/en.c3.js";
import enC4 from "./i18n/en.c4.js";
import enGh from "./i18n/en.gh.js";

// Colisiones de clave: tooling/i18n.test.mjs exige que tengan el mismo texto en cada idioma.
const es = Object.assign({}, esBase, esB1, esB2, esB3, esB4, esC2, esC3, esC4, esGh);
const en = Object.assign({}, enBase, enB1, enB2, enB3, enB4, enC2, enC3, enC4, enGh);
export const dictionaries = { es, en };
const KEY = "excubia.lang";
const PREFS = ["auto", "es", "en"];
const ATTRS = {
  "data-i18n-title": "title",
  "data-i18n-placeholder": "placeholder",
  "data-i18n-aria-label": "aria-label",
  "data-i18n-alt": "alt",
};

let cached = null;

// Preferencia guardada: "auto", "es" o "en" (auto si no hay nada, es inválida o no se puede leer).
export function langPref() {
  try {
    const value = globalThis.localStorage?.getItem(KEY);
    return PREFS.includes(value) ? value : "auto";
  } catch {
    return "auto";
  }
}

function resolve(pref) {
  if (pref === "es" || pref === "en") return pref;
  if (typeof document === "undefined" || typeof navigator === "undefined") return "es";
  return String(navigator.language ?? "").toLowerCase().startsWith("es") ? "es" : "en";
}

// Idioma activo: "es" o "en".
export function lang() {
  if (cached === null) cached = resolve(langPref());
  return cached;
}

// Configuración regional para fechas y horas: "es-ES" o "en-US".
export function locale() {
  return lang() === "en" ? "en-US" : "es-ES";
}

// Texto de la clave en el idioma activo; cae a es y, si falta, devuelve la clave.
// {nombre} se sustituye por vars.nombre.
export function t(key, vars) {
  const dict = dictionaries[lang()];
  const text = Object.hasOwn(dict, key) ? dict[key] : Object.hasOwn(es, key) ? es[key] : key;
  if (!vars) return text;
  return text.replace(/\{(\w+)\}/g, (whole, name) => (Object.hasOwn(vars, name) ? String(vars[name]) : whole));
}

// pref: "auto", "es" o "en". Cambia el idioma en memoria sin guardarlo (pruebas y arranque).
export function setLang(pref) {
  return applyLang(pref);
}

// Guarda la preferencia y aplica el idioma en caliente. Devuelve { lang, saved }.
// Si no se puede guardar, conserva el idioma anterior: recargar volvería atrás, y un cambio sin
// recarga dejaría los módulos ya cargados en el otro idioma. El selector sigue la preferencia guardada.
export function changeLang(pref) {
  const value = PREFS.includes(pref) ? pref : "auto";
  const before = lang();
  try {
    globalThis.localStorage.setItem(KEY, value);
  } catch {
    return { lang: before, saved: false };
  }
  return { lang: applyLang(value), saved: true };
}

function applyLang(pref) {
  const value = PREFS.includes(pref) ? pref : "auto";
  cached = resolve(value);
  if (typeof document !== "undefined") {
    applyDom(document);
    document.dispatchEvent(new CustomEvent("excubia:lang", { detail: { lang: cached, pref: value } }));
  }
  return cached;
}

// Rellena los textos y atributos marcados con data-i18n* dentro de root.
// Al final quita la clase i18n-pending que oculta el HTML estático en inglés (ui/lang-boot.js).
export function applyDom(root = globalThis.document) {
  if (!root) return;
  if (root.documentElement) root.documentElement.lang = lang();
  for (const el of root.querySelectorAll("[data-i18n]")) el.textContent = t(el.dataset.i18n);
  for (const [attr, target] of Object.entries(ATTRS)) {
    for (const el of root.querySelectorAll(`[${attr}]`)) el.setAttribute(target, t(el.getAttribute(attr)));
  }
  root.documentElement?.classList?.remove("i18n-pending");
}
