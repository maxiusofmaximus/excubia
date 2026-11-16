// The state chip of one row of the history: what needs the operator first, then what
// is running. Unread replies keep their dot and count in the meta line.
export function rowBadge(conversation) {
  if ((conversation.waiting_approvals || 0) > 0) {
    const count = conversation.waiting_approvals;
    return { kind: "approval", text: count > 1 ? `${count} aprobaciones` : "Aprobación" };
  }
  if ((conversation.active_runs || 0) > 0) return { kind: "working", text: "Trabajando" };
  return null;
}
