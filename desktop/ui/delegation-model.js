// The state chip and the one-line task of a delegation card, free of the DOM so the tests can run them.
import { t } from "./i18n.js";

const STATUS = {
  queued: { label: t("delegation.status_queued"), tone: "queued" },
  running: { label: t("delegation.status_running"), tone: "running" },
  waiting: { label: t("delegation.status_waiting"), tone: "waiting" },
  done: { label: t("delegation.status_done"), tone: "done" },
  failed: { label: t("delegation.status_failed"), tone: "failed" },
  cancelled: { label: t("delegation.status_cancelled"), tone: "cancelled" },
  interrupted: { label: t("delegation.status_interrupted"), tone: "interrupted" },
};

// A finished task is closed by Alfred's line («Ernesto terminó: …»), so its card would repeat
// it and the thread leaves it out. «Ver detalle» stays under Equipo.
export function showInThread(entry) {
  return Boolean(entry) && entry.status !== "done";
}

export function statusMeta(status) {
  return STATUS[status] || { label: status || t("agents.no_state"), tone: "unknown" };
}

// An operator's refusal leaves the job undone, so it never reads «Hecho». The host
// adds `reason` to the entry; an older host without it keeps the plain status.
export function cardState(entry) {
  if (entry.reason === "operator_denied") return { label: t("delegation.not_done"), tone: "cancelled" };
  return statusMeta(entry.status || "queued");
}

// The short task for the line: the first line of the summary, plain. A summary that
// carries a path or a code span is a report, not a task, so the line leaves it out. The
// text is never cut here; the line truncates it visually and keeps the whole in its title.
export function cardTask(summary) {
  const text = String(summary || "").trim();
  if (!text) return "";
  if (/[A-Za-z]:\\|\\\\\?\\|[\w.-]+[\\/][\w.-]+[\\/]/.test(text) || text.includes("`")) return "";
  const line = text.split(/\r?\n/)[0].replace(/\*\*/g, "").trim();
  // A task that does not fit the line would end in «…»: it waits whole in «Ver detalle» instead.
  return line.length <= TASK_LINE_MAX ? line : "";
}

const TASK_LINE_MAX = 70;

const ACTIVE_STATUSES = ["queued", "running", "waiting"];

// The Equipo panel's list: the last `limit` cards of the conversation that are no longer running,
// newest first. A denied task is finished even when its status still reads as active.
export function finishedTasks(entries, limit = 5) {
  return (entries || [])
    .filter((item) => item && item.kind === "card" && item.entry)
    .map((item) => item.entry)
    .filter((entry) => !ACTIVE_STATUSES.includes(entry.status || "queued") || entry.reason === "operator_denied")
    .slice(-limit)
    .reverse();
}

// The detail's kicker: a task, or a subtask when it was handed down from another task.
export function detailKicker() {
  return t("delegation.task_kicker");
}

// The project's name for the detail; empty when the UI does not know the id, so the id is never shown.
export function projectName(projects, id) {
  const found = (projects || []).find((project) => project && project.id === id);
  return found && found.name ? found.name : "";
}

// A task line that only repeats the card's title adds nothing, so the card leaves it out.
export function taskRepeatsTitle(task, title) {
  const a = String(task || "").trim().toLowerCase();
  const b = String(title || "").trim().toLowerCase();
  return Boolean(a) && Boolean(b) && (a === b || a.startsWith(b));
}
