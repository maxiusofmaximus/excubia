// «Contactos» and «Notas»: the rules that need no DOM, so tooling/agenda-model.test.mjs
// can run them. Local limits mirror the host's; the host still has the last word.

export const CONTACT_LIMITS = { name: 120, aliases: 10, addresses: 10, contacts: 1000 };
export const NOTE_LIMITS = { subject: 120, bodyBytes: 32 * 1024, notes: 2000 };
export const ADDRESS_PATTERN = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

const SOURCE_LABELS = {
  phone: "del teléfono",
  user_message: "dictada",
  alfred: "Alfred",
  app: "creada aquí",
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
  if (elapsed < MINUTE) return "ahora";
  if (elapsed < HOUR) return `hace ${Math.floor(elapsed / MINUTE)} min`;
  if (elapsed < DAY) return `hace ${Math.floor(elapsed / HOUR)} h`;
  const days = Math.floor(elapsed / DAY);
  if (days === 1) return "ayer";
  if (days < 7) return `hace ${days} días`;
  return new Date(then).toLocaleDateString("es-ES", { day: "numeric", month: "short" });
}

// A note without subject is named by its first words, so the list never shows a gap.
const TITLE_WORDS = 8;

export function noteTitle(note) {
  const subject = (note.subject || "").trim();
  if (subject) return subject;
  const words = (note.excerpt || "").split(/\s+/).filter(Boolean);
  if (!words.length) return "Nota sin texto";
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
  if (!name) return problem("name", "Escribe un nombre.");
  if ([...name].length > CONTACT_LIMITS.name) {
    return problem("name", `El nombre admite ${CONTACT_LIMITS.name} caracteres como máximo.`);
  }
  if (draft.aliases.length > CONTACT_LIMITS.aliases) {
    return problem("aliases", `Un contacto admite ${CONTACT_LIMITS.aliases} alias como máximo.`);
  }
  if (draft.aliases.some((alias) => [...alias].length > CONTACT_LIMITS.name)) {
    return problem("aliases", `Cada alias admite ${CONTACT_LIMITS.name} caracteres como máximo.`);
  }
  if (draft.addresses.length > CONTACT_LIMITS.addresses) {
    return problem("addresses", `Un contacto admite ${CONTACT_LIMITS.addresses} direcciones como máximo.`);
  }
  const malformed = draft.addresses.find((address) => !ADDRESS_PATTERN.test(address));
  if (malformed !== undefined) {
    return problem("addresses", `«${malformed}» no tiene la forma nombre@dominio.com, sin espacios.`);
  }
  return null;
}

// The first problem of a note, or null. The body limit counts bytes, as the host does.
export function noteProblem(draft) {
  if (!draft.body.trim()) return problem("body", "Escribe el texto de la nota.");
  if (draft.title && [...draft.title].length > NOTE_LIMITS.subject) {
    return problem("title", `El asunto admite ${NOTE_LIMITS.subject} caracteres como máximo.`);
  }
  if (new TextEncoder().encode(draft.body).length > NOTE_LIMITS.bodyBytes) {
    return problem("body", "El texto admite 32 KiB como máximo.");
  }
  return null;
}

// «Ana Ruiz» out of the host's «the address already belongs to the contact Ana Ruiz».
function ownerName(message) {
  const match = /contact (.+)$/.exec(message || "");
  return match ? match[1].trim() : "";
}

const ARGUMENT_TEXT = {
  name: `El nombre admite ${CONTACT_LIMITS.name} caracteres como máximo.`,
  title: `El asunto admite ${NOTE_LIMITS.subject} caracteres como máximo.`,
  aliases: `Un contacto admite ${CONTACT_LIMITS.aliases} alias como máximo.`,
  addresses: `Un contacto admite ${CONTACT_LIMITS.addresses} direcciones como máximo.`,
};

const REFUSAL_TEXT = {
  address_taken: (failure) => {
    const owner = ownerName(failure.message);
    return owner
      ? `Esa dirección ya es de «${owner}». Quítala de ese contacto o usa otra.`
      : "Esa dirección ya pertenece a otro contacto.";
  },
  alias_taken: () => "Ese alias ya nombra a otro contacto.",
  address_limit: () => `Un contacto admite ${CONTACT_LIMITS.addresses} direcciones como máximo.`,
  alias_limit: () => `Un contacto admite ${CONTACT_LIMITS.aliases} alias como máximo.`,
  contact_limit: () => `Ya tienes ${CONTACT_LIMITS.contacts} contactos. Borra alguno antes de añadir otro.`,
  note_limit: () => `Ya tienes ${NOTE_LIMITS.notes} notas. Borra alguna antes de crear otra.`,
  invalid_address: () => "Una dirección no tiene la forma nombre@dominio.com, sin espacios.",
  empty_message: () => "La nota no puede estar vacía.",
  invalid_body: () => "El texto es demasiado largo (32 KiB como máximo).",
  unknown_message: () => "Ese mensaje ya no está en el chat.",
  not_found: () => "Eso ya no existe. Recarga la lista.",
  invalid_arguments: (failure) => ARGUMENT_TEXT[failure.field] || "Uno de los datos no es válido.",
};

// The operator's words for a failure of the agenda. Unknown codes keep a generic line.
export function agendaErrorText(failure) {
  if (!failure) return "No se pudo completar la operación.";
  if (failure.kind === "offline") return "Sin conexión con el host. Lo cargado sigue visible.";
  if (failure.kind === "agendaRefused") {
    const words = REFUSAL_TEXT[failure.code];
    if (words) return words(failure);
  }
  return "El host rechazó la petición.";
}

// A refusal of the permission: the device lacks the grant the action needs.
export function permissionText(permission) {
  return `Este dispositivo no tiene el permiso ${permission}. Pídeselo al host.`;
}
