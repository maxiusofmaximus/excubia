// Run with: node --test tooling/friendly-error.test.mjs
// Ningún error llega a la pantalla con texto técnico: JS y textos sueltos son «No se pudo cargar».

import { test } from "node:test";
import assert from "node:assert/strict";
import { LOAD_FAILED, OFFLINE_TEXT, REJECTED_TEXT, friendlyError } from "../ui/friendly-error.js";

test("una excepción de JS o un texto suelto se dice «No se pudo cargar»", () => {
  assert.equal(friendlyError(new TypeError("Cannot read properties of undefined (reading 'length')")), LOAD_FAILED);
  assert.equal(friendlyError(new Error("boom")), LOAD_FAILED);
  assert.equal(friendlyError("Connection refused"), LOAD_FAILED);
  assert.equal(friendlyError(undefined), LOAD_FAILED);
  assert.equal(friendlyError(null), LOAD_FAILED);
  assert.equal(LOAD_FAILED, "No se pudo cargar. Inténtalo de nuevo.");
});

test("sin conexión y rechazo del host tienen su frase", () => {
  assert.equal(friendlyError({ kind: "offline", message: "x" }), "Sin conexión con tu PC.");
  assert.equal(friendlyError({ kind: "host", message: "x" }), REJECTED_TEXT);
  assert.equal(friendlyError({ kind: "refused", status: 500 }), REJECTED_TEXT);
  assert.equal(REJECTED_TEXT, "Tu PC no aceptó la petición. Vuelve a vincularlo si sigue pasando.");
  assert.equal(OFFLINE_TEXT, "Sin conexión con tu PC.");
});

test("ninguna frase deja ver jerga ni texto técnico", () => {
  for (const text of [LOAD_FAILED, OFFLINE_TEXT, REJECTED_TEXT]) {
    assert.doesNotMatch(text, /Cannot|undefined|reading|Error|null|rechazó/);
    assert.match(text, /^[A-ZÁÉÍÓÚÑ]/);
    assert.ok(text.endsWith("."));
  }
});
