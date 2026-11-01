// The «Contratar» sheet: the host's catalogue by area, one role contracted per tap.
// An area that already holds its 8 people refuses more, and says so in place.

import { $, invoke, toast } from "./core.js";
import { avatarElement } from "./avatar.js";
import { errorMessage } from "./format.js";
import { AREA_CAP, AREA_CAP_NOTICE, hiredIn, isHired } from "./hires-model.js";

let onChange = null;
let contracting = false;

function showError(message) {
  const box = $("hire-sheet-error");
  box.textContent = message;
  box.classList.toggle("hidden", !message);
}

function roleRow(role, full, hired) {
  const item = document.createElement("li");
  item.className = "role-row";
  const face = avatarElement({ name: role.name, characterId: role.avatar, px: 40, faceFromName: false });
  const text = document.createElement("span");
  text.className = "hire-text";
  const name = document.createElement("span");
  name.className = "hire-name";
  name.textContent = role.name;
  const title = document.createElement("span");
  title.className = "hire-title";
  title.textContent = role.title;
  text.append(name, title);
  const button = document.createElement("button");
  button.type = "button";
  button.className = "btn secondary btn-small";
  button.textContent = hired ? "Contratado" : "Contratar";
  button.disabled = hired || full;
  button.setAttribute(
    "aria-label",
    hired ? `${role.name}, ${role.title}: contratado` : `Contratar a ${role.name}, ${role.title}`,
  );
  button.addEventListener("click", () => contract(role.id));
  item.append(face, text, button);
  return item;
}

function areaBlock(team, area) {
  const section = document.createElement("section");
  section.className = "hire-group";
  const count = hiredIn(team.hires, area.id);
  const full = count >= AREA_CAP;
  const heading = document.createElement("h3");
  heading.className = "hire-group-title";
  heading.textContent = `${area.label} · ${count} de ${AREA_CAP}`;
  const list = document.createElement("ul");
  list.className = "role-list";
  list.setAttribute("role", "list");
  for (const id of area.roles) {
    const role = team.catalog.roles.find((item) => item.id === id);
    if (role) list.append(roleRow(role, full, isHired(team.hires, role.id)));
  }
  section.append(heading);
  if (full) {
    const notice = document.createElement("p");
    notice.className = "hint";
    notice.textContent = AREA_CAP_NOTICE;
    section.append(notice);
  }
  section.append(list);
  return section;
}

export function renderHireSheet(team) {
  $("hire-sheet-body").replaceChildren(...team.catalog.areas.map((area) => areaBlock(team, area)));
}

async function contract(role) {
  if (contracting) return;
  contracting = true;
  showError("");
  try {
    const hire = await invoke("team_hire", { role });
    toast(`${hire.name} se une a tu equipo`);
    await onChange();
  } catch (error) {
    showError(errorMessage(error));
  } finally {
    contracting = false;
  }
}

export function installHireSheet(refresh) {
  onChange = refresh;
  $("hire-sheet-close").addEventListener("click", () => $("hire-sheet").close());
}
