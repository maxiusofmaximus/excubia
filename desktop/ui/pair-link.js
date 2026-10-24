import { $, invoke, setBusy } from "./core.js";
import { errorMessage } from "./format.js";

/// The link form: a `herdr-notify://pair?…` link pasted from the host. The core
/// checks its version and expiry, then pairs only with the host whose
/// certificate matches the pin in the link.
export function installLinkForm(onPaired) {
  $("link-form").addEventListener("submit", (event) => submitLink(event, onPaired));
}

async function submitLink(event, onPaired) {
  event.preventDefault();
  const button = $("btn-pair-link");
  showLinkError("");
  setBusy(button, true, "Emparejando…");
  try {
    const outcome = await invoke("pair_link", {
      link: $("pair-link").value.trim(),
      label: $("device-label").value,
    });
    $("pair-link").value = "";
    onPaired(outcome);
  } catch (error) {
    showLinkError(errorMessage(error));
  } finally {
    setBusy(button, false);
  }
}

export function showLinkError(message) {
  const element = $("link-error");
  element.textContent = message;
  element.classList.toggle("hidden", !message);
}
