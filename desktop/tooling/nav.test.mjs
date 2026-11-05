// Run with: node --test tooling/nav.test.mjs
// One sidebar: every screen of index.html has a destination (or hides the bar),
// and the old per-view tab bars never come back.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { DESTINATIONS, VIEW_NAMES, ownerOf, showsNav } from "../ui/nav.js";

const html = readFileSync(fileURLToPath(new URL("../ui/index.html", import.meta.url)), "utf8");
const screens = [...html.matchAll(/id="view-([a-z]+)"/g)].map((match) => match[1]);
const START = ["pair", "recovery", "lan", "palette", "onboarding"];

test("every screen of index.html is known to the sidebar", () => {
  assert.deepEqual([...screens].sort(), [...VIEW_NAMES].sort());
});

test("every screen outside the start flow has an active destination", () => {
  for (const view of screens.filter((name) => !START.includes(name))) {
    assert.ok(showsNav(view), `${view} should show the sidebar`);
    assert.ok(DESTINATIONS.includes(ownerOf(view)), `${view} has no destination`);
  }
});

test("start screens hide the sidebar and own no destination", () => {
  for (const view of START) {
    assert.equal(showsNav(view), false, `${view} must hide the sidebar`);
    assert.equal(ownerOf(view), null);
  }
});

test("child screens highlight their parent destination", () => {
  assert.equal(ownerOf("mail"), "chat");
  assert.equal(ownerOf("detail"), "agents");
});

test("the sidebar lists the eight destinations, once each, in order", () => {
  const block = html.match(/<nav id="app-nav"[\s\S]*?<\/nav>/)?.[0] ?? "";
  const listed = [...block.matchAll(/data-nav="([a-z]+)"/g)].map((match) => match[1]);
  assert.deepEqual(listed, DESTINATIONS);
  assert.deepEqual(DESTINATIONS, [
    "chat", "agents", "calendar", "notes", "contacts", "alerts", "devices", "settings",
  ]);
});

test("the duplicated per-view tab bars are gone", () => {
  assert.doesNotMatch(html, /class="nav-tabs"/);
  assert.doesNotMatch(html, /class="nav-tab/);
  assert.doesNotMatch(html, /id="(tab|team-tab|contacts-tab|notes-tab|calendar-tab|devices-tab|alerts-tab)-/);
  assert.doesNotMatch(html, /id="btn-settings(-team)?"/);
});
