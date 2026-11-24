import { t } from "./i18n.js";

// The title the host writes for a chat created with no title. The contract does not say whether
// a title came from the user or the host, so the assumption is: an empty chat (last_seq 0) named
// exactly this is the host's default. Only that case is shown in the Master's language.
const HOST_DEFAULT_TITLE = "Nuevo chat";
// The host's notice conversation is found by this literal title (crates/host/src/pairing_notice.rs).
// Only the label changes; the stored title and the host's search keep the Spanish text.
const HOST_NOTICE_TITLE = "Avisos del sistema";

export function conversationTitle(conversation) {
  if (conversation.title === HOST_DEFAULT_TITLE && conversation.last_seq === 0) return t("chat.empty.new");
  if (conversation.title === HOST_NOTICE_TITLE) return t("chat.notices_title");
  return conversation.title;
}

// The state chip of one row of the history: what needs the operator first, then what
// is running. Unread replies keep their dot and count in the meta line.
export function rowBadge(conversation) {
  if ((conversation.waiting_approvals || 0) > 0) {
    const count = conversation.waiting_approvals;
    return { kind: "approval", text: count > 1 ? t("chat.row.approvals_other", { n: count }) : t("chat.row.approvals_one") };
  }
  if ((conversation.active_runs || 0) > 0) return { kind: "working", text: t("chat.row.working") };
  return null;
}

// An empty «Nuevo chat» (no message yet, nothing waiting) is noise in the history. The
// active one always stays, so the operator sees where they are. Only `last_seq` 0 says «no
// message»; a row without that field hides nothing, as an older host sends it.
export function isBlankChat(conversation, selectedId) {
  if (conversation.id === selectedId) return false;
  if (!("last_seq" in conversation) || conversation.last_seq !== 0) return false;
  if ((conversation.unread || 0) > 0 || (conversation.waiting_approvals || 0) > 0) return false;
  if ((conversation.active_runs || 0) > 0 || conversation.pinned) return false;
  return true;
}
