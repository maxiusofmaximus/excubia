// Run with: node --test tooling/pair-qr-model.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";

import { countdownText } from "../ui/format.js";
import {
  EXPIRED_MESSAGE,
  IDLE,
  LEFT_WINDOW_MESSAGE,
  leaveWindow,
  qrMessage,
  qrVisible,
  showQr,
  tickQr,
} from "../ui/pair-qr-model.js";

const EXPIRES = 1_800_000_000;
const at = (secondsLeft) => (EXPIRES - secondsLeft) * 1000;

test("the countdown is written as minutes and seconds", () => {
  assert.equal(countdownText(157), "Caduca en 2:37");
  assert.equal(countdownText(180), "Caduca en 3:00");
  assert.equal(countdownText(5), "Caduca en 0:05");
  assert.equal(countdownText(-3), "Caduca en 0:00");
});

test("a fresh QR is visible and counts down from 180 to 0", () => {
  let state = showQr(EXPIRES, at(180));
  assert.ok(qrVisible(state));
  assert.equal(qrMessage(state), "Caduca en 3:00");
  for (let left = 179; left >= 1; left--) {
    state = tickQr(state, at(left));
    assert.ok(qrVisible(state), `still shown with ${left} s`);
    assert.equal(state.remaining, left);
  }
  assert.equal(qrMessage(tickQr(state, at(157))), "Caduca en 2:37");
});

test("at 0 the QR is taken down and the message asks for another", () => {
  const state = tickQr(showQr(EXPIRES, at(180)), at(0));
  assert.equal(qrVisible(state), false);
  assert.equal(state.phase, "expired");
  assert.equal(qrMessage(state), EXPIRED_MESSAGE);
});

test("a late tick after expiry does not bring the QR back", () => {
  const expired = tickQr(showQr(EXPIRES, at(180)), at(0));
  assert.equal(tickQr(expired, at(100)), expired);
  assert.equal(qrVisible(tickQr(expired, at(100))), false);
});

test("leaving the window takes a visible QR down at once", () => {
  const state = leaveWindow(showQr(EXPIRES, at(120)));
  assert.equal(qrVisible(state), false);
  assert.equal(qrMessage(state), LEFT_WINDOW_MESSAGE);
  assert.equal(tickQr(state, at(100)), state);
});

test("leaving the window changes nothing when no QR is shown", () => {
  const expired = tickQr(showQr(EXPIRES, at(180)), at(0));
  assert.equal(leaveWindow(expired), expired);
  assert.equal(leaveWindow(IDLE), IDLE);
  assert.equal(qrMessage(IDLE), "");
});

test("a code already past its expiry is never shown", () => {
  assert.equal(qrVisible(showQr(EXPIRES, at(-5))), false);
});
