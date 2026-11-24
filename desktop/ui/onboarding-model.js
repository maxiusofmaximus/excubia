// The first-run assistant's rules, free of DOM: which roles a level preselects,
// how a toggle stays under the cap of one area, and the closing sentence.
// tooling/onboarding-model.test.mjs runs them without a window.

import { AREA_CAP, AREA_CAP_NOTICE, hiredIn, joinLabels, hiredNoun } from "./hires-model.js";
import { t } from "./i18n.js";

// People the area would hold once the step is confirmed: the contracted ones plus the picks.
export function areaTotal({ selected, area, hires }) {
  return selected.filter((id) => area.roles.includes(id)).length + hiredIn(hires, area.id);
}

export const LEVELS = [
  { id: "personal", label: t("onboarding.level_personal"), help: t("onboarding.level_personal_help") },
  { id: "profesional", label: t("onboarding.level_profesional"), help: t("onboarding.level_profesional_help") },
  { id: "equipo", label: t("onboarding.level_equipo"), help: t("onboarding.level_equipo_help") },
];

// Profesional is the level a skipped step falls back to, as on the host.
export function presetRoles(areas, level) {
  const roles = [];
  for (const area of areas) {
    const preset = area.levels?.[level] ?? area.levels?.profesional ?? [];
    for (const role of preset) {
      if (area.roles.includes(role) && !roles.includes(role)) roles.push(role);
    }
  }
  return roles;
}

// Turns one role on or off. Turning one on past the cap of its area is refused,
// and the refusal carries the notice to show. `hires` counts the people already
// contracted, so the cap holds for the whole area, not only the new choices.
export function toggleRole({ selected, role, area, hires }) {
  if (selected.includes(role)) {
    return { selected: selected.filter((id) => id !== role), notice: null };
  }
  if (areaTotal({ selected, area, hires }) >= AREA_CAP) {
    return { selected, notice: AREA_CAP_NOTICE };
  }
  return { selected: [...selected, role], notice: null };
}

// The areas the user chose, in the catalogue's order, whatever order they tapped.
export function chosenAreas(areas, areaIds) {
  return areas.filter((area) => areaIds.includes(area.id));
}

export function confirmation(count, areaLabels) {
  return t("onboarding.confirmation", { count, noun: hiredNoun(count), areas: joinLabels(areaLabels) });
}
