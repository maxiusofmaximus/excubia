import { app, $, invoke, copyText, setBusy, showView, toast } from "./core.js";
import { buildCommand, errorMessage } from "./format.js";
import { renderAgents, renderConnection } from "./agents.js";
import { chatAfterConnect, chatReset } from "./chat.js";
import { installLinkForm } from "./pair-link.js";
import { hydrateLan, installLanPairing } from "./pair-lan.js";

export function hydratePairing(boot) {
  const local = boot.mode === "local";
  $("view-pair").classList.toggle("mode-local", local);
  $("local-card").classList.toggle("hidden", !local);
  $("pair-form").classList.toggle("hidden", local);
  $("link-form").classList.toggle("hidden", local);
  $("lan-card").classList.toggle("hidden", local);
  $("btn-use-local").classList.toggle("hidden", local);
  $("plain-warning").classList.toggle("hidden", !boot.unencrypted);
  hydrateLan(boot.lan_pairing === true);
  renderLocalStatus(boot.host);
  $("gateway-url").value = boot.gateway_url || "";
  $("device-id").value = boot.device_id || "";
  $("device-label").value = boot.label || "";
  $("fingerprint").textContent = boot.fingerprint || "No disponible";
  $("notifications-toggle").checked = boot.notifications_enabled !== false;
  $("btn-pair-back").classList.toggle("hidden", !(boot.paired || local));
  refreshCommand();
  if (boot.identity_error) {
    showPairError(`No se pudo abrir el almacén seguro: ${boot.identity_error}`);
    $("btn-pair").disabled = true;
    $("btn-save-advanced").disabled = true;
  }
}

export function applyHostStatus(host) {
  app.boot = { ...(app.boot || {}), host };
  renderLocalStatus(host);
}

function renderLocalStatus(host) {
  const status = $("local-status");
  const restart = $("btn-restart-host");
  const state = host ? host.state : "disabled";
  restart.classList.toggle("hidden", state !== "failed");
  if (state === "ready") {
    status.textContent = `Host local en marcha en 127.0.0.1:${host.port}. Alfred y el equipo aparecerán en cuanto haya datos.`;
  } else if (state === "failed") {
    status.textContent = `El host local no arrancó: ${host.error}`;
  } else {
    status.textContent = "Arrancando el host local…";
  }
}

export function openSettings() {
  showView("pair");
}

export function showPairError(message) {
  const element = $("pair-error");
  element.textContent = message;
  element.classList.toggle("hidden", !message);
}

function commandText() {
  return buildCommand({
    publicKey: app.boot ? app.boot.public_key : null,
    deviceId: $("device-id").value.trim(),
    label: $("device-label").value.trim(),
  });
}

function refreshCommand() {
  $("command").textContent = commandText();
}

async function doPair(event) {
  event.preventDefault();
  const code = $("pair-code").value.trim();
  const button = $("btn-pair");
  showPairError("");
  setBusy(button, true, "Emparejando…");
  try {
    completePairing(await invoke("pair", { code, label: $("device-label").value }));
  } catch (error) {
    showPairError(errorMessage(error));
  } finally {
    setBusy(button, false);
  }
}

/// Shared by the code and the link: the device is paired, so show its recovery code.
function completePairing(outcome) {
  app.boot = {
    ...(app.boot || {}),
    paired: true,
    device_id: outcome.device_id,
    label: outcome.label,
  };
  $("recovery-code").textContent = outcome.recovery_code || "No disponible";
  $("recovery-note").textContent = outcome.recovery_code
    ? "No se volverá a mostrar."
    : "El gateway no devolvió un código de recuperación.";
  $("btn-copy-recovery").disabled = !outcome.recovery_code;
  showView("recovery");
}

