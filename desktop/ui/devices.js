// «Mis dispositivos»: the screen and its entry points. The list loads each time the screen
// opens and after a revoke; nothing polls in the background. Revoking asks first.

import { $, app, copyText, invoke, toast } from "./core.js";
import { showView } from "./nav.js";
import { forgetPairing } from "./pair.js";
import { element } from "./chatdom.js";
import { actionRow, button, notice } from "./agenda-ui.js";
import { emptyState, errorState } from "./empty-state.js";
import {
  APPROVE_WARNING,
  DEFAULT_GRANT,
  NO_SAS_HINT,
  OWN_REVOKE_WARNING,
  REVOKE_WARNING,
  SAS_HINT,
  THIS_DEVICE_REASON,
  addedText,
  canApprove,
  canRevoke,
  deviceFailureText,
  deviceKind,
  grantChoices,
  groupSas,
  orderDevices,
  permissionsText,
  seenText,
  shortDeviceId,
} from "./devices-model.js";

const LOADING = "Cargando dispositivos…";
const EMPTY = "No hay dispositivos emparejados.";

// A list that starts after a newer one must not overwrite it.
let token = 0;

function setStatus(content) {
  const node = $("devices-status");
  node.replaceChildren();
  node.hidden = !content;
  if (content) node.append(content);
}

function emptyNode() {
  return emptyState({ icon: "devices", text: EMPTY, action: "Actualizar" }, loadDevices);
}

function paint(list) {
  const groups = orderDevices(list.devices, list.this_device);
  $("devices-list").replaceChildren(...groups.active.map((device) => row(device, list.this_device)));
  $("devices-revoked").replaceChildren(...groups.revoked.map((device) => row(device, list.this_device)));
  $("devices-revoked-summary").textContent = `Revocados (${groups.revoked.length})`;
  $("devices-revoked-group").hidden = groups.revoked.length === 0;
  setStatus(list.devices.length ? "" : emptyNode());
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
    if (current !== token) return;
    const text = deviceFailureText(failure);
    setStatus(failure?.kind === "forbidden" ? text : errorState(text, loadDevices));
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

  const facts = [deviceKind(device.label || ""),seenText(device.last_seen_at), `Alta ${addedText(device.created_at)}`];
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
  if (canApprove(device, thisDevice)) item.append(approveBlock(device));
  if (!device.revoked) item.append(revokeControl(device, canRevoke(device, thisDevice, app.boot?.mode === "local"), mine));
  if (mine) item.append(notice(THIS_DEVICE_REASON, "agenda-hint"));
  return item;
}

// The first button opens a confirmation; the confirmation is the only way to send the action.
function confirmControl({ askLabel, askClass, sendLabel, warning, allowed = true, send }) {
  const wrap = element("div", "device-revoke");
  const problem = notice("");
  const box = element("div", "agenda-confirm");
  box.setAttribute("role", "alertdialog");
  const ask = button(askLabel, askClass, () => show(true));
  ask.disabled = !allowed;
  const cancel = button("Cancelar", "btn ghost btn-small", () => show(false));
  const confirm = button(sendLabel, askClass, () => send(confirm, problem));
  box.append(element("p", null, warning), actionRow(cancel, confirm), problem);

  function show(open) {
    ask.hidden = open;
    box.hidden = !open;
    problem.hidden = true;
  }
  show(false);
  wrap.append(ask, box);
  return wrap;
}

function revokeControl(device, allowed, mine) {
  return confirmControl({
    askLabel: mine ? "Revocar este equipo" : "Revocar",
    askClass: "btn danger btn-small",
    sendLabel: mine ? "Revocar este equipo" : "Revocar dispositivo",
    warning: mine ? OWN_REVOKE_WARNING : REVOKE_WARNING,
    allowed,
    send: (confirm, problem) =>
      mine
        ? revokeThisDevice(device, confirm, problem)
        : runAction(device, confirm, problem, "devices_revoke", {}, "Dispositivo revocado"),
  });
}

// Revoking this device ends the session it is using: no list to reload, the code screen instead.
async function revokeThisDevice(device, confirm, problem) {
  confirm.disabled = true;
  try {
    await invoke("devices_revoke", { id: device.device_id });
  } catch (failure) {
    confirm.disabled = false;
    problem.textContent = deviceFailureText(failure);
    problem.hidden = false;
    return;
  }
  await forgetPairing();
  toast("Este equipo ya no está vinculado. Introduce un código nuevo del PC.");
}

// A pending device: its verification string to compare, the permissions it gets, and the approval.
function approveBlock(device) {
  const block = element("div", "device-approve");
  block.append(element("p", "device-approve-title", "Pendiente de aprobación"));
  if (device.sas) block.append(element("p", "device-sas", groupSas(device.sas)), element("p", "agenda-row-meta", SAS_HINT));
  else block.append(element("p", "agenda-row-meta", NO_SAS_HINT));

  const group = element("div", "device-permissions");
  group.append(
    ...grantChoices().map(({ id, label }) => {
      const input = element("input");
      input.type = "checkbox";
      input.value = id;
      input.checked = DEFAULT_GRANT.includes(id);
      const choice = element("label", "device-permission");
      choice.append(input, " ", label);
      return choice;
    }),
  );
  block.append(
    group,
    confirmControl({
      askLabel: "Aprobar",
      askClass: "btn secondary btn-small",
      sendLabel: "Aprobar dispositivo",
      warning: APPROVE_WARNING,
      send: (confirm, problem) => {
        const permissions = checkedPermissions(group);
        if (!permissions.length) {
          problem.textContent = "Marca al menos un permiso.";
          problem.hidden = false;
          return;
        }
        return runAction(device, confirm, problem, "devices_grant", { permissions }, "Dispositivo aprobado");
      },
    }),
  );
  return block;
}

function checkedPermissions(group) {
  return [...group.querySelectorAll("input:checked")].map((input) => input.value);
}

// A device gone, no longer pending or no longer administrable is a state of the list: the list
// reloads and says why. Any other failure stays by the confirmation, for another try.
const STATE_FAILURES = ["notFound", "notPending", "forbidden"];

async function runAction(device, confirm, problem, command, args, done) {
  confirm.disabled = true;
  try {
    await invoke(command, { id: device.device_id, ...args });
  } catch (failure) {
    if (STATE_FAILURES.includes(failure.kind)) {
      await loadDevices();
      if (failure.kind !== "forbidden") setStatus(deviceFailureText(failure));
      return;
    }
    confirm.disabled = false;
    problem.textContent = deviceFailureText(failure);
    problem.hidden = false;
    return;
  }
  toast(done);
  await loadDevices();
}

export function openDevices() {
  showView("devices");
  loadDevices();
}
