// Run with: node --test tooling/mail-flow.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";

import { APPROVAL_DENIED_TEXT } from "../ui/mail-model.js";
import { createSendFlows } from "../ui/mail-flow.js";

// A flow driven by hand: the host answers from a script, and timers only fire
// when the test asks, so every step is visible.
function harness(script) {
  const calls = [];
  const timers = new Set();
  let clock = 1_000_000;
  const invoke = async (command, args) => {
    calls.push({ command, args });
    const next = script[command] && script[command].shift();
    if (next === undefined) throw new Error(`sin respuesta para ${command}`);
    if (next instanceof Error) throw next;
    return next;
  };
  const flows = createSendFlows({
    invoke,
    now: () => clock,
    schedule: (fn, ms) => {
      const handle = { fn, ms };
      timers.add(handle);
      return handle;
    },
    unschedule: (handle) => timers.delete(handle),
  });
  // Lets the host answers that are in flight land.
  const settle = () => new Promise((resolve) => setImmediate(resolve));
  // Settles, then fires the one timer that is due, then settles again.
  async function tick() {
    await settle();
    const [next] = timers;
    if (next) {
      timers.delete(next);
      next.fn();
    }
    await settle();
  }
  return {
    flows,
    calls,
    settle,
    tick,
    pending: () => timers.size,
    advance: (ms) => {
      clock += ms;
    },
  };
}

const AWAITING = { state: "awaiting_approval", approval_id: "apr_1", expires_in: 120 };
const WINDOW = { state: "pending", send_id: "snd_1", window_seconds: 10 };

test("a send goes from the request to sent, and leaves no timer behind", async () => {
  const h = harness({
    mail_draft_send: [AWAITING, WINDOW],
    chat_approval_find: [{ id: "apr_1", status: "pending" }, { id: "apr_1", status: "approved" }],
    mail_send_state: [{ state: "pending" }, { state: "sent" }],
  });

  h.flows.start("drf_1", "trabajo");
  await h.tick();
  assert.equal(h.flows.current("drf_1").phase, "awaiting");
  await h.tick();
  assert.equal(h.flows.current("drf_1").phase, "window");
  await h.tick();
  await h.tick();

  assert.equal(h.flows.current("drf_1").phase, "sent");
  assert.equal(h.flows.current("drf_1").open, false);
  assert.equal(h.pending(), 0);
  assert.equal(h.flows.activeTimers(), 0);
  assert.deepEqual(
    h.calls.map((call) => call.command),
    ["mail_draft_send", "chat_approval_find", "chat_approval_find", "mail_draft_send", "mail_send_state", "mail_send_state"],
  );
  assert.equal(h.calls[3].args.approvalId, "apr_1");
});

test("a denied approval ends the flow with the denial and no second request", async () => {
  const h = harness({
    mail_draft_send: [AWAITING],
    chat_approval_find: [{ id: "apr_1", status: "denied" }],
  });

  h.flows.start("drf_1", "trabajo");
  await h.tick();
  await h.tick();

  const view = h.flows.current("drf_1");
  assert.equal(view.phase, "failed");
  assert.equal(view.text, APPROVAL_DENIED_TEXT);
  assert.equal(h.flows.activeTimers(), 0);
  assert.equal(h.calls.filter((call) => call.command === "mail_draft_send").length, 1);
});

test("an approval the host used up before the send ends as stale", async () => {
  const stale = Object.assign(new Error("x"), {
    code: "mail_approval_stale",
    message: "La aprobación ya se usó o el borrador cambió después de aprobarlo.",
  });
  const h = harness({
    mail_draft_send: [AWAITING, stale],
    chat_approval_find: [{ id: "apr_1", status: "approved" }],
  });

  h.flows.start("drf_1", "trabajo");
  await h.tick();
  await h.tick();

  const view = h.flows.current("drf_1");
  assert.equal(view.phase, "failed");
  assert.equal(view.code, "mail_approval_stale");
  assert.match(view.text, /ya se usó/);
  assert.equal(h.flows.activeTimers(), 0);
});

test("cancelling inside the window settles on cancelled and stops the polling", async () => {
  const h = harness({
    mail_draft_send: [WINDOW],
    mail_send_cancel: [{ send_id: "snd_1", state: "cancelled" }],
  });

  h.flows.start("drf_1", "trabajo");
  await h.settle();
  assert.equal(h.flows.current("drf_1").phase, "window");
  assert.equal(h.pending(), 1);
  await h.flows.cancel("drf_1");

  assert.equal(h.flows.current("drf_1").phase, "cancelled");
  assert.equal(h.pending(), 0);
  assert.deepEqual(h.calls.at(-1).args, { sendId: "snd_1" });
});