async function saveAdvanced() {
  const button = $("btn-save-advanced");
  const note = $("advanced-note");
  note.classList.remove("is-error");
  setBusy(button, true, "Guardando…");
  try {
    const outcome = await invoke("save_advanced", {
      gatewayUrl: $("gateway-url").value,
      deviceId: $("device-id").value,
      label: $("device-label").value,
    });
    note.textContent = outcome.message;
    note.classList.toggle("is-error", !outcome.verified);
    if (outcome.verified) {
      app.boot = {
        ...(app.boot || {}),
        paired: true,
        gateway_url: $("gateway-url").value.trim(),
        device_id: $("device-id").value.trim(),
        label: $("device-label").value.trim(),
      };
      app.connection = { kind: "connecting" };
      chatReset();
      showView("chat");
      renderConnection();
      renderAgents();
      chatAfterConnect();
    }
  } catch (error) {
    note.textContent = errorMessage(error);
    note.classList.add("is-error");
  } finally {
    setBusy(button, false);
  }
}

export async function forgetPairing() {
  try {
    await invoke("forget_pairing");
    app.boot = { ...(app.boot || {}), paired: false };
    app.state = null;
    app.connection = { kind: "idle" };
    chatReset();
    $("pair-code").value = "";
    showPairError("");
    showView("pair");
    hydratePairing(app.boot);
    renderConnection();
    $("pair-code").focus();
  } catch (error) {
    toast(errorMessage(error));
  }
}

async function useRemoteHost() {
  try {
    await invoke("use_remote_host");
    app.boot = { ...(app.boot || {}), mode: "remote" };
    app.state = null;
    app.connection = { kind: "idle" };
    chatReset();
    hydratePairing(app.boot);
    showPairError("");
  } catch (error) {
    toast(errorMessage(error));
  }
}

async function useLocalHost() {
  try {
    await invoke("use_local_host");
    app.boot = { ...(app.boot || {}), mode: "local", host: { state: "starting" } };
    app.state = null;
    app.connection = { kind: "idle" };
    chatReset();
    hydratePairing(app.boot);
    showView("chat");
    renderAgents();
    chatAfterConnect();
  } catch (error) {
    toast(errorMessage(error));
  }
}

async function restartHost() {
  app.boot = { ...(app.boot || {}), host: { state: "starting" } };
  hydratePairing(app.boot);
  try {
    await invoke("restart_host");
  } catch (error) {
    toast(errorMessage(error));
  }
}

async function toggleNotifications() {
  const toggle = $("notifications-toggle");
  const enabled = toggle.checked;
  try {
    await invoke("set_notifications", { enabled });
    toast(enabled ? "Avisos activados" : "Avisos desactivados");
  } catch (error) {
    toggle.checked = !enabled;
    toast(errorMessage(error));
  }
}

export function installPairHandlers() {
  $("pair-form").addEventListener("submit", doPair);
  installLinkForm(completePairing);
  installLanPairing();
  $("notifications-toggle").addEventListener("change", toggleNotifications);
  $("btn-settings").addEventListener("click", openSettings);
  $("btn-settings-team").addEventListener("click", openSettings);
  $("btn-pair-back").addEventListener("click", () => {
    showView("chat");
    chatAfterConnect();
  });
  $("btn-use-remote").addEventListener("click", useRemoteHost);
  $("btn-use-local").addEventListener("click", useLocalHost);
  $("btn-restart-host").addEventListener("click", restartHost);
  $("pair-code").addEventListener("input", (event) => {
    const input = event.target;
    const start = input.selectionStart;
    input.value = input.value.toUpperCase();
    input.setSelectionRange(start, start);
  });
  for (const id of ["device-id", "device-label"]) {
    $(id).addEventListener("input", refreshCommand);
  }
  $("btn-copy-command").addEventListener("click", () => {
    copyText(commandText(), "Comando copiado");
  });
  $("btn-save-advanced").addEventListener("click", saveAdvanced);
  $("btn-copy-recovery").addEventListener("click", () => {
    copyText($("recovery-code").textContent, "Código copiado");
  });
  $("btn-recovery-continue").addEventListener("click", () => {
    showView("chat");
    renderConnection();
    renderAgents();
    chatAfterConnect();
  });
}
