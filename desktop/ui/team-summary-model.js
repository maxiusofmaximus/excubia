// «Tu equipo» in Equipo: the rules of the status band, free of DOM so
// tooling/team-summary-model.test.mjs can run them. The host decides each status and
// the order of the cards; the screen words them, counts who waits for the operator
// and says why a read failed.

import { elapsedWords } from "./format.js";
import { friendlyError } from "./friendly-error.js";
import { readStateText } from "./hires-model.js";

export const STATUS_VIEWS = {
  waiting_for_you: { label: "Espera tu visto bueno", tone: "waiting" },
  blocked: { label: "Necesita ayuda", tone: "blocked" },
  working: { label: "Trabajando", tone: "working" },
  done: { label: "Terminó", tone: "done" },
  idle: { label: "Sin actividad", tone: "idle" },
};

// A status the host does not name reads as idle. The screen never guesses one from the headline.
const IDLE = STATUS_VIEWS.idle;

export function memberView(member, now) {
  const known = Object.hasOwn(STATUS_VIEWS, member.status);
  const status = known ? STATUS_VIEWS[member.status] : IDLE;
  const since = Date.parse(member.since);
  return {
    id: member.agent_id,
    name: member.name,
    role: member.role_title,
    label: status.label,
    tone: status.tone,
    waiting: known && member.status === "waiting_for_you",
    headline: member.headline,
    age: Number.isNaN(since) ? "" : `desde hace ${elapsedWords(since, now)}`,
  };
}

// The cards in the order the host sent them.
export function summaryView(members, now) {
  return members.map((member) => memberView(member, now));
}

// «1 espera tu visto bueno», «2 esperan tu visto bueno»: the counter the screen reads out.
export function waitingText(count) {
  if (count === 0) return "Nadie espera tu visto bueno.";
  return count === 1 ? "1 espera tu visto bueno" : `${count} esperan tu visto bueno`;
}

// The headline is left out when it is empty or only repeats the status label.
const plain = (text = "") => text.trim().replace(/\.+$/, "").trim().toLocaleLowerCase("es");

export function showsHeadline(view) {
  const headline = plain(view.headline);
  return headline !== "" && headline !== plain(view.label);
}

// What a card paints. A card is redrawn only when this text changes, so a refresh with
// the same answer leaves the screen alone.
export function cardSignature(view) {
  return JSON.stringify([view.name, view.role, view.label, view.tone, view.headline, view.age]);
}

// A read the host refused: no permission yet is a wait; anything else can be read again.
export function summaryFailure(error) {
  const waiting = readStateText(error);
  if (waiting) return { text: waiting, retry: false };
  return { text: friendlyError(error), retry: true };
}
