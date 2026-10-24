import { createHash } from "node:crypto";

const SECRET_KEY = /(token|secret|password|passwd|api[-_]?key|authorization|cookie|credential)/i;
const MAX_DEPTH = 6;
const MAX_ITEMS = 50;
const MAX_STRING = 2000;

function stable(value, depth = 0) {
  if (value === null || typeof value !== "object") return value;
  if (depth >= MAX_DEPTH) return "[truncado]";
  if (Array.isArray(value)) return value.slice(0, MAX_ITEMS).map((item) => stable(item, depth + 1));
  const out = {};
  for (const key of Object.keys(value).sort()) out[key] = stable(value[key], depth + 1);
  return out;
}

function scrub(value) {
  return String(value)
    .replace(
      /((?:token|secret|password|passwd|api[-_]?key|authorization)\s*[=:]\s*)(?:(?:Bearer|Basic)\s+)?("[^"]*"|'[^']*'|[^\s'"]+)/gi,
      (_match, prefix, raw) => {
        const quote = raw.startsWith('"') || raw.startsWith("'") ? raw[0] : "";
        return `${prefix}${quote}[redactado]${quote}`;
      },
    )
    .replace(/\b(Bearer|Basic)\s+[A-Za-z0-9._~+/=-]+/gi, "$1 [redactado]")
    .replace(/(--(?:token|password|secret|api-key)\s+)\S+/gi, "$1[redactado]");
}

function redactValue(value, depth = 0) {
  if (value === null || typeof value !== "object") {
    if (typeof value === "string") return scrub(value.length > MAX_STRING ? `${value.slice(0, MAX_STRING)}…` : value);
    return value;
  }
  if (depth >= MAX_DEPTH) return "[truncado]";
  if (Array.isArray(value)) return value.slice(0, MAX_ITEMS).map((item) => redactValue(item, depth + 1));
  const out = {};
  for (const [key, item] of Object.entries(value)) {
    out[key] = SECRET_KEY.test(key) ? "[redactado]" : redactValue(item, depth + 1);
  }
  return out;
}

/** JSON estable: el mismo contenido produce el mismo digest aunque cambie el orden de claves. */
export function canonicalJson(value) {
  return JSON.stringify(stable(value));
}

/** SHA-256 de kind + tool + args + cwd + paths, tal como exige el RFC §5. */
export function actionDigest({ kind, tool, args, cwd, paths = [] }) {
  const canonical = canonicalJson({
    kind,
    tool,
    args: args ?? {},
    cwd,
    paths: [...paths].sort(),
  });
  return `sha256:${createHash("sha256").update(canonical, "utf8").digest("hex")}`;
}

/** Copia para mostrar en la bandeja: nunca incluye valores de claves sensibles. */
export function redact(value) {
  return redactValue(value);
}

/** Limpia secretos típicos de un texto libre (comando, URL, resumen). */
export function redactText(value) {
  return scrub(value);
}
