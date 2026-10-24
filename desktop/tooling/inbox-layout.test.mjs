// Run with: node --test tooling/inbox-layout.test.mjs
// The approvals strip and the empty conversation share one column. Their layout
// rules are read from the stylesheets: the strip caps its height and scrolls, and
// the empty state scrolls instead of overflowing into its neighbours.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const stylesheet = (file) => readFileSync(new URL(`../ui/${file}`, import.meta.url), "utf8");

// Declarations of the rule whose selector starts a line and is followed by `{`.
function declarations(source, selector) {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = source.match(new RegExp(`(?:^|\\n)${escaped} \\{([^}]*)\\}`));
  assert.ok(match, `falta la regla ${selector}`);
  return Object.fromEntries(
    match[1]
      .split(";")
      .map((line) => line.trim())
      .filter(Boolean)
      .map((line) => {
        const [property, ...value] = line.split(":");
        return [property.trim(), value.join(":").trim()];
      }),
  );
}

test("la tira de pendientes ocupa su altura natural con tope y scroll propio", () => {
  const rule = declarations(stylesheet("approvals.css"), ".approval-inbox");
  assert.equal(rule.flex, "none");
  assert.equal(rule["max-height"], "40vh");
  assert.equal(rule["overflow-y"], "auto");
  assert.notEqual(rule.position, "absolute");
});

test("el estado vacío cede con scroll y no queda fuera del flujo", () => {
  const rule = declarations(stylesheet("thread.css"), ".chat-empty");
  assert.equal(rule["justify-content"], "safe center");
  assert.equal(rule["overflow-y"], "auto");
  assert.notEqual(rule.position, "absolute");
});
