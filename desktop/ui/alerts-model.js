// «Avisos»: las frases del modo, de cada regla y del formulario, y el texto de cada refusal. Son
// funciones puras: la pantalla pone las palabras en su sitio y el host decide qué se avisa.

import { agendaErrorText } from "./agenda-model.js";
import { t } from "./i18n.js";

const MODES = {
  all: {
    label: t("alerts.all"),
    phrase: t("alerts.phrase_all"),
  },
  rules_only: {
    label: t("alerts.rules_only"),
    phrase: t("alerts.phrase_rules"),
  },
};

export const modeWords = (mode) => MODES[mode] ?? MODES.all;

export const GUARANTEE_TEXT = t("alerts.guarantee");

export const EMPTY_RULES_TEXT = t("alerts.empty_rules");

const verb = (action) => (action === "mute" ? t("alerts.action_mute") : t("alerts.action_notify"));

// One sentence per rule, in the words the operator would say it: «Avisar cuando Bob termine».
export function ruleSentence(rule) {
  if (rule.trigger === "calendar_reminder") return t("alerts.sentence_reminder", { verb: verb(rule.action) });
  const who = rule.agent ?? t("alerts.any_agent");
  const when = rule.status === "blocked" ? t("alerts.when_blocked_lower") : t("alerts.when_done_lower");
  return t("alerts.sentence_agent", { verb: verb(rule.action), who, when });
}

// The body the host takes for a new rule. A reminder has no agent and no status.
export function draftFor({ kind, agent, when, action }) {
  if (kind === "reminder") return { trigger: "calendar_reminder", agent: null, status: null, action };
  return { trigger: "pane_status", agent: agent ?? null, status: when, action };
}

// The names of the agents hired now, once each: the list the form offers besides «Cualquiera».
export function hiredNames(hires) {
  const names = hires.filter((hire) => hire.status === "hired").map((hire) => hire.name);
  return [...new Set(names)];
}

const FIELD_TEXT = {
  status: t("alerts.field_status"),
  agent: t("alerts.field_agent"),
};

// A refused rule carries its own Spanish text from the core; a field the form can name gets its
// words here; everything else is the agenda's generic line.
export function alertErrorText(failure) {
  if (failure.kind === "alertRefused") return failure.message;
  if (failure.kind === "invalid") return FIELD_TEXT[failure.field] ?? t("alerts.check_rule");
  return agendaErrorText(failure);
}
