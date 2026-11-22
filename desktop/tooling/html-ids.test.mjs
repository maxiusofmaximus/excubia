import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

// A repeated id makes `getElementById` answer the first element for both: «Cargar más» once
// hid the whole «Más ▾» menu of the calendar, and with it «Importar/Exportar .ics».
test("no id is used twice in index.html", () => {
  const html = readFileSync(new URL("../ui/index.html", import.meta.url), "utf8");
  const seen = new Set();
  const repeated = new Set();
  for (const [, id] of html.matchAll(/\sid="([^"]+)"/g)) {
    if (seen.has(id)) repeated.add(id);
    seen.add(id);
  }
  assert.deepEqual([...repeated], []);
});
