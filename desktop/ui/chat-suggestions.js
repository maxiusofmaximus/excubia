// The three suggestions of the empty chat, free of DOM so
// tooling/chat-suggestions.test.mjs can run them. The first one is always the
// general question; the other two come from the professions contracted here, one
// phrase of that trade per area, rotated by `turn` so they do not repeat.

import { t } from "./i18n.js";
import { activeHires } from "./hires-model.js";

const GENERAL = {
  mark: "?",
  title: t("suggest.general_title"),
  detail: t("suggest.general_detail"),
  prompt: t("suggest.general_prompt"),
};

// Two phrases per area, so a single contracted area still gets two different ones.
const AREA_PHRASES = {
  programacion: [t("suggest.programacion_1"), t("suggest.programacion_2")],
  video: [t("suggest.video_1"), t("suggest.video_2")],
  diseno_grafico: [t("suggest.diseno_grafico_1"), t("suggest.diseno_grafico_2")],
  audio: [t("suggest.audio_1"), t("suggest.audio_2")],
  documentos: [t("suggest.documentos_1"), t("suggest.documentos_2")],
  investigacion: [t("suggest.investigacion_1"), t("suggest.investigacion_2")],
  ingenieria_cad: [t("suggest.ingenieria_cad_1"), t("suggest.ingenieria_cad_2")],
  marketing: [t("suggest.marketing_1"), t("suggest.marketing_2")],
  otras: [t("suggest.otras_1"), t("suggest.otras_2")],
};

function neutral(title) {
  return { mark: "+", title, detail: "", prompt: title };
}

export const NEUTRAL_SUGGESTIONS = [
  neutral(t("suggest.neutral_1")),
  neutral(t("suggest.neutral_2")),
  neutral(t("suggest.neutral_3")),
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
