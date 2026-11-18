import { test } from "node:test";
import assert from "node:assert/strict";
import { teamPanelOpen } from "../ui/team-panel-model.js";

test("the team panel starts folded; only the operator's own choice or a request waiting on him opens it", () => {
  assert.equal(teamPanelOpen(null, false), false);
  assert.equal(teamPanelOpen(null, true), true);
  assert.equal(teamPanelOpen("1", true), false);
  assert.equal(teamPanelOpen("0", false), true);
  assert.equal(teamPanelOpen("0", true), true);
});
