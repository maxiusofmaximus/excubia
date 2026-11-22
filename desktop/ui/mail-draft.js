// The draft screen: what the operator reads before a send, and the send itself.
// The send flows live at module level, not on the screen: closing the screen
// keeps the approval and the window going, and reopening the draft shows where
// they stand.

import { $, invoke } from "./core.js";
import { showView } from "./nav.js";
import { openSettings } from "./settings.js";
import { element } from "./approval-card.js";
import { autoGrow } from "./chatcomposer.js";
import { errorMessage } from "./format.js";
import { createDraftEditor } from "./mail-edit.js";
import { createSendFlows } from "./mail-flow.js";
import {
  addressLine,
  changeRequestText,
  draftActions,
  formatBytes,
  isAtRest,
  needsSettings,
} from "./mail-model.js";

const flows = createSendFlows({ invoke });
const editor = createDraftEditor({ invoke, forget: flows.forget });
const EDIT_HIDES = ["mail-body-label", "mail-draft-body", "mail-draft-attachments", "mail-send"];

// The draft on screen. `detach` stops listening to its flow; the flow goes on.
// `alfredCanRevise` is set only when the draft is opened from the chat.
let open = null;
let openToken = 0;

export async function openDraft(account, draftId, { alfredCanRevise = false } = {}) {
  detach();
  open = null;
  setEditing(false);
  const token = ++openToken;
  showView("mail");
  setError("");
  let draft;
  let accounts;
  try {
    [draft, accounts] = await Promise.all([
      invoke("mail_draft", { account, draftId }),
      invoke("mail_accounts"),
    ]);
  } catch (error) {
    if (token !== openToken) return;
    clearDraft();
    setError(errorMessage(error));
    return;
  }
  if (token !== openToken) return;
  const entry = accounts.find((item) => item.label === account);
  open = {
    draftId,
    account,
    draft,
    alfredCanRevise,
    sendEnabled: Boolean(entry && entry.send_enabled),
    lastPhase: null,
    detach: null,
  };
  renderDraft(draft);
  attach();
  renderFlow(flows.current(draftId));
  $("mail-draft-title").focus();
}

// Leaves the draft without asking the host anything.
function leaveDraft() {
  detach();
  open = null;
  openToken += 1;
  showView("chat");
}

function closeDraft() {
  leaveDraft();
  // The settings list may be stale now: the send permission can have changed.
  document.dispatchEvent(new Event("mail:refresh"));
}

function currentPhase() {
  const view = flows.current(open.draftId);
  return view ? view.phase : null;
}

// While the operator edits, the body and the send section wait; the fields stay.
function setEditing(on) {
  for (const id of EDIT_HIDES) $(id).classList.toggle("hidden", on);
  $("mail-edit").classList.toggle("hidden", !on);
}

function startEdit() {
  if (!open || !isAtRest(currentPhase())) return;
  $("mail-edit-subject").value = open.draft.subject || "";
  $("mail-edit-body").value = open.draft.body || "";
  setEditing(true);
  $("mail-edit-subject").focus();
}

function cancelEdit() {
  setEditing(false);
  $("btn-mail-edit").focus();
}

async function saveEdit() {
  if (!open) return;
  const { account, draftId, alfredCanRevise } = open;
  const fields = { subject: $("mail-edit-subject").value, body: $("mail-edit-body").value };
  const save = $("btn-mail-save");
  save.disabled = true;
  // The draft is read again only while it is still the one on screen.
  const reload = () => (open && open.draftId === draftId ? openDraft(account, draftId, { alfredCanRevise }) : null);
  const result = await editor.save(account, draftId, fields, reload);
  save.disabled = false;
  if (result.ok) $("mail-status").textContent = result.text;
  else setError(result.text);
}

// «No enviar» closes the draft and changes nothing: it asks the host for nothing.
function decline() {
  if (open && isAtRest(currentPhase())) leaveDraft();
}

// «Pedir cambios a Alfred» leaves the draft and names it in the chat composer;
// the operator adds the change and sends it there, as in the APK.
function requestChanges() {
  if (!open || !isAtRest(currentPhase())) return;
  const text = changeRequestText(open.draftId);
  leaveDraft();
  showView("chat");
  const input = $("chat-input");
  input.value = text;
  autoGrow();
  input.setSelectionRange(text.length, text.length);
  input.focus();
}

