// Run with: node --test tooling/palette-names.test.mjs
// The palette names shown in the picker and in «La decide el sistema»: each id has its name in both
// languages, Telón stays the product name, and the Spanish names are the ones palette-data.js keeps.
import { test } from "node:test";
import assert from "node:assert/strict";

import { PALETTES } from "../ui/palettes.js";
import { dictionaries } from "../ui/i18n.js";

const { es, en } = dictionaries;

test("cada paleta tiene su nombre en español (el de palette-data.js) y en inglés", () => {
  for (const [id, palette] of Object.entries(PALETTES)) {
    assert.equal(es[`palette.name.${id}`], palette.name, `${id} en es`);
    assert.ok(Object.hasOwn(en, `palette.name.${id}`), `${id} sin inglés`);
  }
});

test("los seis nombres descriptivos cambian en inglés y Telón se queda igual", () => {
  for (const id of ["noche", "pergamino", "carbon", "papel", "medianoche", "contraste"]) {
    assert.notEqual(en[`palette.name.${id}`], es[`palette.name.${id}`], id);
  }
  assert.equal(en["palette.name.telon"], "Telón");
  assert.equal(es["palette.name.telon"], "Telón");
});

test("la cabecera del selector de idioma sigue el idioma activo", () => {
  assert.equal(es["settings.language_title"], "Idioma");
  assert.equal(en["settings.language_title"], "Language");
});
