// Run with: node --test tooling/comic-phase2.test.mjs
// Phase 2 of the comic skin: the permission chips of Dispositivos (the model, and the screen
// that draws them), the palette picker's grid and «Nueva» stamp, and the new screen entry.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { permissionChips, permissionWords } from "../ui/devices-model.js";

const uiPath = (name) => fileURLToPath(new URL(`../ui/${name}`, import.meta.url));
const read = (name) => readFileSync(uiPath(name), "utf8");

test("cada permiso del contrato sale como chip, en su orden, concedido o no", () => {
  const chips = permissionChips(["read_state", "approve"]);
  assert.deepEqual(
    chips.map((chip) => chip.id),
    ["read_state", "respond", "send_key", "approve", "admin"],
  );
  assert.deepEqual(
    chips.map((chip) => chip.granted),
    [true, false, false, true, false],
  );
  assert.equal(chips[0].label, "Leer estado");
});

test("sin permisos, todos los chips salen sin conceder: nada se oculta", () => {
  const chips = permissionChips([]);
  assert.equal(chips.length, 5);
  assert.ok(chips.every((chip) => chip.granted === false));
});

test("un permiso desconocido no sale como chip ni en crudo", () => {
  const chips = permissionChips(["surprise"]);
  assert.ok(chips.every((chip) => chip.id !== "surprise"));
  assert.deepEqual(permissionWords(["surprise"]), []);
});

test("el screen pinta los permisos como chips: ✓ concedido, « – » no concedido", () => {
  const devices = read("devices.js");
  assert.match(devices, /function permissionRow\(device\)/);
  assert.match(devices, /granted \? "✓" : "–"/);
  assert.match(devices, /granted \? "perm-chip is-granted" : "perm-chip"/);
  assert.match(devices, /permissionRow\(device\)/);
});

test("la fila de permisos no usa el texto plano en la lista", () => {
  // The list row shows chips now; the plain words are gone from its markup.
  assert.doesNotMatch(read("devices.js"), /permissionsText\(device\.permissions\)/);
});

test("la rejilla de paletas llena las columnas que caben, sin una huérfana", () => {
  const palette = read("palette.css");
  assert.match(palette, /grid-template-columns: repeat\(auto-fill, minmax\(200px, 1fr\)\);/);
  assert.doesNotMatch(palette, /grid-template-columns: repeat\(3, minmax/);
});

test("Telón lleva el sello «Nueva» y cada tarjeta su muestra bg/acento", () => {
  const js = read("palette.js");
  assert.match(js, /NEW_PALETTES = new Set\(\["telon"\]\)/);
  assert.match(js, /stamp\.textContent = "Nueva"/);
  assert.match(js, /--sw-bg/);
  assert.match(js, /--sw-accent/);
});

// The portrait rules: the last sheet wins on equal specificity, so these read comic.css alone.
function cssBlock(selector) {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = read("comic.css").match(new RegExp(`^${escaped} \\{([^}]*)\\}`, "m"));
  return match ? match[1] : "";
}

test("el retrato es rectángulo: .avatar en comic.css no tiene border-radius 50%", () => {
  const block = cssBlock(".avatar");
  assert.match(block, /border-radius: 0;/);
  assert.doesNotMatch(block, /border-radius: 50%/);
});

test("el retrato del mensaje no recorta la cabeza: .msg-avatar.avatar no fuerza overflow hidden", () => {
  const block = cssBlock(".msg-avatar.avatar");
  assert.match(block, /overflow: visible;/);
  assert.doesNotMatch(block, /overflow: hidden/);
});

test("el marco es el retrato: esquina cortada en la imagen y en el borde, sin inclinar ni anclar abajo", () => {
  assert.match(cssBlock(".avatar img"), /clip-path: polygon\(/);
  assert.match(cssBlock(".avatar::before"), /clip-path: polygon\(/);
  for (const block of [cssBlock(".avatar img"), cssBlock(".avatar::before")]) {
    assert.doesNotMatch(block, /skew|118%|bottom: 0/);
  }
});

test("el borde del retrato es --pale de 3 px y no hay anillo azul suelto", () => {
  assert.match(cssBlock(".avatar::before"), /background: var\(--pale\);/);
  assert.match(cssBlock(".avatar img"), /left: 3px;\s*top: 3px;/);
  assert.equal(cssBlock(".avatar::after"), "");
  assert.match(cssBlock(".avatar:not(:has(img))::after"), /background: var\(--hot\);/);
});

test("la sombra dura cae sobre el marco entero, no sobre la imagen", () => {
  assert.match(cssBlock(".avatar"), /filter: drop-shadow\(3px 3px 0 var\(--hard\)\);/);
  assert.doesNotMatch(cssBlock(".avatar img"), /filter/);
});

test("el turno de entrada es una vez por pantalla: nav.js lo quita tras la animación", () => {
  const nav = read("nav.js");
  assert.match(nav, /enteredViews\.has\(name\)/);
  assert.match(nav, /classList\.add\("comic-enter"\)/);
  assert.match(nav, /setTimeout\(\(\) => screen\.classList\.remove\("comic-enter"\), 450\)/);
  assert.match(read("comic.css"), /\.comic-enter \{\s*animation: comic-screen-in 0\.4s ease-out;/);
});
