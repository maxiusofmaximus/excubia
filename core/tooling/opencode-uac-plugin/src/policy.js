import path from "node:path";

const SHELL_TOOLS = new Set(["bash", "shell", "powershell", "cmd", "exec_command", "run_command", "terminal"]);
const NETWORK_TOOLS = new Set(["webfetch", "fetch", "websearch", "web_search"]);
const EDIT_TOOLS = new Set(["edit", "write", "multiedit", "apply_patch", "patch", "create", "delete", "move", "rename"]);
const ALWAYS_RISKY_EDIT = new Set(["apply_patch", "patch"]);

function text(value, max = 160) {
  if (value === undefined || value === null) return "";
  const raw = typeof value === "string" ? value : JSON.stringify(value);
  const flat = String(raw).replace(/\s+/g, " ").trim();
  return flat.length > max ? `${flat.slice(0, max)}…` : flat;
}

function firstString(...values) {
  for (const value of values) {
    if (typeof value === "string" && value.trim()) return value.trim();
  }
  return "";
}

function toolPath(input) {
  if (!input || typeof input !== "object") return "";
  const direct = firstString(input.filePath, input.file_path, input.path, input.file, input.filename, input.target);
  if (direct) return direct;
  for (const value of Object.values(input)) {
    if (Array.isArray(value)) {
      const found = value.map((item) => (typeof item === "string" ? item : item?.filePath ?? item?.path)).find(Boolean);
      if (found) return String(found);
    }
  }
  return "";
}

function commandText(input) {
  return firstString(input?.command, input?.cmd, input?.script, input?.code) || text(input, 200);
}

function urlText(input) {
  return firstString(input?.url, input?.uri, input?.query, input?.q) || text(input, 160);
}

function clip(value, max = 160) {
  const flat = String(value ?? "").replace(/\s+/g, " ").trim();
  return flat.length > max ? `${flat.slice(0, max)}…` : flat;
}

function inside(root, candidate) {
  const base = path.resolve(root);
  const target = path.resolve(candidate);
  const left = process.platform === "win32" ? base.toLowerCase() : base;
  const right = process.platform === "win32" ? target.toLowerCase() : target;
  const separator = left.endsWith(path.sep) ? left : `${left}${path.sep}`;
  return right === left || right.startsWith(separator);
}

export function extraTools(env = process.env) {
  return new Set(
    String(env.HN_UAC_EXTRA_TOOLS ?? "")
      .split(",")
      .map((item) => item.trim().toLowerCase())
      .filter(Boolean),
  );
}

/**
 * Devuelve una acción que requiere UAC, o null si la herramienta es de bajo riesgo.
 * `cwd` debe ser el directorio real del proyecto, no el del proceso del servidor.
 */
export function classify(tool, input, cwd, extra = new Set()) {
  const name = String(tool ?? "").toLowerCase();
  if (!name) return null;

  if (SHELL_TOOLS.has(name)) {
    return { kind: "command", summary: `Ejecutar comando: ${clip(commandText(input))}`, paths: [] };
  }
  if (NETWORK_TOOLS.has(name)) {
    return { kind: "network", summary: `Acceso de red: ${clip(urlText(input))}`, paths: [] };
  }
  if (EDIT_TOOLS.has(name)) {
    const declared = toolPath(input);
    if (!declared) {
      return { kind: "edit", summary: `Editar con ruta no declarada (${name})`, paths: [] };
    }
    const absolute = path.resolve(cwd, declared);
    if (!ALWAYS_RISKY_EDIT.has(name) && inside(cwd, absolute)) return null;
    return { kind: "edit", summary: `Editar fuera del proyecto: ${clip(absolute, 200)}`, paths: [absolute] };
  }
  if (extra.has(name)) {
    return { kind: "tool", summary: `Herramienta sujeta a UAC: ${name}`, paths: [] };
  }
  return null;
}

export const POLICY_TOOLS = {
  shell: [...SHELL_TOOLS],
  network: [...NETWORK_TOOLS],
  edit: [...EDIT_TOOLS],
};