function attach() {
  open.detach = flows.subscribe(open.draftId, renderFlow);
}

function detach() {
  if (open && open.detach) open.detach();
  if (open) open.detach = null;
}

function setError(message) {
  const node = $("mail-draft-error");
  node.textContent = message;
  node.classList.toggle("hidden", !message);
}

function clearDraft() {
  $("mail-draft-account").textContent = "";
  $("mail-draft-fields").replaceChildren();
  $("mail-draft-body").textContent = "";
  $("mail-draft-attachments").replaceChildren();
}

// What the operator reads: the fields, the body as plain text, the attachments.
function renderDraft(draft) {
  $("mail-draft-account").textContent = `Cuenta ${draft.account}`;
  const rows = [
    ["De", draft.from || "—"],
    ["Para", addressLine(draft.to)],
    ["Cc", addressLine(draft.cc)],
    ["Asunto", draft.subject || "—"],
  ];
  $("mail-draft-fields").replaceChildren(
    ...rows.flatMap(([label, value]) => [element("dt", null, label), element("dd", null, value)]),
  );
  $("mail-draft-body").textContent = draft.body || "";
  renderAttachments(draft.attachments || []);
}

function renderAttachments(files) {
  const box = $("mail-draft-attachments");
  if (!files.length) {
    box.replaceChildren(element("p", "hint", "Sin adjuntos."));
    return;
  }
  const list = element("ul", "mail-attachment-list");
  list.append(...files.map((file) => element("li", null, `${file.name} · ${formatBytes(file.size)}`)));
  box.replaceChildren(element("h3", null, "Adjuntos"), list);
}

function startSend() {
  if (!open || !open.sendEnabled) return;
  detach();
  renderFlow(flows.start(open.draftId, open.account));
  attach();
}

// The flow's view decides the screen: the line for the status, the countdown
// with its timer role, and which buttons may be used.
function renderFlow(view) {
  if (!open) return;
  const phase = view ? view.phase : null;
  const windowOpen = phase === "window";
  const sent = phase === "sent";
  const busy = Boolean(view && view.open);
  const send = $("btn-mail-send");
  const cancel = $("btn-mail-cancel");
  $("mail-status").textContent = windowOpen
    ? "Envío programado. Puedes cancelarlo hasta que termine la cuenta atrás."
    : view
      ? view.text
      : "";
  $("mail-countdown").textContent = windowOpen ? view.text : "";
  send.disabled = busy || sent || !open.sendEnabled;
  cancel.classList.toggle("hidden", !windowOpen);
  const actions = draftActions(phase, open.alfredCanRevise);
  $("btn-mail-edit").classList.toggle("hidden", !actions.edit);
  $("btn-mail-edit").textContent = phase === "failed" ? "Editar borrador" : "Editar";
  $("btn-mail-decline").classList.toggle("hidden", !actions.decline);
  $("btn-mail-changes").classList.toggle("hidden", !actions.requestChanges);
  $("btn-mail-settings").classList.toggle("hidden", !(view && phase === "failed" && needsSettings(view.code)));
  $("mail-send-hint").textContent = open.sendEnabled
    ? sent
      ? "Este borrador ya se envió."
      : ""
    : "Activa «Permitir enviar» en Ajustes → Correo para enviar desde esta cuenta.";
  // The send button goes disabled once pressed: the focus moves to the cancel.
  if (windowOpen && open.lastPhase !== "window") cancel.focus();
  open.lastPhase = phase;
}

export function installMailDraft() {
  $("btn-mail-back").addEventListener("click", closeDraft);
  $("btn-mail-settings").addEventListener("click", () => {
    closeDraft();
    openSettings();
  });
  $("btn-mail-send").addEventListener("click", startSend);
  $("btn-mail-cancel").addEventListener("click", () => {
    if (open) flows.cancel(open.draftId);
  });
  $("btn-mail-edit").addEventListener("click", startEdit);
  $("btn-mail-decline").addEventListener("click", decline);
  $("btn-mail-changes").addEventListener("click", requestChanges);
  $("btn-mail-save").addEventListener("click", saveEdit);
  $("btn-mail-edit-cancel").addEventListener("click", cancelEdit);
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && open && !$("view-mail").classList.contains("hidden")) closeDraft();
  });
}
