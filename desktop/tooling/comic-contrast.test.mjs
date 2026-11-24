// Run with: node --test tooling/comic-contrast.test.mjs
// Phase 2 of the comic skin, checked on the real tokens of the seven palettes: the ink on the
// hot slab (the selected history row, the Agenda row and tab, the pressed toggles, the ✓ and the
// granted permission chips), the pale pill, the status stamps, the rows and the disabled button.
// Text needs 4,5:1; the stamps and the ring of a chip are checked the same way (they are text).

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { PALETTES } from "../ui/palettes.js";

const uiPath = (name) => fileURLToPath(new URL(`../ui/${name}`, import.meta.url));
const css = readFileSync(uiPath("comic.css"), "utf8");
const IDS = ["noche", "pergamino", "carbon", "papel", "medianoche", "contraste", "telon"];
const MIN = 4.5;

function channel(value) {
  const c = value / 255;
  return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

function luminance(hex) {
  const digits = hex.replace("#", "");
  const full = digits.length === 3 ? [...digits].map((d) => d + d).join("") : digits;
  const n = Number.parseInt(full, 16);
  return 0.2126 * channel((n >> 16) & 255) + 0.7152 * channel((n >> 8) & 255) + 0.0722 * channel(n & 255);
}

export function contrast(a, b) {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

// The pairs the phase 2 rules draw, by role: [name, ink, ground].
function pairs(t) {
  return [
    // The selected row, the open Agenda row and tab, the pressed toggles and the ✓ (ink on hot).
    ["tinta sobre el tope (fila elegida, pestaña, ✓)", t["on-accent"], t.accent],
    // The state pill «Pide tu OK» inside the hot row: pale pill, its ink on it.
    ["pastilla de estado en fila elegida", t.bg, t.text],
    // The disabled «Nuevo evento»: muted ink on the list pane's surface.
    ["botón apagado sobre la superficie", t.muted, t.surface],
    // Rows and tabs on their slab (raised), and the hover slab (raised-2).
    ["texto de fila sobre el tope", t.text, t.raised],
    ["texto secundario de fila sobre el tope", t["text-2"], t.raised],
    ["texto atenuado de fila sobre el tope", t.muted, t.raised],
    ["texto atenuado de fila en hover", t.muted, t["raised-2"]],
    // A permission not granted: muted ink on the row's slab.
    ["permiso no concedido sobre el tope", t.muted, t.raised],
    // The toast and the banners: text on their slab.
    ["aviso sobre su tope", t.text, t.raised],
    // The palette's new stamp and the slanted swatch's ring.
    ["sello «Nueva»", t["on-accent"], t.accent],
  ];
}

// The status stamps are text in the status colour on the page ground (the sello's own slab).
const STATUS = ["blocked", "working", "done", "idle", "unknown"];

for (const id of IDS) {
  test(`fila elegida y pastillas pasan 4,5:1 en ${PALETTES[id].name}`, () => {
    for (const [name, ink, ground] of pairs(PALETTES[id].tokens)) {
      const ratio = contrast(ink, ground);
      assert.ok(ratio >= MIN, `${PALETTES[id].name}: ${name} = ${ratio.toFixed(2)}:1`);
    }
  });

  test(`los sellos de estado pasan 4,5:1 sobre el fondo en ${PALETTES[id].name}`, () => {
    const t = PALETTES[id].tokens;
    for (const status of STATUS) {
      const ratio = contrast(t[status], t.bg);
      assert.ok(ratio >= MIN, `${PALETTES[id].name}: ${status} sobre bg = ${ratio.toFixed(2)}:1`);
    }
  });
}

// The avatar frame is not text: its edge (--pale) and the outside keyline of its state ring
// must stand out from the page ground by 3:1 in every palette. The hot ring itself is not
// checked alone: --hot is under 3:1 on Pergamino, which is why the keyline exists.
for (const id of IDS) {
  test(`el borde del retrato y el filo de su anillo pasan 3:1 sobre el fondo en ${PALETTES[id].name}`, () => {
    const t = PALETTES[id].tokens;
    const ratio = contrast(t.text, t.bg);
    assert.ok(ratio >= 3, `${PALETTES[id].name}: borde --pale = ${ratio.toFixed(2)}:1`);
  });
}

test("el sello «¡Tu OK!» tiene dos estados y ninguno pasa por la mezcla", () => {
  // Both states are pairs the palettes check: hot ink on hot, and the pale pill.
  for (const id of IDS) {
    const t = PALETTES[id].tokens;
    assert.ok(contrast(t["on-accent"], t.accent) >= MIN, `${id}: hot`);
    assert.ok(contrast(t.bg, t.text) >= MIN, `${id}: pale`);
  }
  // The colours switch in one step (step-end), never through the mix.
  assert.match(css, /comic-stamp-tone 1\.4s step-end infinite/);
  assert.doesNotMatch(css, /comic-stamp 1\.4s steps\(2\)/);
});

test("el botón apagado no baja la opacidad: se ve apagado a propósito", () => {
  assert.match(css, /\.btn:disabled \{\s*opacity: 1;/);
  assert.match(css, /\.btn:disabled::before \{\s*background: transparent;/);
});

test("la fila elegida pinta la pastilla, el menú y el punto con la tinta del tope", () => {
  assert.match(css, /\.chat-row\.is-active \.chat-row-badge \{\s*background: var\(--pale\);\s*color: var\(--on-pale\);/);
  assert.match(css, /\.chat-row\.is-active \.row-more \{\s*color: var\(--on-hot\);/);
  assert.match(css, /\.chat-row\.is-active \.unread-dot \{\s*background: var\(--on-hot\);/);
});

test("el rango activo de Agenda pinta su texto con la tinta del tope, también bajo el puntero", () => {
  // agenda.css sets the accent as the pressed text; on this skin the slab is the accent too.
  assert.match(css, /\.calendar-range \.btn\[aria-pressed="true"\] \{\s*color: var\(--on-hot\);/);
  // The ghost hover rule (0,4,0) outranks the (0,3,0) pressed rule, so the hover needs its own (0,5,0) rule.
  assert.match(css, /\.calendar-range \.btn\[aria-pressed="true"\]:hover:not\(:disabled\) \{\s*color: var\(--on-hot\);/);
});

test("todo @keyframes del skin se apaga con reduced motion: el bloque va la última hoja", () => {
  const block = css.slice(css.indexOf("@media (prefers-reduced-motion: reduce)"));
  assert.match(block, /animation: none !important;/);
  assert.match(block, /transition: none !important;/);
  // The screen entry and the new stamp keyframes are defined here, under that block's reach.
  assert.match(css, /@keyframes comic-screen-in/);
  assert.match(css, /@keyframes comic-stamp-tone/);
});

test("ninguna animación nueva se pinta sin su @keyframes", () => {
  for (const name of ["comic-screen-in", "comic-stamp-tone", "comic-stamp"]) {
    assert.match(css, new RegExp(`@keyframes ${name}`));
  }
});

test("la fase 2 no mete texto inclinado: ninguna regla de texto usa skew", () => {
  // The slant is always on a ::before/::after or on a swatch with no text.
  const textRules = css.match(/^[^{}]*\{[^}]*transform: skewX[^}]*\}/gm) ?? [];
  for (const rule of textRules) {
    assert.doesNotMatch(rule, /font-family: var\(--display\)/, rule.split("{")[0]);
  }
});
