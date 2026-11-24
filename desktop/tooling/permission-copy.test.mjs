// Run with: node --test tooling/permission-copy.test.mjs
// The wording of two settings lines that a tester found unclear: the approvals help
// sentence has a subject, and the mail account name field says what to type.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import esB1 from "../ui/i18n/es.b1.js";
import esB2 from "../ui/i18n/es.b2.js";

const read = (path) => readFileSync(new URL(`../ui/${path}`, import.meta.url), "utf8");

test("Aprobaciones: la frase de ayuda tiene sujeto", () => {
  const source = read("approval-settings.js");
  assert.ok(source.includes('t("approval.help")'));
  assert.equal(esB1["approval.help"], "Una regla o tu equipo decide el resto; solo te llega lo que necesita tu OK.");
  assert.ok(!esB1["approval.help"].includes("Las decide una regla"));
});

test("Correo: el campo del nombre de cuenta tiene un ejemplo de placeholder", () => {
  assert.ok(read("mail.js").includes('name.placeholder = t("mail.name_placeholder");'));
  assert.equal(esB2["mail.name_placeholder"], "Por ejemplo: Trabajo");
});
