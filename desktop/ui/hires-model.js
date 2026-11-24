// The rules of «Tu equipo», free of DOM so tooling/hires-model.test.mjs can run
// them: who counts as active, how the team groups by area, the header counter
// and the words of a refusal the host did not explain itself.

import { CHARACTER_IDS } from "./avatar.js";
import { areaLabel } from "./catalog-labels.js";
import { t } from "./i18n.js";

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

// «Contratados» has something to show: a hire, a read that failed, or the grant this device still
// needs. With nobody hired and the device able to hire, the header stays away.
export function showsHireSection({ hires, canHire, readError }) {
  return Boolean(readError) || !canHire || activeHires(hires).length > 0;
}

// The one «Contratar» of Equipo, in the header that stays on screen whatever the team band or the
// «Contratados» section shows: a device that can hire, and a team read that did not fail.
export function showsHireButton({ canHire, readError }) {
  return Boolean(canHire) && !readError;
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
      label: areaLabel(areas.find((area) => area.id === id)) || id,
      hires: active.filter((hire) => hire.area === id),
    }))
    .filter((group) => group.hires.length > 0);
}

// A project lead is part of the team: each person once, however many projects they lead.
export function leaderCount(projects) {
  const leads = (projects || []).map((project) => project && project.lead).filter(Boolean);
  return new Set(leads.map((lead) => lead.agent_id || lead.id || lead.name || JSON.stringify(lead))).size;
}

// Counts only what the view lists: the contracted ones and the agent sessions it shows (one per pane, restored ones
// included, so the count matches the section's rows). A project
// lead that is not running is not a row, so it is not counted here (the sidebar lists the projects).
export function teamSubtitle(hired, sessions = 0) {
  const parts = [];
  if (hired > 0) parts.push(t(hired === 1 ? "hires.hired_one" : "hires.hired_other", { n: hired }));
  if (sessions > 0) parts.push(t(sessions === 1 ? "hires.sessions_one" : "hires.sessions_other", { n: sessions }));
  return parts.length ? parts.join(" · ") : t("hires.none_yet");
}

// The failure a team read could not show. A read the host refuses with 403 means
// this device is not approved yet, which is not a broken connection.
export function readStateText(failure) {
  if (failure && failure.kind === "forbidden") return t("hires.waiting_approval");
  return null;
}

export const AREA_CAP_NOTICE = t("hires.area_cap", { max: AREA_CAP });

// Joins labels the way the language does: "a", "a y b", "a, b y c" (or "a, b and c").
export function joinLabels(labels) {
  if (labels.length <= 1) return labels.join("");
  return t("hires.join_last", { head: labels.slice(0, -1).join(", "), last: labels[labels.length - 1] });
}

export function hiredNoun(count) {
  return t(count === 1 ? "hires.noun_one" : "hires.noun_other");
}
