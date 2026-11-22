// What Ctrl+K lists: the app's actions and the conversations, filtered by what the
// operator types (no accents, no case). Pure data, so it is tested without a window.

export const ACTIONS = [
  { id: "new-chat", label: "Nuevo chat", hint: "Ctrl+N" },
  { id: "search", label: "Buscar en conversaciones", hint: "Ctrl+F" },
  { id: "new-project", label: "Nuevo proyecto con carpeta" },
  { id: "nav:chat", label: "Ir a Chat" },
  { id: "nav:agents", label: "Ir a Equipo" },
  { id: "nav:agenda", label: "Ir a Agenda" },
  { id: "nav:calendar", label: "Ir a Calendario" },
  { id: "nav:notes", label: "Ir a Notas" },
  { id: "nav:contacts", label: "Ir a Contactos" },
  { id: "nav:alerts", label: "Ir a Avisos" },
  { id: "nav:devices", label: "Ir a Dispositivos" },
  { id: "nav:settings", label: "Ir a Ajustes" },
];

const MAX_ITEMS = 9;

export function fold(text) {
  return String(text || "")
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .trim();
}

/** Actions first, then conversations; each word typed must appear in the label. */
export function paletteItems(query, conversations = []) {
  const words = fold(query).split(/\s+/).filter(Boolean);
  const matches = (label) => words.every((word) => fold(label).includes(word));
  const actions = ACTIONS.filter((action) => matches(action.label)).map((action) => ({
    kind: "action",
    ...action,
  }));
  const chats = words.length
    ? conversations
        .filter((conversation) => matches(conversation.title))
        .map((conversation) => ({
          kind: "conversation",
          id: `chat:${conversation.id}`,
          label: conversation.title,
          hint: "Conversación",
        }))
    : [];
  return [...actions, ...chats].slice(0, MAX_ITEMS);
}
