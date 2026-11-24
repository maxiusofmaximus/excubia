// Run with: node --test tooling/i18n.test.mjs
// The UI language: t() with variables and the fallback to es, the two dictionaries with the
// same keys and placeholders, the B1–B3 fragments joined into them, every data-i18n* marker of
// index.html pointing at a key that exists, the saved language choice, and the pre-paint boot.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

import { applyDom, changeLang, dictionaries, lang, langPref, locale, setLang, t } from "../ui/i18n.js";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const html = read("../ui/index.html");
const { es, en } = dictionaries;

const FILES = {
  es: ["es.js", "es.b1.js", "es.b2.js", "es.b3.js", "es.b4.js", "es.c2.js", "es.c3.js", "es.c4.js"],
  en: ["en.js", "en.b1.js", "en.b2.js", "en.b3.js", "en.b4.js", "en.c2.js", "en.c3.js", "en.c4.js"],
};
const parts = {};
for (const [code, names] of Object.entries(FILES)) {
  parts[code] = [];
  for (const name of names) parts[code].push({ name, dict: (await import(`../ui/i18n/${name}`)).default });
}

// Storage stand-ins: a Map-backed one, and one whose every call throws (private mode, blocked site data).
function memoryStorage() {
  const map = new Map();
  return {
    map,
    getItem: (key) => (map.has(key) ? map.get(key) : null),
    setItem: (key, value) => map.set(key, String(value)),
  };
}
const brokenStorage = {
  getItem() {
    throw new Error("denied");
  },
  setItem() {
    throw new Error("denied");
  },
};
async function withStorage(storage, fn) {
  const before = Object.getOwnPropertyDescriptor(globalThis, "localStorage");
  Object.defineProperty(globalThis, "localStorage", { configurable: true, writable: true, value: storage });
  try {
    return await fn();
  } finally {
    if (before) Object.defineProperty(globalThis, "localStorage", before);
    else delete globalThis.localStorage;
  }
}
// A fresh copy of i18n.js starts like a page reload: its only input is the saved choice.
const freshStart = () => import(`../ui/i18n.js?start=${Math.random()}`);

test("t() substitutes {vars} in the active language", () => {
  setLang("en");
  assert.equal(t("common.count_of", { n: 2, total: 5 }), "2 of 5");
  setLang("es");
  assert.equal(t("common.count_of", { n: 2, total: 5 }), "2 de 5");
});

test("t() falls back to es when en lacks the key", () => {
  const saved = en["pair.title"];
  delete en["pair.title"];
  try {
    setLang("en");
    assert.equal(t("pair.title"), es["pair.title"]);
  } finally {
    en["pair.title"] = saved;
    setLang("es");
  }
});

test("t() returns the key itself when no dictionary has it", () => {
  setLang("en");
  assert.equal(t("no.such.key"), "no.such.key");
  setLang("es");
  assert.equal(t("no.such.key"), "no.such.key");
});

test("en and es have exactly the same keys once the fragments are joined", () => {
  assert.deepEqual(Object.keys(en).sort(), Object.keys(es).sort());
});

test("every key of every fragment reaches the merged dictionary with its own text", () => {
  for (const code of ["es", "en"]) {
    for (const { name, dict } of parts[code]) {
      for (const [key, text] of Object.entries(dict)) {
        assert.equal(dictionaries[code][key], text, `${name} «${key}» is missing or overridden`);
      }
    }
  }
});

test("no file writes the same key twice, and colliding keys have the same text in es and en", () => {
  for (const code of ["es", "en"]) {
    for (const name of FILES[code]) {
      const source = read(`../ui/i18n/${name}`);
      const keys = [...source.matchAll(/^\s*"([^"]+)":/gm)].map((m) => m[1]);
      const twice = keys.filter((key, i) => keys.indexOf(key) !== i);
      assert.deepEqual([...new Set(twice)], [], `${name} repeats keys`);
    }
  }
  const owners = new Map();
  for (const { name, dict } of parts.es) {
    for (const key of Object.keys(dict)) owners.set(key, [...(owners.get(key) ?? []), name]);
  }
  for (const [key, files] of owners) {
    if (files.length < 2) continue;
    const texts = new Set(parts.es.filter((p) => files.includes(p.name)).map((p) => p.dict[key]));
    assert.equal(texts.size, 1, `«${key}» has different es texts in ${files.join(", ")}`);
  }
});

