// «Planes propuestos» in Equipo: the plans the team proposes, under «Tu equipo». It is read
// with the team's own 5 s poll, and only while Equipo is on screen. A decision answers with the
// host's plan and repaints that card at once; a card is redrawn only when its text changes.

import { $, invoke } from "./core.js";
import { element, loadingBlock } from "./chatdom.js";
import { emptyState, errorState } from "./empty-state.js";
import { friendlyError } from "./friendly-error.js";
import { readStateText } from "./hires-model.js";
import { planCard } from "./plan-card.js";
import { PERMISSION_TEXT, planSignature, planView, sortPlans } from "./plan-model.js";
import { showView } from "./nav.js";
import { waitingText } from "./team-summary-model.js";

const COMMANDS = { approve: "plan_approve", reject: "plan_reject", request_changes: "plan_request_changes" };
const EMPTY_TEXT = "Cuando un agente proponga un plan, lo verás aquí.";

// `plans` is null until the first answer. `denied` is set when the host refused a decision.
const state = { plans: null, denied: false, failure: "", inFlight: false, key: "", cards: new Map() };

function showBody(key, nodes) {
  if (state.key === key) return;
  state.key = key;
  state.cards.clear();
  $("team-plans-body").replaceChildren(...nodes);
}

function paintNotice() {
  const text = state.failure || (state.denied ? PERMISSION_TEXT : "");
  const notice = $("team-plans-notice");
  if (notice.textContent !== text) notice.textContent = text;
  notice.hidden = !text;
}

function paintCount(plans) {
  const proposed = plans.filter((plan) => plan.status === "proposed").length;
  const count = $("team-plans-count");
  const text = plans.length === 0 ? "" : waitingText(proposed);
  if (count.textContent !== text) count.textContent = text;
  count.classList.toggle("has-waiting", proposed > 0);
}

// A card is rebuilt only when its view or the permission changes; the rest keep their place and focus.
function cardFor(plan) {
  const view = planView(plan);
  const signature = `${planSignature(view)}|${state.denied}`;
  const known = state.cards.get(plan.id);
  if (known && known.signature === signature) return known.node;
  const node = planCard(plan, view, { readOnly: state.denied, decide: (action, args) => decide(plan, action, args) });
  state.cards.set(plan.id, { signature, node });
  return node;
}

function paintCards(plans) {
  if (state.key !== "cards") {
    const list = element("ul", "plan-list");
    list.setAttribute("role", "list");
    showBody("cards", [list]);
  }
  const list = $("team-plans-body").querySelector(".plan-list");
  const nodes = plans.map(cardFor);
  for (const child of [...list.children]) {
    if (!nodes.includes(child)) child.remove();
  }
  const live = new Set(plans.map((plan) => plan.id));
  for (const id of [...state.cards.keys()]) {
    if (!live.has(id)) state.cards.delete(id);
  }
  let previous = null;
  for (const node of nodes) {
    const expected = previous ? previous.nextSibling : list.firstChild;
    if (node !== expected) list.insertBefore(node, expected);
    previous = node;
  }
}

function paint() {
  if (!state.plans) return;
  paintNotice();
  const plans = sortPlans(state.plans);
  if (plans.length === 0) {
    showBody("empty", [emptyState({ icon: "plan", text: EMPTY_TEXT })]);
  } else {
    paintCards(plans);
  }
  paintCount(state.plans);
}

// A read the host refused: the wait is said, anything else can be read again. A list already on
// screen stays, with the reason in the notice.
function failed(error) {
  const waiting = readStateText(error);
  if (state.plans) {
    state.failure = waiting ?? friendlyError(error);
    paintNotice();
    return;
  }
  if (waiting) showBody(`wait:${waiting}`, [element("p", "hint", waiting)]);
  else showBody("error", [errorState(friendlyError(error), refreshPlans)]);
}

export async function refreshPlans() {
  if (state.inFlight || !invoke) return;
  state.inFlight = true;
  try {
    state.plans = await invoke("plans");
    state.failure = "";
    paint();
  } catch (error) {
    failed(error);
  } finally {
    state.inFlight = false;
  }
}

// Answers one decision. A refusal that says the device lacks APPROVE locks the buttons; every
// other refusal is thrown back to the card, which words it in its own slot.
async function decide(plan, action, args) {
  try {
    const updated = await invoke(COMMANDS[action], args);
    state.plans = state.plans.map((item) => (item.id === updated.id ? updated : item));
    paint();
    refreshPlans();
    return updated;
  } catch (error) {
    if (error && error.kind === "forbidden") {
      state.denied = true;
      paint();
    }
    throw error;
  }
}

// «Ver pendientes» on a team card: the proposal waits here; an approval waits in the chat.
export function openPending() {
  const proposed = (state.plans ?? []).some((plan) => plan.status === "proposed");
  if (!proposed) {
    showView("chat");
    return;
  }
  $("team-plans").scrollIntoView({ block: "start", behavior: "smooth" });
  $("team-plans-title").focus();
}

export function installPlans() {
  if (!$("team-plans-body")) return;
  $("team-plans-body").replaceChildren(loadingBlock("Cargando planes"));
  state.key = "loading";
  // Coming back to Equipo is the operator's chance to have been granted APPROVE: the lock lifts.
  new MutationObserver(() => {
    if (!$("view-agents").classList.contains("hidden") && state.denied) {
      state.denied = false;
      paint();
    }
  }).observe($("view-agents"), { attributes: true, attributeFilter: ["class"] });
}
