// Run with: node --test tooling/schedule-model.test.mjs
// «Automatizar»: la frase de la regla, la próxima y la última ejecución, las etiquetas del historial,
// la validación del formulario y los errores del host.

import { test } from "node:test";
import assert from "node:assert/strict";
import {
  NO_PROJECTS_HELP,
  NO_PROJECTS_TEXT,
  PC_OFF_NOTE,
  PERMISSION_TEXT,
  PLAN_STEP_LIMIT,
  conversationChoices,
  dateSentence,
  formProblems,
  lastRunText,
  nextRunText,
  needsPlan,
  scheduleNextText,
  outcomeLabel,
  payloadOf,
  ruleFor,
  ruleSentence,
  scheduleFailureText,
  scheduleSentence,
  sortSchedules,
  statusLabel,
  stepCount,
} from "../ui/schedule-model.js";
import { REJECTED_TEXT } from "../ui/friendly-error.js";

const NOW = Date.parse("2026-11-05T12:00:00.000Z");

function form(overrides = {}) {
  return {
    title: "Resumen semanal",
    role: "audio-editor",
    source: "text",
    body: "Haz un resumen de la semana.",
    recipeId: "",
    conversationId: "cnv_01HZ",
    kind: "weekly",
    days: ["MO"],
    startDate: "2026-11-09",
    time: "09:00",
    timezone: "Europe/Madrid",
    steps: 1,
    planId: "",
    ...overrides,
  };
}

function schedule(overrides = {}) {
  return {
    id: "sch_1",
    title: "Resumen",
    conversation_id: "cnv_01HZ",
    role_id: "audio-editor",
    body: "Haz un resumen.",
    recipe_id: null,
    rule: "FREQ=WEEKLY;INTERVAL=1;BYDAY=MO",
    start_date: "2026-11-02",
    time_local: "09:00",
    timezone: "Europe/Madrid",
    steps: 1,
    status: "active",
    plan_id: null,
    next_run_at: "2026-11-09T08:00:00.000Z",
    last_run_at: null,
    created_at: "2026-11-01T10:00:00.000Z",
    updated_at: "2026-11-01T10:00:00.000Z",
    ...overrides,
  };
}

test("la frase de la regla dice cuándo se ejecuta, en llano", () => {
  assert.equal(ruleSentence({ kind: "once", startDate: "2026-11-12", time: "09:00", days: [] }), "El 12 de nov. a las 9:00");
  assert.equal(ruleSentence({ kind: "daily", startDate: "2026-11-05", time: "18:30", days: [] }), "Todos los días a las 18:30");
  assert.equal(ruleSentence({ kind: "weekly", startDate: "2026-11-09", time: "09:00", days: ["MO"] }), "Cada lunes a las 9:00");
  assert.equal(
    ruleSentence({ kind: "weekly", startDate: "2026-11-09", time: "09:00", days: ["MO", "WE"] }),
    "Cada lunes y miércoles a las 9:00",
  );
  assert.equal(ruleSentence({ kind: "monthly", startDate: "2026-11-01", time: "09:00", days: [] }), "El 1 de cada mes a las 9:00");
});

test("una fecha se dice con el día y el mes abreviado", () => {
  assert.equal(dateSentence("2026-11-12"), "12 de nov.");
  assert.equal(dateSentence("2026-09-01"), "1 de sept.");
});

test("la regla que se envía al host es RFC 5545 diaria, semanal o mensual, sin tope", () => {
  assert.equal(ruleFor({ kind: "once", days: [] }), null);
  assert.equal(ruleFor({ kind: "daily", days: [] }), "FREQ=DAILY;INTERVAL=1");
  assert.equal(ruleFor({ kind: "monthly", days: [] }), "FREQ=MONTHLY;INTERVAL=1");
  assert.equal(ruleFor({ kind: "weekly", days: ["WE", "MO"] }), "FREQ=WEEKLY;INTERVAL=1;BYDAY=MO,WE");
});

test("una regla semanal sin día no se puede guardar", () => {
  const problems = formProblems(form({ days: [] }), NOW);
  assert.equal(problems.days, "Elige al menos un día.");
});

test("la frase de una programación leída del host coincide con la del formulario", () => {
  assert.equal(scheduleSentence(schedule()), "Cada lunes a las 9:00");
  assert.equal(scheduleSentence(schedule({ rule: null, start_date: "2026-11-12" })), "El 12 de nov. a las 9:00");
  assert.equal(scheduleSentence(schedule({ rule: "FREQ=DAILY;INTERVAL=1", time_local: "18:30" })), "Todos los días a las 18:30");
  assert.equal(
    scheduleSentence(schedule({ rule: "FREQ=MONTHLY;INTERVAL=1", start_date: "2026-11-01" })),
    "El 1 de cada mes a las 9:00",
  );
});

