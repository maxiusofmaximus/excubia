// Puente simulado para el arnés de «Automatizar» (tooling/schedules-harness.html). Sustituye a
// window.__TAURI__ con respuestas fijas por escenario, con la forma que devuelven los comandos de
// src-tauri/src/schedule_commands.rs y de src-tauri/src/team_commands.rs. Solo lo usa la página de
// pruebas, nunca la app.

(function () {
  const params = new URLSearchParams(location.search);
  const scenario = params.get("s") || "lista";

  const ROLES = [
    { id: "audio-editor", title: "Editor de audio" },
    { id: "researcher", title: "Investigadora" },
  ];

  const HIRES = [
    { id: "hir_01HZ", role: "audio-editor", area: "Producción", name: "Marta", avatar: null, status: "hired" },
    { id: "hir_02HZ", role: "researcher", area: "Investigación", name: "Ernesto", avatar: null, status: "hired" },
  ];

  const CONVERSATIONS = [
    { id: "cnv_01HZ", title: "Episodio 12", project_id: "prj_01HZ", pinned: false, section_id: null },
    { id: "cnv_02HZ", title: "Notas sueltas", project_id: null, pinned: false, section_id: null },
  ];

  const PLANS = [
    {
      id: "plan_01HZ", status: "approved",
      steps: [{ title: "Leer las notas", touches: ["files"] }, { title: "Escribir el resumen", touches: ["files"] }],
      summary: "Plan de 2 pasos · lo propone Marta · toca: ficheros",
      note: null, delegation_id: null, created_at: "2026-11-04T10:00:00.000Z", updated_at: "2026-11-04T10:00:00.000Z",
    },
    {
      id: "plan_02HZ", status: "proposed",
      steps: [{ title: "Enviar el resumen", touches: ["mail"] }],
      summary: "Plan de 1 paso · lo propone Ernesto · toca: correo",
      note: null, delegation_id: null, created_at: "2026-11-05T09:00:00.000Z", updated_at: "2026-11-05T09:00:00.000Z",
    },
  ];

  const RECIPES = [
    {
      id: "rcp_01HZA1", role_id: "audio-editor", title: "Limpieza de audio de entrevistas",
      body: "Quita el ruido de fondo, normaliza el volumen a -16 LUFS y exporta en WAV de 48 kHz.",
      uses: 4, created_at: "2026-11-02T10:00:00.000Z", updated_at: "2026-11-04T09:12:00.000Z",
    },
    {
      id: "rcp_01HZA2", role_id: "audio-editor", title: "Notas de guion del capítulo",
      body: "Escribe tres notas por escena: intención, tono y duración aproximada.",
      uses: 12, created_at: "2026-10-28T08:05:00.000Z", updated_at: "2026-11-01T12:30:00.000Z",
    },
  ];

  const SCHEDULES = [
    {
      id: "sch_01HZA", title: "Resumen semanal de reuniones", conversation_id: "cnv_01HZ", role_id: "audio-editor",
      body: "Resume las reuniones de la semana con acuerdos, responsables y fechas.", recipe_id: null,
      rule: "FREQ=WEEKLY;INTERVAL=1;BYDAY=MO,WE", start_date: "2026-11-02", time_local: "09:00",
      timezone: "Europe/Madrid", steps: 1, status: "active", plan_id: null,
      next_run_at: "2026-11-09T08:00:00.000Z", last_run_at: "2026-11-04T08:00:00.000Z",
      created_at: "2026-11-02T10:00:00.000Z", updated_at: "2026-11-04T08:00:00.000Z",
    },
    {
      id: "sch_02HZB", title: "Copia diaria de notas", conversation_id: "cnv_01HZ", role_id: "researcher",
      body: "Copia las notas del día a la carpeta de archivo.", recipe_id: null,
      rule: "FREQ=DAILY;INTERVAL=1", start_date: "2026-11-01", time_local: "22:00",
      timezone: "Europe/Madrid", steps: 1, status: "paused", plan_id: null,
      next_run_at: null, last_run_at: "2026-10-31T20:00:00.000Z",
      created_at: "2026-11-01T10:00:00.000Z", updated_at: "2026-11-03T10:00:00.000Z",
    },
    {
      id: "sch_03HZC", title: "Entrega del episodio", conversation_id: "cnv_01HZ", role_id: "audio-editor",
      body: null, recipe_id: "rcp_01HZA1", rule: null, start_date: "2026-11-03", time_local: "18:30",
      timezone: "Europe/Madrid", steps: 1, status: "done", plan_id: null,
      next_run_at: null, last_run_at: "2026-11-03T17:30:00.000Z",
      created_at: "2026-11-02T10:00:00.000Z", updated_at: "2026-11-03T17:30:00.000Z",
    },
  ];

  const RUNS = [
    { id: "run_1", due_at: "2026-11-04T08:00:00.000Z", outcome: "started" },
    { id: "run_2", due_at: "2026-11-02T08:00:00.000Z", outcome: "skipped_overlap" },
    { id: "run_3", due_at: "2026-10-30T08:00:00.000Z", outcome: "skipped_cap" },
    { id: "run_4", due_at: "2026-10-28T08:00:00.000Z", outcome: "missed_offline" },
    { id: "run_5", due_at: "2026-10-21T08:00:00.000Z", outcome: "failed" },
  ];

  const FORBIDDEN = { kind: "forbidden" };
  const never = () => new Promise(() => {});

  function saved(draft, id) {
    return {
      id: id || "sch_01HZNEW", title: draft.title, conversation_id: draft.conversation_id, role_id: draft.role,
      body: draft.body ?? null, recipe_id: draft.recipe_id ?? null, rule: draft.rule ?? null,
      start_date: draft.start_date, time_local: draft.time, timezone: draft.timezone, steps: draft.steps,
      status: "active", plan_id: draft.plan_id ?? null, next_run_at: "2026-11-09T08:00:00.000Z",
      last_run_at: null, created_at: "2026-11-05T17:00:00.000Z", updated_at: "2026-11-05T17:00:00.000Z",
    };
  }

  // A save the host refuses, by the scenario the screen is showing.
  function refusalOf(draft) {
    if (scenario === "sin-permiso-guardar") return FORBIDDEN;
    if (scenario === "limite") return { kind: "scheduleRefused", code: "schedule_limit", reason: null };
    if (scenario === "plan-no-aprobado") return { kind: "planRefused", code: "plan_not_approved" };
    if (scenario === "pasado") return { kind: "scheduleRefused", code: "schedule_no_future_run", reason: null };
    return null;
  }

  const handlers = {
    team_roles() {
      return Promise.resolve({ roles: ROLES });
    },
    team_hires() {
      return Promise.resolve({ hires: HIRES, can_hire: true });
    },
    chat_home() {
      const conversations = scenario === "sin-proyectos" ? CONVERSATIONS.filter((chat) => !chat.project_id) : CONVERSATIONS;
      return Promise.resolve({ bootstrap: { projects: [{ id: "prj_01HZ", name: "Podcast" }] }, conversations, archived: [], sections: [] });
    },
    plans() {
      return Promise.resolve(PLANS);
    },
    recipes(args) {
      return Promise.resolve(RECIPES.filter((recipe) => recipe.role_id === args.role));
    },
    schedules() {
      if (scenario === "cargando") return never();
      if (scenario === "error") return Promise.reject({ kind: "offline" });
      if (scenario === "vacio") return Promise.resolve([]);
      return Promise.resolve(SCHEDULES);
    },
    schedule_create(args) {
      return refusalOf(args.draft) ? Promise.reject(refusalOf(args.draft)) : Promise.resolve(saved(args.draft));
    },
    schedule_update(args) {
      return refusalOf(args.draft) ? Promise.reject(refusalOf(args.draft)) : Promise.resolve(saved(args.draft, args.id));
    },
    schedule_delete(args) {
      return Promise.resolve(args.id);
    },
    schedule_set_paused(args) {
      if (scenario === "sin-permiso") return Promise.reject(FORBIDDEN);
      const item = SCHEDULES.find((schedule) => schedule.id === args.id);
      return Promise.resolve({ ...item, status: args.paused ? "paused" : "active" });
    },
    schedule_run_now() {
      if (scenario === "sin-permiso") return Promise.reject(FORBIDDEN);
      if (scenario === "ocupada") {
        return Promise.resolve({ delegation_id: null, status: null, run: { id: "run_x", due_at: "2026-11-05T10:00:00.000Z", outcome: "skipped_overlap" } });
      }
      return Promise.resolve({ delegation_id: "dlg_01HZRUN", status: "queued", run: null });
    },
    schedule_runs() {
      return Promise.resolve(scenario === "historial-vacio" ? [] : RUNS);
    },
  };

  window.__calls = [];
  window.__TAURI__ = {
    core: {
      invoke(command, args) {
        window.__calls.push({ command, args });
        const handler = handlers[command];
        if (!handler) return Promise.reject({ kind: "notFound" });
        return handler(args || {});
      },
    },
    event: {
      listen() {
        return Promise.resolve(() => {});
      },
    },
  };
})();
