// What Ctrl+K lists: the app's actions and the conversations, filtered by what the
// operator types (no accents, no case). Pure data, so it is tested without a window.

import { t } from "./i18n.js";
import { conversationTitle } from "./chat-row-model.js";

export const ACTIONS = [
  { id: "new-chat", label: t("palette.act_new_chat"), hint: "Ctrl+N" },
  { id: "search", label: t("palette.act_search"), hint: "Ctrl+F" },
  { id: "new-project", label: t("palette.act_new_project") },
  { id: "nav:chat", label: t("palette.go_chat") },
  { id: "nav:agents", label: t("palette.go_agents") },
  { id: "nav:agenda", label: t("palette.go_agenda") },
  { id: "nav:calendar", label: t("palette.go_calendar") },
  { id: "nav:notes", label: t("palette.go_notes") },
  { id: "nav:contacts", label: t("palette.go_contacts") },
  { id: "nav:alerts", label: t("palette.go_alerts") },
  { id: "nav:devices", label: t("palette.go_devices") },
  { id: "nav:settings", label: t("palette.go_settings") },
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
          label: conversationTitle(conversation),
          hint: t("palette.hint_chat"),
        }))
    : [];
  return [...actions, ...chats].slice(0, MAX_ITEMS);
}
