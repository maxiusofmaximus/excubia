// Run with: node --test tooling/output-model.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";

import { plainOutput } from "../ui/output-model.js";

test("una orden de delegación se lee como encargo, sin JSON ni ids", () => {
  const raw = '{"delegation_id":"dlg_01M7M8TR21","from":"Alfred","task":"Crear redi-cinco.txt con el texto uno"}';
  assert.equal(plainOutput(raw), "Encargo de Alfred: Crear redi-cinco.txt con el texto uno");
});

test("las rutas largas, los backticks y los ids salen de la salida", () => {
  assert.equal(
    plainOutput("Escribí `C:\\Users\\Max\\bench-proj\\redi-cinco.txt` ok dlg_01M7 fin"),
    "Escribí redi-cinco.txt ok fin",
  );
  assert.equal(plainOutput("{no es json"), "{no es json");
  assert.equal(plainOutput(""), "");
});
