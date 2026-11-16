// A small Markdown reader for agent replies. It returns plain data (blocks and inline
// runs), never HTML: the view builds DOM nodes from it with textContent, so a reply can
// never inject markup. Only http(s) links survive; any other target stays as text.

const FENCE = /^```/;
const HEADING = /^(#{1,3})\s+(.*)$/;
const BULLET = /^\s*[-*+]\s+(.*)$/;
const NUMBERED = /^\s*\d+[.)]\s+(.*)$/;
const QUOTE = /^>\s?(.*)$/;
const RULE = /^\s*([-*_])(\s*\1){2,}\s*$/;

export function safeUrl(raw) {
  const url = String(raw || "").trim();
  return /^https?:\/\/[^\s<>"'`]+$/i.test(url) ? url : null;
}

/** Splits the text into blocks: paragraph, heading, list, code, quote, rule. */
export function parseBlocks(text) {
  const lines = String(text || "").replace(/\r\n?/g, "\n").split("\n");
  const blocks = [];
  let paragraph = [];
  const flush = () => {
    if (paragraph.length) blocks.push({ type: "paragraph", lines: paragraph });
    paragraph = [];
  };
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    if (FENCE.test(line)) {
      flush();
      const code = [];
      i += 1;
      while (i < lines.length && !FENCE.test(lines[i])) {
        code.push(lines[i]);
        i += 1;
      }
      blocks.push({ type: "code", text: code.join("\n") });
      continue;
    }
    if (!line.trim()) {
      flush();
      continue;
    }
    if (RULE.test(line)) {
      flush();
      blocks.push({ type: "rule" });
      continue;
    }
    const heading = HEADING.exec(line);
    if (heading) {
      flush();
      blocks.push({ type: "heading", level: heading[1].length, text: heading[2] });
      continue;
    }
    const bullet = BULLET.exec(line);
    const numbered = bullet ? null : NUMBERED.exec(line);
    if (bullet || numbered) {
      flush();
      const ordered = Boolean(numbered);
      const last = blocks[blocks.length - 1];
      const item = (bullet || numbered)[1];
      if (last && last.type === "list" && last.ordered === ordered) last.items.push(item);
      else blocks.push({ type: "list", ordered, items: [item] });
      continue;
    }
    const quote = QUOTE.exec(line);
    if (quote) {
      flush();
      const last = blocks[blocks.length - 1];
      if (last && last.type === "quote") last.lines.push(quote[1]);
      else blocks.push({ type: "quote", lines: [quote[1]] });
      continue;
    }
    paragraph.push(line);
  }
  flush();
  return blocks;
}

const INLINE =
  /(`[^`]+`)|(\*\*[^*]+\*\*|__[^_]+__)|(\*[^*\s][^*]*\*|_[^_\s][^_]*_)|(\[[^\]]+\]\([^)\s]+\))|(https?:\/\/[^\s<>"'`)]+)/g;

/** Splits one line into runs: text, strong, em, code, link. */
export function parseInline(text) {
  const runs = [];
  const source = String(text || "");
  let at = 0;
  for (const match of source.matchAll(INLINE)) {
    if (match.index > at) runs.push({ type: "text", text: source.slice(at, match.index) });
    const [whole, code, strong, em, link, bare] = match;
    if (code) runs.push({ type: "code", text: code.slice(1, -1) });
    else if (strong) runs.push({ type: "strong", text: strong.slice(2, -2) });
    else if (em) runs.push({ type: "em", text: em.slice(1, -1) });
    else if (link) {
      const split = link.indexOf("](");
      const label = link.slice(1, split);
      const href = safeUrl(link.slice(split + 2, -1));
      runs.push(href ? { type: "link", text: label, href } : { type: "text", text: whole });
    } else if (bare) {
      // A sentence's closing punctuation is not part of the address.
      const trimmed = bare.replace(/[.,;:!?]+$/, "");
      runs.push({ type: "link", text: trimmed, href: trimmed });
      if (trimmed.length < bare.length) runs.push({ type: "text", text: bare.slice(trimmed.length) });
    }
    at = match.index + whole.length;
  }
  if (at < source.length) runs.push({ type: "text", text: source.slice(at) });
  return runs;
}
