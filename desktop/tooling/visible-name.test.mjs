// Run with: node --test tooling/visible-name.test.mjs
// The product is called Excubia on screen. Any "herdr notify" left in the UI source is a
// visible string nobody renamed, unless it is listed below with the reason it stays.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";

const NAME = /herdr[ -]notify/gi;
const BARE = /herdr/i;

// Each entry is an exact visible line that keeps the old spelling, and why.
const KEEP = [];

// Comments are not on screen; the scan skips them.
const isComment = (text) => /^(\/\/|\/\*|\*|<!--)/.test(text.trim());

const uiFiles = readdirSync(new URL("../ui/", import.meta.url)).filter((name) =>
  /\.(html|js)$/.test(name),
);

test("ui/ no muestra «herdr notify» salvo las cadenas de la lista blanca", () => {
  const found = [];
  for (const file of uiFiles) {
    const lines = readFileSync(new URL(`../ui/${file}`, import.meta.url), "utf8").split("\n");
    lines.forEach((text, index) => {
      if (!isComment(text) && NAME.test(text)) found.push({ file, line: text.trim(), at: index + 1 });
      NAME.lastIndex = 0;
    });
  }

  const unlisted = found.filter((hit) => !KEEP.some((keep) => hit.file === keep.file && hit.line.includes(keep.line)));
  assert.deepEqual(
    unlisted.map((hit) => `${hit.file}:${hit.at} ${hit.line}`),
    [],
    "cadenas visibles con el nombre antiguo: renómbralas a «Excubia» o añádelas a KEEP con motivo",
  );

  const stale = KEEP.filter((keep) => !found.some((hit) => hit.file === keep.file && hit.line.includes(keep.line)));
  assert.deepEqual(
    stale.map((keep) => keep.line),
    [],
    "entradas de KEEP que ya no existen en ui/: bórralas",
  );
});

// The product no longer depends on Tailscale, so its name must not reach any screen.
test("ui/ no menciona Tailscale, tailnet ni ts.net en texto visible", () => {
  const hits = [];
  for (const file of uiFiles) {
    const lines = readFileSync(new URL(`../ui/${file}`, import.meta.url), "utf8").split("\n");
    lines.forEach((text, index) => {
      if (!isComment(text) && /tailscale|tailnet|ts\.net/i.test(text)) hits.push(`${file}:${index + 1} ${text.trim()}`);
    });
  }
  assert.deepEqual(hits, [], "texto visible de Tailscale: redáctalo en neutro (red de casa u oficina, o VPN)");
});

// The bare word (the host's old name) must not reach the chat or agents screens.
test("index.html, agents.js y format.js no muestran «herdr» salvo las excepciones de KEEP", () => {
  const hits = [];
  for (const file of ["index.html", "agents.js", "format.js"]) {
    const lines = readFileSync(new URL(`../ui/${file}`, import.meta.url), "utf8").split("\n");
    lines.forEach((text, index) => {
      const kept = KEEP.some((keep) => keep.file === file && text.includes(keep.line));
      if (!isComment(text) && BARE.test(text) && !kept) hits.push(`${file}:${index + 1} ${text.trim()}`);
    });
  }
  assert.deepEqual(hits, [], "«herdr» visible: renómbralo a «Excubia» o añádelo a KEEP con motivo");
});

// The manual `add-device` key pairing is a legacy flow the host no longer has.
test("ui/ no contiene el comando heredado add-device", () => {
  const hits = [];
  for (const file of uiFiles) {
    const lines = readFileSync(new URL(`../ui/${file}`, import.meta.url), "utf8").split("\n");
    lines.forEach((text, index) => {
      if (/add-device/.test(text)) hits.push(`${file}:${index + 1} ${text.trim()}`);
    });
  }
  assert.deepEqual(hits, [], "add-device no existe en el host 2.0: quita el flujo, no cambies el texto");
});
