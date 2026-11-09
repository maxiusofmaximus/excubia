// Attachments before a send, free of the DOM so the tests can run them: the limit of
// four, the kind of each file, the states of one file, the size a picture is reduced
// to, the point a failed chunk resumes from and the words of each refusal.

export const MAX_ATTACHMENTS = 4;
export const MAX_BYTES = 10 * 1024 * 1024;
export const CHUNK_BYTES = 128 * 1024;
export const MAX_SIDE = 2048;
export const WEBP_QUALITY = 0.85;
export const CHUNK_ATTEMPTS = 3;
// Pictures are re-encoded; documents go up exactly as picked.
const IMAGE_EXTENSIONS = ["png", "jpg", "jpeg", "webp"];
const DOCUMENT_EXTENSIONS = ["xlsx", "docx", "pptx", "pdf", "csv", "txt", "md", "json"];
export const ACCEPT_ATTR = [...IMAGE_EXTENSIONS, ...DOCUMENT_EXTENSIONS].map((ext) => `.${ext}`).join(",");
// The body the host stores for a message that is only attachments.
export const ATTACHMENT_BODY = "(adjunto)";

const DECODE_FAILED = "No se pudo procesar la imagen. Prueba con otra";
const ATTACHMENT_TEXT = {
  unsupported_type: "Ese tipo de archivo no se puede adjuntar",
  invalid_image: DECODE_FAILED,
  timeout: DECODE_FAILED,
  worker_failed: DECODE_FAILED,
  image_too_large: "La imagen es demasiado grande",
  dimensions_too_large: "La imagen es demasiado grande",
  input_too_large: "La imagen es demasiado grande",
  file_too_large: "El archivo supera los 10 MB",
  conversation_quota: "Se llenó el espacio de adjuntos de esta conversación",
  host_quota: "Se llenó el espacio de adjuntos del equipo",
  too_many_attachments: "Solo puedes adjuntar hasta 4 archivos",
  unknown_attachment: "Una imagen ya no está disponible. Quítala y vuelve a adjuntarla",
  no_such_attachment: "Esa subida ya no existe. Vuelve a adjuntar la imagen",
  not_uploading: "Esa subida ya no existe. Vuelve a adjuntar la imagen",
  already_linked: "Esa imagen ya está en un mensaje",
  not_ready: "La imagen todavía no está lista",
  size_mismatch: "La subida no coincide con el archivo. Vuelve a intentarlo",
  hash_mismatch: "La subida no coincide con el archivo. Vuelve a intentarlo",
  unsupported_device: "Este equipo no puede preparar la imagen",
};
const FALLBACK_TEXT = "No se pudo adjuntar la imagen.";
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
    label: ext ? ext.toUpperCase() : "ARCHIVO",
  };
}

// What a document card shows: the name as the host has it, its size in Spanish and its badge.
export function documentCard(attachment) {
  return {
    name: attachment.name || "Archivo",
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
  return `${(bytes / mib).toFixed(1).replace(".", ",")} MB`;
}

// The status line under a picture's name.
export function itemStatus(item) {
  if (item.state === "preparing") return "Preparando…";
  if (item.state === "uploading") return `Subiendo ${progressPercent(item.sent, item.size)} %`;
  if (item.state === "ready") return "Lista para enviar";
  return itemFailureText(item.failure || {});
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
