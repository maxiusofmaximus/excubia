// Run with: node --test tooling/approval-shown.test.mjs
// The TTL starts at the host's first showing: no countdown or "checking" state before it,
// and a showing is a strip of the card in a visible window, not the whole card.
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  countdownStarted,
  createShownReporter,
  isChecking,
  isShowing,
  seenState,
  showingApprovalIds,
  SHOWING_THRESHOLDS,
} from "../ui/approval-model.js";

const past = "2026-11-25T10:00:00.000Z";
const user = { id: "apr_1", status: "pending", level: "user", expires_at: past, shown_at: null };

test("a card nobody has shown starts no countdown, even with a provisional expiry in the past", () => {
  assert.equal(countdownStarted(user), false);
  assert.equal(isChecking(user, Date.parse("2026-11-25T12:00:00.000Z")), false);
});

test("the first showing starts the countdown; the host's deadline then governs", () => {
  const shown = { ...user, shown_at: "2026-11-25T11:00:00.000Z" };
  assert.equal(countdownStarted(shown), true);
  assert.equal(isChecking(shown, Date.parse("2026-11-25T12:00:00.000Z")), true);
  assert.equal(isChecking(shown, Date.parse("2026-11-25T09:00:00.000Z")), false);
});

test("only an operator-level pending card waits to be shown; policy, agent and terminal cards never do", () => {
  assert.equal(seenState(user), "wait");
  assert.equal(seenState({ ...user, level: "lead" }), "");
  assert.equal(seenState({ ...user, level: "cto" }), "");
  assert.equal(seenState({ ...user, shown_at: "2026-11-25T11:00:00.000Z" }), "shown");
  for (const status of ["approved", "denied", "expired", "consumed"]) {
    assert.equal(seenState({ ...user, status }), "", `${status} is never shown by the UI`);
  }
});

test("a host before this version sends no shown_at: the card reads as unshown, not as checking", () => {
  const legacy = { id: "apr_2", status: "pending", level: "user", expires_at: past };
  assert.equal(countdownStarted(legacy), false);
  assert.equal(seenState(legacy), "wait");
});

test("a compact strip in a visible window is a showing, even of a card taller than the viewport", () => {
  // A 1400 px card with a 700 px viewport: 700 px in view, far more than the 48 px strip.
  assert.equal(isShowing({ intersecting: true, visibleHeight: 700, cardHeight: 1400, visibility: "visible" }), true);
  // A short card needs all of it in view.
  assert.equal(isShowing({ intersecting: true, visibleHeight: 90, cardHeight: 90, visibility: "visible" }), true);
  assert.equal(isShowing({ intersecting: true, visibleHeight: 30, cardHeight: 90, visibility: "visible" }), false, "a sliver of a short card");
});

test("hidden window, scrolled away, or collapsed card is not a showing", () => {
  const seen = { intersecting: true, visibleHeight: 200, cardHeight: 200, visibility: "visible" };
  assert.equal(isShowing(seen), true);
  assert.equal(isShowing({ ...seen, visibility: "hidden" }), false, "hidden window");
  assert.equal(isShowing({ ...seen, intersecting: false, visibleHeight: 0 }), false, "scrolled away");
  assert.equal(isShowing({ ...seen, cardHeight: 0, visibleHeight: 0 }), false, "collapsed");
});

// A stand-in for IntersectionObserver: it delivers an entry when the card's visible ratio crosses
// one of the thresholds, or when it starts or stops intersecting, as the browser does. Each call
// is one scroll position, given as the visible height of a card of `cardHeight` px.
function scrolledObserver(thresholds, cardHeight, id = "apr_1") {
  let previous = null;
  return (visibleHeight) => {
    const ratio = visibleHeight / cardHeight;
    const isIntersecting = visibleHeight > 0;
    const was = previous;
    previous = { ratio, isIntersecting };
    const changed =
      was === null ||
      was.isIntersecting !== isIntersecting ||
      thresholds.some((step) => was.ratio >= step !== ratio >= step);
    if (!changed) return [];
    return [
      {
        target: { dataset: { approval: id } },
        isIntersecting,
        intersectionRect: { height: visibleHeight },
        boundingClientRect: { height: cardHeight },
      },
    ];
  };
}

