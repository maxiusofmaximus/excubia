import { app, $, invoke, copyText, setBusy, toast } from "./core.js";
import { showView } from "./nav.js";
import { errorMessage } from "./format.js";
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
  $("device-label").value = boot.label || "";
  $("fingerprint").textContent = boot.fingerprint || "No disponible";
  $("notifications-toggle").checked = boot.notifications_enabled !== false;
  $("btn-pair-back").classList.toggle("hidden", !(boot.paired || local));
  if (boot.identity_error) {
    showPairError(`No se pudo abrir el almacén seguro: ${boot.identity_error}`);
    $("btn-pair").disabled = true;
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
    status.textContent = `Excubia está en marcha en este equipo. Alfred y el equipo aparecerán en cuanto haya datos.`;
  } else if (state === "failed") {
    status.textContent = `Excubia no arrancó en este equipo: ${host.error}`;
  } else {
    status.textContent = "Arrancando Excubia en este equipo…";
  }
}

export function showPairError(message) {
  const element = $("pair-error");
  element.textContent = message;
  element.classList.toggle("hidden", !message);
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
    : "El PC no devolvió un código de recuperación.";
  $("btn-copy-recovery").disabled = !outcome.recovery_code;
  showView("recovery");
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
