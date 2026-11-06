// «Plan propuesto»: the rules of the plan card, free of DOM so tooling/plan-model.test.mjs
// can run them. The host decides the status and the steps; the words here are the operator's.

import { friendlyError } from "./friendly-error.js";

export const NOTE_MAX = 300;

export const PERMISSION_TEXT =
  "Este dispositivo solo puede ver los planes. Concede el permiso APPROVE desde el PC para decidir.";

// The order of the words in a summary, so the same plan always reads the same way.
const TOUCH_ORDER = ["files", "mail", "network", "tools"];
const TOUCH_LABELS = {
  files: "Toca ficheros",
  network: "Usa internet",
  mail: "Toca correo",
  tools: "Usa herramientas",
  nothing: "Solo lee",
};
const TOUCH_WORDS = { files: "ficheros", mail: "correo", network: "internet", tools: "herramientas" };

export const STATUS_LABELS = {
  proposed: "Espera tu visto bueno",
  approved: "Aprobado",
  rejected: "Rechazado",
  changes_requested: "Pediste cambios",
};

const DECIDED_CODES = {
  plan_decided: "Este plan ya se decidió desde otro dispositivo.",
  no_such_plan: "Ese plan ya no existe. Se quitará de la lista.",
  plan_not_approved: "Este plan todavía no está aprobado.",
};

// Touches the steps name, once each, in TOUCH_ORDER. A plan that names none reads as «Solo lee».
function touchedKinds(steps) {
  const named = new Set(steps.flatMap((step) => step.touches));
  return TOUCH_ORDER.filter((kind) => named.has(kind));
}

export function touchLabels(steps) {
  const kinds = touchedKinds(steps);
  return kinds.length === 0 ? [TOUCH_LABELS.nothing] : kinds.map((kind) => TOUCH_LABELS[kind]);
}

// «ficheros», «ficheros e internet», «correo, ficheros y herramientas»: the Spanish «e» goes before an i.
function joinWords(words) {
  if (words.length <= 1) return words.join("");
  const last = words[words.length - 1];
  const connector = /^i/i.test(last) ? "e" : "y";
  return `${words.slice(0, -1).join(", ")} ${connector} ${last}`;
}

// «4 pasos · toca ficheros e internet», «1 paso · solo lee».
export function summaryLine(steps) {
  const count = steps.length === 1 ? "1 paso" : `${steps.length} pasos`;
  const kinds = touchedKinds(steps);
  const touch = kinds.length === 0 ? "solo lee" : `toca ${joinWords(kinds.map((kind) => TOUCH_WORDS[kind]))}`;
  return `${count} · ${touch}`;
}

// The proposals first, in the order the host sent them; then the decided ones, newest decision first.
export function sortPlans(plans) {
  const rank = (plan) => (plan.status === "proposed" ? 0 : 1);
  const decidedAt = (plan) => Date.parse(plan.updated_at) || 0;
  return [...plans].sort((a, b) => rank(a) - rank(b) || (rank(a) === 1 ? decidedAt(b) - decidedAt(a) : 0));
}

export function needsExtraConfirm(plan) {
  return plan.steps.some((step) => step.touches.includes("mail") || step.touches.includes("network"));
}

export function confirmApproveText() {
  return "Este plan toca correo o internet. Revisa los pasos antes de aprobarlo.";
}

// «Escribe qué cambiarías del plan.» or the limit; `null` when the note can be sent.
export function noteError(note) {
  const text = note.trim();
  if (!text) return "Escribe qué cambiarías del plan.";
  if ([...text].length > NOTE_MAX) return `La nota admite ${NOTE_MAX} caracteres como máximo.`;
  return null;
}

export function noteCount(note) {
  return `${[...note].length}/${NOTE_MAX}`;
}

// A refused decision keeps the host's rule in words; anything else goes through friendlyError.
export function planFailureText(error) {
  if (error && error.kind === "planRefused" && Object.hasOwn(DECIDED_CODES, error.code)) {
    return DECIDED_CODES[error.code];
  }
  return friendlyError(error);
}

// What the card paints for one plan. The host's own summary says who proposes it.
export function planView(plan) {
  const decided = plan.status !== "proposed";
  return {
    id: plan.id,
    lede: plan.summary,
    summary: summaryLine(plan.steps),
    steps: plan.steps.map((step) => ({ title: step.title, labels: touchLabels([step]) })),
    status: plan.status,
    statusLabel: STATUS_LABELS[plan.status],
    decided,
    waiting: !decided,
    note: plan.status === "changes_requested" ? plan.note ?? null : null,
  };
}

// What a card paints. A card is redrawn only when this changes, so the same answer leaves the screen alone.
export function planSignature(view) {
  return JSON.stringify([view.lede, view.summary, view.steps, view.status, view.note]);
}
