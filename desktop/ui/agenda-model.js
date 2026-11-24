// «Contactos» and «Notas»: the rules that need no DOM, so tooling/agenda-model.test.mjs
// can run them. Local limits mirror the host's; the host still has the last word.

import { locale, t } from "./i18n.js";
import { friendlyError, OFFLINE_TEXT } from "./friendly-error.js";

export const CONTACT_LIMITS = { name: 120, aliases: 10, addresses: 10, contacts: 1000 };
export const NOTE_LIMITS = { subject: 120, bodyBytes: 32 * 1024, notes: 2000 };
export const ADDRESS_PATTERN = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

const SOURCE_LABELS = {
  phone: t("agenda.source_phone"),
  user_message: t("agenda.source_dictated"),
  alfred: "Alfred",
  app: t("agenda.source_app"),
};

export function sourceLabel(source) {
  return SOURCE_LABELS[source] || "";
}

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

// «hace 5 min», «hace 2 h», «ayer», «hace 3 días»; older than a week, the day.
export function relativeDate(iso, now = Date.now()) {
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return "";
  const elapsed = now - then;
  if (elapsed < MINUTE) return t("agenda.rel_now");
  if (elapsed < HOUR) return t("agenda.rel_min", { n: Math.floor(elapsed / MINUTE) });
  if (elapsed < DAY) return t("agenda.rel_hours", { n: Math.floor(elapsed / HOUR) });
  const days = Math.floor(elapsed / DAY);
  if (days === 1) return t("agenda.rel_yesterday");
  if (days < 7) return t("agenda.rel_days", { n: days });
  return new Date(then).toLocaleDateString(locale(), { day: "numeric", month: "short" });
}

// A note without subject is named by its first words, so the list never shows a gap.
const TITLE_WORDS = 8;

export function noteTitle(note) {
  const subject = (note.subject || "").trim();
  if (subject) return subject;
  const words = (note.excerpt || "").split(/\s+/).filter(Boolean);
  if (!words.length) return t("agenda.note_untitled");
  const head = words.slice(0, TITLE_WORDS).join(" ");
  return words.length > TITLE_WORDS ? `${head}…` : head;
}

// The row the list keeps for a note that was just read or written: the host's own
// shape, with the body cut to one line as the host cuts it.
const EXCERPT_CHARS = 200;

export function noteSummary(note) {
  const excerpt = note.body.replace(/\s+/g, " ").trim();
  return {
    id: note.id,
    subject: note.subject,
    excerpt: [...excerpt].slice(0, EXCERPT_CHARS).join(""),
    updated_at: note.updated_at,
  };
}

export function contactPrimaryAddress(contact) {
  return contact.addresses[0]?.address || "";
}

// What the form's text boxes hold: commas or new lines separate the entries.
export function parseList(text) {
  return text
    .split(/[,\n]/)
    .map((entry) => entry.trim())
    .filter(Boolean);
}

function problem(field, message) {
  return { field, message };
}

// The first problem of a contact, or null. Addresses are compared in lowercase
// by the host, so the form sends them that way.
export function contactProblem(draft) {
  const name = draft.name.trim();
  if (!name) return problem("name", t("agenda.name_required"));
  if ([...name].length > CONTACT_LIMITS.name) {
    return problem("name", t("agenda.name_too_long", { max: CONTACT_LIMITS.name }));
  }
  if (draft.aliases.length > CONTACT_LIMITS.aliases) {
    return problem("aliases", t("agenda.aliases_max", { max: CONTACT_LIMITS.aliases }));
  }
  if (draft.aliases.some((alias) => [...alias].length > CONTACT_LIMITS.name)) {
    return problem("aliases", t("agenda.alias_too_long", { max: CONTACT_LIMITS.name }));
  }
  if (draft.addresses.length > CONTACT_LIMITS.addresses) {
    return problem("addresses", t("agenda.addresses_max", { max: CONTACT_LIMITS.addresses }));
  }
  const malformed = draft.addresses.find((address) => !ADDRESS_PATTERN.test(address));
  if (malformed !== undefined) {
    return problem("addresses", t("agenda.address_format", { address: malformed }));
  }
  return null;
}

// The first problem of a note, or null. The body limit counts bytes, as the host does.
export function noteProblem(draft) {
  if (!draft.body.trim()) return problem("body", t("agenda.note_body_required"));
  if (draft.title && [...draft.title].length > NOTE_LIMITS.subject) {
    return problem("title", t("agenda.subject_too_long", { max: NOTE_LIMITS.subject }));
  }
  if (new TextEncoder().encode(draft.body).length > NOTE_LIMITS.bodyBytes) {
    return problem("body", t("agenda.body_too_long"));
  }
  return null;
}

// «Ana Ruiz» out of the host's «the address already belongs to the contact Ana Ruiz».
function ownerName(message) {
  const match = /contact (.+)$/.exec(message || "");
  return match ? match[1].trim() : "";
}

const ARGUMENT_TEXT = {
  name: t("agenda.name_too_long", { max: CONTACT_LIMITS.name }),
  title: t("agenda.subject_too_long", { max: NOTE_LIMITS.subject }),
  aliases: t("agenda.aliases_max", { max: CONTACT_LIMITS.aliases }),
  addresses: t("agenda.addresses_max", { max: CONTACT_LIMITS.addresses }),
};

const REFUSAL_TEXT = {
  address_taken: (failure) => {
    const owner = ownerName(failure.message);
    return owner ? t("agenda.err_address_taken_owner", { owner }) : t("agenda.err_address_taken");
  },
  alias_taken: () => t("agenda.err_alias_taken"),
  address_limit: () => t("agenda.addresses_max", { max: CONTACT_LIMITS.addresses }),
  alias_limit: () => t("agenda.aliases_max", { max: CONTACT_LIMITS.aliases }),
  contact_limit: () => t("agenda.err_contact_limit", { n: CONTACT_LIMITS.contacts }),
  note_limit: () => t("agenda.err_note_limit", { n: NOTE_LIMITS.notes }),
  invalid_address: () => t("agenda.err_invalid_address"),
  empty_message: () => t("agenda.err_empty_message"),
  invalid_body: () => t("agenda.err_invalid_body"),
  unknown_message: () => t("agenda.err_unknown_message"),
  not_found: () => t("agenda.err_not_found"),
  invalid_arguments: (failure) => ARGUMENT_TEXT[failure.field] || t("agenda.err_invalid_data"),
};

// The operator's words for a failure of the agenda. Unknown codes keep a generic line.
export function agendaErrorText(failure) {
  if (failure?.kind === "offline") return `${OFFLINE_TEXT} ${t("agenda.loaded_visible")}`;
  if (failure?.kind === "agendaRefused") {
    const words = REFUSAL_TEXT[failure.code];
    if (words) return words(failure);
  }
  return friendlyError(failure);
}

// A refusal of the permission: the device lacks the grant the action needs.
export function permissionText(permission) {
  return t("agenda.err_permission", { permission });
}
