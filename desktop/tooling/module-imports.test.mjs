import { test } from "node:test";
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";

// A named import that its module does not export kills the whole UI at load (a blank pair screen);
// node tests of single modules never see it, so check every relative import here.
const dir = new URL("../ui/", import.meta.url);
const sources = Object.fromEntries(
  readdirSync(dir).filter((name) => name.endsWith(".js")).map((name) => [name, readFileSync(new URL(name, dir), "utf8")]),
);

function exportsOf(source) {
  const names = new Set();
  for (const m of source.matchAll(/export\s+(?:async\s+)?(?:function\*?|const|let|var|class)\s+([\w$]+)/g)) names.add(m[1]);
  for (const m of source.matchAll(/export\s*\{([^}]*)\}/g)) {
    for (const part of m[1].split(",")) if (part.trim()) names.add(part.trim().split(/\s+as\s+/).pop());
  }
  return names;
}

test("every named import resolves to an export", () => {
  const problems = [];
  for (const [file, source] of Object.entries(sources)) {
    for (const m of source.matchAll(/import\s*\{([^}]*)\}\s*from\s*"\.\/([^"]+)"/g)) {
      const target = sources[m[2]];
      if (!target) { problems.push(`${file}: missing file ${m[2]}`); continue; }
      const available = exportsOf(target);
      for (const raw of m[1].split(",")) {
        const name = raw.trim().split(/\s+as\s+/)[0];
        if (name && !available.has(name)) problems.push(`${file}: ${name} is not exported by ${m[2]}`);
      }
    }
  }
  assert.deepEqual(problems, []);
});
