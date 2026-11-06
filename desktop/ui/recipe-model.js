// «Recetas»: the rules of a saved text, free of DOM so tooling/recipe-model.test.mjs can run
// them. The host decides what is saved; the words here are the operator's, and each host
// reason gets its plain sentence. A reason this list does not know is never shown raw.

import { friendlyError } from "./friendly-error.js";
import { planFailureText } from "./plan-model.js";

export const TITLE_MAX = 80;
export const BODY_MAX = 2000;
export const RISK_CONFIRM_TEXT = "Entiendo y quiero guardarla";

export const PERMISSION_TEXT =
  "Este dispositivo no tiene el permiso APPROVE: puedes ver las recetas, pero no guardarlas, repetirlas ni borrarlas.";
export const SECRET_TEXT = "Quita la clave o contraseña del texto. Con un secreto dentro no se puede guardar.";
const UNKNOWN_REASON = "Hay una parte del texto que no podemos explicar. Revísala.";

// The sentences the host writes for each secret or order it finds, in its own Spanish.
const REASON_TEXT = {
  "parece una clave de API (sk-…)": "Parece una clave de API.",
  "parece un token de GitHub": "Parece un token de GitHub.",
  "parece una clave de AWS (AKIA…)": "Parece una clave de AWS.",
  "parece una cadena hexadecimal larga (posible clave)": "Parece una clave escrita en hexadecimal.",
  "contiene una cadena larga de letras y números (posible clave)": "Parece una clave: una cadena larga de letras y números.",
  "parece una contraseña escrita como «clave: valor»": "Parece una contraseña escrita como «clave: valor».",
  "contiene una clave privada": "Contiene una clave privada.",
  "lleva usuario y contraseña dentro de una dirección web": "Lleva usuario y contraseña dentro de una dirección web.",
  "intenta anular las instrucciones anteriores": "Intenta anular las instrucciones anteriores.",
  "ordena borrar todo": "Ordena borrar todo.",
  "pide actuar sin aprobación": "Pide actuar sin aprobación.",
};

const RECIPE_TEXT = {
  recipe_limit: "Este oficio ya tiene 50 recetas. Borra una para guardar otra.",
  recipe_title_taken: "Ya hay una receta con ese título en este oficio. Cámbiale el título.",
  no_hire_for_role: "Este oficio ya no está en tu equipo. Contrátalo de nuevo para repetir sus recetas.",
  delegation_not_done: "Este encargo todavía no ha terminado. Guárdalo cuando esté hecho.",
  conversation_has_no_project: "Esta conversación no tiene proyecto. Abre una del proyecto para repetir el encargo.",
  invalid_recipe: "El texto no es válido: no admite HTML ni caracteres raros.",
};
const NOT_FOUND_TEXT = "Esa receta o ese encargo ya no existe.";

// Characters, not UTF-16 units: an emoji is one character to the person and to the host.
function count(text) {
  return [...text].length;
}

export function titleError(title) {
  if (!title.trim()) return "Ponle un título a la receta.";
  if (count(title.trim()) > TITLE_MAX) return `El título admite ${TITLE_MAX} caracteres como máximo.`;
  return null;
}

export function bodyError(body) {
  if (!body.trim()) return "Escribe el texto de la receta.";
  if (count(body) > BODY_MAX) return `El texto admite ${BODY_MAX} caracteres como máximo.`;
  return null;
}

export function limitCount(text, max) {
  return `${count(text)}/${max}`;
}

export function canSave(title, body) {
  return titleError(title) === null && bodyError(body) === null;
}

export function plainReasons(reasons = []) {
  return reasons.map((reason) => REASON_TEXT[reason] ?? UNKNOWN_REASON);
}

// The busy text names the wait the host sent; the screen never retries on its own.
function busyText(seconds) {
  return seconds > 0 ? `Ahora está ocupado; vuelve a intentarlo en ${seconds} s.` : "Ahora está ocupado; vuelve a intentarlo en unos segundos.";
}

// What the operator reads for a failed recipe call. A plan refusal keeps the plan's words.
export function recipeFailureText(error) {
  if (error && error.kind === "recipeRefused" && Object.hasOwn(RECIPE_TEXT, error.code)) {
    return RECIPE_TEXT[error.code];
  }
  if (error && error.kind === "notFound") return NOT_FOUND_TEXT;
  if (error && error.kind === "planRefused") return planFailureText(error);
  if (error && error.kind === "forbidden") return PERMISSION_TEXT;
  if (error && error.kind === "agentBusy") return busyText(error.retry_after_secs);
  return friendlyError(error);
}

// What the review editor shows for a refused save: a secret blocks it, a risk waits for a tick.
export function reviewNotice(error) {
  if (error && error.kind === "recipeRefused" && error.code === "secret_detected") {
    return { tone: "secret", text: SECRET_TEXT, reasons: plainReasons(error.reasons) };
  }
  if (error && error.kind === "recipeRefused" && error.code === "risk_needs_confirmation") {
    return {
      tone: "risk",
      text: "Esta receta da órdenes de riesgo. Revísala; si la quieres guardar igualmente, márcalo abajo.",
      reasons: plainReasons(error.reasons),
    };
  }
  return { tone: "error", text: recipeFailureText(error), reasons: [] };
}

// The most used first; between equals, the one changed most recently.
export function sortRecipes(recipes) {
  const recent = (item) => Date.parse(item.updated_at) || 0;
  return [...recipes].sort((a, b) => b.uses - a.uses || recent(b) - recent(a));
}

// «Sin usar todavía», «Usada 1 vez», «Usada 4 veces».
export function usesText(uses) {
  if (uses === 0) return "Sin usar todavía";
  return uses === 1 ? "Usada 1 vez" : `Usada ${uses} veces`;
}
