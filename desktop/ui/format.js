export const STATUS_LABELS = {
  blocked: "Necesita tu atención",
  working: "Trabajando",
  done: "Terminó",
  idle: "Listo",
  unknown: "Sin clasificar",
};

export function agentTitle(agent) {
  const title = agent.title || agent.agent || "Agente";
  if (/[\\/]/.test(title) && /\.(exe|cmd|bat)$/i.test(title)) {
    return title.split(/[\\/]/).pop();
  }
  return title;
}

export function statusLabel(status) {
  return STATUS_LABELS[status] || STATUS_LABELS.unknown;
}

export function errorMessage(error) {
  if (typeof error === "string") return error;
  if (error && typeof error.message === "string") return error.message;
  try {
    return JSON.stringify(error);
  } catch {
    return String(error);
  }
}

// How long ago `fromMs` was, in the words the screens share: «un momento», «12 min», «3 h», «2 días».
export function elapsedWords(fromMs, now) {
  const minutes = Math.floor((now - fromMs) / 60_000);
  if (minutes < 1) return "un momento";
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} h`;
  const days = Math.floor(hours / 24);
  return days === 1 ? "1 día" : `${days} días`;
}

/// The line that says when a pairing code stops working.
export function expiryText(expiresAt, now = Date.now()) {
  if (now >= expiresAt * 1000) return "Este código ha caducado. Genera otro.";
  const time = new Date(expiresAt * 1000).toLocaleTimeString("es-ES", {
    hour: "2-digit",
    minute: "2-digit",
  });
  return `Caduca a las ${time}.`;
}

/// «Caduca en 2:37» for whole seconds left.
export function countdownText(seconds) {
  const left = Math.max(0, Math.floor(seconds));
  return `Caduca en ${Math.floor(left / 60)}:${String(left % 60).padStart(2, "0")}`;
}

/// An SVG as an image source; the QR is drawn in Rust and never leaves the app.
export function svgDataUrl(svg) {
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

export function subtitle(state) {
  const agents = state.agents || [];
  if (!agents.length) return "Sin agentes en marcha";
  const blocked = agents.filter((agent) => agent.status === "blocked").length;
  const working = agents.filter((agent) => agent.status === "working").length;
  const done = agents.filter((agent) => agent.status === "done").length;
  const idle = agents.filter((agent) => agent.status === "idle").length;
  const unknown = agents.length - blocked - working - done - idle;
  const parts = [];
  if (blocked) parts.push(`${blocked} ${blocked === 1 ? "bloqueado" : "bloqueados"}`);
  if (working) parts.push(`${working} trabajando`);
  if (done) parts.push(`${done} ${done === 1 ? "terminado" : "terminados"}`);
  if (idle) parts.push(`${idle} ${idle === 1 ? "listo" : "listos"}`);
  if (unknown) parts.push(`${unknown} sin clasificar`);
  return `${agents.length} ${agents.length === 1 ? "agente" : "agentes"} · ${parts.join(" · ")}`;
}

/// Test adapters put scenario markers in the prompt. They are not operator
/// text, so they never reach the screen even though they stay in the journal.
export function stripTestMarkers(text) {
  return String(text || "").replace(/\[\[fake:[^\]]*\]\]\s*/g, "");
}

export function stripAnsi(text) {
  return String(text)
    .replace(/\u001B\][^\u0007]*(?:\u0007|\u001B\\)/g, "")
    .replace(/\u001B\[[0-?]*[ -\/]*[@-~]/g, "")
    .replace(/\r\n/g, "\n")
    .replace(/\r/g, "\n");
}

// A host snapshot is only a state with its agent list; without one the screens have nothing to read.
export function readableState(state) {
  return state && Array.isArray(state.agents) ? state : null;
}
