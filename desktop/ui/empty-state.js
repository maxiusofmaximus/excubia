// The DOM of the empty and error states: the model's line icon, its one sentence and, when
// the screen has one, its single action. Every screen that has nothing to show uses this.

import { element } from "./chatdom.js";
import { ICONS, emptyStateSpec, errorStateSpec } from "./empty-state-model.js";

const SVG_NS = "http://www.w3.org/2000/svg";

function lineIcon(name) {
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("class", "state-icon");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("aria-hidden", "true");
  for (const [tag, attrs] of ICONS[name]) {
    const shape = document.createElementNS(SVG_NS, tag);
    for (const [key, value] of Object.entries(attrs)) shape.setAttribute(key, String(value));
    svg.append(shape);
  }
  return svg;
}

function render(spec, onAction) {
  const node = element("div", spec.tone === "error" ? "empty-state is-error" : "empty-state");
  node.append(lineIcon(spec.icon), element("p", null, spec.text));
  if (spec.action) {
    const button = element("button", "btn secondary btn-small", spec.action);
    button.type = "button";
    button.addEventListener("click", onAction);
    node.append(button);
  }
  return node;
}

// Nothing to show yet: the icon, one sentence and, when it helps, one action.
export function emptyState(options, onAction = () => {}) {
  return render(emptyStateSpec(options), onAction);
}

// A read that failed: its sentence and «Reintentar», which runs `retry` again.
export function errorState(text, retry) {
  return render(errorStateSpec(text), retry);
}
