// Run with: node --test tooling/approval-stamp.test.mjs
// The «¡Tu OK!» stamp of the permission card comes from the dictionary, not from the CSS literal.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { dictionaries } from "../ui/i18n.js";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");

test("the stamp has a translation in both languages", () => {
  assert.equal(dictionaries.es["approval.stamp"], "¡Tu OK!");
  assert.ok(dictionaries.en["approval.stamp"]);
  assert.notEqual(dictionaries.en["approval.stamp"], dictionaries.es["approval.stamp"]);
});

test("the CSS reads the stamp from the card attribute, with no literal text", () => {
  const css = read("../ui/comic.css");
  assert.doesNotMatch(css, /¡Tu OK!/);
  assert.match(css, /content:\s*attr\(data-stamp\);/);
});

// The plain, mail/agenda and GitHub variants of the approval card all set the stamp from i18n.
test("every card variant sets the stamp from i18n", () => {
  const js = read("../ui/approval-card.js");
  const matches = js.match(/card\.dataset\.stamp = t\("approval\.stamp"\);/g) || [];
  assert.equal(matches.length, 3);
});
