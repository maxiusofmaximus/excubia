// The agent's output as the operator reads it, free of the DOM so the tests can run it. A
// delegation order is a JSON line the host writes: it reads «Encargo de Alfred: <task>».

import { displayPath } from "./format.js";
import { t } from "./i18n.js";

export function plainOutput(text) {
  return String(text || "")
    .split("\n")
    .map(plainLine)
    .join("\n");
}

function plainLine(line) {
  const order = orderOf(line);
  if (order) return t("output.order", { from: order.from || "Alfred", task: plainWords(order.task) });
  return plainWords(line);
}

// A line that parses as a delegation order: a JSON object with a task and a delegation id.
function orderOf(line) {
  const trimmed = line.trim();
  if (!trimmed.startsWith("{")) return null;
  try {
    const order = JSON.parse(trimmed);
    return order && typeof order.task === "string" && "delegation_id" in order ? order : null;
  } catch {
    return null;
  }
}

// A Windows path keeps only its last name; code marks and the host's dlg_ ids do not reach the operator.
function plainWords(text) {
  return String(text)
    .replace(/`/g, "")
    .replace(/(?:\\\\\?\\)?[A-Za-z]:\\[^\s"'`]+/g, (path) => displayPath(path).split(/[\\/]/).pop())
    .replace(/\bdlg_\w+/g, "")
    .replace(/ {2,}/g, " ")
    .trim();
}
