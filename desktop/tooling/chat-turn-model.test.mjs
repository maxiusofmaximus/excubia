// Run with: node --test tooling/chat-turn-model.test.mjs
// The chat's turn rules: a turn never moves backwards in either order, the line
// under the operator's message, when Alfred counts as working, the «ir al final»
// counter and the words and gate of a stop.
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  advanceTurn,
  composerMode,
  jumpLabel,
  noteTurn,
  stopFailureText,
  turnInFlight,
  turnOf,
  turnStatusLine,
  unseenCount,
} from "../ui/chat-turn-model.js";

const own = (seq, id, turn_state) => ({
  kind: "message",
  seq,
  message: { id, seq, author_kind: "user", body_md: "hola", turn_state },
});
const agent = (seq, id) => ({
  kind: "message",
  seq,
  message: { id, seq, author_kind: "agent", body_md: "respuesta" },
});

test("a turn never moves backwards, whichever arrives first", () => {
  assert.equal(advanceTurn("running", "queued"), "running", "late 202 after running");
  assert.equal(advanceTurn("queued", "running"), "running", "running after queued");
  assert.equal(advanceTurn("completed", "running"), "completed", "stale timeline after the end");
  assert.equal(advanceTurn("running", "completed"), "completed", "end after running");
  assert.equal(advanceTurn("completed", "failed"), "completed", "first terminal state stays");
  assert.equal(advanceTurn(null, "queued"), "queued");
  assert.equal(advanceTurn("queued", undefined), "queued", "a missing state changes nothing");
});

test("a frame that arrives before its message is kept and applied on insert", () => {
  const turns = {};
  noteTurn(turns, "msg_1", "running");
  // The message arrives later with an older state from the feed: running wins.
  assert.equal(turnOf({ id: "msg_1", turn_state: "queued" }, turns), "running");
  // A closing frame stored early wins over a message still shown as running.
  noteTurn(turns, "msg_2", "completed");
  assert.equal(turnOf({ id: "msg_2", turn_state: "running" }, turns), "completed");
  // A stale frame after the end cannot reopen it.
  noteTurn(turns, "msg_2", "running");
  assert.equal(turnOf({ id: "msg_2", turn_state: "queued" }, turns), "completed");
});

test("the line under the last message shows each state, the others only failures", () => {
  assert.deepEqual(turnStatusLine("queued", "Alfred", true), { text: "Enviado", failed: false });
  assert.deepEqual(turnStatusLine("running", "Alfred", true), {
    text: "Alfred lo está atendiendo",
    failed: false,
  });
  assert.deepEqual(turnStatusLine("running", "", true), { text: "Atendiendo", failed: false });
  assert.deepEqual(turnStatusLine("completed", "Alfred", true), { text: "Respondido", failed: false });
  assert.deepEqual(turnStatusLine("interrupted", "Alfred", true), { text: "Detenido", failed: false });
  assert.deepEqual(turnStatusLine("failed", "Alfred", true), { text: "Falló", failed: true });

  assert.equal(turnStatusLine("completed", "Alfred", false), null, "older bubbles stay quiet");
  assert.equal(turnStatusLine("interrupted", "Alfred", false), null);
  assert.deepEqual(turnStatusLine("failed", "Alfred", false), { text: "Falló", failed: true });
  assert.equal(turnStatusLine(null, "Alfred", true), null, "no state, no line");
  assert.equal(turnStatusLine(undefined, "Alfred", true), null, "an older host has no line");
});

test("working follows the operator's last turn, not the socket", () => {
  assert.equal(turnInFlight([own(1, "msg_1", "queued")], {}, 0), true);
  assert.equal(turnInFlight([own(1, "msg_1", "running")], {}, 0), true);
  assert.equal(turnInFlight([own(1, "msg_1", "completed")], {}, 1), false, "the state wins over active_runs");
  // The turn closed by an agent message counts as finished even if the state is stale.
  assert.equal(turnInFlight([own(1, "msg_1", "running"), agent(2, "msg_2")], {}, 1), false);
  // The state of a frame counts when the message itself has none.
  assert.equal(turnInFlight([own(1, "msg_1", undefined)], { msg_1: "running" }, 0), true);
  // An older host: no states, active_runs decides.
  assert.equal(turnInFlight([own(1, "msg_1", undefined)], {}, 2), true);
  assert.equal(turnInFlight([own(1, "msg_1", undefined)], {}, 0), false);
  assert.equal(turnInFlight([], {}, 1), true, "no own message: active_runs alone");
  assert.equal(turnInFlight([], {}, 0), false);
});

test("the «ir al final» counter counts agent messages after the reader left, with plural", () => {
  const entries = [own(1, "msg_1", "completed"), agent(2, "msg_2"), agent(3, "msg_3"), own(4, "msg_4")];
  assert.equal(unseenCount(entries, 1), 2);
  assert.equal(unseenCount(entries, 3), 0);
  assert.equal(unseenCount(entries, null), 0, "nothing counted while at the bottom");
  assert.equal(jumpLabel(0), "Ir al final");
  assert.equal(jumpLabel(1), "Ir al final, 1 mensaje nuevo");
  assert.equal(jumpLabel(2), "Ir al final, 2 mensajes nuevos");
});

test("the stop gate: one request while flying, off without permission, off offline", () => {
  assert.deepEqual(composerMode({ inFlight: false, offline: false, sent: false, denied: null }), {
    stop: false,
    disabled: false,
    hint: null,
  });
  assert.deepEqual(composerMode({ inFlight: true, offline: false, sent: false, denied: null }), {
    stop: true,
    disabled: false,
    hint: null,
  });
  // After one tap the button waits for the state to arrive by SSE.
  assert.equal(composerMode({ inFlight: true, offline: false, sent: true, denied: null }).disabled, true);
  const denied = "No tienes permiso para detener esto desde este dispositivo.";
  assert.deepEqual(composerMode({ inFlight: true, offline: false, sent: false, denied }), {
    stop: true,
    disabled: true,
    hint: denied,
  });
  assert.equal(composerMode({ inFlight: true, offline: true, sent: false, denied: null }).stop, false);
});

test("a failed stop says why in Spanish", () => {
  assert.equal(
    stopFailureText({ kind: "forbidden", message: "x" }),
    "No tienes permiso para detener esto desde este dispositivo.",
  );
  assert.equal(stopFailureText({ kind: "notFound" }), "Este equipo no admite detener todavía.");
  assert.equal(stopFailureText({ kind: "gateway", status: 405 }), "Este equipo no admite detener todavía.");
  assert.equal(stopFailureText({ kind: "gateway", status: 404 }), "Este equipo no admite detener todavía.");
  assert.equal(stopFailureText({ kind: "offline" }), "No se pudo detener. Inténtalo de nuevo.");
  assert.equal(stopFailureText({ kind: "gateway", status: 500 }), "No se pudo detener. Inténtalo de nuevo.");
});
