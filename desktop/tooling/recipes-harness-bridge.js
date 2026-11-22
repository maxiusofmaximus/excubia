// Puente simulado para el arnés de Recetas (tooling/recipes-harness.html). Sustituye a
// window.__TAURI__ con respuestas fijas por escenario, con la forma que devuelven los comandos
// de src-tauri/src/recipe_commands.rs. Solo lo usa la página de pruebas, nunca la app.

(function () {
  const params = new URLSearchParams(location.search);
  const scenario = params.get("s") || "lista";

  const RECIPES = [
    {
      id: "rcp_01HZA1", role_id: "audio-editor", title: "Resumen de reunión",
      body: "Haz un resumen de la reunión con los acuerdos, los responsables y las fechas de cada pendiente.",
      uses: 4, created_at: "2026-11-02T10:00:00.000Z", updated_at: "2026-11-04T09:12:00.000Z",
    },
    {
      id: "rcp_01HZA2", role_id: "audio-editor", title: "Limpieza de audio de entrevistas",
      body: "Quita el ruido de fondo, normaliza el volumen a -16 LUFS y exporta en WAV de 48 kHz.\nNo cortes silencios de menos de un segundo.\nDeja una copia sin procesar junto al archivo final.",
      uses: 0, created_at: "2026-11-03T16:40:00.000Z", updated_at: "2026-11-03T16:40:00.000Z",
    },
    {
      id: "rcp_01HZA3", role_id: "audio-editor", title: "Notas de guion del capítulo",
      body: "Escribe tres notas por escena: intención, tono y duración aproximada. Usa frases cortas.",
      uses: 12, created_at: "2026-10-28T08:05:00.000Z", updated_at: "2026-11-01T12:30:00.000Z",
    },
  ];

  const DRAFT = {
    role_id: "audio-editor",
    title: "Limpieza de audio de entrevistas",
    body: "Quita el ruido de fondo, normaliza el volumen a -16 LUFS y exporta en WAV de 48 kHz.",
  };

  const SECRET = {
    kind: "recipeRefused",
    code: "secret_detected",
    reasons: ["parece una clave de API (sk-…)"],
  };

  const RISK = {
    kind: "recipeRefused",
    code: "risk_needs_confirmation",
    reasons: ["pide actuar sin aprobación"],
  };

  function saved(args) {
    return {
      id: "rcp_01HZNEW", role_id: args.role || "audio-editor", title: args.title, body: args.body,
      uses: 0, created_at: "2026-11-05T17:00:00.000Z", updated_at: "2026-11-05T17:00:00.000Z",
    };
  }

  const never = () => new Promise(() => {});

  // Each command answers the way its Rust handler does; a failure is the typed ChatFailure.
  const handlers = {
    recipes() {
      if (scenario === "cargando") return never();
      if (scenario === "error") return Promise.reject({ kind: "offline" });
      if (scenario === "vacio") return Promise.resolve([]);
      return Promise.resolve(RECIPES);
    },
    recipe_draft() {
      return Promise.resolve(DRAFT);
    },
    recipe_create(args) {
      if (scenario === "secreto") return Promise.reject(SECRET);
      if (scenario === "sin-permiso-guardar") return Promise.reject({ kind: "forbidden" });
      if (scenario === "riesgo" && !args.confirmRisks) return Promise.reject(RISK);
      return Promise.resolve(saved(args));
    },
    recipe_update(args) {
      return Promise.resolve(saved(args));
    },
    recipe_delete(args) {
      return Promise.resolve(args.id);
    },
    recipe_run() {
      if (scenario === "ocupado") return Promise.reject({ kind: "agentBusy", retry_after_secs: 30 });
      if (scenario === "sin-permiso") return Promise.reject({ kind: "forbidden" });
      return Promise.resolve({ delegation_id: "dlg_01HZRUN", status: "queued" });
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
