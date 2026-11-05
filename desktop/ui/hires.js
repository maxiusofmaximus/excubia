// «Tu equipo»: the professionals contracted on this host, grouped by area, above
// the live agents of the Equipo view, which it never replaces. The buttons show
// only when this device holds APPROVE; without it the list stays, with the grant.

import { $, app, invoke } from "./core.js";
import { avatarElement } from "./avatar.js";
import { friendlyError } from "./friendly-error.js";
import { errorState } from "./empty-state.js";
import { currentDeviceId, grantCommand } from "./approval-model.js";
import { activeHires, groupByArea, readStateText } from "./hires-model.js";
import { installHireDialogs, openHireActions } from "./hire-dialogs.js";
import { installHireSheet, renderHireSheet } from "./hire-sheet.js";

const team = { catalog: null, hires: [], canHire: false, readError: null };
let afterChange = () => {};

export function teamHired() {
  return activeHires(team.hires).length;
}

export function teamSnapshot() {
  return team;
}

export async function refreshTeam() {
  try {
    const [catalog, list] = await Promise.all([invoke("team_roles"), invoke("team_hires")]);
    Object.assign(team, { catalog, hires: list.hires, canHire: list.can_hire, readError: null });
  } catch (error) {
    // A 403 waits for the grant; anything else can be read again.
    const waiting = readStateText(error);
    team.readError = waiting ? { text: waiting, retry: false } : { text: friendlyError(error), retry: true };
  }
  renderTeam();
  return team;
}

function roleTitle(hire) {
  return team.catalog?.roles.find((role) => role.id === hire.role)?.title || "";
}

function hireRow(hire) {
  const item = document.createElement("li");
  const row = document.createElement(team.canHire ? "button" : "div");
  row.className = "hire-row";
  if (team.canHire) {
    row.type = "button";
    row.setAttribute("aria-label", `Opciones de ${hire.name}${roleTitle(hire) ? `, ${roleTitle(hire)}` : ""}`);
    row.addEventListener("click", () => openHireActions(hire));
  }
  const face = avatarElement({ name: hire.name, characterId: hire.avatar, px: 40, faceFromName: false });
  const text = document.createElement("span");
  text.className = "hire-text";
  const name = document.createElement("span");
  name.className = "hire-name";
  name.textContent = hire.name;
  const title = document.createElement("span");
  title.className = "hire-title";
  title.textContent = roleTitle(hire);
  text.append(name, title);
  row.append(face, text);
  if (team.canHire) {
    const chevron = document.createElement("span");
    chevron.className = "hire-chevron";
    chevron.setAttribute("aria-hidden", "true");
    chevron.textContent = "›";
    row.append(chevron);
  }
  item.append(row);
  return item;
}

function groupBlock(group) {
  const section = document.createElement("section");
  section.className = "hire-group";
  section.setAttribute("aria-label", group.label);
  const heading = document.createElement("h3");
  heading.className = "hire-group-title";
  heading.textContent = group.label;
  const list = document.createElement("ul");
  list.className = "hire-list";
  list.setAttribute("role", "list");
  list.append(...group.hires.map(hireRow));
  section.append(heading, list);
  return section;
}

function grantNote() {
  const note = document.createElement("div");
  note.className = "hire-note";
  const text = document.createElement("p");
  text.textContent =
    "Este dispositivo no tiene el permiso APPROVE: ves tu equipo, pero no puedes cambiarlo. Concédelo desde el PC:";
  const code = document.createElement("code");
  code.className = "approval-grant";
  code.textContent = grantCommand(currentDeviceId() || app.boot?.device_id);
  note.append(text, code);
  return note;
}

function readErrorBlock({ text, retry }) {
  if (retry) return errorState(text, refreshTeam);
  const note = document.createElement("p");
  note.className = "hint";
  note.textContent = text;
  return note;
}

function bodyBlocks() {
  if (team.readError) return [readErrorBlock(team.readError)];
  const blocks = [];
  if (!team.canHire) blocks.push(grantNote());
  const groups = groupByArea(team.hires, team.catalog?.areas || []);
  if (groups.length) {
    blocks.push(...groups.map(groupBlock));
  } else {
    const empty = document.createElement("p");
    empty.className = "hint";
    empty.textContent =
      "Todavía no has contratado a nadie. Alfred siempre está en tu equipo; contrata a quien necesites.";
    blocks.push(empty);
  }
  return blocks;
}

function renderTeam() {
  const hidden = Boolean(team.readError) || !team.canHire;
  $("btn-hire").classList.toggle("hidden", hidden);
  $("team-hires-body").replaceChildren(...bodyBlocks());
  if ($("hire-sheet").open && team.catalog) renderHireSheet(team);
}

async function changed() {
  await refreshTeam();
  afterChange();
}

// `onChange` runs after each change of the team, so the header of Equipo can follow.
export function installTeam(onChange) {
  afterChange = onChange;
  installHireDialogs(changed);
  installHireSheet(changed);
  $("btn-hire").addEventListener("click", () => {
    renderHireSheet(team);
    $("hire-sheet").showModal();
  });
}
