// Run with: node --test tooling/chat-card-model.test.mjs
// A failed card paints the words of its reason when it has them; an answer names the model that
// wrote it, small and without the provider prefix.

import { test } from "node:test";
import assert from "node:assert/strict";

import { answerModel, failedTitle } from "../ui/chat-card-model.js";

const GENERIC = "Alfred terminó sin responder";
const SPENT = "Los modelos gratuitos de OpenCode han llegado a su límite de uso. Vuelve a intentarlo más tarde.";

test("a failed card with the words of its reason paints those words instead of the generic title", () => {
  const card = { state: "failed", body_md: "", failure_text: SPENT };
  assert.equal(failedTitle(card, GENERIC), SPENT);
});

test("a failed card without words keeps the generic title", () => {
  assert.equal(failedTitle({ state: "failed", body_md: "" }, GENERIC), GENERIC);
  assert.equal(failedTitle({ state: "failed", body_md: "", failure_text: null }, GENERIC), GENERIC);
});

test("an answer names the model that wrote it, without the provider prefix", () => {
  assert.equal(answerModel({ model: "opencode/deepseek-v4.1-flash-free" }), "deepseek-v4.1-flash-free");
  assert.equal(answerModel({ model: "opencode/nemotron-3.5-lightning-free" }), "nemotron-3.5-lightning-free");
});

test("an answer without a model names none", () => {
  assert.equal(answerModel({ body_md: "Listo" }), "");
  assert.equal(answerModel({ model: null }), "");
});
