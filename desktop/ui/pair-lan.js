import { $, invoke, setBusy, showView } from "./core.js";
import { errorMessage, expiryText, svgDataUrl } from "./format.js";
import { IDLE, leaveWindow, qrMessage, qrVisible, showQr as startQr, tickQr } from "./pair-qr-model.js";

let expiryTimer = null;
// The QR without an address (pake) is the one that comes down on its own; null while the v1 QR is up.
let pake = null;
let pakeTimer = null;

/// The LAN switch decides whether the host listens for phones at all.
export function hydrateLan(enabled) {
  $("lan-toggle").checked = enabled;
  $("btn-show-lan").disabled = !enabled;
  $("btn-show-pake").disabled = !enabled;
}

export function installLanPairing() {
  $("lan-toggle").addEventListener("change", toggleLan);
  $("btn-show-lan").addEventListener("click", (event) => showQr(event.currentTarget));
  $("btn-show-pake").addEventListener("click", (event) => showPakeQr(event.currentTarget));
  $("btn-lan-new").addEventListener("click", (event) =>
    pake ? showPakeQr(event.currentTarget) : showQr(event.currentTarget),
  );
  $("btn-lan-back").addEventListener("click", () => {
    dropPake();
    showView("pair");
  });
  window.addEventListener("blur", leave);
  document.addEventListener("visibilitychange", () => document.hidden && leave());
}

async function toggleLan() {
  const toggle = $("lan-toggle");
  const enabled = toggle.checked;
  showLanError("");
  try {
    await invoke("set_lan_pairing", { enabled });
    hydrateLan(enabled);
  } catch (error) {
    toggle.checked = !enabled;
    showLanError(errorMessage(error));
  }
}

async function showQr(button) {
  showLanError("");
  setBusy(button, true, "Preparando…");
  try {
    const pairing = await invoke("lan_pairing");
    dropPake();
    $("lan-qr").classList.remove("hidden");
    $("lan-qr").src = svgDataUrl(pairing.svg);
    showView("lan");
    countDown(pairing.expires_at);
  } catch (error) {
    showLanError(errorMessage(error));
  } finally {
    setBusy(button, false);
  }
}

async function showPakeQr(button) {
  showLanError("");
  setBusy(button, true, "Preparando…");
  try {
    const pairing = await invoke("pake_pairing");
    clearTimeout(expiryTimer);
    clearInterval(pakeTimer);
    pake = startQr(pairing.expires_at, Date.now());
    $("lan-qr").src = svgDataUrl(pairing.svg);
    showView("lan");
    renderPake();
    pakeTimer = setInterval(() => {
      pake = tickQr(pake, Date.now());
      renderPake();
    }, 1000);
  } catch (error) {
    showLanError(errorMessage(error));
  } finally {
    setBusy(button, false);
  }
}

/// The QR is on the page only while the model says it is visible; otherwise its source is gone.
function renderPake() {
  const qr = $("lan-qr");
  const visible = qrVisible(pake);
  if (!visible) {
    qr.removeAttribute("src");
    clearInterval(pakeTimer);
  }
  qr.classList.toggle("hidden", !visible);
  const expiry = $("lan-expiry");
  expiry.textContent = qrMessage(pake);
  expiry.classList.toggle("is-error", !visible);
}

function leave() {
  if (!pake) return;
  pake = leaveWindow(pake);
  renderPake();
}

function dropPake() {
  if (!pake) return;
  pake = IDLE;
  renderPake();
  pake = null;
}

/// Says when the code stops working, and says so once it has.
function countDown(expiresAt) {
  clearTimeout(expiryTimer);
  const expiry = $("lan-expiry");
  const update = () => {
    expiry.textContent = expiryText(expiresAt);
    expiry.classList.toggle("is-error", Date.now() >= expiresAt * 1000);
  };
  update();
  expiryTimer = setTimeout(update, Math.max(0, expiresAt * 1000 - Date.now()));
}

function showLanError(message) {
  const element = $("lan-error");
  element.textContent = message;
  element.classList.toggle("hidden", !message);
}
