// The first-run assistant: the palette screen comes first, then three steps set
// up the team (areas, level, the people of each area); Alfred greets at the top
// of the first one. Shown only when the host says onboarding is not done and this
// device can hire; «Saltar» closes it for good.

import { $, app, invoke, toast } from "./core.js";
import { showView } from "./nav.js";
import { avatarElement } from "./avatar.js";
import { errorMessage } from "./format.js";
import { AREA_CAP } from "./hires-model.js";
import {
  LEVELS,
  areaTotal,
  chosenAreas,
  confirmation,
  presetRoles,
  toggleRole,
} from "./onboarding-model.js";
import { renderAgents } from "./agents.js";
import { refreshTeam, teamSnapshot } from "./hires.js";
import { offerFirstRunPalette } from "./palette.js";

const STEPS = 3;
const wizard = {};
let checked = false;

function resetWizard() {
  Object.assign(wizard, { step: 0, areas: [], level: "profesional", roles: [], notice: null, busy: false });
}
resetWizard();

function body() {
  return $("onboarding-body");
}

function heading(text) {
  const title = document.createElement("h1");
  title.textContent = text;
  return title;
}

function lede(text, className = "dialog-lede") {
  const p = document.createElement("p");
  p.className = className;
  p.textContent = text;
  return p;
}

function chip(text, pressed, onClick) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "choice";
  button.setAttribute("aria-pressed", String(pressed));
  button.textContent = text;
  button.addEventListener("click", onClick);
  return button;
}

function areasStep(catalog) {
  const face = avatarElement({ name: "Alfred", characterId: "alfred", px: 72, faceFromName: false });
  face.classList.add("onboarding-face");
  return [
    face,
    heading("Hola, soy Alfred"),
    lede("Soy tu CTO y te ayudo a montar el equipo. Para empezar, ¿en qué áreas trabajas? Elige una o varias.", "hint"),
    ...catalog.areas.map((area) =>
      chip(area.label, wizard.areas.includes(area.id), () => {
        wizard.areas = wizard.areas.includes(area.id)
          ? wizard.areas.filter((id) => id !== area.id)
          : [...wizard.areas, area.id];
        render();
      }),
    ),
  ];
}

function levelStep() {
  return [
    heading("¿Cuánto equipo quieres?"),
    ...LEVELS.map((level) =>
      chip(`${level.label}: ${level.help}`, wizard.level === level.id, () => {
        wizard.level = level.id;
        render();
      }),
    ),
  ];
}

// A row that works as a checkbox: the button takes Space, the box shows the mark.
// Without `onToggle` the row is fixed and disabled, as Alfred is.
function pickRow({ key, name, title, avatar, picked, onToggle }) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "pick";
  button.setAttribute("role", "checkbox");
  button.setAttribute("aria-checked", String(picked));
  button.dataset.key = key;
  button.disabled = !onToggle;
  const text = document.createElement("span");
  text.className = "pick-text";
  const who = document.createElement("span");
  who.className = "pick-name";
  who.textContent = name;
  const what = document.createElement("span");
  what.className = "pick-title";
  what.textContent = title;
  text.append(who, what);
  const box = document.createElement("span");
  box.className = "pick-box";
  box.setAttribute("aria-hidden", "true");
  box.textContent = picked ? "✓" : "";
  button.append(avatarElement({ name, characterId: avatar, px: 40, faceFromName: false }), text, box);
  if (onToggle) button.addEventListener("click", onToggle);
  const item = document.createElement("li");
  item.append(button);
  return item;
}

function pickList(rows) {
  const list = document.createElement("ul");
  list.className = "pick-list";
  list.setAttribute("role", "list");
  list.append(...rows);
  return list;
}

function areaGroup(title) {
  const section = document.createElement("section");
  section.className = "hire-group";
  const h3 = document.createElement("h3");
  h3.className = "hire-group-title";
  h3.textContent = title;
  section.append(h3);
  return section;
}

// Turns one role of an area on or off. A refused pick says so in that area's group.
function pick(roleId, area) {
  const result = toggleRole({ selected: wizard.roles, role: roleId, area, hires: teamSnapshot().hires });
  wizard.roles = result.selected;
  wizard.notice = result.notice ? { areaId: area.id, text: result.notice } : null;
  render(roleId);
}

