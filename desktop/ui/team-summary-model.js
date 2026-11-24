// «Tu equipo» in Equipo: the rules of the status band, free of DOM so
// tooling/team-summary-model.test.mjs can run them. The host decides each status and
// the order of the cards; the screen words them, counts who waits for the operator
// and says why a read failed.

import { roleLabel } from "./catalog-labels.js";
import { elapsedWords } from "./format.js";
import { friendlyError } from "./friendly-error.js";
import { readStateText } from "./hires-model.js";
import { t } from "./i18n.js";

export const STATUS_VIEWS = {
  waiting_for_you: { label: t("team.status.waiting"), tone: "waiting" },
  blocked: { label: t("team.status.blocked"), tone: "blocked" },
  working: { label: t("team.status.working"), tone: "working" },
  done: { label: t("team.status.done"), tone: "done" },
  idle: { label: t("team.status.idle"), tone: "idle" },
};

// A status the host does not name reads as idle. The screen never guesses one from the headline.
const IDLE = STATUS_VIEWS.idle;

// The host's fixed headlines (crates/chat/src/model_team_status.rs), matched word for word. The first
// three reuse the status labels; a headline the host does not name keeps the text it sent.
const FIXED_HEADLINES = {
  "Sin actividad": "team.status.idle",
  "Necesita ayuda": "team.status.blocked",
  Terminó: "team.status.done",
};
const APPROVAL_ACTIONS = {
  "ejecutar un comando": "approval.phrase_run",
  "usar internet": "team.action_network",
  "cambiar ficheros": "team.action_edit",
  "usar una herramienta": "approval.phrase_tool",
};

export function headlineText(headline = "") {
  if (Object.hasOwn(FIXED_HEADLINES, headline)) return t(FIXED_HEADLINES[headline]);
  const plan = /^Espera tu visto bueno para un plan de (\d+) pasos$/.exec(headline);
  if (plan) return t("team.headline.plan", { n: plan[1] });
  const approval = /^Espera tu visto bueno para (.+)$/.exec(headline);
  if (approval && Object.hasOwn(APPROVAL_ACTIONS, approval[1])) {
    return t("team.headline.approval", { action: t(APPROVAL_ACTIONS[approval[1]]) });
  }
  const working = /^Trabajando desde hace (\d+) min$/.exec(headline);
  if (working) return t("team.headline.working", { n: working[1] });
  return headline;
}

// The office in the language on screen: the hire's role ID is translated. A hire or role that is
// not known keeps the title the host sent.
function roleName(member, team) {
  const hire = team.hires?.find((item) => item.id === member.agent_id);
  const role = team.catalog?.roles?.find((item) => item.id === hire?.role);
  return (role && roleLabel(role)) || member.role_title;
}

// `team` is the Equipo snapshot ({ hires, catalog }); without it the host's titles are kept.
export function memberView(member, now, team = {}) {
  const known = Object.hasOwn(STATUS_VIEWS, member.status);
  const status = known ? STATUS_VIEWS[member.status] : IDLE;
  const since = Date.parse(member.since);
  return {
    id: member.agent_id,
    name: member.name,
    role: roleName(member, team),
    label: status.label,
    tone: status.tone,
    waiting: known && member.status === "waiting_for_you",
    headline: headlineText(member.headline),
    age: Number.isNaN(since) ? "" : t("team.since", { when: elapsedWords(since, now) }),
  };
}

// The cards in the order the host sent them.
export function summaryView(members, now, team = {}) {
  return members.map((member) => memberView(member, now, team));
}

// «1 espera tu visto bueno», «2 esperan tu visto bueno»: the counter the screen reads out.
export function waitingText(count) {
  if (count === 0) return t("team.waiting_none");
  return count === 1 ? t("team.waiting_one") : t("team.waiting_other", { n: count });
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
