// «Tu equipo» band at the top of Equipo: one card per hired professional, with the status
// the host reports. It asks the host every 5 s, and only while Equipo is on screen and the
// window is visible. Cards are updated by difference: a refresh that brings the same answer
// touches nothing on screen.

import { $, invoke } from "./core.js";
import { avatarElement } from "./avatar.js";
import { element, loadingBlock } from "./chatdom.js";
import { emptyState, errorState } from "./empty-state.js";
import { teamSnapshot } from "./hires.js";
import { openPending, refreshPlans } from "./plans.js";
import { refreshSchedules } from "./schedules.js";
import { cardSignature, showsHeadline, summaryFailure, summaryView, waitingText } from "./team-summary-model.js";

const POLL_MS = 5000;

// `key` names what the body shows, so the same state is never drawn twice.
const state = { key: "", cards: new Map(), inFlight: false, timer: null };

function showBody(key, nodes) {
  if (state.key === key) return;
  state.key = key;
  state.cards.clear();
  $("team-summary-body").replaceChildren(...nodes);
}

function reviewButton(name) {
  const button = element("button", "btn secondary btn-small ts-review", "Ver pendientes");
  button.type = "button";
  button.setAttribute("aria-label", `Ver pendientes de ${name}`);
  button.addEventListener("click", openPending);
  return button;
}

function fillCard(card, view) {
  card.className = `ts-card tone-${view.tone}${view.waiting ? " is-waiting" : ""}`;
  const hire = teamSnapshot().hires.find((item) => item.id === view.id);
  const face = avatarElement({ name: view.name, characterId: hire?.avatar, px: 40, faceFromName: false });

  const name = element("p", "ts-name", view.name);
  if (view.role) name.append(element("span", "ts-role", view.role));
  const status = element("p", "ts-status");
  const dot = element("span", "ts-dot");
  dot.setAttribute("aria-hidden", "true");
  status.append(dot, element("span", "ts-label", view.label));
  if (view.age) status.append(element("span", "ts-age", `· ${view.age}`));
  const text = element("div", "ts-text");
  text.append(name, status);
  if (showsHeadline(view)) text.append(element("p", "ts-headline", view.headline));

  const nodes = [face, text];
  if (view.waiting) nodes.push(reviewButton(view.name));
  card.replaceChildren(...nodes);
}

function newCard(id) {
  const card = element("li", "ts-card");
  card.dataset.agent = id;
  return card;
}

// The list is kept while the cards stay, so each card changes in place.
function paintCards(views) {
  if (state.key !== "cards") {
    const list = element("ul", "ts-list");
    list.setAttribute("role", "list");
    const notice = element("p", "hint ts-notice");
    notice.setAttribute("role", "status");
    showBody("cards", [list, notice]);
  }
  const body = $("team-summary-body");
  const list = body.querySelector(".ts-list");
  const seen = new Set();
  let previous = null;
  for (const view of views) {
    seen.add(view.id);
    let card = state.cards.get(view.id);
    if (!card) {
      card = newCard(view.id);
      state.cards.set(view.id, card);
    }
    const signature = cardSignature(view);
    if (card.dataset.signature !== signature) {
      fillCard(card, view);
      card.dataset.signature = signature;
    }
    const expected = previous ? previous.nextSibling : list.firstChild;
    if (card !== expected) list.insertBefore(card, expected);
    previous = card;
  }
  for (const [id, card] of state.cards) {
    if (!seen.has(id)) {
      card.remove();
      state.cards.delete(id);
    }
  }
}

// The line under the cards that says why the last refresh did not arrive. Empty when it did.
function setNotice(text) {
  const notice = $("team-summary-body").querySelector(".ts-notice");
  if (!notice) return;
  if (notice.textContent !== text) notice.textContent = text;
  notice.hidden = !text;
}

function paintCount(text) {
  const count = $("team-summary-count");
  if (count.textContent !== text) count.textContent = text;
}

function applyMembers(members) {
  const views = summaryView(members, Date.now());
  if (views.length === 0) {
    const canHire = teamSnapshot().canHire;
    showBody(`empty:${canHire}`, [
      emptyState(
        {
          icon: "team",
          text: "Todavía no tienes a nadie contratado. Contrata a quien necesites y verás aquí en qué anda cada uno.",
          action: canHire ? "Contratar" : null,
        },
        () => $("btn-hire").click(),
      ),
    ]);
    paintCount("");
    $("team-summary-count").classList.remove("has-waiting");
    return;
  }
  paintCards(views);
  setNotice("");
  const waiting = views.filter((view) => view.waiting).length;
  paintCount(waitingText(waiting));
  $("team-summary-count").classList.toggle("has-waiting", waiting > 0);
}

// Cards already on screen stay, with the reason under them; with nothing shown yet, the reason takes the body.
function applyFailure(failure) {
  if (state.key === "cards") {
    setNotice(failure.text);
    return;
  }
  if (failure.retry) showBody(`error:${failure.text}`, [errorState(failure.text, () => poll())]);
  else showBody(`wait:${failure.text}`, [element("p", "hint", failure.text)]);
  paintCount("");
}

function visibleNow() {
  return !$("view-agents").classList.contains("hidden") && !document.hidden;
}

function scheduleNext() {
  clearTimeout(state.timer);
  state.timer = visibleNow() ? setTimeout(poll, POLL_MS) : null;
}

// One request at a time: the next one is planned only when this one has answered.
export async function poll() {
  if (state.inFlight || !invoke) return;
  state.inFlight = true;
  clearTimeout(state.timer);
  refreshPlans();
  refreshSchedules();
  try {
    const { members } = await invoke("team_summary");
    applyMembers(members);
  } catch (error) {
    applyFailure(summaryFailure(error));
  } finally {
    state.inFlight = false;
    scheduleNext();
  }
}

// Polls while the band is on screen; stops as soon as Equipo is hidden or the window is.
function sync() {
  if (!visibleNow()) {
    clearTimeout(state.timer);
    state.timer = null;
    return;
  }
  if (!state.inFlight && !state.timer) poll();
}

// The team changed (a hire, a dismissal): the band reads it now, not at the next tick.
export function refreshTeamSummary() {
  if (visibleNow()) poll();
}

export function installTeamSummary() {
  if (!$("team-summary-body")) return;
  $("team-summary-body").replaceChildren(loadingBlock("Cargando tu equipo"));
  state.key = "loading";
  new MutationObserver(sync).observe($("view-agents"), { attributes: true, attributeFilter: ["class"] });
  document.addEventListener("visibilitychange", sync);
}
