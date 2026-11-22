// The sign-in kinds as a radio group of our own, with the arrow keys of a native one.
// Ajustes → Correo asks for the kind of the account before it signs in.

import { element } from "./approval-card.js";
import { SIGN_IN_KINDS, radioStep } from "./mail-model.js";

export function kindPicker() {
  const group = element("div", "mail-kinds");
  group.setAttribute("role", "radiogroup");
  group.setAttribute("aria-label", "Tipo de cuenta");
  const options = Object.entries(SIGN_IN_KINDS).map(([kind, { label }]) => {
    const option = element("button", "mail-kind");
    option.type = "button";
    option.setAttribute("role", "radio");
    option.dataset.kind = kind;
    option.append(element("span", "mail-kind-dot"), element("span", null, label));
    return option;
  });
  let index = 0;
  const paint = () =>
    options.forEach((option, at) => {
      option.setAttribute("aria-checked", String(at === index));
      option.tabIndex = at === index ? 0 : -1;
    });
  group.addEventListener("click", (event) => {
    const option = event.target.closest(".mail-kind");
    if (!option) return;
    index = options.indexOf(option);
    paint();
  });
  group.addEventListener("keydown", (event) => {
    const next = radioStep(index, event.key, options.length);
    if (next === null) return;
    event.preventDefault();
    index = next;
    paint();
    options[index].focus();
  });
  group.append(...options);
  paint();
  return { group, selected: () => options[index].dataset.kind };
}
