// The send of one draft, from the request to the final answer. It lives apart
// from the draft screen: closing the screen keeps the flow watching the approval
// and the window, and reopening the draft shows where it stands. The timers and
// the host calls are injected, so the tests can drive the flow without a clock.

import { errorMessage } from "./format.js";
import { approvalStep, isOpen, phaseText, sendStep } from "./mail-model.js";

export const APPROVAL_POLL_MS = 2000;
export const SEND_POLL_MS = 1000;

export function createSendFlows({
  invoke,
  now = () => Date.now(),
  schedule = (fn, ms) => setTimeout(fn, ms),
  unschedule = (handle) => clearTimeout(handle),
}) {
  const flows = new Map();

  function snapshot(flow) {
    const windowLeft = Math.max(0, Math.ceil((flow.windowEndsAt - now()) / 1000));
    return {
      draftId: flow.draftId,
      account: flow.account,
      phase: flow.phase,
      open: isOpen(flow.phase),
      code: flow.code,
      text: phaseText(flow.phase, { windowLeft, text: flow.text }),
      windowEndsAt: flow.windowEndsAt,
      note: flow.note,
    };
  }

  function publish(flow) {
    const view = snapshot(flow);
    for (const listener of flow.listeners) listener(view);
  }

  // One timer per flow at most: the next step replaces the last one.
  function later(flow, step, ms) {
    clearTimer(flow);
    if (flow.done) return;
    flow.timer = schedule(() => {
      flow.timer = null;
      step(flow);
    }, ms);
  }

  function clearTimer(flow) {
    if (flow.timer !== null) unschedule(flow.timer);
    flow.timer = null;
  }

  function finish(flow, phase, { text = "", code = null } = {}) {
    clearTimer(flow);
    flow.done = true;
    flow.phase = phase;
    flow.text = text;
    flow.code = code;
    flow.note = "";
    publish(flow);
  }

  function fail(flow, error) {
    const code = error && typeof error === "object" ? error.code || null : null;
    finish(flow, "failed", { text: errorMessage(error), code });
  }

  function enterWindow(flow, request) {
    flow.sendId = request.send_id;
    flow.windowEndsAt = now() + request.window_seconds * 1000;
    flow.phase = "window";
    publish(flow);
    later(flow, pollSend, SEND_POLL_MS);
  }

  async function askForApproval(flow) {
    try {
      const request = await invoke("mail_draft_send", {
        account: flow.account,
        draftId: flow.draftId,
        approvalId: null,
      });
      if (flow.done) return;
      if (request.state === "awaiting_approval") {
        flow.approvalId = request.approval_id;
        flow.phase = "awaiting";
        publish(flow);
        later(flow, pollApproval, APPROVAL_POLL_MS);
        return;
      }
      enterWindow(flow, request);
    } catch (error) {
      if (!flow.done) fail(flow, error);
    }
  }

  async function pollApproval(flow) {
    let approval;
    try {
      approval = await invoke("chat_approval_find", { approvalId: flow.approvalId });
    } catch (error) {
      // An unreachable host keeps the approval open; the next poll tries again.
      if (flow.done) return;
      flow.note = errorMessage(error);
      publish(flow);
      later(flow, pollApproval, APPROVAL_POLL_MS);
      return;
    }
    if (flow.done) return;
    flow.note = "";
    const step = approvalStep(approval);
    if (step.next === "awaiting") {
      later(flow, pollApproval, APPROVAL_POLL_MS);
    } else if (step.next === "send") {
      await sendApproved(flow);
    } else {
      finish(flow, "failed", { text: step.text });
    }
  }

  async function sendApproved(flow) {
    flow.phase = "send";
    publish(flow);
    try {
      const request = await invoke("mail_draft_send", {
        account: flow.account,
        draftId: flow.draftId,
        approvalId: flow.approvalId,
      });
      if (flow.done) return;
      enterWindow(flow, request);
    } catch (error) {
      if (!flow.done) fail(flow, error);
    }
  }

  async function pollSend(flow) {
    let state;
    try {
      state = await invoke("mail_send_state", { sendId: flow.sendId });
    } catch (error) {
      if (flow.done) return;
      flow.note = errorMessage(error);
      publish(flow);
      later(flow, pollSend, SEND_POLL_MS);
      return;
    }
    if (flow.done) return;
    settleSend(flow, state);
  }

  function settleSend(flow, state) {
    const step = sendStep(state);
    if (step.next === "window") {
      flow.note = "";
      // Republished each poll: the countdown in the view moves with the clock.
      publish(flow);
      later(flow, pollSend, SEND_POLL_MS);
    } else if (step.next === "failed") {
      finish(flow, "failed", { text: step.text });
    } else {
      finish(flow, step.next);
    }
  }

  function start(draftId, account) {
    const existing = flows.get(draftId);
    if (existing && !existing.done) return snapshot(existing);
    const flow = {
      draftId,
      account,
      phase: "asking",
      done: false,
      approvalId: null,
      sendId: null,
      windowEndsAt: 0,
      text: "",
      code: null,
      note: "",
      timer: null,
      listeners: existing ? existing.listeners : new Set(),
    };
    flows.set(draftId, flow);
    publish(flow);
    askForApproval(flow);
    return snapshot(flow);
  }

  // Cancelling is only possible inside the window. The host answers with the
  // state it settled on, which may be `sent` if the window had just closed.
  async function cancel(draftId) {
    const flow = flows.get(draftId);
    if (!flow || flow.phase !== "window") return;
    clearTimer(flow);
    try {
      const state = await invoke("mail_send_cancel", { sendId: flow.sendId });
      if (!flow.done) settleSend(flow, state);
    } catch (error) {
      if (flow.done) return;
      flow.note = errorMessage(error);
      publish(flow);
      later(flow, pollSend, SEND_POLL_MS);
    }
  }

  function current(draftId) {
    const flow = flows.get(draftId);
    return flow ? snapshot(flow) : null;
  }

  // A saved edit leaves the draft at rest: the flow that ended on it is dropped.
  function forget(draftId) {
    const flow = flows.get(draftId);
    if (flow && flow.done) flows.delete(draftId);
  }

  // The returned function stops the listening only. A flow ends by its own
  // answer, never because its screen closed.
  function subscribe(draftId, listener) {
    const flow = flows.get(draftId);
    if (!flow) return () => {};
    flow.listeners.add(listener);
    return () => {
      flow.listeners.delete(listener);
    };
  }

  function activeTimers() {
    let count = 0;
    for (const flow of flows.values()) if (flow.timer !== null) count += 1;
    return count;
  }

  function dispose() {
    for (const flow of flows.values()) {
      flow.done = true;
      clearTimer(flow);
      flow.listeners.clear();
    }
    flows.clear();
  }

  return { start, cancel, current, forget, subscribe, activeTimers, dispose };
}
