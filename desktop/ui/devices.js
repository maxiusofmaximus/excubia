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
  SERVER_DEVICE_REASON,
  THIS_DEVICE_REASON,
  addedText,
  canApprove,
  canRevoke,
  deviceFailureText,
  deviceKind,
  grantChoices,
  groupSas,
  orderDevices,
  permissionChips,
  seenText,
  shortDeviceId,
} from "./devices-model.js";
import { t } from "./i18n.js";

const LOADING = t("devices.loading");
const EMPTY = t("devices.empty");

// A list that starts after a newer one must not overwrite it.
let token = 0;

function setStatus(content) {
  const node = $("devices-status");
  node.replaceChildren();
  node.hidden = !content;
  if (content) node.append(content);
}

function emptyNode() {
  return emptyState({ icon: "devices", text: EMPTY, action: t("agents.refresh") }, loadDevices);
}

function paint(list) {
  const groups = orderDevices(list.devices, list.this_device);
  $("devices-list").replaceChildren(...groups.active.map((device) => row(device, list.this_device)));
  $("devices-revoked").replaceChildren(...groups.revoked.map((device) => row(device, list.this_device)));
  $("devices-revoked-summary").textContent = t("devices.revoked_count", { n: groups.revoked.length });
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
  title.append(element("strong", null, device.label || t("devices.no_name")));
  if (mine) title.append(element("span", "device-tag", t("devices.this_device_tag")));
  if (device.revoked) title.append(element("span", "device-tag is-revoked", t("devices.revoked_tag")));

  const facts = [deviceKind(device.label || ""), seenText(device.last_seen_at), t("devices.added", { date: addedText(device.created_at) })];
  const id = element("p", "device-id");
  id.append(
    element("code", null, shortDeviceId(device.device_id)),
    " ",
    button(t("devices.copy_id"), "btn ghost btn-small", () => copyText(device.device_id, t("devices.id_copied"))),
  );
  item.append(
    title,
    element("p", "agenda-row-meta", facts.filter(Boolean).join(" · ")),
    permissionRow(device),
    id,
  );
  const local = app.boot?.mode === "local";
  if (canApprove(device, thisDevice)) item.append(approveBlock(device));
  // The action is not drawn where the host would refuse it; the reason line below says why.
  if (canRevoke(device, thisDevice, local)) item.append(revokeControl(device, mine));
  if (mine) item.append(notice(local ? SERVER_DEVICE_REASON : THIS_DEVICE_REASON, "agenda-hint"));
  return item;
}

// The first button opens a confirmation; the confirmation is the only way to send the action.
function confirmControl({ askLabel, askClass, sendLabel, warning, send }) {
  const wrap = element("div", "device-revoke");
  const problem = notice("");
  const box = element("div", "agenda-confirm");
  box.setAttribute("role", "alertdialog");
  const ask = button(askLabel, askClass, () => show(true));
  const cancel = button(t("common.cancel"), "btn ghost btn-small", () => show(false));
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

function revokeControl(device, mine) {
  return confirmControl({
    askLabel: mine ? t("devices.revoke_this") : t("devices.revoke"),
    askClass: "btn danger btn-small",
    sendLabel: mine ? t("devices.revoke_this") : t("devices.revoke_device"),
    warning: mine ? OWN_REVOKE_WARNING : REVOKE_WARNING,
    send: (confirm, problem) =>
      mine
        ? revokeThisDevice(device, confirm, problem)
        : runAction(device, confirm, problem, "devices_revoke", {}, t("devices.revoked_done")),
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
  toast(t("devices.own_unlinked"));
}

// The permissions of a device as chips: granted ✓ on a filled chip, the others «–» as an outline.
// The text carries the difference too, so the state never rests on the colour alone.
function permissionRow(device) {
  const row = element("div", "perm-chips");
  for (const { id, label, granted } of permissionChips(device.permissions)) {
    const chip = element("span", granted ? "perm-chip is-granted" : "perm-chip", `${granted ? "✓" : "–"} ${label}`);
    chip.dataset.permission = id;
    row.append(chip);
  }
  return row;
}

// A pending device: its verification string to compare, the permissions it gets, and the approval.
function approveBlock(device) {
  const block = element("div", "device-approve");
  block.append(element("p", "device-approve-title", t("devices.pending")));
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
      askLabel: t("devices.perm_approve"),
      askClass: "btn secondary btn-small",
      sendLabel: t("devices.approve_device"),
      warning: APPROVE_WARNING,
      send: (confirm, problem) => {
        const permissions = checkedPermissions(group);
        if (!permissions.length) {
          problem.textContent = t("devices.pick_permission");
          problem.hidden = false;
          return;
        }
        return runAction(device, confirm, problem, "devices_grant", { permissions }, t("devices.approved_done"));
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
