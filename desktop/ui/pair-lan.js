import { $, invoke, setBusy, showView } from "./core.js";
import { errorMessage, expiryText, svgDataUrl } from "./format.js";

let expiryTimer = null;

/// The LAN switch decides whether the host listens for phones at all.
export function hydrateLan(enabled) {
  $("lan-toggle").checked = enabled;
  $("btn-show-lan").disabled = !enabled;
}

export function installLanPairing() {
  $("lan-toggle").addEventListener("change", toggleLan);
  $("btn-show-lan").addEventListener("click", (event) => showQr(event.currentTarget));
  $("btn-lan-new").addEventListener("click", (event) => showQr(event.currentTarget));
  $("btn-lan-back").addEventListener("click", () => showView("pair"));
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
    $("lan-qr").src = svgDataUrl(pairing.svg);
    showView("lan");
    countDown(pairing.expires_at);
  } catch (error) {
    showLanError(errorMessage(error));
  } finally {
    setBusy(button, false);
  }
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