test("a cancel that arrives after the window closed shows it as sent", async () => {
  const h = harness({
    mail_draft_send: [WINDOW],
    mail_send_cancel: [{ send_id: "snd_1", state: "sent" }],
  });

  h.flows.start("drf_1", "trabajo");
  await h.settle();
  await h.flows.cancel("drf_1");

  assert.equal(h.flows.current("drf_1").phase, "sent");
  assert.equal(h.flows.activeTimers(), 0);
});

test("cancelling outside the window does nothing", async () => {
  const h = harness({
    mail_draft_send: [AWAITING],
    chat_approval_find: [{ id: "apr_1", status: "pending" }],
  });

  h.flows.start("drf_1", "trabajo");
  await h.settle();
  await h.flows.cancel("drf_1");

  assert.equal(h.flows.current("drf_1").phase, "awaiting");
  assert.equal(h.calls.some((call) => call.command === "mail_send_cancel"), false);
});

test("closing the screen does not stop the flow: it finishes while nobody listens", async () => {
  const h = harness({
    mail_draft_send: [WINDOW],
    mail_send_state: [{ state: "sent" }],
  });
  const seen = [];
  h.flows.start("drf_1", "trabajo");
  const stop = h.flows.subscribe("drf_1", (view) => seen.push(view.phase));
  await h.settle();
  stop();

  await h.tick();

  assert.equal(h.flows.current("drf_1").phase, "sent");
  assert.deepEqual(seen, ["window"]);
});

test("starting a draft that is still open reattaches instead of asking again", async () => {
  const h = harness({ mail_draft_send: [AWAITING] });

  const first = h.flows.start("drf_1", "trabajo");
  const second = h.flows.start("drf_1", "trabajo");

  assert.equal(first.phase, "asking");
  assert.equal(second.draftId, "drf_1");
  await h.settle();
  assert.equal(h.calls.filter((call) => call.command === "mail_draft_send").length, 1);
});

test("a host refusal keeps its code and its own Spanish text", async () => {
  const refusal = Object.assign(new Error("x"), {
    code: "mail_send_rate_limited",
    message: "Se alcanzó el límite de envíos por hora. Vuelve a intentarlo en 25 min.",
  });
  const h = harness({ mail_draft_send: [refusal] });

  h.flows.start("drf_1", "trabajo");
  await h.settle();

  const view = h.flows.current("drf_1");
  assert.equal(view.phase, "failed");
  assert.equal(view.code, "mail_send_rate_limited");
  assert.match(view.text, /25 min/);
  assert.equal(h.pending(), 0);
});

test("an unreachable host keeps the approval open and tries again", async () => {
  const h = harness({
    mail_draft_send: [AWAITING],
    chat_approval_find: [new Error("Sin conexión"), { id: "apr_1", status: "pending" }],
  });

  h.flows.start("drf_1", "trabajo");
  await h.tick();
  const offline = h.flows.current("drf_1");
  assert.equal(offline.phase, "awaiting");
  assert.equal(offline.note, "Sin conexión");
  assert.equal(h.flows.activeTimers(), 1);

  await h.tick();
  assert.equal(h.flows.current("drf_1").note, "");
});

test("the window countdown follows the clock the flow was given", async () => {
  const h = harness({ mail_draft_send: [WINDOW] });

  h.flows.start("drf_1", "trabajo");
  await h.settle();
  assert.match(h.flows.current("drf_1").text, /Se enviará en 10 s/);
  h.advance(4_000);
  assert.match(h.flows.current("drf_1").text, /Se enviará en 6 s/);
});

test("a disposed flow stops, and its late answer schedules nothing", async () => {
  const h = harness({
    mail_draft_send: [AWAITING],
    chat_approval_find: [{ id: "apr_1", status: "pending" }],
  });

  h.flows.start("drf_1", "trabajo");
  h.flows.dispose();
  await h.tick();

  assert.equal(h.pending(), 0);
  assert.equal(h.flows.current("drf_1"), null);
});

test("timers and listeners are back at zero once the flow has ended", async () => {
  const h = harness({
    mail_draft_send: [WINDOW],
    mail_send_state: [{ state: "cancelled" }],
  });
  h.flows.start("drf_1", "trabajo");
  const stop = h.flows.subscribe("drf_1", () => {});
  await h.tick();
  stop();

  assert.equal(h.flows.current("drf_1").open, false);
  assert.equal(h.flows.activeTimers(), 0);
  assert.equal(h.pending(), 0);
});
