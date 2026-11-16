// Run with: node --test tooling/command-palette-model.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";

import { paletteItems } from "../ui/command-palette-model.js";

const chats = [
  { id: "c1", title: "Creemos un proyecto web" },
  { id: "c2", title: "Bitácora 7 para el Rector" },
];

test("sin texto se ven las acciones y ninguna conversación", () => {
  const items = paletteItems("", chats);
  assert.ok(items.length > 0);
  assert.ok(items.every((item) => item.kind === "action"));
});

test("busca sin tildes ni mayúsculas, acciones y conversaciones", () => {
  assert.deepEqual(
    paletteItems("bitacora", chats).map((item) => item.id),
    ["chat:c2"],
  );
  assert.deepEqual(
    paletteItems("calendario", chats).map((item) => item.id),
    ["nav:calendar"],
  );
  assert.ok(paletteItems("proyecto", chats).some((item) => item.id === "new-project"));
  assert.ok(paletteItems("proyecto", chats).some((item) => item.id === "chat:c1"));
});
