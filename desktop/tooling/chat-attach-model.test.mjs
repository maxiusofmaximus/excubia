// Run with: node --test tooling/chat-attach-model.test.mjs
// Picture attachments: the limit of four, the send gate, the reduction to 2048 px,
// the choice between the WebP copy and the original, resumes and the words of each refusal.
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  ACCEPT_ATTR,
  CHUNK_BYTES,
  MAX_BYTES,
  attachmentKind,
  blocksSend,
  bubbleBody,
  chooseUpload,
  chunkRetry,
  classifyFile,
  documentBadge,
  documentCard,
  failureText,
  hasRoom,
  itemFailureText,
  itemStatus,
  keepOutside,
  outsideConversation,
  progressPercent,
  readyItems,
  reducedSize,
  refusal,
  resumeOffset,
  safeName,
  sendFailure,
  sizeLabel,
} from "../ui/chatattach-model.js";

const DECODE = "No se pudo procesar la imagen. Prueba con otra";

test("a message takes at most four pictures", () => {
  assert.equal(hasRoom(0), true);
  assert.equal(hasRoom(3), true);
  assert.equal(hasRoom(4), false);
});

test("pictures and the documents the chat keeps are accepted, by extension", () => {
  assert.deepEqual(classifyFile("foto.PNG", 10), { ok: true, kind: "image" });
  assert.deepEqual(classifyFile("informe.xlsx", 10), { ok: true, kind: "document" });
  assert.deepEqual(classifyFile("tabla.csv", 10), { ok: true, kind: "document" });
  assert.deepEqual(classifyFile("a.b.pdf", 10), { ok: true, kind: "document" });
  for (const name of ["macro.xlsm", "viejo.xls", "viejo.doc", "app.exe", "gif.gif", "sin-extension", "x.constructor"]) {
    assert.deepEqual(classifyFile(name, 10), { ok: false, code: "unsupported_type" }, name);
  }
  assert.ok(ACCEPT_ATTR.split(",").includes(".xlsx"));
  assert.ok(!ACCEPT_ATTR.split(",").includes(".xls"));
});

test("a document over 10 MiB is refused before it leaves the machine", () => {
  assert.deepEqual(classifyFile("grande.pdf", MAX_BYTES), { ok: true, kind: "document" });
  assert.deepEqual(classifyFile("grande.pdf", MAX_BYTES + 1), { ok: false, code: "file_too_large" });
  assert.equal(failureText("file_too_large"), "El archivo supera los 10 MB");
});

test("a thread attachment without kind is a picture, as before documents existed", () => {
  assert.equal(attachmentKind({ id: "att_1" }), "image");
  assert.equal(attachmentKind({ id: "att_1", kind: null }), "image");
  assert.equal(attachmentKind({ id: "att_1", kind: "image" }), "image");
  assert.equal(attachmentKind({ id: "att_1", kind: "document" }), "document");
});

test("each document family has its badge; an unknown extension gets a plain one", () => {
  assert.deepEqual(documentBadge("datos.xlsx"), { family: "sheet", label: "XLSX" });
  assert.deepEqual(documentBadge("carta.docx"), { family: "doc", label: "DOCX" });
  assert.deepEqual(documentBadge("deck.pptx"), { family: "slides", label: "PPTX" });
  assert.deepEqual(documentBadge("factura.PDF"), { family: "pdf", label: "PDF" });
  assert.deepEqual(documentBadge("notas.md"), { family: "text", label: "MD" });
  assert.deepEqual(documentBadge("leeme"), { family: "file", label: "ARCHIVO" });
  assert.deepEqual(documentBadge("x.constructor"), { family: "file", label: "CONSTRUCTOR" });
});

test("a document card shows its name as written, its size in Spanish and its badge", () => {
  const card = documentCard({ id: "att_1", kind: "document", name: "Bitácora 6.xlsx", size: 3_565_158 });
  assert.deepEqual(card, { name: "Bitácora 6.xlsx", size: "3,4 MB", family: "sheet", label: "XLSX" });
});

