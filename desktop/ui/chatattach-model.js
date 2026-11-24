// Attachments before a send, free of the DOM so the tests can run them: the limit of
// four, the kind of each file, the states of one file, the size a picture is reduced
// to, the point a failed chunk resumes from and the words of each refusal.

import { locale, t } from "./i18n.js";

export const MAX_ATTACHMENTS = 4;
export const MAX_BYTES = 10 * 1024 * 1024;
export const CHUNK_BYTES = 128 * 1024;
export const MAX_SIDE = 2048;
export const WEBP_QUALITY = 0.85;
export const CHUNK_ATTEMPTS = 3;
// A refusal line leaves on its own after this long (ms).
export const NOTICE_MS = 6000;
// Pictures are re-encoded; documents go up exactly as picked.
const IMAGE_EXTENSIONS = ["png", "jpg", "jpeg", "webp"];
const DOCUMENT_EXTENSIONS = ["xlsx", "docx", "pptx", "pdf", "csv", "txt", "md", "json"];
export const ACCEPT_ATTR = [...IMAGE_EXTENSIONS, ...DOCUMENT_EXTENSIONS].map((ext) => `.${ext}`).join(",");
// The body the host stores for a message that is only attachments.
export const ATTACHMENT_BODY = "(adjunto)";

const DECODE_FAILED = t("chatattach.err_decode");
const ATTACHMENT_TEXT = {
  // Names the families IMAGE_EXTENSIONS and DOCUMENT_EXTENSIONS admit; the tests keep it in step.
  unsupported_type: t("chatattach.err_unsupported_type"),
  invalid_image: DECODE_FAILED,
  timeout: DECODE_FAILED,
  worker_failed: DECODE_FAILED,
  image_too_large: t("chatattach.err_image_too_large"),
  dimensions_too_large: t("chatattach.err_image_too_large"),
  input_too_large: t("chatattach.err_image_too_large"),
  file_too_large: t("chatattach.err_file_too_large"),
  conversation_quota: t("chatattach.err_conversation_quota"),
  host_quota: t("chatattach.err_host_quota"),
  too_many_attachments: t("chatattach.err_too_many", { max: MAX_ATTACHMENTS }),
  folder: t("chatattach.err_folder"),
  unknown_attachment: t("chatattach.err_unknown"),
  no_such_attachment: t("chatattach.err_upload_gone"),
  not_uploading: t("chatattach.err_upload_gone"),
  already_linked: t("chatattach.err_already_linked"),
  not_ready: t("chatattach.err_not_ready"),
  size_mismatch: t("chatattach.err_mismatch"),
  hash_mismatch: t("chatattach.err_mismatch"),
  unsupported_device: t("chatattach.err_device"),
};
const FALLBACK_TEXT = t("chatattach.err_fallback");
// Only these failures can succeed on a second try; a refusal never can.
const RETRYABLE = ["offline", "host", "agentBusy"];

// The lower-case extension of a file name, or "" when it has none.
export function extensionOf(name) {
  const text = String(name || "");
  const dot = text.lastIndexOf(".");
  return dot < 0 ? "" : text.slice(dot + 1).toLowerCase();
}

// The kind a picked file goes up as, or the refusal code when the chat keeps no such file.
export function classifyFile(name, size) {
  const ext = extensionOf(name);
  if (IMAGE_EXTENSIONS.includes(ext)) return { ok: true, kind: "image" };
  if (!DOCUMENT_EXTENSIONS.includes(ext)) return { ok: false, code: "unsupported_type" };
  if (size > MAX_BYTES) return { ok: false, code: "file_too_large" };
  return { ok: true, kind: "document" };
}

// A folder comes through a drop as an empty entry with no type and no extension.
export function isFolder(file) {
  return file.size === 0 && !file.type && !extensionOf(file.name);
}

// What a set of picked or dropped files adds to a tray that already holds `count` rows: the
// files that go up with their kind, and the last refusal to show (null when all of them fit).
export function planAdds(files, count) {
  const accepted = [];
  let code = null;
  for (const file of files) {
    if (isFolder(file)) {
      code = "folder";
      continue;
    }
    const verdict = classifyFile(file.name, file.size);
    if (!verdict.ok) {
      code = verdict.code;
      continue;
    }
    if (!hasRoom(count + accepted.length)) {
      code = "too_many_attachments";
      break;
    }
    accepted.push({ file, kind: verdict.kind });
  }
  return { accepted, code };
}

// A thread attachment without `kind` is a picture, as before documents existed.
export function attachmentKind(attachment) {
  return attachment.kind === "document" ? "document" : "image";
}

const FAMILY_BY_EXTENSION = new Map([
  ["xlsx", "sheet"],
  ["csv", "sheet"],
  ["docx", "doc"],
  ["pptx", "slides"],
  ["pdf", "pdf"],
  ["txt", "text"],
  ["md", "text"],
  ["json", "text"],
]);

