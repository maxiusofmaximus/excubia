// Run with: node --test tooling/format-path.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";

import { displayPath } from "../ui/format.js";

test("la ruta del host se muestra sin el prefijo de ruta larga", () => {
  const verbatim = String.raw`\\?\C:\Users\Max\bench-proj`;
  const plain = String.raw`C:\Users\Max\bench-proj`;
  assert.equal(displayPath(verbatim), plain);
  assert.equal(displayPath(plain), plain);
  assert.equal(displayPath(null), "");
});
