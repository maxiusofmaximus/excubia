// The screen is DOM code no test loads, so a name it uses from the model but never imports only
// shows up on the Master's screen as «No se pudo cargar». This keeps that from slipping through.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (name) => readFileSync(new URL(`../ui/${name}`, import.meta.url), "utf8");

test("every name devices.js uses from devices-model.js is imported", () => {
  const model = read("devices-model.js");
  const screen = read("devices.js");
  const exported = [...model.matchAll(/export (?:const|function) (\w+)/g)].map((match) => match[1]);
  const importBlock = screen.match(/import \{([^}]*)\} from "\.\/devices-model\.js"/)?.[1] ?? "";
  const imported = new Set(importBlock.split(",").map((name) => name.trim()).filter(Boolean));
  const body = screen.replace(/import \{[^}]*\} from "\.\/devices-model\.js";/, "");
  const missing = exported.filter((name) => new RegExp(`\\b${name}\\b`).test(body) && !imported.has(name));
  assert.deepEqual(missing, []);
});
