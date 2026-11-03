// The QR without an address lives on screen for a few minutes and is single use. This is the rule
// for when it is shown and when it is taken down, with no DOM so tooling/pair-qr-model.test.mjs can run it.

import { countdownText } from "./format.js";

export const EXPIRED_MESSAGE = "Caducó, genera otro.";
export const LEFT_WINDOW_MESSAGE = "Oculté el QR al salir de la ventana; genera otro.";

/// phase: idle | shown | expired | hidden. `remaining` is whole seconds left while shown.
export const IDLE = { phase: "idle", expiresAt: 0, remaining: 0 };

export function remainingSeconds(expiresAt, now) {
  return Math.max(0, Math.ceil((expiresAt * 1000 - now) / 1000));
}

export function showQr(expiresAt, now) {
  return tickQr({ phase: "shown", expiresAt, remaining: 0 }, now);
}

/// One second passes. Only a shown QR changes; an expired or hidden one is never brought back.
export function tickQr(state, now) {
  if (state.phase !== "shown") return state;
  const remaining = remainingSeconds(state.expiresAt, now);
  return remaining > 0 ? { ...state, remaining } : { ...state, phase: "expired", remaining: 0 };
}

/// The window lost focus or went out of sight: a visible QR comes down, anything else stays as it was.
export function leaveWindow(state) {
  return state.phase === "shown" ? { ...state, phase: "hidden", remaining: 0 } : state;
}

export function qrVisible(state) {
  return state.phase === "shown";
}

export function qrMessage(state) {
  if (state.phase === "shown") return countdownText(state.remaining);
  if (state.phase === "expired") return EXPIRED_MESSAGE;
  if (state.phase === "hidden") return LEFT_WINDOW_MESSAGE;
  return "";
}
