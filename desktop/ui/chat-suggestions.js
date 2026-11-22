// The three suggestions of the empty chat, free of DOM so
// tooling/chat-suggestions.test.mjs can run them. The first one is always the
// general question; the other two come from the professions contracted here, one
// phrase of that trade per area, rotated by `turn` so they do not repeat.

import { activeHires } from "./hires-model.js";

const GENERAL = {
  mark: "?",
  title: "¿Cómo va el equipo?",
  detail: "qué hace cada persona y qué está pendiente",
  prompt: "¿Cómo va el equipo? Qué hace cada persona y qué está pendiente",
};

// Two phrases per area, so a single contracted area still gets two different ones.
const AREA_PHRASES = {
  programacion: ["Revisa que todo funcione antes de entregar", "Dime qué parte del código conviene revisar primero"],
  video: ["Revisa el montaje y dime qué falta", "Propón un guion corto para el próximo vídeo"],
  diseno_grafico: ["Propón tres ideas para la portada", "Revisa que el cartel se lea bien a distancia"],
  audio: ["Revisa el audio y dime qué ajustar", "Prepara la lista de pistas para la sesión"],
  documentos: ["Prepara un informe de esta semana", "Resume estos documentos en una página"],
  investigacion: ["Busca fuentes fiables sobre este tema", "Compara lo que dicen los estudios sobre esto"],
  ingenieria_cad: ["Revisa las medidas del plano", "Dime qué piezas faltan en el despiece"],
  marketing: ["Redacta el plan de publicaciones de la semana", "Propón tres titulares para el anuncio"],
  otras: ["Ayúdame a ordenar las tareas de hoy", "Haz una lista de lo urgente de esta semana"],
};

function neutral(title) {
  return { mark: "+", title, detail: "", prompt: title };
}

export const NEUTRAL_SUGGESTIONS = [
  neutral("Cuéntame a qué te dedicas y armo tu equipo"),
  neutral("¿Qué puedes hacer por mí?"),
  neutral("Apunta una nota para mañana"),
];

// The areas with someone contracted that the catalogue knows, in catalogue order.
function teamAreas(hires) {
  const present = new Set(activeHires(hires).map((hire) => hire.area));
  return Object.keys(AREA_PHRASES).filter((area) => present.has(area));
}

function areaPhrases(areas, turn) {
  return [0, 1].map((step) => {
    const index = turn + step;
    const phrases = AREA_PHRASES[areas[index % areas.length]];
    return neutral(phrases[Math.floor(index / areas.length) % phrases.length]);
  });
}

export function chatSuggestions(hires, turn = 0) {
  if (activeHires(hires).length === 0) return [...NEUTRAL_SUGGESTIONS];
  const areas = teamAreas(hires);
  if (areas.length === 0) return [GENERAL, NEUTRAL_SUGGESTIONS[1], NEUTRAL_SUGGESTIONS[2]];
  return [GENERAL, ...areaPhrases(areas, turn)];
}
