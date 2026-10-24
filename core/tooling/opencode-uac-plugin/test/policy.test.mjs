import test from "node:test";
import assert from "node:assert/strict";
import path from "node:path";
import { classify, extraTools } from "../src/policy.js";

const cwd = path.resolve("D:\\proyectos\\demo");

test("shell siempre es riesgo command", () => {
  const result = classify("bash", { command: "cargo test" }, cwd);
  assert.equal(result.kind, "command");
  assert.match(result.summary, /cargo test/);
});

test("write dentro del cwd no pide UAC", () => {
  assert.equal(classify("write", { filePath: path.join(cwd, "src", "a.ts") }, cwd), null);
});

test("write fuera del cwd pide UAC con ruta absoluta", () => {
  const result = classify("write", { filePath: "D:\\otro\\secreto.txt" }, cwd);
  assert.equal(result.kind, "edit");
  assert.deepEqual(result.paths, ["D:\\otro\\secreto.txt"]);
});

test("read no pide UAC", () => {
  assert.equal(classify("read", { filePath: "C:\\Windows\\win.ini" }, cwd), null);
});

test("webfetch pide UAC de red", () => {
  const result = classify("webfetch", { url: "https://example.com" }, cwd);
  assert.equal(result.kind, "network");
});

test("apply_patch siempre pide UAC aunque la ruta declarada esté dentro", () => {
  const result = classify("apply_patch", { path: path.join(cwd, "a.txt") }, cwd);
  assert.equal(result.kind, "edit");
});

test("herramienta extra configurada pide UAC", () => {
  const extra = extraTools({ HN_UAC_EXTRA_TOOLS: "mcp_cliente, Otra" });
  const result = classify("mcp_cliente", {}, cwd, extra);
  assert.equal(result.kind, "tool");
});

test("herramienta desconocida no se bloquea por defecto", () => {
  assert.equal(classify("glob", { pattern: "**/*.ts" }, cwd), null);
});