test("special characters in a long name reach the card as plain text", () => {
  const hostile = `<img src=x onerror="alert(1)"> ${"é".repeat(150)}.pdf`;
  const card = documentCard({ id: "att_1", kind: "document", name: hostile, size: 500 });
  assert.equal(card.name, hostile);
  assert.equal(card.size, "1 KB");
  assert.equal(card.label, "PDF");
});

test("a card with no name or size still reads", () => {
  assert.deepEqual(documentCard({ id: "att_1", kind: "document", name: null, size: null }), {
    name: "Archivo",
    size: "",
    family: "file",
    label: "ARCHIVO",
  });
});

test("a send waits until every picture is ready, failed ones included", () => {
  const ready = { id: "att_1", state: "ready" };
  assert.equal(blocksSend([]), false);
  assert.equal(blocksSend([ready, { state: "uploading" }]), true);
  assert.equal(blocksSend([ready, { state: "preparing" }]), true);
  assert.equal(blocksSend([ready, { state: "failed" }]), true);
  assert.equal(blocksSend([ready, { id: "att_2", state: "ready" }]), false);
});

test("the pictures that are ready keep the order they were added", () => {
  const first = { id: "att_2", state: "ready" };
  const second = { id: "att_1", state: "ready" };
  const items = [first, { id: null, state: "uploading" }, second];
  assert.deepEqual(readyItems(items), [first, second]);
});

// The tray of a send without the pictures it took: one added while the send was in flight stays.
test("a send takes only its own pictures out of the tray", () => {
  const sent = { id: "att_1", state: "ready" };
  const preparing = { id: null, state: "preparing" };
  const added = { id: "att_2", state: "ready" };
  assert.deepEqual(keepOutside([sent, preparing, added], [sent]), [preparing, added]);
});

// A picture of another conversation is given up when the chat changes to a different one.
test("only a picture of another conversation is given up", () => {
  assert.equal(outsideConversation({ conversationId: "cnv_1" }, "cnv_2"), true);
  assert.equal(outsideConversation({ conversationId: "cnv_1" }, "cnv_1"), false);
  assert.equal(outsideConversation({ conversationId: null }, "cnv_2"), false);
});

// Before, the browser's own TypeError reached the row as its message; now the row says it in Spanish.
test("a device without the digest API shows its own Spanish line, not a TypeError", () => {
  const row = { state: "failed", failure: refusal("unsupported_device") };
  assert.equal(itemStatus(row), "Este equipo no puede preparar la imagen");
  assert.equal(failureText("unsupported_device"), "Este equipo no puede preparar la imagen");
});

test("a picture bigger than 2048 px on its longest side is reduced, aspect kept", () => {
  assert.deepEqual(reducedSize(4000, 3000), { width: 2048, height: 1536 });
  assert.deepEqual(reducedSize(800, 4000), { width: 410, height: 2048 });
  assert.deepEqual(reducedSize(1000, 800), { width: 1000, height: 800 }, "never enlarged");
});

test("the WebP copy is sent only when it is smaller", () => {
  assert.equal(chooseUpload({ originalSize: 900_000, convertedSize: 300_000 }), "converted");
  assert.equal(chooseUpload({ originalSize: 300_000, convertedSize: 900_000 }), "original");
  assert.equal(chooseUpload({ originalSize: 300_000, convertedSize: 300_000 }), "original");
});

test("an original over 10 MiB goes only when its WebP copy fits", () => {
  const big = MAX_BYTES + 1;
  assert.equal(chooseUpload({ originalSize: big, convertedSize: 2_000_000 }), "converted");
  assert.equal(chooseUpload({ originalSize: big, convertedSize: big }), "tooLarge");
  assert.equal(chooseUpload({ originalSize: big, convertedSize: null }), "tooLarge");
  assert.equal(chooseUpload({ originalSize: 500_000, convertedSize: null }), "original");
});

