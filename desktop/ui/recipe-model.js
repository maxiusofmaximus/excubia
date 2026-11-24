// «Recetas»: the rules of a saved text, free of DOM so tooling/recipe-model.test.mjs can run
// them. The host decides what is saved; the words here are the operator's, and each host
// reason gets its plain sentence. A reason this list does not know is never shown raw.

import { friendlyError } from "./friendly-error.js";
import { planFailureText } from "./plan-model.js";
import { t } from "./i18n.js";

export const TITLE_MAX = 80;
export const BODY_MAX = 2000;
export const RISK_CONFIRM_TEXT = t("recipe.risk_confirm");

export const PERMISSION_TEXT = t("recipe.permission");
export const SECRET_TEXT = t("recipe.secret");
const UNKNOWN_REASON = t("recipe.reason_unknown");

// The sentences the host writes for each secret or order it finds, in its own Spanish.
const REASON_TEXT = {
  "parece una clave de API (sk-…)": t("recipe.reason.api_key"),
  "parece un token de GitHub": t("recipe.reason.github_token"),
  "parece una clave de AWS (AKIA…)": t("recipe.reason.aws_key"),
  "parece una cadena hexadecimal larga (posible clave)": t("recipe.reason.hex_key"),
  "contiene una cadena larga de letras y números (posible clave)": t("recipe.reason.long_key"),
  "parece una contraseña escrita como «clave: valor»": t("recipe.reason.password"),
  "contiene una clave privada": t("recipe.reason.private_key"),
  "lleva usuario y contraseña dentro de una dirección web": t("recipe.reason.url_credentials"),
  "intenta anular las instrucciones anteriores": t("recipe.reason.prompt_injection"),
  "ordena borrar todo": t("recipe.reason.delete_all"),
  "pide actuar sin aprobación": t("recipe.reason.no_approval"),
};

const RECIPE_TEXT = {
  recipe_limit: t("recipe.err.limit"),
  recipe_title_taken: t("recipe.err.title_taken"),
  no_hire_for_role: t("recipe.err.no_hire"),
  delegation_not_done: t("recipe.err.not_done"),
  conversation_has_no_project: t("recipe.err.no_project"),
  invalid_recipe: t("recipe.err.invalid"),
};
const NOT_FOUND_TEXT = t("recipe.err.not_found");

// Characters, not UTF-16 units: an emoji is one character to the person and to the host.
function count(text) {
  return [...text].length;
}

export function titleError(title) {
  if (!title.trim()) return t("recipe.title_required");
  if (count(title.trim()) > TITLE_MAX) return t("recipe.title_max", { max: TITLE_MAX });
  return null;
}

export function bodyError(body) {
  if (!body.trim()) return t("recipe.body_required");
  if (count(body) > BODY_MAX) return t("recipe.body_max", { max: BODY_MAX });
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
  return seconds > 0 ? t("recipe.busy_secs", { s: seconds }) : t("recipe.busy");
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
      text: t("recipe.risk_notice"),
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
  if (uses === 0) return t("recipe.uses_none");
  return uses === 1 ? t("recipe.uses_one") : t("recipe.uses_other", { n: uses });
}
