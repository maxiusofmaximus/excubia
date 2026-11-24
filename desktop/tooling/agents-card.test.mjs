// The row's «⋯» belongs to its agent card. It sits in the card's corner, beside the row button:
// a button inside a button would hide the menu from screen readers. Reads the source as text,
// like the other markup tests, so a rename breaks the test, not the app.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const source = readFileSync(new URL("../ui/agents.js", import.meta.url), "utf8");
const css = readFileSync(new URL("../ui/comic.css", import.meta.url), "utf8");
const item = source.slice(source.indexOf("function agentItem("), source.indexOf("function renderList("));

test("the ⋯ menu is a child of the agent card, not a sibling under it", () => {
  assert.match(item, /item\.className = "agent-card"/, "falta la tarjeta .agent-card");
  assert.match(item, /item\.append\(button, foot\)/, "la tarjeta no contiene la fila y el pie");
  assert.match(item, /foot\.append\(menu\)/, "el menú no va en el pie");
});

test("the list shows one row per pane (renderList uses oneRowPerPane)", () => {
  assert.match(source, /for \(const agent of oneRowPerPane\(app\.state\.agents\)\)/, "renderList no deduplica");
});

test("the ⋯ is never inside the row button (no interactive content nested in a button)", () => {
  assert.doesNotMatch(item, /button\.append\([^)]*(menu|foot|toggle|copies)/);
});

test("the card is the positioning box and the ⋯ sits in its corner", () => {
  assert.match(css, /\.agent-card\s*\{[^}]*position:\s*relative/, "falta position: relative en .agent-card");
  assert.match(css, /\.agent-card\s+\.agent-menu-toggle\s*\{[^}]*position:\s*absolute/, "el ⋯ no está en la esquina");
  assert.match(css, /\.agent-card\s+\.agent-menu-toggle\s*\{[^}]*top:\s*4px[^}]*right:\s*6px/, "el ⋯ no está en la esquina superior derecha");
});

test("an open panel takes room in the card's flow: it pushes the cards below, it covers nothing", () => {
  assert.match(css, /\.agent-card\s+\.agent-menu-panel\s*\{[^}]*position:\s*static/, "el panel abierto no ocupa sitio en el flujo");
  assert.doesNotMatch(css, /\.agent-card:has\(\.agent-menu\[open\]\)\s*\{[^}]*z-index/, "no debe haber capa encima: el panel no tapa tarjetas");
});

test("the detail menu keeps its own panel: the base rules in restore.css are unchanged", () => {
  assert.match(restoreCss, /\.agent-menu\s*\{[^}]*position:\s*relative/, ".agent-menu base cambió");
  assert.match(restoreCss, /\.agent-menu-panel\s*\{\s*position:\s*absolute;\s*right:\s*0;/, ".agent-menu-panel base cambió");
});

test("one document listener per key closes the open menus; no listener per card", () => {
  assert.match(source, /document\.addEventListener\("keydown", dismissMenus\)/, "falta el listener de Esc");
  assert.match(source, /document\.addEventListener\("click", dismissMenus\)/, "falta el listener de clic fuera");
  assert.doesNotMatch(item, /document\.addEventListener/, "cada tarjeta no debe añadir listeners al documento");
});

const restoreCss = readFileSync(new URL("../ui/restore.css", import.meta.url), "utf8");
const indexHtml = readFileSync(new URL("../ui/index.html", import.meta.url), "utf8");

test("the agent sessions have their own heading, not the hires' one", () => {
  const sessions = indexHtml.slice(indexHtml.indexOf('id="team-sessions"'), indexHtml.indexOf("</section>", indexHtml.indexOf('id="team-sessions"')));
  assert.match(sessions, /data-i18n="agents\.sessions_title"/, "falta el título «Sesiones de agentes»");
  assert.match(sessions, /id="agent-list"/, "la lista de agentes no está en su sección");
  assert.doesNotMatch(sessions, /team-hires/, "la lista no debe ir bajo «Contratados»");
  assert.match(indexHtml, /id="team-hires" class="team-section hidden"/, "«Contratados» debe nacer oculta hasta la primera lectura");
});

test("the one «Contratar» sits in the stable Equipo header, not under the Contratados title", () => {
  const header = indexHtml.slice(indexHtml.indexOf('class="view-actions"'), indexHtml.indexOf("</header>", indexHtml.indexOf('class="view-actions"')));
  assert.equal(indexHtml.match(/id="btn-hire"/g).length, 1, "debe haber un único #btn-hire");
  assert.match(header, /id="btn-hire"/, "el CTA no está en la cabecera estable de Equipo");
  const hires = indexHtml.slice(indexHtml.indexOf('id="team-hires"'), indexHtml.indexOf("</section>", indexHtml.indexOf('id="team-hires"')));
  assert.doesNotMatch(hires, /btn-hire/, "el CTA no debe ir bajo el título Contratados");
});
