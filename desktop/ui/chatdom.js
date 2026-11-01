// DOM helpers shared by the chat screens: one place for the element and button shapes.

export function element(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

export function action(label, data, className = "btn ghost btn-small") {
  const button = element("button", className, label);
  button.type = "button";
  for (const [name, value] of Object.entries(data)) {
    button.dataset[name] = value;
  }
  return button;
}
