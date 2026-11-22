// Arranca un escenario de «Automatizar» sobre los módulos reales de ui/ (sin copiarlos). El escenario
// y la paleta vienen en la URL: ?s=<escenario>&p=<noche|papel>. El puente simulado lo lee igual.

import { $ } from "../ui/core.js";
import { installSchedules, refreshSchedules } from "../ui/schedules.js";
import { openScheduleEditor } from "../ui/schedule-editor.js";

const params = new URLSearchParams(location.search);
const scenario = params.get("s") || "lista";
const invoke = window.__TAURI__.core.invoke;

const PLAN_TEXT = [
  "1. Leer las notas de la semana",
  "2. Escribir el resumen por temas",
  "3. Marcar los pendientes con responsable",
  "4. Dejar el resumen en la carpeta de archivo",
].join("\n");
const NEW_EDITOR = ["editor-una", "editor-semanal", "editor-mensual", "plan", "sin-permiso-guardar", "limite", "plan-no-aprobado", "pasado"];
const SAVES = ["sin-permiso-guardar", "limite", "plan-no-aprobado", "pasado", "plan"];
const KIND_OF = { "editor-una": "once", "editor-semanal": "weekly", "editor-mensual": "monthly" };

const settle = () => new Promise((resolve) => setTimeout(resolve, 80));
const action = (name) => document.querySelector(`[data-schedule-action="${name}"]`);

// Types into the form the way the operator would: its own fields, with their own events.
function type(id, value) {
  const node = $(id);
  node.value = value;
  node.dispatchEvent(new Event(node.tagName === "SELECT" ? "change" : "input", { bubbles: true }));
}

function fillNew(kind, body) {
  type("schedule-title", "Entrega del episodio 13");
  type("schedule-body", body);
  type("schedule-conversation", "cnv_01HZ");
  type("schedule-date", "2026-11-20");
  type("schedule-time", "18:30");
  type("schedule-zone", "Europe/Madrid");
  if (kind) type("schedule-kind", kind);
}

installSchedules();
window.ExcubiaPalettes.applyPalette(params.get("p") || "noche");
document.body.dataset.scenario = scenario;

(async () => {
  if (scenario === "editar") {
    await refreshSchedules();
    const [item] = await invoke("schedules");
    openScheduleEditor({ schedule: item, onSaved: refreshSchedules });
    await settle();
    return;
  }
  if (scenario === "sin-proyectos") {
    await refreshSchedules();
    openScheduleEditor({ schedule: null, onSaved: refreshSchedules });
    await settle();
    return;
  }
  if (NEW_EDITOR.includes(scenario)) {
    await refreshSchedules();
    openScheduleEditor({ schedule: null, onSaved: refreshSchedules });
    await settle();
    const body = scenario === "plan" ? PLAN_TEXT : "Termina la mezcla y deja el archivo listo para subir.";
    fillNew(KIND_OF[scenario], body);
    if (SAVES.includes(scenario)) $("schedule-save").click();
    return;
  }
  await refreshSchedules();
  if (scenario === "historial" || scenario === "historial-vacio") action("history").click();
  if (scenario === "ejecutar" || scenario === "ocupada") action("run").click();
  if (scenario === "sin-permiso") action("switch").click();
  await settle();
})();
