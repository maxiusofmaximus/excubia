// Run with: node --test tooling/palettes.test.mjs
// The six approved palettes, their contrast rules and the first-run decision.
// The colours live in ui/palettes.js; tooling/contrast.test.mjs only knew the
// old two themes and is replaced by this file.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  PALETTES,
  DEFAULT_PALETTE,
  initialSetup,
  loadPrefs,
  resolvePalette,
  savePrefs,
} from "../ui/palettes.js";

const IDS = ["noche", "pergamino", "carbon", "papel", "medianoche", "contraste"];
const SURFACES = ["bg", "surface", "raised", "raised-2"];
const TEXT = [
  "text",
  "text-2",
  "muted",
  "accent-text",
  "warning",
  "blocked",
  "working",
  "done",
  "idle",
  "unknown",
];

function luminance(hex) {
  const channels = [1, 3, 5].map((index) => {
    const value = parseInt(hex.slice(index, index + 2), 16) / 255;
    return value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
}

function ratio(foreground, background) {
  const [lighter, darker] = [luminance(foreground), luminance(background)].sort((a, b) => b - a);
  return (lighter + 0.05) / (darker + 0.05);
}

// A readable list of every pair under the minimum, so a red run says which one broke.
function below(tokens, pairs, minimum) {
  return pairs
    .map(([fg, bg]) => [fg, bg, ratio(tokens[fg], tokens[bg])])
    .filter(([, , value]) => value < minimum)
    .map(([fg, bg, value]) => `${fg} sobre ${bg}: ${value.toFixed(2)}:1`);
}

test("las seis paletas existen con id, nombre, nota y modo", () => {
  assert.deepEqual(Object.keys(PALETTES), IDS);
  assert.equal(DEFAULT_PALETTE, "noche");
  for (const id of IDS) {
    const palette = PALETTES[id];
    assert.equal(palette.id, id);
    assert.ok(palette.name.length > 0);
    assert.ok(palette.note.length > 0);
    assert.ok(["claro", "oscuro"].includes(palette.scheme));
  }
});

test("todas las paletas definen exactamente el mismo conjunto de variables", () => {
  const expected = Object.keys(PALETTES.noche.tokens).sort();
  for (const id of IDS) {
    assert.deepEqual(Object.keys(PALETTES[id].tokens).sort(), expected, id);
  }
  for (const [id, palette] of Object.entries(PALETTES)) {
    for (const [name, value] of Object.entries(palette.tokens)) {
      if (name === "accent-rgb") {
        assert.match(value, /^\d{1,3}, \d{1,3}, \d{1,3}$/, `${id}.${name}`);
      } else if (!name.startsWith("shadow-")) {
        assert.match(value, /^#[0-9a-f]{6}$/i, `${id}.${name}`);
      }
    }
  }
});

test("base.css: el respaldo sin JavaScript declara exactamente Noche", () => {
  const css = readFileSync(new URL("../ui/base.css", import.meta.url), "utf8");
  const start = css.indexOf(":root {");
  const root = css.slice(start, css.indexOf("}", start));
  const declared = Object.fromEntries(
    [...root.matchAll(/--([\w-]+):\s*([^;]+);/g)].map(([, name, value]) => [name, value.trim()]),
  );
  for (const [name, value] of Object.entries(PALETTES.noche.tokens)) {
    assert.equal(declared[name], value, name);
  }
});

for (const id of IDS) {
  test(`${id}: texto y estados ≥ 4,5:1 sobre cada superficie`, () => {
    const tokens = PALETTES[id].tokens;
    const pairs = TEXT.flatMap((fg) => SURFACES.map((bg) => [fg, bg]));
    assert.deepEqual(below(tokens, pairs, 4.5), []);
  });

  test(`${id}: tinta sobre acento, error y acento suave ≥ 4,5:1`, () => {
    const tokens = PALETTES[id].tokens;
    const pairs = [
      ["on-accent", "accent"],
      ["on-attention", "blocked"],
      ["text", "accent-soft"],
      ["accent-text", "accent-soft"],
    ];
    assert.deepEqual(below(tokens, pairs, 4.5), []);
  });

  test(`${id}: bordes de control ≥ 3:1`, () => {
    const tokens = PALETTES[id].tokens;
    const pairs = SURFACES.map((bg) => ["line-strong", bg]);
    assert.deepEqual(below(tokens, pairs, 3), []);
  });

  // The lit switch is told apart from the page by its border: the accent fill alone is too close in Pergamino and Papel.
  test(`${id}: borde del interruptor encendido ≥ 3:1`, () => {
    const tokens = PALETTES[id].tokens;
    const pairs = SURFACES.map((bg) => ["accent-text", bg]);
    assert.deepEqual(below(tokens, pairs, 3), []);
  });
}

test("resolvePalette: por defecto Noche, con o sin sistema oscuro", () => {
  assert.equal(resolvePalette(null, true), "noche");
  assert.equal(resolvePalette(null, false), "noche");
  assert.equal(resolvePalette({ id: "no-existe", followSystem: false }, true), "noche");
  assert.equal(resolvePalette({ id: "__proto__", followSystem: false }, false), "noche");
});

test("resolvePalette: una preferencia explícita manda sobre el sistema", () => {
  assert.equal(resolvePalette({ id: "papel", followSystem: false }, true), "papel");
  assert.equal(resolvePalette({ id: "carbon", followSystem: false }, false), "carbon");
});

test("resolvePalette: seguir el sistema elige Noche de noche y Pergamino de día", () => {
  assert.equal(resolvePalette({ id: "papel", followSystem: true }, true), "noche");
  assert.equal(resolvePalette({ id: "carbon", followSystem: true }, false), "pergamino");
});

test("migración: sin preferencia y onboarding hecho sigue el sistema, sin pantalla", () => {
  assert.deepEqual(initialSetup({ stored: null, onboardingDone: true, systemIsDark: true }), {
    prefs: { id: "noche", followSystem: true },
    showPicker: false,
  });
  assert.deepEqual(initialSetup({ stored: null, onboardingDone: true, systemIsDark: false }), {
    prefs: { id: "pergamino", followSystem: true },
    showPicker: false,
  });
});

test("primer arranque: sin preferencia y onboarding pendiente muestra la pantalla", () => {
  assert.deepEqual(initialSetup({ stored: null, onboardingDone: false, systemIsDark: true }), {
    prefs: null,
    showPicker: true,
  });
});

test("una preferencia guardada no vuelve a mostrar la pantalla", () => {
  const stored = { id: "medianoche", followSystem: false };
  assert.deepEqual(initialSetup({ stored, onboardingDone: false, systemIsDark: true }), {
    prefs: stored,
    showPicker: false,
  });
});

test("loadPrefs: lee la clave excubia-palette y normaliza el booleano", () => {
  globalThis.localStorage = {
    getItem: (key) => (key === "excubia-palette" ? '{"id":"carbon","followSystem":1}' : null),
  };
  assert.deepEqual(loadPrefs(), { id: "carbon", followSystem: true });
});

test("loadPrefs: sin clave no hay preferencia (para poder mostrar la pantalla)", () => {
  globalThis.localStorage = { getItem: () => null };
  assert.equal(loadPrefs(), null);
});

test("loadPrefs: localStorage roto degrada a Noche", () => {
  globalThis.localStorage = {
    getItem: () => {
      throw new Error("sin almacenamiento");
    },
  };
  assert.deepEqual(loadPrefs(), { id: "noche", followSystem: false });
});

test("loadPrefs: JSON corrupto o paleta desconocida degrada a Noche", () => {
  globalThis.localStorage = { getItem: () => "{roto" };
  assert.deepEqual(loadPrefs(), { id: "noche", followSystem: false });
  globalThis.localStorage = { getItem: () => '{"id":"neon","followSystem":false}' };
  assert.deepEqual(loadPrefs(), { id: "noche", followSystem: false });
  globalThis.localStorage = { getItem: () => '{"id":"__proto__","followSystem":false}' };
  assert.deepEqual(loadPrefs(), { id: "noche", followSystem: false });
});

test("savePrefs escribe la clave y no rompe si el almacenamiento falla", () => {
  let written = null;
  globalThis.localStorage = { setItem: (key, value) => (written = [key, value]) };
  savePrefs({ id: "papel", followSystem: true });
  assert.deepEqual(JSON.parse(written[1]), { id: "papel", followSystem: true });
  savePrefs({ id: "__proto__", followSystem: true });
  assert.deepEqual(JSON.parse(written[1]), { id: "noche", followSystem: true });
  globalThis.localStorage = {
    setItem: () => {
      throw new Error("lleno");
    },
  };
  assert.doesNotThrow(() => savePrefs({ id: "papel", followSystem: false }));
});
