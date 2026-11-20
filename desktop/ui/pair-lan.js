import { $, invoke, setBusy } from "./core.js";
import { showView } from "./nav.js";
import { errorMessage, svgDataUrl } from "./format.js";
import { IDLE, leaveWindow, qrMessage, qrVisible, showQr as startQr, tickQr } from "./pair-qr-model.js";

// The QR on this screen, with or without an address; null until one is shown.
let qr = null;
let qrTimer = null;
let qrCommand = "lan_pairing";

/// The LAN switch decides whether the host listens for phones at all.
export function hydrateLan(enabled) {
  $("lan-toggle").checked = enabled;
  $("btn-show-lan").disabled = !enabled;
  $("btn-show-pake").disabled = !enabled;
}

export function installLanPairing() {
  $("lan-toggle").addEventListener("change", toggleLan);
  $("btn-show-lan").addEventListener("click", (event) => showPairingQr(event.currentTarget, "lan_pairing"));
  $("btn-show-pake").addEventListener("click", (event) => showPairingQr(event.currentTarget, "pake_pairing"));
  $("btn-lan-new").addEventListener("click", (event) => showPairingQr(event.currentTarget, qrCommand));
  $("btn-lan-back").addEventListener("click", () => {
    dropQr();
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

/// Both QRs take the same road: the host makes the code, the model counts it down and takes it down.
async function showPairingQr(button, command) {
  showLanError("");
  setBusy(button, true, "Preparando…");
  try {
    const pairing = await invoke(command);
    clearInterval(qrTimer);
    qrCommand = command;
    qr = startQr(pairing.expires_at, Date.now());
    $("lan-qr").src = svgDataUrl(pairing.svg);
    showView("lan");
    renderQr();
    qrTimer = setInterval(() => {
      qr = tickQr(qr, Date.now());
      renderQr();
    }, 1000);
  } catch (error) {
    showLanError(errorMessage(error));
  } finally {
    setBusy(button, false);
  }
}

/// The QR is on the page only while the model says it is visible; otherwise its source is gone.
function renderQr() {
  const image = $("lan-qr");
  const visible = qrVisible(qr);
  if (!visible) {
    image.removeAttribute("src");
    clearInterval(qrTimer);
  }
  image.classList.toggle("hidden", !visible);
  const expiry = $("lan-expiry");
  expiry.textContent = qrMessage(qr);
  expiry.classList.toggle("is-error", !visible);
}

function leave() {
  if (!qr) return;
  qr = leaveWindow(qr);
  renderQr();
}

function dropQr() {
  if (!qr) return;
  qr = IDLE;
  renderQr();
  qr = null;
}

function showLanError(message) {
  const element = $("lan-error");
  element.textContent = message;
  element.classList.toggle("hidden", !message);
}
