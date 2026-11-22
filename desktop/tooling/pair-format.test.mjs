// Run with: node --test tooling/pair-format.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";

import { expiryText, svgDataUrl } from "../ui/format.js";

test("a code shows its expiry time until it has passed", () => {
  const expiresAt = 1792811272;
  assert.equal(expiryText(expiresAt, expiresAt * 1000 - 1), `Caduca a las ${new Date(expiresAt * 1000).toLocaleTimeString("es-ES", { hour: "2-digit", minute: "2-digit" })}.`);
});

test("an expired code says so and asks for a new one", () => {
  assert.equal(expiryText(100, 100_000), "Este código ha caducado. Genera otro.");
  assert.equal(expiryText(100, 200_000), "Este código ha caducado. Genera otro.");
});

test("the QR SVG becomes an encoded image source, not markup", () => {
  const url = svgDataUrl('<svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0"/></svg>');
  assert.ok(url.startsWith("data:image/svg+xml;charset=utf-8,"), url);
  assert.ok(!url.includes("<"), "angle brackets must be encoded");
  assert.equal(decodeURIComponent(url.split(",")[1]).startsWith("<svg"), true);
});
