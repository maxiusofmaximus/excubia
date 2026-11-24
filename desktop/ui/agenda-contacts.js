// «Contactos»: the people and addresses kept on the host. The list pages with «Cargar
// más»; the panel shows one contact, edits it or creates one, and deletes it only after
// the operator confirms. Without APPROVE the delete stays off and says why.

import { $, invoke } from "./core.js";
import { showView } from "./nav.js";
import { element } from "./chatdom.js";
import { readStateText } from "./hires-model.js";
import { createAgendaList } from "./agenda-list.js";
import { emptyState } from "./empty-state.js";
import { t } from "./i18n.js";
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
      { icon: "contacts", text: t("contacts.none_match"), action: t("contacts.clear_search") },
      () => {
        $("contacts-search").value = "";
        list.reload("");
      },
    );
  }
  return emptyState(
    { icon: "contacts", text: t("contacts.empty"), action: t("contacts.new") },
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
  if (!addresses.length) return element("p", "agenda-muted", t("contacts.no_addresses"));
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
  const edit = button(t("contacts.edit"), "btn secondary btn-small", () => showForm(contact));
  const notes = contact.notes ? [element("p", "agenda-notes", contact.notes)] : [];
  fillPanel(parts, contact.name, [
    detailLine(t("contacts.alias"), contact.aliases.join(", ") || t("contacts.none")),
    addressList(contact.addresses),
    ...notes,
    actionRow(edit, deleteAction(() => showDelete(contact))),
    approveReason(),
  ]);
  edit.focus();
}

function showForm(contact) {
  const fields = {
    name: field({ id: "contact-name", label: t("contacts.name_label"), value: contact?.name || "" }),
    aliases: field({
      id: "contact-aliases",
      label: t("contacts.aliases_label"),
      value: contact ? contact.aliases.join(", ") : "",
      multiline: true,
      rows: 2,
    }),
    addresses: field({
      id: "contact-addresses",
      label: t("contacts.addresses_label"),
      value: contact ? contact.addresses.map((entry) => entry.address).join("\n") : "",
      multiline: true,
      rows: 3,
    }),
    notes: field({
      id: "contact-notes",
      label: t("contacts.notes_label"),
      value: contact?.notes || "",
      multiline: true,
      rows: 4,
    }),
  };
  const problem = notice("");
  const save = button(contact ? t("contacts.save") : t("contacts.create"), "btn primary btn-small", () =>
    submit(contact, fields, problem, save),
  );
  const cancel = button(t("common.cancel"), "btn ghost btn-small", () => (contact ? showContact(contact) : closePanel()));
  fillPanel(parts, contact ? t("contacts.edit_title") : t("contacts.new"), [
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
  fillPanel(parts, t("contacts.delete_title", { name: contact.name }), [confirm.node]);
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
    loadingText: t("contacts.loading"),
    failureText,
  });
  $("contacts-search").addEventListener("input", searchSoon);
  $("contacts-more").addEventListener("click", () => list.loadMore());
  $("btn-contact-new").addEventListener("click", () => showForm(null));
  $("contacts-panel-close").addEventListener("click", closePanel);
  closeOnEscape("view-contacts", parts, closePanel);
}
