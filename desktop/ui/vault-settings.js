// Ajustes → Seguridad, the row «Historial cifrado». It shows the state the host reports and
// offers the protect dialog, or the removal of the plain backup. When the host gives no
// answer, or has no vault route (503 vault_unavailable), the whole section stays hidden.

import { $, invoke } from "./core.js";
import { installVaultDialog, openVaultProtect } from "./vault-dialog.js";
import { finishErrorText } from "./vault-model.js";

let status = null;

export function mountVaultSettings() {
  installVaultDialog();
  $("btn-vault-protect").addEventListener("click", () => openVaultProtect(refresh));
  $("btn-vault-finish").addEventListener("click", () => {
    showFinishError("");
    $("vault-finish").showModal();
  });
  $("vault-finish-form").addEventListener("submit", confirmFinish);
  $("vault-finish-cancel").addEventListener("click", () => $("vault-finish").close());
  document.addEventListener("settings:open", refresh);
}

async function refresh() {
  try {
    status = await invoke("vault_status");
  } catch {
    status = null;
  }
  paint();
}

function paint() {
  $("settings-security").classList.toggle("hidden", !status);
  if (!status) return;
  const protectedNow = status.state === "protected";
  const backup = protectedNow && status.backup;
  $("vault-state").textContent = protectedNow
    ? "Tu historial está cifrado (AES-256)."
    : "Tu historial está guardado sin cifrar en este PC.";
  $("btn-vault-protect").classList.toggle("hidden", protectedNow);
  $("vault-backup").textContent = "Queda una copia sin cifrar de antes de proteger.";
  $("vault-backup").classList.toggle("hidden", !backup);
  $("btn-vault-finish").classList.toggle("hidden", !backup);
}

function showFinishError(message) {
  const box = $("vault-finish-error");
  box.textContent = message;
  box.classList.toggle("hidden", !message);
}

async function confirmFinish(event) {
  event.preventDefault();
  try {
    await invoke("vault_finish");
    $("vault-finish").close();
    await refresh();
  } catch (error) {
    showFinishError(finishErrorText(error));
    // Nothing is left to remove: close and show the row as it is.
    if (error?.code === "vault_no_backup") {
      $("vault-finish").close();
      await refresh();
    }
  }
}
