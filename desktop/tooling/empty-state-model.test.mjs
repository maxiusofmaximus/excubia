// Run with: node --test tooling/empty-state-model.test.mjs
// El ayudante de vacío: un icono de línea del catálogo, una frase y, como mucho, una acción.

import { test } from "node:test";
import assert from "node:assert/strict";
import { ICONS, emptyStateSpec, errorStateSpec } from "../ui/empty-state-model.js";

test("cada icono es una lista de trazos SVG del mismo estilo que la barra lateral", () => {
  for (const name of ["chat", "calendar", "notes", "contacts", "alerts", "devices", "team", "alert"]) {
    assert.ok(Array.isArray(ICONS[name]) && ICONS[name].length > 0, `falta el icono ${name}`);
    for (const [tag, attrs] of ICONS[name]) {
      assert.ok(["path", "rect", "circle"].includes(tag), `${name}: trazo ${tag} no permitido`);
      assert.equal(typeof attrs, "object");
    }
  }
});

test("un estado vacío lleva icono, una frase y, opcionalmente, una acción", () => {
  assert.deepEqual(
    emptyStateSpec({ icon: "notes", text: "Aún no hay notas. Apunta la primera.", action: "Nueva nota" }),
    { tone: "empty", icon: "notes", text: "Aún no hay notas. Apunta la primera.", action: "Nueva nota" },
  );
  assert.equal(emptyStateSpec({ icon: "calendar", text: "No hay eventos." }).action, null);
});

test("un estado vacío mal formado no se pinta", () => {
  assert.throws(() => emptyStateSpec({ icon: "nope", text: "Hola." }));
  assert.throws(() => emptyStateSpec({ icon: "notes", text: "   " }));
  assert.throws(() => emptyStateSpec({ icon: "notes", text: "Sin punto final" }));
  assert.throws(() => emptyStateSpec({ icon: "notes", text: "Hola.", action: "" }));
});

test("el estado de error lleva su frase, el icono de alerta y Reintentar", () => {
  assert.deepEqual(errorStateSpec("No se pudo cargar. Inténtalo de nuevo."), {
    tone: "error",
    icon: "alert",
    text: "No se pudo cargar. Inténtalo de nuevo.",
    action: "Reintentar",
  });
  assert.throws(() => errorStateSpec(""));
});