test("a failed chunk resumes from the offset the host names", () => {
  const mismatch = { kind: "attachmentRefused", code: "offset_mismatch", nextOffset: 2 * CHUNK_BYTES };
  assert.equal(resumeOffset(mismatch), 2 * CHUNK_BYTES);
  assert.equal(resumeOffset({ kind: "attachmentRefused", code: "offset_mismatch" }), null);
  assert.equal(resumeOffset({ kind: "attachmentRefused", code: "not_ready", nextOffset: 5 }), null);
  assert.deepEqual(chunkRetry(mismatch, 1), { retry: true, resume: 2 * CHUNK_BYTES });
});

test("a network or busy chunk is tried again, three times at most", () => {
  assert.deepEqual(chunkRetry({ kind: "offline" }, 1), { retry: true, resume: null });
  assert.deepEqual(chunkRetry({ kind: "agentBusy" }, 2), { retry: true, resume: null });
  assert.equal(chunkRetry({ kind: "offline" }, 3).retry, false);
  assert.equal(chunkRetry(refusal("unsupported_type"), 1).retry, false);
  assert.equal(chunkRetry({ kind: "forbidden" }, 1).retry, false);
});

test("each refusal has its words in Spanish, and an unknown one a plain fallback", () => {
  assert.equal(failureText("unsupported_type"), "Ese tipo de archivo no se puede adjuntar");
  assert.equal(failureText("image_too_large"), "La imagen es demasiado grande");
  assert.equal(failureText("dimensions_too_large"), "La imagen es demasiado grande");
  assert.equal(failureText("conversation_quota"), "Se llenó el espacio de adjuntos de esta conversación");
  assert.equal(failureText("timeout"), DECODE);
  assert.equal(failureText("worker_failed"), DECODE);
  assert.equal(failureText("tipo_nuevo"), "No se pudo adjuntar la imagen.");
});

test("a refused picture shows its code's words, other failures their message", () => {
  assert.equal(itemFailureText(refusal("unsupported_type")), "Ese tipo de archivo no se puede adjuntar");
  assert.equal(itemFailureText({ kind: "offline", message: "Sin conexión" }), "Sin conexión");
  assert.equal(itemFailureText({}), "No se pudo adjuntar la imagen.");
});

test("a refused send reads as an invalid message, with the picture's words", () => {
  assert.deepEqual(sendFailure({ kind: "attachmentRefused", code: "unknown_attachment" }), {
    kind: "invalid",
    message: "Una imagen ya no está disponible. Quítala y vuelve a adjuntarla",
  });
  const offline = { kind: "offline", message: "Sin conexión" };
  assert.equal(sendFailure(offline), offline);
});

test("the upload shows its progress while it runs", () => {
  assert.equal(progressPercent(0, 0), 0);
  assert.equal(progressPercent(1, 3), 33);
  assert.equal(progressPercent(9, 3), 100);
  assert.equal(itemStatus({ state: "preparing" }), "Preparando…");
  assert.equal(itemStatus({ state: "uploading", sent: 131_072, size: 262_144 }), "Subiendo 50 %");
  assert.equal(itemStatus({ state: "ready" }), "Lista para enviar");
});

test("sizes read in Spanish, in KB or MB", () => {
  assert.equal(sizeLabel(48_212), "47 KB");
  assert.equal(sizeLabel(1_572_864), "1,5 MB");
});

test("the name the host stores has no control characters and at most 120 characters", () => {
  assert.equal(safeName("captura\u0000\n.png"), "captura.png");
  assert.equal(safeName("a".repeat(200)).length, 120);
  assert.equal(safeName("   "), "imagen");
});

test("a picture-only message shows no placeholder text", () => {
  const withFiles = { body_md: "(adjunto)", attachments: [{ id: "att_1" }] };
  assert.equal(bubbleBody(withFiles), "");
  assert.equal(bubbleBody({ body_md: "Hola", attachments: [] }), "Hola");
  assert.equal(bubbleBody({ body_md: "(adjunto)" }), "(adjunto)");
});