// The badge of a document: its family, which sets the colour, and its extension in capitals.
export function documentBadge(name) {
  const ext = extensionOf(name);
  return {
    family: FAMILY_BY_EXTENSION.get(ext) || "file",
    label: ext ? ext.toUpperCase() : t("chatattach.badge_file"),
  };
}

// What a document card shows: the name as the host has it, its size in Spanish and its badge.
export function documentCard(attachment) {
  return {
    name: attachment.name || t("chatattach.file_fallback_name"),
    size: attachment.size ? sizeLabel(attachment.size) : "",
    ...documentBadge(attachment.name),
  };
}

export function hasRoom(count) {
  return count < MAX_ATTACHMENTS;
}

// The longest side is capped at `max`, the aspect kept; a small picture is never enlarged.
export function reducedSize(width, height, max = MAX_SIDE) {
  const longest = Math.max(width, height);
  if (longest <= max) return { width, height };
  const scale = max / longest;
  return { width: Math.round(width * scale), height: Math.round(height * scale) };
}

// The WebP copy is sent only when it is smaller; otherwise the original goes,
// as long as the original itself fits the host's 10 MiB.
export function chooseUpload({ originalSize, convertedSize }) {
  const converted = convertedSize ?? Infinity;
  if (converted < originalSize && converted <= MAX_BYTES) return "converted";
  if (originalSize <= MAX_BYTES) return "original";
  return converted <= MAX_BYTES ? "converted" : "tooLarge";
}

// The name the host stores: no control characters, at most 120 characters.
export function safeName(name) {
  const clean = String(name || "")
    .replace(/[\u0000-\u001f\u007f]/g, "")
    .trim()
    .slice(0, 120);
  return clean || "imagen";
}

// A send waits while any picture is still preparing, uploading or failed.
export function blocksSend(items) {
  return items.some((item) => item.state !== "ready");
}

// The pictures that are ready, in the order they were added.
export function readyItems(items) {
  return items.filter((item) => item.state === "ready");
}

// The tray without the pictures a send took: one still preparing stays where it is.
export function keepOutside(items, taken) {
  return items.filter((item) => !taken.includes(item));
}

// A tray picture of another conversation (or of none) is given up; the current one stays.
export function outsideConversation(item, conversationId) {
  return Boolean(item.conversationId) && item.conversationId !== conversationId;
}

export function failureText(code) {
  return ATTACHMENT_TEXT[code] || FALLBACK_TEXT;
}

// The line an operator reads for a failed picture: a refusal by its code, anything else by its message.
export function itemFailureText(failureInfo) {
  if (failureInfo.kind === "attachmentRefused") return failureText(failureInfo.code);
  return failureInfo.message || FALLBACK_TEXT;
}

// A refused send is shown as a message refusal, so the failure card keeps its usual shape.
export function sendFailure(failureInfo) {
  if (failureInfo.kind !== "attachmentRefused") return failureInfo;
  return { kind: "invalid", message: failureText(failureInfo.code) };
}

// `offset_mismatch` names the byte where the host expects the next chunk.
export function resumeOffset(failureInfo) {
  if (failureInfo.kind !== "attachmentRefused" || failureInfo.code !== "offset_mismatch") return null;
  return typeof failureInfo.nextOffset === "number" ? failureInfo.nextOffset : null;
}

// What to do after a failed chunk: resume at the offset the host gave, try the same
// chunk again for a network or busy failure, or give up once the attempts are spent.
export function chunkRetry(failureInfo, attempt) {
  if (attempt >= CHUNK_ATTEMPTS) return { retry: false, resume: null };
  const resume = resumeOffset(failureInfo);
  if (resume !== null) return { retry: true, resume };
  return { retry: RETRYABLE.includes(failureInfo.kind), resume: null };
}

export function progressPercent(sent, total) {
  if (!total) return 0;
  return Math.min(100, Math.round((100 * sent) / total));
}

export function sizeLabel(bytes) {
  const mib = 1024 * 1024;
  if (bytes < mib) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  return `${(bytes / mib).toLocaleString(locale(), { minimumFractionDigits: 1, maximumFractionDigits: 1 })} MB`;
}

// The status line under a picture's name.
export function itemStatus(item) {
  if (item.state === "preparing") return t("chatattach.status_preparing");
  if (item.state === "uploading") return t("chatattach.status_uploading", { n: progressPercent(item.sent, item.size) });
  if (item.state === "ready") return t("chatattach.status_ready");
  return itemFailureText(item.failure || {});
}

// «Reintentar» is offered only after a failure; «Quitar» is offered in every state.
export function canRetry(item) {
  return item.state === "failed";
}

// The failure of a picture the browser itself refuses, shaped like the host's refusals.
export function refusal(code) {
  return { kind: "attachmentRefused", code, field: "file", nextOffset: null };
}

// A picture-only message carries the host's placeholder text; the thumbnails say it instead.
export function bubbleBody(message) {
  const files = message.attachments || [];
  return files.length && message.body_md === ATTACHMENT_BODY ? "" : message.body_md;
}
