// «+ Proyecto» in the side panel: the operator picks a folder of this PC and names the
// project. Its lead and specialists work in that folder; the lead starts the first time
// Alfred gives it work. Only this PC can do it: the host asks for ADMIN.
import { $, invoke, toast } from "./core.js";
import { friendlyError } from "./friendly-error.js";

function baseName(path) {
  const parts = String(path || "").split(/[\\/]+/).filter(Boolean);
  return parts.length ? parts[parts.length - 1] : "";
}

function showError(text) {
  $("project-new-error").textContent = text;
  $("project-new-error").classList.toggle("hidden", !text);
}

export function installNewProject(refreshHome) {
  if (!invoke) return;
  const dialog = $("project-new");

  async function pick() {
    const folder = await invoke("pick_folder").catch(() => null);
    if (!folder) return;
    $("project-new-folder").textContent = folder;
    dialog.dataset.folder = folder;
    if (!$("project-new-name").value.trim()) $("project-new-name").value = baseName(folder);
    showError("");
  }

  $("btn-new-project").addEventListener("click", async () => {
    $("project-new-name").value = "";
    $("project-new-folder").textContent = "Ninguna carpeta elegida";
    delete dialog.dataset.folder;
    showError("");
    dialog.showModal();
    await pick();
  });
  $("project-new-pick").addEventListener("click", pick);
  $("project-new-cancel").addEventListener("click", () => dialog.close());
  $("project-new-form").addEventListener("submit", async (event) => {
    event.preventDefault();
    const name = $("project-new-name").value.trim();
    const rootPath = dialog.dataset.folder;
    if (!rootPath) return showError("Elige la carpeta del proyecto.");
    if (!name) return showError("Escribe un nombre.");
    $("project-new-save").disabled = true;
    try {
      await invoke("team_project_create", { name, rootPath });
      dialog.close();
      toast(`Proyecto «${name}» creado`);
      await refreshHome();
    } catch (error) {
      showError(friendlyError(error));
    } finally {
      $("project-new-save").disabled = false;
    }
  });
}