test("a card that enters as a sliver and grows into its strip is reported at the strip, once", async () => {
  // A 1400 px card: the 48 px strip is 3.4% of it, so the first step past it is 49 px.
  const observe = scrolledObserver(SHOWING_THRESHOLDS, 1400);
  const reports = [];
  const reporter = createShownReporter({
    report: async (id) => {
      reports.push(id);
      return { expires_at: "2026-11-25T12:02:00.000Z" };
    },
    confirm: () => {},
    release: () => {},
  });
  const seenAt = {};
  for (const visible of [0, 1, 8, 20, 47, 49, 300, 1400]) {
    await reporter.handle(showingApprovalIds(observe(visible), "visible"));
    seenAt[visible] = reports.length;
  }
  assert.deepEqual(seenAt, { 0: 0, 1: 0, 8: 0, 20: 0, 47: 0, 49: 1, 300: 1, 1400: 1 });
  assert.deepEqual(reports, ["apr_1"], "no second report once confirmed");
});

test("the single-threshold observer this replaces never reports a card that grows after entering", async () => {
  // Regression guard: with threshold 0 the 1 px entry is the only callback; 47 px and 49 px give none.
  const observe = scrolledObserver([0], 1400);
  const reports = [];
  const reporter = createShownReporter({
    report: async (id) => (reports.push(id), { expires_at: "x" }),
    confirm: () => {},
    release: () => {},
  });
  for (const visible of [0, 1, 8, 20, 47, 49, 300]) {
    await reporter.handle(showingApprovalIds(observe(visible), "visible"));
  }
  assert.deepEqual(reports, [], "the old observer missed the showing");
});

test("a failed report keeps the card observed and is retried at its next exposure", async () => {
  const observe = scrolledObserver(SHOWING_THRESHOLDS, 1400);
  let attempts = 0;
  const confirmed = [];
  const released = [];
  const reporter = createShownReporter({
    report: async () => {
      attempts += 1;
      if (attempts === 1) throw new Error("network down");
      return { expires_at: "2026-11-25T12:02:00.000Z" };
    },
    confirm: (id) => confirmed.push(id),
    release: (id) => released.push(id),
  });
  // Shown, then scrolled out of the window, then shown again.
  await reporter.handle(showingApprovalIds(observe(60), "visible"));
  assert.equal(attempts, 1);
  assert.deepEqual(released, [], "a failed card is still observed");
  await reporter.handle(showingApprovalIds(observe(0), "visible"));
  assert.equal(attempts, 1, "leaving the window reports nothing");
  await reporter.handle(showingApprovalIds(observe(60), "visible"));
  assert.equal(attempts, 2, "the next exposure reports it again");
  assert.deepEqual(confirmed, ["apr_1"]);
  assert.deepEqual(released, ["apr_1"], "confirmed, so it is released");
});

test("a card is reported once at a time: overlapping batches do not repeat an in-flight request", async () => {
  let resolveFirst;
  let calls = 0;
  const reporter = createShownReporter({
    report: () => {
      calls += 1;
      return new Promise((resolve) => (resolveFirst = resolve));
    },
    confirm: () => {},
    release: () => {},
  });
  const first = reporter.handle(["apr_1", "apr_1"]);
  await reporter.handle(["apr_1"]);
  assert.equal(calls, 1);
  resolveFirst({ expires_at: "x" });
  await first;
});

test("only the approval ids of showing entries are reported; a sliver and a hidden window are not", () => {
  const card = (id, visibleHeight, cardHeight) => ({
    target: { dataset: { approval: id } },
    isIntersecting: visibleHeight > 0,
    intersectionRect: { height: visibleHeight },
    boundingClientRect: { height: cardHeight },
  });
  const entries = [card("a", 60, 1400), card("b", 1, 1400), card("c", 200, 200)];
  assert.deepEqual(showingApprovalIds(entries, "visible"), ["a", "c"]);
  assert.deepEqual(showingApprovalIds(entries, "hidden"), []);
});
