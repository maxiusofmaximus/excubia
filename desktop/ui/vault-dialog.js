// «Protege tu historial»: one dialog, three steps. The mouse pool fills the first; the
// recovery phrase is shown once in the second; the third asks three of its words before the
// dialog closes. The phrase and the samples live only in this module and are wiped on every
// close. Nothing here logs them, copies them or stores them.

import { $, invoke } from "./core.js";
import {
  checkChallenge,
  confirmChallenge,
  createCollector,
  protectErrorText,
} from "./vault-model.js";

const TITLES = {
  move: "Mueve el ratón",
  phrase: "Tu frase de recuperación",
  check: "Comprobación",
};
const ANSWERS = [0, 1, 2];

let collector = null;
let phrase = "";
let challenge = [];
let step = "move";
let busy = false;
// The key exists: a close from now on refreshes the row, even if the user never confirmed.
let created = false;
let onDone = null;

function showStep(name) {
  step = name;
  for (const pane of ["move", "phrase", "check"]) {
    $(`vault-step-${pane}`).classList.toggle("hidden", pane !== name);
  }
  $("vault-title").textContent = TITLES[name];
}

function showError(id, message) {
  const box = $(id);
  box.textContent = message;
  box.classList.toggle("hidden", !message);
}

function paintProgress() {
  $("vault-progress").value = collector ? collector.progress() : 0;
  $("vault-create").disabled = busy || !collector || !collector.isEnough();
  $("vault-move-cancel").disabled = busy;
}

function onMove(event) {
  if (step !== "move" || busy || !collector) return;
  // Microseconds from the page clock, as the host's samples carry them.
  collector.add(event.clientX, event.clientY, Math.round(performance.now() * 1000));
  paintProgress();
}

async function createKey() {
  // The samples leave the pool with the call: nothing keeps them afterwards.
  const samples = collector.pack();
  collector.clear();
  busy = true;
  showError("vault-move-error", "");
  paintProgress();
  try {
    const result = await invoke("vault_init", { samples });
    created = true;
    phrase = result.phrase;
    paintPhrase();
    showStep("phrase");
    $("vault-saved").focus();
  } catch (error) {
    if (error?.code === "vault_already_protected") created = true;
    showError("vault-move-error", protectErrorText(error));
  } finally {
    busy = false;
    paintProgress();
  }
}

function paintPhrase() {
  const items = phrase
    .trim()
    .split(/\s+/)
    .map((word, index) => {
      const item = document.createElement("li");
      const number = document.createElement("span");
      number.className = "vault-num";
      number.textContent = `${index + 1}.`;
      const text = document.createElement("span");
      text.className = "vault-word";
      text.textContent = word;
      item.append(number, text);
      return item;
    });
  $("vault-words").replaceChildren(...items);
}

function startCheck() {
  challenge = confirmChallenge(phrase);
  for (const index of ANSWERS) {
    $(`vault-answer-label-${index}`).textContent = `Palabra ${challenge[index]}`;
    $(`vault-answer-${index}`).value = "";
  }
  showError("vault-check-error", "");
  showStep("check");
  $("vault-answer-0").focus();
}

function checkAnswers(event) {
  event.preventDefault();
  const answers = ANSWERS.map((index) => $(`vault-answer-${index}`).value);
  if (checkChallenge(phrase, challenge, answers)) {
    $("vault-dialog").close();
    return;
  }
  for (const index of ANSWERS) $(`vault-answer-${index}`).value = "";
  showError("vault-check-error", "No coinciden. Revisa la frase y vuelve a intentarlo.");
  $("vault-answer-0").focus();
}

// Wipes what the dialog held: the variables, the words on screen and the typed answers.
function wipe() {
  if (collector) collector.clear();
  collector = null;
  phrase = "";
  challenge = [];
  $("vault-words").replaceChildren();
  for (const index of ANSWERS) $(`vault-answer-${index}`).value = "";
}

function onClose() {
  const changed = created;
  created = false;
  wipe();
  if (changed && onDone) onDone();
}

export function openVaultProtect(refresh) {
  onDone = refresh;
  collector = createCollector();
  busy = false;
  showError("vault-move-error", "");
  showStep("move");
  paintProgress();
  $("vault-dialog").showModal();
  $("vault-pad").focus();
}

export function installVaultDialog() {
  const dialog = $("vault-dialog");
  // The phrase is shown only once: from the second step on Esc and a click outside do not close it,
  // and nothing closes it while the key is being made.
  dialog.addEventListener("cancel", (event) => {
    if (step !== "move" || busy) event.preventDefault();
  });
  dialog.addEventListener("close", onClose);
  $("vault-pad").addEventListener("pointermove", onMove);
  $("vault-create").addEventListener("click", createKey);
  $("vault-move-cancel").addEventListener("click", () => dialog.close());
  $("vault-saved").addEventListener("click", startCheck);
  $("vault-check-form").addEventListener("submit", checkAnswers);
  $("vault-check-back").addEventListener("click", () => showStep("phrase"));
}
