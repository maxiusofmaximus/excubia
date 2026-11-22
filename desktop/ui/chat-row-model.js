// The state chip of one row of the history: what needs the operator first, then what
// is running. Unread replies keep their dot and count in the meta line.
export function rowBadge(conversation) {
  if ((conversation.waiting_approvals || 0) > 0) {
    const count = conversation.waiting_approvals;
    return { kind: "approval", text: count > 1 ? `${count} piden tu OK` : "Pide tu OK" };
  }
  if ((conversation.active_runs || 0) > 0) return { kind: "working", text: "En curso" };
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
