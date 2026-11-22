// Builds the DOM of an agent reply from markdown-model.js. Every piece of text goes in
// with textContent; a link is an <a> whose click asks the shell to open the page in the
// system browser, so the app never navigates away.
import { parseBlocks, parseInline, safeUrl } from "./markdown-model.js";

function inline(parent, text) {
  for (const run of parseInline(text)) {
    if (run.type === "text") {
      parent.append(document.createTextNode(run.text));
      continue;
    }
    const tag = { strong: "strong", em: "em", code: "code", link: "a" }[run.type];
    const node = document.createElement(tag);
    node.textContent = run.text;
    if (run.type === "link") {
      node.href = run.href;
      node.title = run.href;
      node.rel = "noopener noreferrer";
      node.className = "md-link";
    }
    parent.append(node);
  }
}

function lines(parent, rows) {
  rows.forEach((row, index) => {
    if (index) parent.append(document.createElement("br"));
    inline(parent, row);
  });
}

export function renderMarkdown(text) {
  const root = document.createElement("div");
  root.className = "md";
  for (const block of parseBlocks(text)) {
    let node;
    if (block.type === "paragraph") {
      node = document.createElement("p");
      lines(node, block.lines);
    } else if (block.type === "heading") {
      node = document.createElement(`h${block.level + 2}`);
      inline(node, block.text);
    } else if (block.type === "list") {
      node = document.createElement(block.ordered ? "ol" : "ul");
      for (const item of block.items) {
        const li = document.createElement("li");
        inline(li, item);
        node.append(li);
      }
    } else if (block.type === "code") {
      node = document.createElement("pre");
      const code = document.createElement("code");
      code.textContent = block.text;
      node.append(code);
    } else if (block.type === "quote") {
      node = document.createElement("blockquote");
      lines(node, block.lines);
    } else {
      node = document.createElement("hr");
    }
    root.append(node);
  }
  return root;
}

// One listener for every reply: a click on a link opens it outside the app.
export function installLinkOpener(container, open) {
  container.addEventListener("click", (event) => {
    const link = event.target.closest?.("a.md-link");
    if (!link) return;
    event.preventDefault();
    const url = safeUrl(link.getAttribute("href"));
    if (url) open(url);
  });
}
