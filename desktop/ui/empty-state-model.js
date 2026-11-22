// The rules of the empty and error states, free of DOM so tooling/empty-state-model.test.mjs
// can check them: which line icon, which one sentence, which single action. The icons are
// the sidebar's own strokes (24×24, currentColor) so every screen draws the same hand.

export const ICONS = {
  chat: [["path", { d: "M4.5 5h15a1 1 0 0 1 1 1v9.5a1 1 0 0 1-1 1H10l-4.5 3.5V16.5h-1a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1z" }]],
  calendar: [
    ["rect", { x: 3.5, y: 5, width: 17, height: 15, rx: 3 }],
    ["path", { d: "M3.5 10h17M8 3v4M16 3v4" }],
  ],
  notes: [
    ["path", { d: "M7 3.5h8l4 4V20a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1V4.5a1 1 0 0 1 1-1z" }],
    ["path", { d: "M9 12h6M9 16h4" }],
  ],
  contacts: [
    ["rect", { x: 4, y: 3.5, width: 16, height: 17, rx: 3 }],
    ["circle", { cx: 12, cy: 10, r: 2.6 }],
    ["path", { d: "M7.5 17c.8-2 2.6-3 4.5-3s3.7 1 4.5 3" }],
  ],
  alerts: [
    ["path", { d: "M6 17V11a6 6 0 1 1 12 0v6l1.5 2h-15z" }],
    ["path", { d: "M10 21h4" }],
  ],
  devices: [
    ["rect", { x: 3, y: 4, width: 18, height: 12, rx: 2.5 }],
    ["path", { d: "M8 20h8M12 16v4" }],
  ],
  team: [
    ["circle", { cx: 9, cy: 8, r: 3.2 }],
    ["path", { d: "M3 20c0-3.3 2.7-5.5 6-5.5s6 2.2 6 5.5" }],
    ["path", { d: "M16 5.5a3 3 0 0 1 0 5.6M18 14.8c1.8.6 3 2.2 3 5.2" }],
  ],
  plan: [
    ["rect", { x: 5, y: 3.5, width: 14, height: 17, rx: 2.5 }],
    ["path", { d: "M9 9.5l1.6 1.6L14 8M9 15.5h6" }],
  ],
  restore: [
    ["path", { d: "M4.5 12a7.5 7.5 0 1 0 2.2-5.3" }],
    ["path", { d: "M4.5 4.8v3.9h3.9" }],
    ["path", { d: "M12 8.5V12l2.5 1.5" }],
  ],
  clock: [
    ["circle", { cx: 12, cy: 12, r: 8.5 }],
    ["path", { d: "M12 7.5V12l3 2" }],
  ],
  alert: [
    ["circle", { cx: 12, cy: 12, r: 9 }],
    ["path", { d: "M12 7.5v5.5M12 16.5v.01" }],
  ],
};

const SHAPES = new Set(["path", "rect", "circle"]);

// A state is read by the operator: one sentence, ending in a full stop, with no
// blank text and at most one action. A malformed state fails loudly in the tests.
export function emptyStateSpec({ icon, text, action = null }) {
  if (!ICONS[icon] || ICONS[icon].some(([tag]) => !SHAPES.has(tag))) throw new Error(`icono desconocido: ${icon}`);
  if (typeof text !== "string" || !text.trim()) throw new Error("un estado necesita una frase");
  if (!/[.?!]$/.test(text.trim())) throw new Error(`la frase no acaba en punto: ${text}`);
  if (action !== null && (typeof action !== "string" || !action.trim())) throw new Error("la acción necesita texto");
  return { tone: "empty", icon, text: text.trim(), action };
}

// A failure keeps its sentence; the one way out is «Reintentar».
export function errorStateSpec(text) {
  return { ...emptyStateSpec({ icon: "alert", text }), tone: "error", action: "Reintentar" };
}