test("each key has the same {placeholders} in es and en", () => {
  const placeholders = (text) => [...String(text).matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(",");
  const mismatched = Object.keys(es).filter((key) => placeholders(es[key]) !== placeholders(en[key]));
  assert.deepEqual(mismatched, []);
});

test("every data-i18n* marker in index.html has a key in es and en", () => {
  const keys = [...html.matchAll(/\sdata-i18n(?:-[a-z-]+)?="([^"]+)"/g)].map((m) => m[1]);
  assert.ok(keys.length > 0);
  const missing = keys.filter((key) => !Object.hasOwn(es, key) || !Object.hasOwn(en, key));
  assert.deepEqual([...new Set(missing)], []);
});

test("setLang switches the language and the date locale", () => {
  setLang("en");
  assert.equal(lang(), "en");
  assert.equal(locale(), "en-US");
  setLang("es");
  assert.equal(lang(), "es");
  assert.equal(locale(), "es-ES");
});

test("auto without a DOM (node tests) resolves to es", () => {
  setLang("auto");
  assert.equal(lang(), "es");
  assert.equal(locale(), "es-ES");
});

test("a saved choice is written and a fresh start reads it back", async () => {
  const storage = memoryStorage();
  await withStorage(storage, async () => {
    const result = changeLang("en");
    assert.deepEqual(result, { lang: "en", saved: true });
    assert.equal(storage.map.get("excubia.lang"), "en");
    const fresh = await freshStart();
    assert.equal(fresh.langPref(), "en");
    assert.equal(fresh.lang(), "en");
  });
  setLang("es");
});

test("auto is saved as auto and a fresh start resolves it again", async () => {
  const storage = memoryStorage();
  await withStorage(storage, async () => {
    assert.equal(changeLang("auto").saved, true);
    assert.equal(storage.map.get("excubia.lang"), "auto");
    const fresh = await freshStart();
    assert.equal(fresh.langPref(), "auto");
    assert.equal(fresh.lang(), "es");
  });
});

test("a choice that cannot be saved keeps the previous language and the saved preference", async () => {
  setLang("es");
  await withStorage(brokenStorage, () => {
    assert.deepEqual(changeLang("en"), { lang: "es", saved: false });
    assert.equal(lang(), "es");
    assert.equal(langPref(), "auto");
  });
  setLang("es");
});

test("changeLang does not throw when the localStorage getter itself fails", () => {
  const before = Object.getOwnPropertyDescriptor(globalThis, "localStorage");
  Object.defineProperty(globalThis, "localStorage", {
    configurable: true,
    get() {
      throw new Error("denied");
    },
  });
  try {
    setLang("es");
    assert.deepEqual(changeLang("en"), { lang: "es", saved: false });
    assert.equal(langPref(), "auto");
  } finally {
    if (before) Object.defineProperty(globalThis, "localStorage", before);
    else delete globalThis.localStorage;
  }
  setLang("es");
});

test("applyDom translates the page and clears the pre-paint class", () => {
  const classes = new Set(["i18n-pending"]);
  const root = {
    documentElement: { lang: "", classList: { remove: (name) => classes.delete(name) } },
    querySelectorAll: () => [],
  };
  setLang("en");
  applyDom(root);
  assert.equal(root.documentElement.lang, "en");
  assert.equal(classes.has("i18n-pending"), false);
  setLang("es");
});

test("lang-boot hides the static page only when the start is English", () => {
  const source = read("../ui/lang-boot.js");
  const hidden = (stored, language) => {
    const classes = new Set();
    const sandbox = {
      localStorage: { getItem: () => stored },
      navigator: { language },
      document: { documentElement: { classList: { add: (c) => classes.add(c), remove: (c) => classes.delete(c) } } },
      setTimeout() {},
      addEventListener() {},
    };
    vm.runInNewContext(source, sandbox);
    return classes.has("i18n-pending");
  };
  assert.equal(hidden("en", "es-ES"), true);
  assert.equal(hidden("es", "en-US"), false);
  assert.equal(hidden(null, "en-US"), true);
  assert.equal(hidden(null, "es-ES"), false);
  assert.equal(hidden("fr", "es-MX"), false);
  assert.equal(hidden("fr", "fr-FR"), true);
});

test("lang-boot is loaded in the head of index.html and the hide rule exists", () => {
  const head = html.slice(0, html.indexOf("</head>"));
  assert.ok(head.includes('src="lang-boot.js"'));
  assert.ok(read("../ui/base.css").includes("html.i18n-pending body"));
});
