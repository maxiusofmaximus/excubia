// «Notas»: the notes the app and Alfred keep on the host. The list pages back in time
// with «Cargar más»; the panel reads one note, edits it or creates one, and deletes it
// only after the operator confirms. A note that came from a chat message opens that chat.

import { $, invoke, showView } from "./core.js";
import { element } from "./chatdom.js";
import { selectConversation } from "./chat.js";
import { readStateText } from "./hires-model.js";
import { createAgendaList } from "./agenda-list.js";
import {
  actionRow,
  approveReason,
  button,
  closeOnEscape,
  debounced,
  deleteAction,
  deleteConfirm,
  emptyPanel,
  field,
  fillPanel,
  notice,
  panelParts,
  removeRow,
  reportWrite,
} from "./agenda-ui.js";
import {
  agendaErrorText,
  noteProblem,
  noteSummary,
  noteTitle,
  relativeDate,
} from "./agenda-model.js";

const parts = panelParts("notes");
let selectedId = null;
let openToken = 0;
let list = null;

const failureText = (failure) => readStateText(failure) || agendaErrorText(failure);

function emptyText(query) {
  if (query) return "Ninguna nota coincide con la búsqueda.";
  return "Aún no hay notas. Alfred las guarda cuando se lo pides, o crea una.";
}

async function fetchPage({ q, cursor }) {
  const page = await invoke("agenda_notes", { q, before: cursor });
  return { items: page.notes, next: page.next };
}

function noteRow(summary) {
  const item = element("li");
  const row = button("", "agenda-row", () => openNote(summary.id));
  if (summary.id === selectedId) row.setAttribute("aria-current", "true");
  row.append(element("span", "agenda-row-title", noteTitle(summary)));
  row.append(element("span", "agenda-row-meta", relativeDate(summary.updated_at)));
  if (summary.excerpt) row.append(element("span", "agenda-row-excerpt", summary.excerpt));
  item.append(row);
  return item;
}

function closePanel() {
  selectedId = null;
  emptyPanel(parts);
  list.redraw();
}

// A later click wins: an earlier read that lands after it is dropped.
async function openNote(id) {
  const token = ++openToken;
  selectedId = id;
  list.redraw();
  fillPanel(parts, "Nota", [element("p", "agenda-muted", "Cargando…")]);
  try {
    const note = await invoke("agenda_note", { id });
    if (token === openToken) showNote(note);
  } catch (failure) {
    if (token === openToken) fillPanel(parts, "Nota", [notice(failureText(failure))]);
  }
}

function showNote(note) {
  selectedId = note.id;
  list.redraw();
  const edit = button("Editar", "btn secondary btn-small", () => showForm(note));
  const actions = [edit, deleteAction(() => showDelete(note))];
  if (note.message_id && note.conversation_id) {
    actions.unshift(button("Ver en el chat", "btn secondary btn-small", () => openChat(note.conversation_id)));
  }
  fillPanel(parts, noteTitle(noteSummary(note)), [
    element("p", "agenda-muted", `Actualizada ${relativeDate(note.updated_at)}`),
    element("p", "agenda-body", note.body),
    actionRow(...actions),
    approveReason(),
  ]);
  edit.focus();
}

function openChat(conversationId) {
  showView("chat");
  selectConversation(conversationId);
}

function showForm(note) {
  const fields = {
    title: field({ id: "note-subject", label: "Asunto (opcional)", value: note?.subject || "" }),
    body: field({ id: "note-body", label: "Texto", value: note?.body || "", multiline: true, rows: 12 }),
  };
  const problem = notice("");
  const save = button(note ? "Guardar" : "Crear nota", "btn primary btn-small", () =>
    submit(note, fields, problem, save),
  );
  const cancel = button("Cancelar", "btn ghost btn-small", () => (note ? showNote(note) : closePanel()));
  fillPanel(parts, note ? "Editar nota" : "Nueva nota", [
    fields.title.node,
    fields.body.node,
    problem,
    actionRow(cancel, save),
  ]);
  fields.body.control.focus();
}

async function submit(note, fields, problem, save) {
  const title = fields.title.control.value.trim();
  const body = fields.body.control.value;
  const check = noteProblem({ title, body });
  Object.values(fields).forEach((entry) => entry.setError(""));
  if (check) {
    fields[check.field].setError(check.message);
    return;
  }
  save.disabled = true;
  try {
    // A new note without subject sends no `title`; an edit sends "" to clear one.
    const saved = note
      ? await invoke("agenda_note_update", { id: note.id, title, body })
      : await invoke("agenda_note_create", { draft: { title: title || null, body, message_id: null } });
    list.upsert(noteSummary(saved));
    showNote(saved);
  } catch (failure) {
    save.disabled = false;
    reportWrite(failure, { fields, problem, permission: "RESPOND" });
  }
}

function showDelete(note) {
  const confirm = deleteConfirm({
    onConfirm: (problem) =>
      removeRow({
        command: "agenda_note_delete",
        id: note.id,
        problem,
        onDone: () => {
          list.remove(note.id);
          closePanel();
        },
        onDenied: () => showNote(note),
      }),
    onCancel: () => showNote(note),
  });
  fillPanel(parts, `Borrar la nota «${noteTitle(noteSummary(note))}»`, [confirm.node]);
}

const searchSoon = debounced(() => list.reload($("notes-search").value.trim()));

export function openNotes() {
  showView("notes");
  closePanel();
  list.reload($("notes-search").value.trim());
}

export function installNotes() {
  list = createAgendaList({
    list: $("notes-list"),
    status: $("notes-status"),
    more: $("notes-more"),
    fetchPage,
    renderRow: noteRow,
    emptyText,
    loadingText: "Cargando notas…",
    failureText,
  });
  $("notes-search").addEventListener("input", searchSoon);
  $("notes-more").addEventListener("click", () => list.loadMore());
  $("btn-note-new").addEventListener("click", () => showForm(null));
  $("notes-panel-close").addEventListener("click", closePanel);
  closeOnEscape("view-notes", parts, closePanel);
}
