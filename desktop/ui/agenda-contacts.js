// «Contactos»: the people and addresses kept on the host. The list pages with «Cargar
// más»; the panel shows one contact, edits it or creates one, and deletes it only after
// the operator confirms. Without APPROVE the delete stays off and says why.

import { $, invoke } from "./core.js";
import { showView } from "./nav.js";
import { element } from "./chatdom.js";
import { readStateText } from "./hires-model.js";
import { createAgendaList } from "./agenda-list.js";
import { emptyState } from "./empty-state.js";
import {
  actionRow,
  approveReason,
  button,
  closeOnEscape,
  debounced,
  detailLine,
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
  contactPrimaryAddress,
  contactProblem,
  parseList,
  sourceLabel,
} from "./agenda-model.js";

const parts = panelParts("contacts");
let selectedId = null;
let list = null;

const failureText = (failure) => readStateText(failure) || agendaErrorText(failure);

function emptyNode(query) {
  if (query) {
    return emptyState(
      { icon: "contacts", text: "Ningún contacto coincide con la búsqueda.", action: "Borrar búsqueda" },
      () => {
        $("contacts-search").value = "";
        list.reload("");
      },
    );
  }
  return emptyState(
    { icon: "contacts", text: "Aún no hay contactos. Alfred los guarda cuando se los pides, o añade uno.", action: "Nuevo contacto" },
    () => $("btn-contact-new").click(),
  );
}

async function fetchPage({ q, cursor }) {
  const page = await invoke("agenda_contacts", { q, after: cursor });
  return { items: page.contacts, next: page.next };
}

function contactRow(contact) {
  const item = element("li");
  const row = button("", "agenda-row", () => showContact(contact));
  if (contact.id === selectedId) row.setAttribute("aria-current", "true");
  row.append(element("span", "agenda-row-title", contact.name));

  const meta = element("span", "agenda-row-meta");
  meta.textContent = [contact.aliases[0], contactPrimaryAddress(contact)].filter(Boolean).join(" · ");
  const origin = contact.addresses[0] ? sourceLabel(contact.addresses[0].source) : "";
  if (origin) meta.append(" ", element("span", "agenda-badge", origin));
  row.append(meta);
  item.append(row);
  return item;
}

function addressList(addresses) {
  if (!addresses.length) return element("p", "agenda-muted", "Sin direcciones.");
  const items = element("ul", "agenda-addresses");
  for (const entry of addresses) {
    const item = element("li", null, entry.address);
    const origin = sourceLabel(entry.source);
    if (origin) item.append(" ", element("span", "agenda-badge", origin));
    items.append(item);
  }
  return items;
}

function closePanel() {
  selectedId = null;
  emptyPanel(parts);
  list.redraw();
}

function showContact(contact) {
  selectedId = contact.id;
  list.redraw();
  const edit = button("Editar", "btn secondary btn-small", () => showForm(contact));
  const notes = contact.notes ? [element("p", "agenda-notes", contact.notes)] : [];
  fillPanel(parts, contact.name, [
    detailLine("Alias", contact.aliases.join(", ") || "Ninguno"),
    addressList(contact.addresses),
    ...notes,
    actionRow(edit, deleteAction(() => showDelete(contact))),
    approveReason(),
  ]);
  edit.focus();
}

function showForm(contact) {
  const fields = {
    name: field({ id: "contact-name", label: "Nombre", value: contact?.name || "" }),
    aliases: field({
      id: "contact-aliases",
      label: "Alias (separados por comas)",
      value: contact ? contact.aliases.join(", ") : "",
      multiline: true,
      rows: 2,
    }),
    addresses: field({
      id: "contact-addresses",
      label: "Direcciones (una por línea)",
      value: contact ? contact.addresses.map((entry) => entry.address).join("\n") : "",
      multiline: true,
      rows: 3,
    }),
    notes: field({
      id: "contact-notes",
      label: "Notas",
      value: contact?.notes || "",
      multiline: true,
      rows: 4,
    }),
  };
  const problem = notice("");
  const save = button(contact ? "Guardar" : "Crear contacto", "btn primary btn-small", () =>
    submit(contact, fields, problem, save),
  );
  const cancel = button("Cancelar", "btn ghost btn-small", () => (contact ? showContact(contact) : closePanel()));
  fillPanel(parts, contact ? "Editar contacto" : "Nuevo contacto", [
    fields.name.node,
    fields.aliases.node,
    fields.addresses.node,
    fields.notes.node,
    problem,
    actionRow(cancel, save),
  ]);
  fields.name.control.focus();
}

async function submit(contact, fields, problem, save) {
  const draft = {
    name: fields.name.control.value,
    aliases: parseList(fields.aliases.control.value),
    addresses: parseList(fields.addresses.control.value).map((address) => address.toLowerCase()),
    notes: fields.notes.control.value,
  };
  const check = contactProblem(draft);
  Object.values(fields).forEach((entry) => entry.setError(""));
  if (check) {
    fields[check.field].setError(check.message);
    return;
  }
  save.disabled = true;
  try {
    const saved = contact
      ? await invoke("agenda_contact_update", { id: contact.id, draft })
      : await invoke("agenda_contact_create", { draft });
    list.upsert(saved);
    showContact(saved);
  } catch (failure) {
    save.disabled = false;
    reportWrite(failure, { fields, problem, permission: "RESPOND" });
  }
}

function showDelete(contact) {
  const confirm = deleteConfirm({
    onConfirm: (problem) =>
      removeRow({
        command: "agenda_contact_delete",
        id: contact.id,
        problem,
        onDone: () => {
          list.remove(contact.id);
          closePanel();
        },
        onDenied: () => showContact(contact),
      }),
    onCancel: () => showContact(contact),
  });
  fillPanel(parts, `Borrar a ${contact.name}`, [confirm.node]);
}

const searchSoon = debounced(() => list.reload($("contacts-search").value.trim()));

export function openContacts() {
  showView("contacts");
  closePanel();
  list.reload($("contacts-search").value.trim());
}

export function installContacts() {
  list = createAgendaList({
    list: $("contacts-list"),
    status: $("contacts-status"),
    more: $("contacts-more"),
    fetchPage,
    renderRow: contactRow,
    emptyNode,
    loadingText: "Cargando contactos…",
    failureText,
  });
  $("contacts-search").addEventListener("input", searchSoon);
  $("contacts-more").addEventListener("click", () => list.loadMore());
  $("btn-contact-new").addEventListener("click", () => showForm(null));
  $("contacts-panel-close").addEventListener("click", closePanel);
  closeOnEscape("view-contacts", parts, closePanel);
}
