// Run with: node --test tooling/contrast.test.mjs
// The palette's pairs, read from the tokens in ui/base.css: dark is the plain :root,
// light is the prefers-color-scheme block. Text needs 4.5:1 on every surface it sits
// on, controls and focus rings 3:1 (WCAG 2.1 AA).
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const css = readFileSync(new URL("../ui/base.css", import.meta.url), "utf8");

// The declarations inside the first `{…}` after `header` (no nested blocks here).
function body(source, header) {
  const start = source.indexOf(header);
  assert.notEqual(start, -1, `falta ${header}`);
  const open = source.indexOf("{", start);
  return source.slice(open + 1, source.indexOf("}", open));
}

function tokens(declarations) {
  return Object.fromEntries(
    [...declarations.matchAll(/--([\w-]+):\s*(#[0-9a-f]{6})\s*;/gi)].map(([, name, hex]) => [
      name,
      hex.toLowerCase(),
    ]),
  );
}

const THEMES = {
  dark: tokens(body(css, ":root {")),
  light: tokens(
    body(css.slice(css.indexOf("@media (prefers-color-scheme: light)")), ":root {"),
  ),
};

function luminance(hex) {
  const [r, g, b] = [1, 3, 5].map((i) => {
    const channel = parseInt(hex.slice(i, i + 2), 16) / 255;
    return channel <= 0.03928 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function ratio(foreground, background) {
  const [lighter, darker] = [luminance(foreground), luminance(background)].sort((a, b) => b - a);
  return (lighter + 0.05) / (darker + 0.05);
}

const SURFACES = ["bg", "surface", "raised", "raised-2"];
const TEXT = ["text", "text-2", "muted", "accent-text", "warning", "blocked", "working", "done", "idle", "unknown"];
const CONTROLS = ["line-strong", "accent-text"];

// Each failing pair as a readable line, so a red run says which pair broke.
function below(theme, pairs, minimum) {
  return pairs
    .map(([fg, bg]) => [fg, bg, ratio(theme[fg], theme[bg])])
    .filter(([, , value]) => value < minimum)
    .map(([fg, bg, value]) => `${fg} sobre ${bg}: ${value.toFixed(2)}:1`);
}

for (const [name, theme] of Object.entries(THEMES)) {
  test(`${name}: texto de la paleta ≥ 4,5:1 sobre cada superficie`, () => {
    const pairs = TEXT.flatMap((fg) => SURFACES.map((bg) => [fg, bg]));
    assert.deepEqual(below(theme, pairs, 4.5), []);
  });

  test(`${name}: texto sobre el acento suave y rellenos ≥ 4,5:1`, () => {
    const pairs = [
      ["text", "accent-soft"],
      ["accent-text", "accent-soft"],
      ["on-accent", "accent"],
      ["on-attention", "blocked"],
    ];
    assert.deepEqual(below(theme, pairs, 4.5), []);
  });

  test(`${name}: bordes y anillos de foco ≥ 3:1`, () => {
    const pairs = CONTROLS.flatMap((fg) => SURFACES.map((bg) => [fg, bg]));
    assert.deepEqual(below(theme, pairs, 3), []);
  });
}
