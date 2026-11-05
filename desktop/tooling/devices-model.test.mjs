// Run with: node --test tooling/devices-model.test.mjs
// «Mis dispositivos»: the order of the list, the words of the permissions and of the
// last use, the type a label shows, the cut id, who may be revoked and the words of
// each failure.
import { test } from "node:test";
import assert from "node:assert/strict";

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
  permissionWords,
  permissionsText,
  seenText,
  shortDeviceId,
} from "../ui/devices-model.js";

const NOW = Date.parse("2026-10-29T12:00:00.000Z");
const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;
// The host counts seconds since the epoch; «ms ago» becomes that.
const secondsAgo = (ms) => (NOW - ms) / 1000;

function device(id, extra = {}) {
  return {
    device_id: id,
    label: id,
    created_at: 1730000000,
    last_seen_at: secondsAgo(MINUTE),
    revoked: false,
    permissions: ["read_state"],
    ...extra,
  };
}

test("this device comes first, then the active by last use, then the revoked", () => {
  const groups = orderDevices(
    [
      device("dev-old", { last_seen_at: secondsAgo(5 * HOUR) }),
      device("dev-mine", { last_seen_at: secondsAgo(3 * DAY) }),
      device("dev-new", { last_seen_at: secondsAgo(MINUTE) }),
      device("dev-never", { last_seen_at: null }),
      device("dev-gone", { revoked: true, last_seen_at: secondsAgo(MINUTE) }),
    ],
    "dev-mine",
  );
  assert.deepEqual(
    groups.active.map((entry) => entry.device_id),
    ["dev-mine", "dev-new", "dev-old", "dev-never"],
  );
  assert.deepEqual(groups.revoked.map((entry) => entry.device_id), ["dev-gone"]);
});

test("permissions read as words in the contract's order, and unknown ids are left out", () => {
  assert.deepEqual(
    permissionWords(["admin", "read_state", "surprise", "approve", "send_key", "respond"]),
    ["Leer estado", "Responder", "Enviar teclas", "Aprobar", "Administrar"],
  );
  assert.deepEqual(permissionWords([]), []);
  assert.equal(permissionsText([]), "Sin permisos");
  assert.equal(permissionsText(["read_state", "respond"]), "Leer estado · Responder");
});

test("the last use reads as a relative time, and a device never used says so", () => {
  assert.equal(seenText(secondsAgo(30_000), NOW), "Visto ahora");
  assert.equal(seenText(secondsAgo(5 * MINUTE), NOW), "Visto hace 5 min");
  assert.equal(seenText(secondsAgo(DAY), NOW), "Visto ayer");
  assert.equal(seenText(null, NOW), "Sin uso registrado");
});

test("the date of entry is written as a day", () => {
  assert.match(addedText(1730000000), /2024/);
});

test("the type comes from the label only when the label says it plainly", () => {
  assert.equal(deviceKind("Pixel 9"), "Móvil");
  assert.equal(deviceKind("iPhone de Ana"), "Móvil");
  assert.equal(deviceKind("PC de escritorio"), "Ordenador");
  assert.equal(deviceKind("Ana"), "");
  assert.equal(deviceKind(""), "");
});

test("a device id is shown cut, and kept whole when it is already short", () => {
  assert.equal(shortDeviceId("dev-0123456789abcdef"), "dev-0123…cdef");
  assert.equal(shortDeviceId("dev-1"), "dev-1");
});

test("this device cannot revoke itself, and a revoked one has nothing to revoke", () => {
  assert.equal(canRevoke(device("dev-a"), "dev-b"), true);
  assert.equal(canRevoke(device("dev-a", { revoked: true }), "dev-b"), false);
  // This device may revoke itself in remote mode, where the code screen brings it back; not in the local one.
  assert.equal(canRevoke(device("dev-a"), "dev-a"), true);
  assert.equal(canRevoke(device("dev-a"), "dev-a", true), false);
  assert.equal(canRevoke(device("dev-b"), "dev-a", true), true);
  assert.equal(canRevoke(device("dev-a", { revoked: true }), "dev-a"), false);
  assert.equal(
    OWN_REVOKE_WARNING,
    "Este equipo dejará de estar vinculado y tendrás que emparejarlo de nuevo. ¿Continuar?",
  );
  assert.match(THIS_DEVICE_REASON, /Olvidar emparejamiento/);
  assert.equal(
    REVOKE_WARNING,
    "Este dispositivo dejará de poder entrar y no podrá volver sin emparejarse de nuevo.",
  );
});

test("each failure has its words, and a refused grant is a state rather than an error", () => {
  assert.equal(deviceFailureText({ kind: "forbidden" }), "Este equipo no puede administrar dispositivos.");
  assert.equal(deviceFailureText({ kind: "notFound" }), "Ya no existe.");
  assert.match(deviceFailureText({ kind: "unauthorized" }), /Vuelve a emparejarlo/);
  assert.match(deviceFailureText({ kind: "offline" }), /Sin conexión/);
  assert.equal(
    deviceFailureText({ kind: "refused", status: 500 }),
    "Tu PC no aceptó la petición. Vuelve a vincularlo si sigue pasando.",
  );
  assert.equal(deviceFailureText({ kind: "protocol" }), "Tu PC no aceptó la petición. Vuelve a vincularlo si sigue pasando.");
  assert.equal(deviceFailureText(undefined), "No se pudo cargar. Inténtalo de nuevo.");
  assert.equal(deviceFailureText({ kind: "notPending" }), "Ya tenía permisos.");
});

test("a six-digit chain reads in two groups, and anything else is shown as it came", () => {
  assert.equal(groupSas("097665"), "097 665");
  assert.equal(groupSas("827242"), "827 242");
  assert.equal(groupSas("12 34"), "12 34");
  assert.equal(groupSas(""), "");
});

test("only a device that holds no permission, is not revoked and is not this one can be approved", () => {
  assert.equal(canApprove(device("dev-a", { permissions: [] }), "dev-b"), true);
  assert.equal(canApprove(device("dev-a"), "dev-b"), false);
  assert.equal(canApprove(device("dev-a", { permissions: [], revoked: true }), "dev-b"), false);
  assert.equal(canApprove(device("dev-a", { permissions: [] }), "dev-a"), false);
});

test("the grant offers four permissions without admin, and reads read_state by default", () => {
  assert.deepEqual(
    grantChoices(),
    [
      { id: "read_state", label: "Leer estado" },
      { id: "respond", label: "Responder" },
      { id: "send_key", label: "Enviar teclas" },
      { id: "approve", label: "Aprobar" },
    ],
  );
  assert.equal(grantChoices().some((choice) => choice.id === "admin"), false);
  assert.deepEqual(DEFAULT_GRANT, ["read_state"]);
});

test("the approval texts say what to compare, or that there is nothing to compare", () => {
  assert.equal(SAS_HINT, "Compárala con la que muestra el móvil. Si no coincide, revoca el dispositivo.");
  assert.equal(NO_SAS_HINT, "No hay cadena de verificación: aprueba solo si estás seguro de que es tu móvil.");
  assert.equal(APPROVE_WARNING, "Se concederán los permisos marcados a este dispositivo.");
});
