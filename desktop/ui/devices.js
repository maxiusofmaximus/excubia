// «Mis dispositivos»: the screen and its entry points. The list loads each time the screen
// opens and after a revoke; nothing polls in the background. Revoking asks first.

import { $, copyText, invoke, showView, toast } from "./core.js";
import { element } from "./chatdom.js";
import { actionRow, button, notice } from "./agenda-ui.js";
import { openContacts } from "./agenda-contacts.js";
import { openNotes } from "./agenda-notes.js";
import { openCalendar } from "./calendar.js";
import { openAlerts } from "./alerts.js";
import {
  REVOKE_WARNING,
  THIS_DEVICE_REASON,
  addedText,
  canRevoke,
  deviceFailureText,
  deviceKind,
  orderDevices,
  permissionsText,
  seenText,
  shortDeviceId,
} from "./devices-model.js";

const LOADING = "Cargando dispositivos…";
const EMPTY = "No hay dispositivos emparejados.";

// A list that starts after a newer one must not overwrite it.
let token = 0;

function setStatus(text, retry = false) {
  const node = $("devices-status");
  node.replaceChildren();
  node.hidden = !text;
  if (!text) return;
  node.append(text);
  if (retry) node.append(" ", button("Reintentar", "btn secondary btn-small", loadDevices));
}

function paint(list) {
  const groups = orderDevices(list.devices, list.this_device);
  $("devices-list").replaceChildren(...groups.active.map((device) => row(device, list.this_device)));
  $("devices-revoked").replaceChildren(...groups.revoked.map((device) => row(device, list.this_device)));
  $("devices-revoked-summary").textContent = `Revocados (${groups.revoked.length})`;
  $("devices-revoked-group").hidden = groups.revoked.length === 0;
  setStatus(list.devices.length ? "" : EMPTY);
}

// A refused grant is a state of the screen, not an error: the device cannot manage devices.
export async function loadDevices() {
  const current = ++token;
  setStatus(LOADING);
  $("devices-list").replaceChildren();
  $("devices-revoked").replaceChildren();
  $("devices-revoked-group").hidden = true;
  try {
    const list = await invoke("devices_list");
    if (current === token) paint(list);
  } catch (failure) {
    if (current === token) setStatus(deviceFailureText(failure), failure.kind !== "forbidden");
  }
}

function row(device, thisDevice) {
  const item = element("li", "agenda-row device-row");
  item.classList.toggle("is-revoked", device.revoked);
  const mine = device.device_id === thisDevice;

  const title = element("p", "device-title");
  title.append(element("strong", null, device.label || "Sin nombre"));
  if (mine) title.append(element("span", "device-tag", "Este equipo"));
  if (device.revoked) title.append(element("span", "device-tag is-revoked", "Revocado"));

  const facts = [deviceKind(device.label), seenText(device.last_seen_at), `Alta ${addedText(device.created_at)}`];
  const id = element("p", "device-id");
  id.append(
    element("code", null, shortDeviceId(device.device_id)),
    " ",
    button("Copiar ID", "btn ghost btn-small", () => copyText(device.device_id, "ID copiado")),
  );
  item.append(
    title,
    element("p", "agenda-row-meta", facts.filter(Boolean).join(" · ")),
    element("p", "agenda-row-meta", permissionsText(device.permissions)),
    id,
  );
  if (!device.revoked) item.append(revokeControl(device, canRevoke(device, thisDevice)));
  if (mine) item.append(notice(THIS_DEVICE_REASON, "agenda-hint"));
  return item;
}

// The revoke button opens a confirmation; the confirmation is the only way to send it.
function revokeControl(device, allowed) {
  const wrap = element("div", "device-revoke");
  const problem = notice("");
  const box = element("div", "agenda-confirm");
  box.setAttribute("role", "alertdialog");
  const ask = button("Revocar", "btn danger btn-small", () => show(true));
  ask.disabled = !allowed;
  const cancel = button("Cancelar", "btn ghost btn-small", () => show(false));
  const confirm = button("Revocar dispositivo", "btn danger btn-small", () => revoke(device, confirm, problem));
  box.append(element("p", null, REVOKE_WARNING), actionRow(cancel, confirm), problem);

  function show(open) {
    ask.hidden = open;
    box.hidden = !open;
    problem.hidden = true;
  }
  show(false);
  wrap.append(ask, box);
  return wrap;
}

async function revoke(device, confirm, problem) {
  confirm.disabled = true;
  try {
    await invoke("devices_revoke", { id: device.device_id });
  } catch (failure) {
    // A device already gone, or a grant that was taken away: the list says which.
    if (failure.kind === "notFound" || failure.kind === "forbidden") {
      await loadDevices();
      if (failure.kind === "notFound") setStatus(deviceFailureText(failure));
      return;
    }
    confirm.disabled = false;
    problem.textContent = deviceFailureText(failure);
    problem.hidden = false;
    return;
  }
  toast("Dispositivo revocado");
  await loadDevices();
}

export function openDevices() {
  showView("devices");
  loadDevices();
}

// The «Dispositivos» tab sits in every top bar next to Contactos, Notas and Calendario.
export function installDevices() {
  for (const id of ["tab-devices", "team-tab-devices", "contacts-tab-devices", "notes-tab-devices", "calendar-tab-devices"]) {
    $(id).addEventListener("click", openDevices);
  }
  $("devices-tab-chat").addEventListener("click", () => showView("chat"));
  $("devices-tab-team").addEventListener("click", () => showView("agents"));
  $("devices-tab-contacts").addEventListener("click", openContacts);
  $("devices-tab-notes").addEventListener("click", openNotes);
  $("devices-tab-calendar").addEventListener("click", openCalendar);
  $("devices-tab-alerts").addEventListener("click", openAlerts);
  $("alerts-tab-devices").addEventListener("click", openDevices);
}