test("cuenta los pasos del encargo por sus líneas numeradas o con viñeta; nunca menos de uno", () => {
  assert.equal(stepCount(""), 1);
  assert.equal(stepCount("Haz un resumen."), 1);
  assert.equal(stepCount("1. Abre el audio\n2. Quita el ruido\n3. Exporta\n4. Envíalo"), 4);
  assert.equal(stepCount("- uno\n* dos\n• tres"), 3);
});

test("un encargo de más de tres pasos necesita un plan aprobado", () => {
  assert.equal(PLAN_STEP_LIMIT, 3);
  assert.equal(needsPlan(3), false);
  assert.equal(needsPlan(4), true);
});

test("la validación del formulario dice qué falta, campo a campo", () => {
  assert.deepEqual(formProblems(form(), NOW), {});
  assert.equal(formProblems(form({ title: "  " }), NOW).title, "Ponle un título a la tarea.");
  assert.match(formProblems(form({ title: "x".repeat(81) }), NOW).title, /80 caracteres/);
  assert.equal(formProblems(form({ body: " " }), NOW).body, "Escribe el encargo.");
  assert.equal(formProblems(form({ source: "recipe", recipeId: "" }), NOW).recipe, "Elige una receta.");
  assert.equal(formProblems(form({ time: "25:00" }), NOW).time, "Escribe la hora como 09:00.");
  assert.equal(formProblems(form({ startDate: "2026-13-01" }), NOW).startDate, "Elige la fecha de inicio.");
  assert.equal(formProblems(form({ timezone: "Mars/Olympus" }), NOW).timezone, "Esa zona no existe. Usa un nombre como Europe/Madrid.");
  assert.equal(formProblems(form({ conversationId: "" }), NOW).conversation, "Elige una conversación con proyecto.");
});

test("una vez en una fecha que ya pasó no se programa", () => {
  const problems = formProblems(form({ kind: "once", startDate: "2026-11-04", time: "09:00" }), NOW);
  assert.equal(problems.startDate, "Esa fecha y hora ya pasaron. Elige una futura.");
});

test("el cuerpo del envío lleva el texto o la receta, el plan y los pasos, sin campos vacíos", () => {
  const once = payloadOf(form({ kind: "once", startDate: "2026-11-12" }));
  assert.equal(once.rule, undefined);
  assert.equal(once.body, "Haz un resumen de la semana.");
  assert.equal(once.recipe_id, undefined);
  assert.equal(once.plan_id, undefined);
  assert.equal(once.role, "audio-editor");
  assert.equal(once.start_date, "2026-11-12");
  assert.equal(once.time, "09:00");
  assert.equal(once.steps, 1);

  const recipe = payloadOf(form({ source: "recipe", body: "", recipeId: "rcp_1", planId: "plan_1", steps: 4 }));
  assert.equal(recipe.recipe_id, "rcp_1");
  assert.equal(recipe.body, undefined);
  assert.equal(recipe.plan_id, "plan_1");
  assert.equal(recipe.rule, "FREQ=WEEKLY;INTERVAL=1;BYDAY=MO");
});

test("la próxima y la última ejecución se muestran en la hora de la zona de la tarea", () => {
  assert.equal(nextRunText("2026-11-12T08:00:00.000Z", "Europe/Madrid"), "jue 12 nov., 9:00");
  assert.equal(nextRunText(null, "Europe/Madrid"), "Sin próxima ejecución");
  assert.equal(lastRunText(null), "Todavía no se ha ejecutado");
  assert.equal(lastRunText("2026-11-05T08:00:00.000Z", "Europe/Madrid"), "jue 5 nov., 9:00");
});

test("una tarea en pausa no muestra próxima ejecución aunque el host la envíe", () => {
  const paused = { status: "paused", next_run_at: "2026-11-12T08:00:00.000Z", timezone: "Europe/Madrid" };
  assert.equal(scheduleNextText(paused), "en pausa");
  assert.equal(scheduleNextText({ ...paused, status: "active" }), "jue 12 nov., 9:00");
});

test("estado y resultado tienen su etiqueta en llano", () => {
  assert.equal(statusLabel("active"), "Activa");
  assert.equal(statusLabel("paused"), "En pausa");
  assert.equal(statusLabel("done"), "Hecha");
  assert.equal(outcomeLabel("started"), "Se lanzó");
  assert.equal(outcomeLabel("skipped_overlap"), "Omitida: ya hay una en marcha");
  assert.equal(outcomeLabel("skipped_cap"), "Omitida: tope de ejecuciones");
  assert.equal(outcomeLabel("missed_offline"), "Perdida: tu PC estaba apagado");
  assert.equal(outcomeLabel("failed"), "Falló");
});

test("las programaciones activas van primero, luego en pausa y las hechas", () => {
  const sorted = sortSchedules([
    schedule({ id: "a", status: "done" }),
    schedule({ id: "b", status: "paused" }),
    schedule({ id: "c", status: "active", next_run_at: "2026-11-10T08:00:00.000Z" }),
    schedule({ id: "d", status: "active", next_run_at: "2026-11-06T08:00:00.000Z" }),
  ]);
  assert.deepEqual(sorted.map((item) => item.id), ["d", "c", "b", "a"]);
});

