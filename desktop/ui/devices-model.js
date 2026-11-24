// «Mis dispositivos»: the rules that need no DOM, so tooling/devices-model.test.mjs can run
// them. The host decides what may happen; this only orders, words and explains.

import { relativeDate } from "./agenda-model.js";
import { friendlyError, OFFLINE_TEXT } from "./friendly-error.js";
import { locale, t } from "./i18n.js";

export const THIS_DEVICE_REASON = t("devices.this_device_reason");
export const SERVER_DEVICE_REASON = t("devices.server_reason");
export const REVOKE_WARNING = t("devices.revoke_warning");
export const OWN_REVOKE_WARNING = t("devices.own_revoke_warning");

// The order of the contract; an id the screen does not know is left out, never shown raw.
const PERMISSION_WORDS = {
  read_state: t("devices.perm_read_state"),
  respond: t("devices.perm_respond"),
  send_key: t("devices.perm_send_key"),
  approve: t("devices.perm_approve"),
  admin: t("devices.perm_admin"),
};

export function permissionWords(permissions) {
  return Object.keys(PERMISSION_WORDS)
    .filter((id) => permissions.includes(id))
    .map((id) => PERMISSION_WORDS[id]);
}

export function permissionsText(permissions) {
  const words = permissionWords(permissions);
  return words.length ? words.join(" · ") : t("devices.no_permissions");
}

// Every permission the screen shows, in the contract's order, with whether this device has it.
// The ones it lacks stay visible, so a reader sees what was not given.
export function permissionChips(permissions) {
  return Object.entries(PERMISSION_WORDS).map(([id, label]) => ({
    id,
    label,
    granted: permissions.includes(id),
  }));
}

// The host sends seconds since the epoch, and null for a device never used.
export function seenText(lastSeenSeconds, now = Date.now()) {
  if (lastSeenSeconds == null) return t("devices.never_used");
  const when = relativeDate(new Date(lastSeenSeconds * 1000).toISOString(), now);
  return when === t("agenda.rel_now") ? t("devices.seen_now") : t("devices.seen", { when });
}

export function addedText(createdSeconds) {
  return new Date(createdSeconds * 1000).toLocaleDateString(locale(), {
    day: "numeric",
    month: "short",
    year: "numeric",
  });
}

const PHONE = /\b(android|iphone|ipad|pixel|samsung|xiaomi|móvil|movil|teléfono|telefono|tablet)\b/i;
const COMPUTER = /\b(pc|windows|mac|portátil|portatil|laptop|escritorio|desktop|linux)\b/i;

// Only a label that says it plainly gives a type; anything else shows no type.
export function deviceKind(label) {
  if (PHONE.test(label)) return t("devices.kind_phone");
  if (COMPUTER.test(label)) return t("devices.kind_computer");
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

// This device may revoke itself only in remote mode: the code screen is what brings it back.
export function canRevoke(device, thisDevice, local = false) {
  return !device.revoked && (device.device_id !== thisDevice || !local);
}

// A pending device holds no permission yet; the host decides the rest (ADMIN never comes from here).
export function canApprove(device, thisDevice) {
  return !device.revoked && device.permissions.length === 0 && device.device_id !== thisDevice;
}

export const GRANTABLE_PERMISSIONS = ["read_state", "respond", "send_key", "approve"];
export const DEFAULT_GRANT = ["read_state"];
export const SAS_HINT = t("devices.sas_hint");
export const NO_SAS_HINT = t("devices.no_sas_hint");
export const APPROVE_WARNING = t("devices.approve_warning");

export function grantChoices() {
  return GRANTABLE_PERMISSIONS.map((id) => ({ id, label: PERMISSION_WORDS[id] }));
}

// Six digits read in two groups, as the phone shows them: «097 665». Anything else is shown as it came.
export function groupSas(sas) {
  return /^\d{6}$/.test(sas) ? `${sas.slice(0, 3)} ${sas.slice(3)}` : String(sas);
}

const FAILURE_TEXT = {
  offline: `${OFFLINE_TEXT} ${t("agenda.loaded_visible")}`,
  unauthorized: t("devices.err_unauthorized"),
  forbidden: t("devices.err_forbidden"),
  notFound: t("devices.err_not_found"),
  notPending: t("devices.err_not_pending"),
  ownServer: t("devices.err_own_server"),
};

// The operator's words for a failure: the screen's own line, else the one translator.
export function deviceFailureText(failure) {
  return FAILURE_TEXT[failure?.kind] || friendlyError(failure);
}
