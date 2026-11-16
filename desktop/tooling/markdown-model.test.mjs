// Run with: node --test tooling/markdown-model.test.mjs
// The reply reader: blocks, inline runs, and that only http(s) links become links.
import { test } from "node:test";
import assert from "node:assert/strict";

import { parseBlocks, parseInline, safeUrl } from "../ui/markdown-model.js";

test("listas, encabezados, código y párrafos", () => {
  const blocks = parseBlocks("## Equipo\n\n- **Frontend**: web\n- QA\n\n1. uno\n2. dos\n\n```\nlet a = 1;\n```\nfin");
  assert.deepEqual(
    blocks.map((block) => block.type),
    ["heading", "list", "list", "code", "paragraph"],
  );
  assert.equal(blocks[1].items.length, 2);
  assert.equal(blocks[2].ordered, true);
  assert.equal(blocks[3].text, "let a = 1;");
});

test("negrita, cursiva, código y enlaces en línea", () => {
  const runs = parseInline("**Upwork** y *Malt*: `npm` [GitHub](https://github.com) o https://workana.com.");
  assert.deepEqual(
    runs.map((run) => run.type),
    ["strong", "text", "em", "text", "code", "text", "link", "text", "link", "text"],
  );
  assert.equal(runs[6].href, "https://github.com");
  assert.equal(runs[8].href, "https://workana.com");
  assert.equal(runs[9].text, ".");
});

test("un enlace que no es http(s) se queda como texto", () => {
  const runs = parseInline("[clic](javascript:alert(1)) y [archivo](file:///C:/x)");
  assert.ok(runs.every((run) => run.type === "text"));
  assert.equal(safeUrl("javascript:alert(1)"), null);
  assert.equal(safeUrl("https://ok.example/a?b=1"), "https://ok.example/a?b=1");
});

test("el texto con < y > no se interpreta como HTML", () => {
  const runs = parseInline("<img src=x onerror=alert(1)>");
  assert.deepEqual(runs, [{ type: "text", text: "<img src=x onerror=alert(1)>" }]);
});