test("cada refusal del host se dice en español, nunca como JSON", () => {
  const cases = [
    [{ kind: "scheduleRefused", code: "schedule_limit" }, "Ya tienes 20 tareas activas. Pausa o borra una para programar otra."],
    [{ kind: "scheduleRefused", code: "invalid_schedule", reason: "rule" }, "La repetición no es válida. Elige una de la lista."],
    [{ kind: "scheduleRefused", code: "invalid_schedule", reason: "time" }, "La hora o la zona no son válidas."],
    [{ kind: "scheduleRefused", code: "invalid_schedule", reason: "steps" }, "El número de pasos no es válido."],
    [{ kind: "scheduleRefused", code: "schedule_no_future_run" }, "Esa fecha y hora ya pasaron. Elige una futura."],
    [{ kind: "scheduleRefused", code: "schedule_done" }, "Esta tarea ya se hizo y no se puede cambiar."],
    [{ kind: "scheduleRefused", code: "no_such_schedule" }, "Esa tarea ya no existe. Se quitará de la lista."],
    [{ kind: "planRefused", code: "plan_expired" }, "Cambiaste el texto: hay que aprobar un plan nuevo."],
    [{ kind: "planRefused", code: "plan_not_approved" }, "Ese plan no sirve para este encargo: tiene que estar aprobado, ser de esta conversación y no usarlo otro programado."],
    [{ kind: "recipeRefused", code: "secret_detected", reasons: [] }, "Quita la clave o contraseña del texto. Con un secreto dentro no se puede guardar."],
    [{ kind: "forbidden" }, PERMISSION_TEXT],
    [{ kind: "notFound" }, "Esa tarea ya no existe. Se quitará de la lista."],
    [{ kind: "archived" }, "Esta conversación está archivada. Desarchívala para programar tareas en ella."],
    [{ kind: "offline" }, "Sin conexión con tu PC."],
    [{ kind: "scheduleRefused", code: "algo_nuevo" }, REJECTED_TEXT],
    [new Error("boom"), "No se pudo cargar. Inténtalo de nuevo."],
  ];
  for (const [error, text] of cases) {
    assert.equal(scheduleFailureText(error), text);
  }
});

test("la frase de PC encendido es la que pide el contrato, idéntica en cada pantalla", () => {
  assert.equal(
    PC_OFF_NOTE,
    "Solo se ejecuta con tu PC encendido. Si estaba apagado a la hora, no se lanza al encenderlo: te avisamos aquí y puedes lanzarla tú.",
  );
});

test("solo las conversaciones con proyecto se pueden elegir para una tarea", () => {
  const chats = [
    { id: "cnv_01HZ", title: "Episodio 12", project_id: "prj_01HZ" },
    { id: "cnv_02HZ", title: "Notas sueltas", project_id: null },
  ];
  const projects = [{ id: "prj_01HZ", name: "Podcast" }];
  assert.deepEqual(conversationChoices(chats, projects), [["cnv_01HZ", "Episodio 12 · Podcast"]]);
  assert.deepEqual(conversationChoices([chats[1]], projects), []);
});

test("la etiqueta dice el proyecto, sin repetir su nombre ni inventarlo", () => {
  const projects = [{ id: "prj_01HZ", name: "prueba-t221" }];
  const chat = (title, project_id = "prj_01HZ") => ({ id: "cnv_01HZ", title, project_id });
  assert.deepEqual(conversationChoices([chat("Nuevo chat")], projects), [["cnv_01HZ", "Nuevo chat · prueba-t221"]]);
  assert.deepEqual(conversationChoices([chat("Plan prueba-t221")], projects), [["cnv_01HZ", "Plan prueba-t221"]]);
  assert.deepEqual(conversationChoices([chat("Nuevo chat", "prj_otro")], projects), [["cnv_01HZ", "Nuevo chat"]]);
});

test("el título que repite el proyecto no distingue mayúsculas", () => {
  const projects = [{ id: "prj_01HZ", name: "Podcast" }];
  const chat = (title) => ({ id: "cnv_01HZ", title, project_id: "prj_01HZ" });
  assert.deepEqual(conversationChoices([chat("podcast")], projects), [["cnv_01HZ", "podcast"]]);
  assert.deepEqual(conversationChoices([chat("PODCAST")], projects), [["cnv_01HZ", "PODCAST"]]);
  assert.deepEqual(conversationChoices([chat("Episodio 3")], projects), [["cnv_01HZ", "Episodio 3 · Podcast"]]);
});

test("sin proyectos, el formulario dice qué hacer con las palabras exactas", () => {
  assert.equal(NO_PROJECTS_TEXT, "Aún no tienes proyectos");
  assert.equal(NO_PROJECTS_HELP, "Pídele a Alfred que cree un proyecto y vuelve aquí para programar la tarea.");
});
