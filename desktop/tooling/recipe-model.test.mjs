// Run with: node --test tooling/recipe-model.test.mjs
// «Recetas»: límites del texto, frases de secreto y riesgo, errores del host y orden de la lista.

import { test } from "node:test";
import assert from "node:assert/strict";
import {
  BODY_MAX,
  PERMISSION_TEXT,
  SECRET_TEXT,
  TITLE_MAX,
  bodyError,
  canSave,
  limitCount,
  plainReasons,
  recipeFailureText,
  reviewNotice,
  sortRecipes,
  titleError,
  usesText,
} from "../ui/recipe-model.js";
import { REJECTED_TEXT } from "../ui/friendly-error.js";

function recipe(overrides = {}) {
  return {
    id: "rcp_1",
    role_id: "audio-editor",
    title: "Resumen",
    body: "Haz un resumen.",
    uses: 0,
    created_at: "2026-11-05T10:00:00.000Z",
    updated_at: "2026-11-05T10:00:00.000Z",
    ...overrides,
  };
}

test("el título admite 80 caracteres y pide uno", () => {
  assert.equal(TITLE_MAX, 80);
  assert.equal(titleError("   "), "Ponle un título a la receta.");
  assert.equal(titleError("a".repeat(80)), null);
  assert.equal(titleError("a".repeat(81)), "El título admite 80 caracteres como máximo.");
});

test("el texto admite 2000 caracteres y pide uno", () => {
  assert.equal(BODY_MAX, 2000);
  assert.equal(bodyError("\n  "), "Escribe el texto de la receta.");
  assert.equal(bodyError("é".repeat(2000)), null);
  assert.equal(bodyError("é".repeat(2001)), "El texto admite 2000 caracteres como máximo.");
});

test("los contadores cuentan caracteres, no unidades de código", () => {
  assert.equal(limitCount("😀😀", 80), "2/80");
  assert.equal(limitCount("", 2000), "0/2000");
});

test("se puede guardar solo con título y texto válidos", () => {
  assert.equal(canSave("Resumen", "Haz un resumen."), true);
  assert.equal(canSave("", "Haz un resumen."), false);
  assert.equal(canSave("Resumen", ""), false);
});

test("cada motivo de secreto y de riesgo que envía el host tiene su frase llana", () => {
  assert.deepEqual(plainReasons(["parece una clave de API (sk-…)"]), ["Parece una clave de API."]);
  assert.deepEqual(plainReasons(["intenta anular las instrucciones anteriores"]), [
    "Intenta anular las instrucciones anteriores.",
  ]);
  assert.deepEqual(plainReasons(["ordena borrar todo"]), ["Ordena borrar todo."]);
  assert.deepEqual(plainReasons(["pide actuar sin aprobación"]), ["Pide actuar sin aprobación."]);
  assert.deepEqual(plainReasons(["texto que el host no conoce"]), [
    "Hay una parte del texto que no podemos explicar. Revísala.",
  ]);
  assert.deepEqual(plainReasons(undefined), []);
});

test("un secreto se explica sin dejar guardar, y un riesgo se puede confirmar", () => {
  const secret = reviewNotice({
    kind: "recipeRefused",
    code: "secret_detected",
    reasons: ["parece una contraseña escrita como «clave: valor»"],
  });
  assert.equal(secret.tone, "secret");
  assert.equal(secret.text, SECRET_TEXT);
  assert.match(SECRET_TEXT, /^Quita la clave o contraseña del texto/);
  assert.deepEqual(secret.reasons, ["Parece una contraseña escrita como «clave: valor»."]);

  const risk = reviewNotice({ kind: "recipeRefused", code: "risk_needs_confirmation", reasons: ["ordena borrar todo"] });
  assert.equal(risk.tone, "risk");
  assert.deepEqual(risk.reasons, ["Ordena borrar todo."]);
});

test("cada código de refusal de receta tiene su frase y los demás caen al texto general", () => {
  const cases = {
    recipe_limit: "Este oficio ya tiene 50 recetas. Borra una para guardar otra.",
    recipe_title_taken: "Ya hay una receta con ese título en este oficio. Cámbiale el título.",
    no_hire_for_role: "Este oficio ya no está en tu equipo. Contrátalo de nuevo para repetir sus recetas.",
    delegation_not_done: "Este encargo todavía no ha terminado. Guárdalo cuando esté hecho.",
    conversation_has_no_project: "Esta conversación no tiene proyecto. Abre una del proyecto para repetir el encargo.",
    invalid_recipe: "El texto no es válido: no admite HTML ni caracteres raros.",
  };
  for (const [code, text] of Object.entries(cases)) {
    assert.equal(recipeFailureText({ kind: "recipeRefused", code }), text, code);
  }
  assert.equal(recipeFailureText({ kind: "recipeRefused", code: "algo_nuevo" }), REJECTED_TEXT);
  assert.equal(recipeFailureText({ kind: "notFound", message: "x" }), "Esa receta o ese encargo ya no existe.");
});

test("un plan sin aprobar y un permiso ausente usan sus frases, nunca el JSON", () => {
  assert.equal(recipeFailureText({ kind: "planRefused", code: "plan_not_approved" }), "Este plan todavía no está aprobado.");
  assert.equal(recipeFailureText({ kind: "forbidden", message: "x", action: "APPROVE" }), PERMISSION_TEXT);
});

test("un agente ocupado dice cuántos segundos esperar, sin reintentar solo", () => {
  assert.equal(
    recipeFailureText({ kind: "agentBusy", retry_after_secs: 30, status: 409 }),
    "Ahora está ocupado; vuelve a intentarlo en 30 s.",
  );
  assert.equal(
    recipeFailureText({ kind: "agentBusy", retry_after_secs: null, status: 409 }),
    "Ahora está ocupado; vuelve a intentarlo en unos segundos.",
  );
});

test("la lista va por más usadas y, a igual uso, por la más reciente", () => {
  const sorted = sortRecipes([
    recipe({ id: "a", uses: 1, updated_at: "2026-11-05T10:00:00.000Z" }),
    recipe({ id: "b", uses: 3, updated_at: "2026-10-01T10:00:00.000Z" }),
    recipe({ id: "c", uses: 1, updated_at: "2026-11-06T10:00:00.000Z" }),
  ]);
  assert.deepEqual(
    sorted.map((item) => item.id),
    ["b", "c", "a"],
  );
});

test("el contador de usos se lee en plural y singular", () => {
  assert.equal(usesText(0), "Sin usar todavía");
  assert.equal(usesText(1), "Usada 1 vez");
  assert.equal(usesText(4), "Usada 4 veces");
});
