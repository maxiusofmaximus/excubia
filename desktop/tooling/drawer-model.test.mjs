// Run with: node --test tooling/drawer-model.test.mjs
// The drawer (☰, Ctrl+B) as a pure model: what opens and closes it, the keys that
// reach it, the Tab cycle inside it, and the markup that holds it (the history inside it,
// one ☰ per screen that has a menu, the chat column with no side panel).

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { cycleIndex, drawerNext, keyAction, pickAction } from "../ui/drawer-model.js";

const html = readFileSync(fileURLToPath(new URL("../ui/index.html", import.meta.url)), "utf8");
const at = (needle) => html.indexOf(needle);

test("the drawer opens with ☰ or Ctrl+B and closes with Esc, the veil or a choice", () => {
  assert.equal(drawerNext(false, "toggle"), true);
  assert.equal(drawerNext(true, "toggle"), false);
  assert.equal(drawerNext(false, "open"), true);
  assert.equal(drawerNext(true, "close"), false);
  assert.equal(drawerNext(true, "veil"), false);
  assert.equal(drawerNext(true, "pick"), false);
});

test("Esc closes an open drawer; closed, it has nothing to close", () => {
  assert.equal(drawerNext(true, "close"), false);
  assert.equal(drawerNext(false, "close"), false);
  assert.equal(keyAction("Escape", {}), "close");
});

test("an action nobody asked for leaves the drawer as it is", () => {
  assert.equal(drawerNext(true, "something-else"), true);
  assert.equal(drawerNext(false, undefined), false);
});

test("Ctrl+B and Cmd+B toggle; other keys and other combinations are not the drawer's", () => {
  assert.equal(keyAction("b", { ctrlKey: true }), "toggle");
  assert.equal(keyAction("B", { ctrlKey: true }), "toggle");
  assert.equal(keyAction("b", { metaKey: true }), "toggle");
  assert.equal(keyAction("b", {}), null, "a plain b is text");
  assert.equal(keyAction("b", { ctrlKey: true, shiftKey: true }), null);
  assert.equal(keyAction("b", { ctrlKey: true, altKey: true }), null);
  assert.equal(keyAction("k", { ctrlKey: true }), null, "Ctrl+K is the palette");
});

test("a plain click on a choice picks it; Ctrl, Shift or Cmd keeps the drawer open for several rows", () => {
  assert.equal(pickAction({}), "pick");
  assert.equal(pickAction({ ctrlKey: true }), null);
  assert.equal(pickAction({ metaKey: true }), null);
  assert.equal(pickAction({ shiftKey: true }), null);
});

test("Tab inside the drawer wraps at both ends", () => {
  assert.equal(cycleIndex(4, 0), 1);
  assert.equal(cycleIndex(4, 2), 3);
  assert.equal(cycleIndex(4, 3), 0, "the last item goes back to the first");
  assert.equal(cycleIndex(4, 0, true), 3, "Shift+Tab on the first goes to the last");
  assert.equal(cycleIndex(4, 2, true), 1);
});

test("with nothing focused in the drawer, Tab takes the first item and Shift+Tab the last", () => {
  assert.equal(cycleIndex(4, -1), 0);
  assert.equal(cycleIndex(4, -1, true), 3);
  assert.equal(cycleIndex(0, -1), -1, "an empty drawer has nothing to focus");
});

test("the drawer and its veil exist once, hidden, with the dialog semantics", () => {
  assert.equal([...html.matchAll(/id="app-drawer"/g)].length, 1);
  assert.match(html, /<aside id="app-drawer" class="drawer" role="dialog" aria-modal="true"[^>]*\shidden>/);
  assert.match(html, /<div id="drawer-veil" class="drawer-veil" hidden><\/div>/);
});

test("the history lives in the drawer, not in the chat column", () => {
  assert.equal([...html.matchAll(/id="chat-history"/g)].length, 1);
  assert.ok(at('id="chat-history"') > at('id="app-drawer"'), "the history comes after the drawer opens");
  const shell = html.slice(at('id="chat-shell"'), at('id="chat-main"'));
  assert.doesNotMatch(shell, /chat-history/);
});

test("the sections and Ajustes are in the drawer, and Ajustes comes after the history", () => {
  const drawer = html.slice(at('id="app-drawer"'), at('<dialog id="new-conversation"'));
  const listed = [...drawer.matchAll(/data-nav="([a-z]+)"/g)].map((match) => match[1]);
  assert.deepEqual(listed, ["chat", "agents", "agenda", "settings"]);
  assert.ok(at('id="nav-settings"') > at('id="chat-history"'), "Ajustes sits at the foot, under the history");
});

test("the side panel is hidden, and its open button is gone", () => {
  assert.match(html, /<aside id="chat-side" class="side" aria-label="Equipo" hidden>/);
  assert.equal(at('id="btn-side-toggle"'), -1);
  assert.equal(at('id="btn-open-team"'), -1);
});

test("the Alfred portrait in the chat head opens Equipo", () => {
  assert.match(html, /<button id="btn-chat-team" class="thread-team"[^>]*>\s*<span id="chat-avatar"/);
});

test("every screen with a menu has its ☰ inside its own heading", () => {
  const screens = ["chat", "agents", "settings", "contacts", "notes", "calendar", "devices", "alerts"];
  for (const name of screens) {
    const start = at(`id="view-${name}"`);
    assert.ok(start >= 0, `view-${name} exists`);
    const head = html.slice(start, html.indexOf("</header>", start));
    assert.match(head, /data-drawer-toggle/, `${name} needs a ☰ in its heading`);
  }
});

test("the ☰ buttons say they are the menu and start closed", () => {
  const toggles = [...html.matchAll(/<button[^>]*data-drawer-toggle[^>]*>/g)].map((match) => match[0]);
  assert.ok(toggles.length >= 8, `found ${toggles.length}`);
  for (const tag of toggles) {
    assert.match(tag, /aria-expanded="false"/);
    assert.match(tag, /aria-controls="app-drawer"/);
    assert.match(tag, /aria-label="Abrir el menú"/);
  }
});
