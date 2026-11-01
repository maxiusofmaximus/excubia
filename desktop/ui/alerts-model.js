// «Avisos»: las frases del modo, de cada regla y del formulario, y el texto de cada refusal. Son
// funciones puras: la pantalla pone las palabras en su sitio y el host decide qué se avisa.

import { agendaErrorText } from "./agenda-model.js";

const MODES = {
  all: {
    label: "Avisar de todo",
    phrase: "Avisa cada vez que un agente se bloquea o termina. El silencio gana siempre: lo que silencies no avisa.",
  },
  rules_only: {
    label: "Solo lo que yo elija",
    phrase: "Avisa solo lo que casa con una regla de aviso. El silencio gana siempre: lo que silencies no avisa.",
  },
};

export const modeWords = (mode) => MODES[mode] ?? MODES.all;

export const GUARANTEE_TEXT =
  "Los avisos siguen respetando los frenos del servidor (esperas entre avisos, horas de silencio).";

export const EMPTY_RULES_TEXT =
  "Todavía no hay reglas. Con «Avisar de todo» llega cada bloqueo y cada fin; con «Solo lo que yo elija» no llega ninguno. Crea una regla para elegir qué avisa.";

const verb = (action) => (action === "mute" ? "Silenciar" : "Avisar");

// One sentence per rule, in the words the operator would say it: «Avisar cuando Bob termine».
export function ruleSentence(rule) {
  if (rule.trigger === "calendar_reminder") return `${verb(rule.action)} de recordatorios del calendario`;
  const who = rule.agent ?? "cualquier agente";
  const when = rule.status === "blocked" ? "se bloquee" : "termine";
  return `${verb(rule.action)} cuando ${who} ${when}`;
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
  status: "Elige cuándo avisa: se bloquea o termina.",
  agent: "Ese agente ya no está en tu equipo. Elige otro o «Cualquiera».",
};

// A refused rule carries its own Spanish text from the core; a field the form can name gets its
// words here; everything else is the agenda's generic line.
export function alertErrorText(failure) {
  if (failure.kind === "alertRefused") return failure.message;
  if (failure.kind === "invalid") return FIELD_TEXT[failure.field] ?? "Revisa los datos de la regla.";
  return agendaErrorText(failure);
}
