// «Recetas» of one professional, in its ficha: the list with «Repetir», «Editar» and «Borrar», and
// the save of a finished encargo as a recipe. A host answer is worded by recipe-model; nothing is
// stored without the review in recipe-editor.

import { $, invoke, toast } from "./core.js";
import { chat } from "./chatstate.js";
import { element, loadingBlock } from "./chatdom.js";
import { button, deleteConfirm } from "./agenda-ui.js";
import { emptyState, errorState } from "./empty-state.js";
import { statusMeta } from "./delegations.js";
import { installRecipeEditor, openRecipeEditor } from "./recipe-editor.js";
import { PERMISSION_TEXT, recipeFailureText, sortRecipes, usesText } from "./recipe-model.js";

const EMPTY_TEXT = "Cuando algo salga bien, guárdalo como receta y repítelo con un toque.";
const NO_CONVERSATION = "Abre una conversación del proyecto para repetir el encargo.";

// `recipes` stays null until the first answer; `runs` keeps each recipe's last run line by id.
const state = { hire: null, recipes: null, error: null, notice: "", locked: false, runs: {}, confirm: null };

function tagged(node, action, id) {
  node.dataset.recipeAction = action;
  node.dataset.recipeId = id;
  return node;
}

function runText(status) {
  return `Encargo lanzado · ${statusMeta(status).label}`;
}

// Keeps keyboard focus on the same control after a repaint; the list is rebuilt on every answer.
function focusedControl() {
  const node = document.activeElement;
  return node?.dataset?.recipeAction ? { action: node.dataset.recipeAction, id: node.dataset.recipeId } : null;
}

function restoreFocus(saved) {
  if (!saved) return;
  $("hire-recipes-body")
    .querySelector(`[data-recipe-action="${saved.action}"][data-recipe-id="${saved.id}"]`)
    ?.focus();
}

function headOf(item) {
  const head = element("div", "recipe-head");
  head.append(element("h3", "recipe-name", item.title), element("span", "recipe-uses", usesText(item.uses)));
  return head;
}

function actionsOf(item) {
  const row = element("div", "recipe-actions");
  const repeat = tagged(button("Repetir", "btn primary btn-small", () => run(item)), "repeat", item.id);
  const edit = tagged(button("Editar", "btn ghost btn-small", () => editRecipe(item)), "edit", item.id);
  const remove = tagged(button("Borrar", "btn ghost btn-small", () => askDelete(item)), "delete", item.id);
  for (const control of [repeat, edit, remove]) control.disabled = state.locked;
  row.append(repeat, edit, remove);
  return row;
}

// The line under a recipe after «Repetir»: the status it reached, or the reason it did not start.
function runLine(run, id) {
  const line = element("p", run.error ? "recipe-run is-error" : "recipe-run", run.text);
  line.setAttribute("role", run.error ? "alert" : "status");
  if (!run.action) return line;
  line.append(" ", tagged(button(run.action.label, "btn secondary btn-small", run.action.onClick), "hire", id));
  return line;
}

function rowFor(item) {
  const row = element("li", "recipe-row");
  row.append(headOf(item));
  if (state.confirm?.id === item.id) {
    row.append(state.confirm.node);
    return row;
  }
  row.append(element("p", "recipe-text", item.body), actionsOf(item));
  if (state.runs[item.id]) row.append(runLine(state.runs[item.id], item.id));
  return row;
}

function bodyNodes() {
  if (state.recipes === null) {
    return state.error ? [errorState(recipeFailureText(state.error), loadRecipes)] : [loadingBlock("Cargando recetas")];
  }
  if (state.recipes.length === 0) return [emptyState({ icon: "notes", text: EMPTY_TEXT })];
  const list = element("ul", "recipe-list");
  list.setAttribute("role", "list");
  list.append(...sortRecipes(state.recipes).map(rowFor));
  return [list];
}

function paintNotice() {
  const notice = $("hire-recipes-notice");
  const text = state.notice || (state.locked ? PERMISSION_TEXT : "");
  notice.textContent = text;
  notice.hidden = !text;
}

