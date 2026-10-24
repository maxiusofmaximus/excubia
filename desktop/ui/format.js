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

/// The line that says when a pairing code stops working.
export function expiryText(expiresAt, now = Date.now()) {
  if (now >= expiresAt * 1000) return "Este código ha caducado. Genera otro.";
  const time = new Date(expiresAt * 1000).toLocaleTimeString("es-ES", {
    hour: "2-digit",
    minute: "2-digit",
  });
  return `Caduca a las ${time}.`;
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

export function buildCommand({ publicKey, deviceId, label }) {
  if (!publicKey) return "No disponible: falta la identidad del dispositivo.";
  const safeLabel = String(label || "Escritorio Windows").replace(/"/g, '\\"');
  return `herdr-notify add-device --curve p256 --public-key "${publicKey}" --label "${safeLabel}" --device-id "${deviceId}"`;
}
