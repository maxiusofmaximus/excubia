// Arranca un escenario de Recetas sobre los módulos reales de ui/ (sin copiarlos). El escenario
// y la paleta vienen en la URL: ?s=<escenario>&p=<noche|papel>. El puente simulado lo lee igual.

import { chat } from "../ui/chatstate.js";
import { installRecipes, openRecipes, saveRecipeFromDelegation } from "../ui/recipes.js";

const params = new URLSearchParams(location.search);
const scenario = params.get("s") || "lista";

const HIRE = { id: "hir_01HZ", name: "Marta", role: "audio-editor", avatar: null };
const FROM_DELEGATION = ["draft", "riesgo", "secreto", "sin-permiso-guardar"];
const IN_CONVERSATION = ["ejecutar", "ocupado", "sin-permiso"];

window.ExcubiaPalettes.applyPalette(params.get("p") || "noche");
installRecipes();

if (IN_CONVERSATION.includes(scenario)) chat.conversation = { id: "cnv_01HZ" };

if (FROM_DELEGATION.includes(scenario)) {
  saveRecipeFromDelegation("dlg_01HZ");
} else {
  openRecipes(HIRE);
}

document.body.dataset.scenario = scenario;