function paint() {
  if (!state.hire) return;
  const saved = focusedControl();
  paintNotice();
  $("hire-recipes-body").replaceChildren(...bodyNodes());
  restoreFocus(saved);
}

async function loadRecipes() {
  try {
    state.recipes = await invoke("recipes", { role: state.hire.role });
    state.error = null;
    state.notice = "";
  } catch (error) {
    // A list already on screen stays; the failure is said above it.
    if (state.recipes) state.notice = recipeFailureText(error);
    else state.error = error;
  }
  paint();
}

export function openRecipes(hire) {
  Object.assign(state, { hire, recipes: null, error: null, notice: "", locked: false, runs: {}, confirm: null });
  $("hire-recipes-title").textContent = `Recetas de ${hire.name}`;
  const dialog = $("hire-recipes");
  if (!dialog.open) dialog.showModal();
  paint();
  loadRecipes();
}

// «Repetir»: starts the recipe in the open conversation. The button is not retried by the screen.
async function run(item) {
  const conversation = chat.conversation?.id;
  if (!conversation) {
    state.runs[item.id] = { error: true, text: NO_CONVERSATION };
    paint();
    return;
  }
  state.runs[item.id] = { text: "Lanzando el encargo…" };
  paint();
  try {
    const started = await invoke("recipe_run", { id: item.id, conversationId: conversation });
    state.runs[item.id] = { text: runText(started.status) };
    await loadRecipes();
  } catch (error) {
    // A denied device is said once, by the notice above the list, not again under the recipe.
    if (error.kind === "forbidden") {
      state.locked = true;
      delete state.runs[item.id];
      paint();
      return;
    }
    const noHire = error.kind === "recipeRefused" && error.code === "no_hire_for_role";
    state.runs[item.id] = {
      error: true,
      text: recipeFailureText(error),
      action: noHire ? { label: "Contratar", onClick: hireAgain } : null,
    };
    paint();
  }
}

// The ficha closes and the contracting sheet opens, the same button the team shows.
function hireAgain() {
  $("hire-recipes").close();
  $("btn-hire").click();
}

function editRecipe(item) {
  openRecipeEditor({
    heading: "Editar receta",
    lede: "Revisa el texto antes de guardarlo. Solo se guarda cuando pulses «Guardar».",
    title: item.title,
    body: item.body,
    save: ({ title, body, confirmRisks }) => invoke("recipe_update", { id: item.id, title, body, confirmRisks }),
    onSaved: () => {
      toast("Receta guardada");
      loadRecipes();
    },
  });
}

function askDelete(item) {
  const { node, problem } = deleteConfirm({
    onConfirm: () => remove(item),
    onCancel: () => {
      state.confirm = null;
      paint();
    },
  });
  state.confirm = { id: item.id, node, problem };
  paint();
}

async function remove(item) {
  const { problem } = state.confirm;
  try {
    await invoke("recipe_delete", { id: item.id });
    state.confirm = null;
    toast("Receta borrada");
    await loadRecipes();
  } catch (error) {
    if (error.kind === "forbidden") state.locked = true;
    problem.textContent = recipeFailureText(error);
    problem.hidden = false;
    paint();
  }
}

// «Guardar como receta» on a finished encargo: Alfred drafts the text, the operator reviews and stores it.
export async function saveRecipeFromDelegation(delegationId) {
  let draft = null;
  const save = ({ title, body, confirmRisks }) =>
    invoke("recipe_create", { role: draft.role_id, title, body, confirmRisks });
  const show = (options) =>
    openRecipeEditor({
      heading: "Guardar como receta",
      lede: "Revisa el título y el texto. Solo se guarda cuando pulses «Guardar».",
      save,
      onSaved: () => toast("Receta guardada"),
      ...options,
    });
  const load = async () => {
    show({ phase: "loading", lede: "Alfred prepara el borrador…", retry: null });
    try {
      draft = await invoke("recipe_draft", { delegationId });
      show({ title: draft.title, body: draft.body });
    } catch (error) {
      show({ phase: "error", errorText: recipeFailureText(error), retry: load });
    }
  };
  await load();
}

export function installRecipes() {
  installRecipeEditor();
  $("hire-recipes-close").addEventListener("click", () => $("hire-recipes").close());
}
