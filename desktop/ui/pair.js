import { app, $, invoke, copyText, setBusy, toast } from "./core.js";
import { showView } from "./nav.js";
import { errorMessage } from "./format.js";
import { renderAgents, renderConnection } from "./agents.js";
import { chatAfterConnect, chatReset } from "./chat.js";
import { installLinkForm } from "./pair-link.js";
import { hydrateLan, installLanPairing } from "./pair-lan.js";
import { t } from "./i18n.js";

export function hydratePairing(boot) {
  const local = boot.mode === "local";
  $("view-pair").classList.toggle("mode-local", local);
  $("local-card").classList.toggle("hidden", !local);
  $("pair-form").classList.toggle("hidden", local);
  $("link-form").classList.toggle("hidden", local);
  $("lan-card").classList.toggle("hidden", !local);
  $("btn-use-local").classList.toggle("hidden", local);
  $("plain-warning").classList.toggle("hidden", !boot.unencrypted);
  hydrateLan(boot.lan_pairing === true);
  renderLocalStatus(boot.host);
  $("device-label").value = boot.label || "";
  $("fingerprint").textContent = boot.fingerprint || t("pair.unavailable");
  $("notifications-toggle").checked = boot.notifications_enabled !== false;
  // Local mode has no code form to read the lede from: it asks for the phone instead.
  $("pair-title").textContent = local ? t("pair.title_local") : t("pair.title");
  $("pair-lede").classList.toggle("hidden", local);
  for (const id of ["btn-pair-top-back", "btn-pair-back"]) {
    $(id).classList.toggle("hidden", !(boot.paired || local));
  }
  if (boot.identity_error) {
    showPairError(t("pair.identity_error", { error: boot.identity_error }));
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
    status.textContent = t("pair.local_running");
  } else if (state === "failed") {
    status.textContent = t("pair.local_failed", { error: host.error });
  } else {
    status.textContent = t("pair.local_starting");
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
  setBusy(button, true, t("pair.pairing"));
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
  $("recovery-code").textContent = outcome.recovery_code || t("pair.unavailable");
  $("recovery-note").textContent = outcome.recovery_code
    ? t("recovery.note")
    : t("pair.no_recovery");
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

// Leaving the local host stops it and unpairs the phones it serves: the button asks twice.
const REMOTE_CONFIRM = t("pair.remote_confirm");
let remoteArmedTimer = null;

async function useRemoteHost() {
  const button = $("btn-use-remote");
  if (!remoteArmedTimer) {
    const label = button.textContent;
    button.textContent = REMOTE_CONFIRM;
    remoteArmedTimer = setTimeout(() => {
      button.textContent = label;
      remoteArmedTimer = null;
    }, 6000);
    return;
  }
  clearTimeout(remoteArmedTimer);
  remoteArmedTimer = null;
  button.textContent = t("pair.use_remote");
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
  // applyHostStatus replaces app.boot.host with a new object, so a changed reference means
  // an hn-host event arrived while the command was pending: it is newer than "starting".
  const before = app.boot?.host;
  try {
    await invoke("use_local_host");
    const host = app.boot?.host !== before ? app.boot.host : { state: "starting" };
    app.boot = { ...(app.boot || {}), mode: "local", host };
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
    toast(enabled ? t("pair.notifications_on") : t("pair.notifications_off"));
  } catch (error) {
    toggle.checked = !enabled;
    toast(errorMessage(error));
  }
}

function backToChat() {
  showView("chat");
  chatAfterConnect();
}

export function installPairHandlers() {
  $("pair-form").addEventListener("submit", doPair);
  installLinkForm(completePairing);
  installLanPairing();
  $("notifications-toggle").addEventListener("change", toggleNotifications);
  for (const id of ["btn-pair-top-back", "btn-pair-back"]) {
    $(id).addEventListener("click", backToChat);
  }
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
    copyText($("recovery-code").textContent, t("pair.code_copied"));
  });
  $("btn-recovery-continue").addEventListener("click", () => {
    showView("chat");
    renderConnection();
    renderAgents();
    chatAfterConnect();
  });
}
