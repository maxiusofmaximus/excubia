import { locale, t } from "./i18n.js";

export const STATUS_LABELS = {
  blocked: "format.status_blocked",
  working: "format.status_working",
  done: "format.status_done",
  idle: "format.status_idle",
  unknown: "format.status_unknown",
};

export function agentTitle(agent) {
  const title = agent.title || agent.agent || t("format.agent_fallback");
  if (/[\\/]/.test(title) && /\.(exe|cmd|bat)$/i.test(title)) {
    return title.split(/[\\/]/).pop();
  }
  // The host titles a lead «Nombre — lead»; the operator reads «Nombre, líder».
  return title.replace(/\s—\slead\b/, t("format.lead_suffix"));
}

export function statusLabel(status) {
  return t(STATUS_LABELS[status] || STATUS_LABELS.unknown);
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
  if (minutes < 1) return t("format.just_now");
  if (minutes < 60) return t("format.minutes", { n: minutes });
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return t("format.hours", { n: hours });
  const days = Math.floor(hours / 24);
  return t(days === 1 ? "format.days.one" : "format.days.other", { n: days });
}

/// The line that says when a pairing code stops working.
export function expiryText(expiresAt, now = Date.now()) {
  if (now >= expiresAt * 1000) return t("format.code_expired");
  const time = new Date(expiresAt * 1000).toLocaleTimeString(locale(), {
    hour: "2-digit",
    minute: "2-digit",
  });
  return t("format.expires_at", { time });
}

/// «Caduca en 2:37» for whole seconds left.
export function countdownText(seconds) {
  const left = Math.max(0, Math.floor(seconds));
  return t("format.countdown", { time: `${Math.floor(left / 60)}:${String(left % 60).padStart(2, "0")}` });
}

/// An SVG as an image source; the QR is drawn in Rust and never leaves the app.
export function svgDataUrl(svg) {
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

/// Test adapters put scenario markers in the prompt. They are not operator
/// text, so they never reach the screen even though they stay in the journal.
export function stripTestMarkers(text) {
  return String(text || "").replace(/\[\[fake:[^\]]*\]\]\s*/g, "");
}

/// The host writes Windows folders with the verbatim prefix `\\?\`. The operator sees the folder.
export function displayPath(path) {
  return String(path || "").replace(/^\\\\\?\\/, "");
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
