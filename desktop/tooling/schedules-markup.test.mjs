// «Automatizar» in index.html: the PC-off note is in the section itself (never behind a click), and
// the ids the scripts read exist. Reads the page as text, so a rename breaks the test, not the app.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { PC_OFF_NOTE } from "../ui/schedule-model.js";

const html = readFileSync(new URL("../ui/index.html", import.meta.url), "utf8");
const section = html.slice(html.indexOf('id="team-schedules"'), html.indexOf('id="team-hires"'));

test("the PC-off note is in the Equipo section, visible without a click", () => {
  assert.ok(section.includes(PC_OFF_NOTE));
});

test("the ids the list and the editor read exist in index.html", () => {
  const ids = [
    "team-schedules-body",
    "team-schedules-new",
    "team-schedules-count",
    "team-schedules-notice",
    "schedule-editor",
    "schedule-form",
    "schedule-days-wrap",
    "schedule-permission",
    "schedule-plans-link",
    "schedule-conversation-hint",
  ];
  for (const id of ids) assert.ok(html.includes(`id="${id}"`), `falta #${id}`);
});
