// Run with: node --test tooling/comic-skin.test.mjs
// The comic skin loads last, ships its font locally under the OFL, has no
// remote URLs and turns every animation off for reduced motion.

import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const uiPath = (name) => fileURLToPath(new URL(`../ui/${name}`, import.meta.url));
const html = readFileSync(uiPath("index.html"), "utf8");
const css = readFileSync(uiPath("comic.css"), "utf8");

test("comic.css se enlaza el último en index.html", () => {
  const sheets = [...html.matchAll(/<link rel="stylesheet" href="([^"]+)">/g)].map((match) => match[1]);
  assert.equal(sheets.at(-1), "comic.css");
});

test("la fuente va local: woff2 real y licencia OFL junto a ella", () => {
  assert.match(css, /url\("assets\/fonts\/dela-gothic-one-latin\.woff2"\)/);
  assert.match(css, /font-display: swap;/);
  const font = readFileSync(uiPath("assets/fonts/dela-gothic-one-latin.woff2"));
  assert.equal(font.subarray(0, 4).toString("latin1"), "wOF2");
  assert.ok(existsSync(uiPath("assets/fonts/OFL.txt")));
  assert.match(readFileSync(uiPath("assets/fonts/OFL.txt"), "utf8"), /SIL OPEN FONT LICENSE/i);
});

test("nada de la red: ni enlaces ni URLs remotas en el skin ni en index.html", () => {
  assert.doesNotMatch(css, /https?:\/\//);
  assert.doesNotMatch(html, /<link[^>]+href="https?:/);
});

test("reduced motion apaga todas las animaciones del skin", () => {
  const block = css.slice(css.indexOf("@media (prefers-reduced-motion: reduce)"));
  assert.match(block, /animation: none !important;/);
  assert.match(block, /transition: none !important;/);
});

test("la cortina tiene un único elemento en index.html", () => {
  assert.equal([...html.matchAll(/id="wipe"/g)].length, 1);
});