function teamStep(catalog) {
  const { hires } = teamSnapshot();
  const alfred = document.createElement("section");
  alfred.setAttribute("aria-label", "Siempre en tu equipo");
  alfred.append(
    pickList([
      pickRow({ key: "alfred", name: "Alfred", title: "Director técnico (CTO)", avatar: "alfred", picked: true }),
    ]),
  );
  const blocks = [heading("Tu equipo inicial"), lede("Quita o pon a quien no necesites. Hasta 8 por área.", "hint"), alfred];
  for (const area of chosenAreas(catalog.areas, wizard.areas)) {
    const section = areaGroup(`${area.label} · ${areaTotal({ selected: wizard.roles, area, hires })}/${AREA_CAP}`);
    if (wizard.notice?.areaId === area.id) {
      const notice = lede(wizard.notice.text, "form-error");
      notice.setAttribute("role", "alert");
      section.append(notice);
    }
    const roles = area.roles.map((id) => catalog.roles.find((item) => item.id === id)).filter(Boolean);
    section.append(
      pickList(
        roles.map((role) =>
          pickRow({
            key: role.id,
            name: role.name,
            title: role.title,
            avatar: role.avatar,
            picked: wizard.roles.includes(role.id),
            onToggle: () => pick(role.id, area),
          }),
        ),
      ),
    );
    blocks.push(section);
  }
  return blocks;
}

function renderNav() {
  const first = wizard.step === 0;
  const last = wizard.step === STEPS - 1;
  $("onboarding-progress").textContent = `Paso ${wizard.step + 2} de ${STEPS + 1}`;
  $("onboarding-back").classList.toggle("hidden", first);
  $("onboarding-next").textContent = last ? "Contratar equipo" : "Siguiente";
  $("onboarding-next").disabled =
    wizard.busy ||
    (wizard.step === 0 && wizard.areas.length === 0) ||
    (last && wizard.roles.length === 0);
  $("onboarding-skip").disabled = wizard.busy;
  $("onboarding-back").disabled = wizard.busy;
}

function showError(message) {
  $("onboarding-error").textContent = message;
  $("onboarding-error").classList.toggle("hidden", !message);
}

// `focusKey` is the row a toggle touched: redrawing the step drops focus, so it goes back to that row.
function render(focusKey = null) {
  const catalog = teamSnapshot().catalog;
  const blocks = [() => areasStep(catalog), levelStep, () => teamStep(catalog)];
  body().replaceChildren(...blocks[wizard.step]());
  if (focusKey) [...body().querySelectorAll("[data-key]")].find((el) => el.dataset.key === focusKey)?.focus();
  renderNav();
}

function enterStep(step) {
  if (step === STEPS - 1) {
    wizard.roles = presetRoles(chosenAreas(teamSnapshot().catalog.areas, wizard.areas), wizard.level);
    wizard.notice = null;
  }
  wizard.step = step;
  showError("");
  render();
}

// Both exits land on Equipo, where the team the user just chose now shows.
function close() {
  showView("agents");
  renderAgents();
}

// One host call at a time: the buttons stay off while it runs, and a refusal shows in place.
async function run(task) {
  wizard.busy = true;
  renderNav();
  try {
    await task();
  } catch (error) {
    showError(errorMessage(error));
  } finally {
    wizard.busy = false;
    renderNav();
  }
}

function skip() {
  return run(async () => {
    await invoke("team_onboarding_skip");
    await refreshTeam();
    close();
  });
}

function contract() {
  return run(async () => {
    const done = await invoke("team_onboarding_contract", { areas: wizard.areas, roles: wizard.roles });
    const labels = chosenAreas(teamSnapshot().catalog.areas, wizard.areas).map((area) => area.label);
    await refreshTeam();
    close();
    toast(confirmation(done.hires.length, labels));
  });
}

// Draws the first wizard step, once the palette screen (if any) is done.
function startWizard() {
  resetWizard();
  showError("");
  render();
  showView("onboarding");
}

// Asks the host once per session, when the first connection is up. A failed read
// (offline, or no APPROVE) keeps the wizard closed and asks again on the next connect.
export async function checkOnboarding() {
  if (checked || !app.connection || app.connection.kind !== "connected") return;
  let onboarding;
  try {
    onboarding = await invoke("team_onboarding");
  } catch {
    return;
  }
  checked = true;
  if (!onboarding.done && onboarding.can_hire) await refreshTeam();
  const shown = offerFirstRunPalette({
    onboardingDone: onboarding.done,
    onContinue: () => (onboarding.can_hire ? startWizard() : showView("chat")),
  });
  if (!shown && !onboarding.done && onboarding.can_hire) startWizard();
}

export function installOnboarding() {
  $("onboarding-next").addEventListener("click", () => {
    if (wizard.step === STEPS - 1) contract();
    else enterStep(wizard.step + 1);
  });
  $("onboarding-back").addEventListener("click", () => enterStep(wizard.step - 1));
  $("onboarding-skip").addEventListener("click", skip);
}
