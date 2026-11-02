// «Mis dispositivos»: the rules that need no DOM, so tooling/devices-model.test.mjs can run
// them. The host decides what may happen; this only orders, words and explains.

import { relativeDate } from "./agenda-model.js";

export const THIS_DEVICE_REASON = "Para salir de este equipo, usa Olvidar emparejamiento.";
export const REVOKE_WARNING =
  "Este dispositivo dejará de poder entrar y no podrá volver sin emparejarse de nuevo.";

// The order of the contract; an id the screen does not know is left out, never shown raw.
const PERMISSION_WORDS = {
  read_state: "Leer estado",
  respond: "Responder",
  send_key: "Enviar teclas",
  approve: "Aprobar",
  admin: "Administrar",
};

export function permissionWords(permissions) {
  return Object.keys(PERMISSION_WORDS)
    .filter((id) => permissions.includes(id))
    .map((id) => PERMISSION_WORDS[id]);
}

export function permissionsText(permissions) {
  const words = permissionWords(permissions);
  return words.length ? words.join(" · ") : "Sin permisos";
}

// The host sends seconds since the epoch, and null for a device never used.
export function seenText(lastSeenSeconds, now = Date.now()) {
  if (lastSeenSeconds == null) return "Sin uso registrado";
  const when = relativeDate(new Date(lastSeenSeconds * 1000).toISOString(), now);
  return when === "ahora" ? "Visto ahora" : `Visto ${when}`;
}

export function addedText(createdSeconds) {
  return new Date(createdSeconds * 1000).toLocaleDateString("es-ES", {
    day: "numeric",
    month: "short",
    year: "numeric",
  });
}

const PHONE = /\b(android|iphone|ipad|pixel|samsung|xiaomi|móvil|movil|teléfono|telefono|tablet)\b/i;
const COMPUTER = /\b(pc|windows|mac|portátil|portatil|laptop|escritorio|desktop|linux)\b/i;

// Only a label that says it plainly gives a type; anything else shows no type.
export function deviceKind(label) {
  if (PHONE.test(label)) return "Móvil";
  if (COMPUTER.test(label)) return "Ordenador";
  return "";
}

// The id as the screen shows it; the whole id goes to the clipboard only.
export function shortDeviceId(deviceId) {
  if (deviceId.length <= 12) return deviceId;
  return `${deviceId.slice(0, 8)}…${deviceId.slice(-4)}`;
}

// Active devices: this one first, then by last use (never used last). Revoked apart.
export function orderDevices(devices, thisDevice) {
  // A device never used counts as epoch zero, so it sorts after every real use.
  const byLastUse = (a, b) => (b.last_seen_at ?? 0) - (a.last_seen_at ?? 0);
  const active = devices.filter((device) => !device.revoked);
  const revoked = devices.filter((device) => device.revoked).sort(byLastUse);
  active.sort((a, b) => {
    const mineFirst = Number(b.device_id === thisDevice) - Number(a.device_id === thisDevice);
    return mineFirst || byLastUse(a, b);
  });
  return { active, revoked };
}

// This device is never revoked from here: that would lock the screen out of the host.
export function canRevoke(device, thisDevice) {
  return !device.revoked && device.device_id !== thisDevice;
}

// A pending device holds no permission yet; the host decides the rest (ADMIN never comes from here).
export function canApprove(device, thisDevice) {
  return !device.revoked && device.permissions.length === 0 && device.device_id !== thisDevice;
}

export const GRANTABLE_PERMISSIONS = ["read_state", "respond", "send_key", "approve"];
export const DEFAULT_GRANT = ["read_state"];
export const SAS_HINT = "Compárala con la que muestra el móvil. Si no coincide, revoca el dispositivo.";
export const NO_SAS_HINT = "No hay cadena de verificación: aprueba solo si estás seguro de que es tu móvil.";
export const APPROVE_WARNING = "Se concederán los permisos marcados a este dispositivo.";

export function grantChoices() {
  return GRANTABLE_PERMISSIONS.map((id) => ({ id, label: PERMISSION_WORDS[id] }));
}

// Six digits read in two groups, as the phone shows them: «097 665». Anything else is shown as it came.
export function groupSas(sas) {
  return /^\d{6}$/.test(sas) ? `${sas.slice(0, 3)} ${sas.slice(3)}` : String(sas);
}

const FAILURE_TEXT = {
  offline: "Sin conexión con el host. Lo cargado sigue visible.",
  unauthorized: "El PC ya no confía en este dispositivo. Vuelve a emparejarlo.",
  forbidden: "Este equipo no puede administrar dispositivos.",
  notFound: "Ya no existe.",
  notPending: "Ya tenía permisos.",
};

// The operator's words for a failure. A refusal names its status; anything else is generic.
export function deviceFailureText(failure) {
  if (failure?.kind === "refused") return `El PC rechazó la petición (${failure.status}).`;
  return FAILURE_TEXT[failure?.kind] || "No se pudo completar la operación.";
}
