// The rules of «Tu equipo», free of DOM so tooling/hires-model.test.mjs can run
// them: who counts as active, how the team groups by area, the header counter
// and the words of a refusal the host did not explain itself.

import { CHARACTER_IDS } from "./avatar.js";

export const AREA_CAP = 8;

// Alfred is fixed on the team and the host refuses him as an avatar, so the
// picker offers the eight characters that can be hired.
export const HIRE_CHARACTERS = CHARACTER_IDS.filter((id) => id !== "alfred");

export function characterLabel(id) {
  return id ? id[0].toUpperCase() + id.slice(1) : "";
}

export function activeHires(hires) {
  return hires.filter((hire) => hire.status === "hired");
}

export function hiredIn(hires, areaId) {
  return activeHires(hires).filter((hire) => hire.area === areaId).length;
}

export function isHired(hires, roleId) {
  return activeHires(hires).some((hire) => hire.role === roleId);
}

// The team grouped by the catalogue's areas, in catalogue order. An area with no
// one contracted is left out, and a hire whose area is not in the catalogue
// keeps its own group.
export function groupByArea(hires, areas) {
  const active = activeHires(hires);
  const order = areas.map((area) => area.id);
  for (const hire of active) {
    if (!order.includes(hire.area)) order.push(hire.area);
  }
  return order
    .map((id) => ({
      id,
      label: areas.find((area) => area.id === id)?.label || id,
      hires: active.filter((hire) => hire.area === id),
    }))
    .filter((group) => group.hires.length > 0);
}

// «3 en el equipo · 2 activos». `hired` counts the contracted people; `active`
// counts the live agents on screen.
export function teamCountLabel(hired, active) {
  return `${hired} en el equipo · ${active === 1 ? "1 activo" : `${active} activos`}`;
}

// The failure a team read could not show. A read the host refuses with 403 means
// this device is not approved yet, which is not a broken connection.
export function readStateText(failure) {
  if (failure && failure.kind === "forbidden") return "Esperando que aprueben este dispositivo.";
  return null;
}

export const AREA_CAP_NOTICE = `Máximo ${AREA_CAP} por área.`;

// Joins labels the way a Spanish sentence does: "a", "a y b", "a, b y c".
export function joinLabels(labels) {
  if (labels.length <= 1) return labels.join("");
  return `${labels.slice(0, -1).join(", ")} y ${labels[labels.length - 1]}`;
}

export function hiredNoun(count) {
  return count === 1 ? "agente" : "agentes";
}
