// The chat's team panel («Equipo», the right side). Stored "1" means the operator folded it, "0" that he
// opened it. With no choice yet it opens only when something there waits for him.
export function teamPanelOpen(stored, needsAttention) {
  if (stored === "0") return true;
  if (stored === "1") return false;
  return Boolean(needsAttention);
}
