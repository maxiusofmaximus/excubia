// Run with: node --test tooling/chat-menu-placement.test.mjs
// The menu of a row of the history opens from inside the drawer: it must live outside .app
// (inert while the drawer is open) and above the drawer's z-index; the header has no ⋯.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const html = readFileSync(new URL("../ui/index.html", import.meta.url), "utf8");
const chatCss = readFileSync(new URL("../ui/chat.css", import.meta.url), "utf8");
const shellCss = readFileSync(new URL("../ui/shell.css", import.meta.url), "utf8");

// The highest z-index any block of the selector sets (a media query repeats the selector).
const zOf = (css, selector) => {
  const blocks = css.matchAll(new RegExp(String.raw`${selector.replace(".", String.raw`\.`)}\s*\{[^}]*\}`, "g"));
  return Math.max(0, ...[...blocks].map((block) => Number(block[0].match(/z-index:\s*(\d+)/)?.[1] ?? 0)));
};

test("el menú de una fila va fuera de .app, que queda inerte con el cajón abierto", () => {
  const appStart = html.indexOf('<div class="app">');
  const drawerVeil = html.indexOf('id="drawer-veil"');
  const menu = html.indexOf('id="chat-menu"');
  assert.ok(menu > 0 && drawerVeil > 0 && appStart > 0);
  // The drawer's veil is a sibling of .app; so must the menu be, i.e. it comes after it closes.
  assert.ok(menu < drawerVeil, "el menú queda justo antes del velo del cajón");
  const between = html.slice(menu, drawerVeil);
  assert.ok(!between.includes("<section"), "ninguna sección abierta entre el menú y el velo");
});

test("el menú se pinta sobre el cajón", () => {
  assert.ok(zOf(chatCss, ".menu") > zOf(shellCss, ".drawer"));
});

test("la cabecera del chat ya no tiene el botón ⋯", () => {
  assert.ok(!html.includes("btn-chat-more"));
});
