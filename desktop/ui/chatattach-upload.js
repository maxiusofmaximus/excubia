// Moves one prepared picture to the host: its digest, then chunks that resume from
// the offset the host names, then complete. The tray row is updated in place.
import { invoke } from "./core.js";
import { CHUNK_BYTES, chunkRetry, refusal } from "./chatattach-model.js";
import { failure } from "./chatstate.js";

async function sha256Hex(blob) {
  // An older WebView has no crypto.subtle: a refusal the row can show, not a TypeError.
  if (!crypto.subtle) throw refusal("unsupported_device");
  const digest = await crypto.subtle.digest("SHA-256", await blob.arrayBuffer());
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

function toBase64(bytes) {
  let binary = "";
  // Slices keep the argument list of String.fromCharCode short.
  for (let index = 0; index < bytes.length; index += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(index, index + 0x8000));
  }
  return btoa(binary);
}

// Gives up the upload a row no longer wants. Best effort: the host drops it on its own otherwise.
export function dropRemote(conversationId, attachmentId) {
  invoke("chat_attachment_delete", { conversationId, attachmentId }).catch(() => {});
}

// Resolves to true when the picture is ready; false when the row was removed mid-way.
export async function uploadPicture(conversationId, item) {
  item.state = "uploading";
  const sha256 = await sha256Hex(item.blob);
  const created = await invoke("chat_attachment_create", {
    conversationId,
    clientId: item.clientId,
    name: item.name,
    size: item.blob.size,
    sha256,
  });
  item.id = created.id;
  if (item.removed) {
    dropRemote(conversationId, item.id);
    return false;
  }
  let offset = 0;
  let attempt = 0;
  while (offset < item.blob.size) {
    if (item.removed) return false;
    const end = Math.min(item.blob.size, offset + CHUNK_BYTES);
    const bytes = new Uint8Array(await item.blob.slice(offset, end).arrayBuffer());
    try {
      offset = await invoke("chat_attachment_chunk", {
        conversationId,
        attachmentId: item.id,
        offset,
        data: toBase64(bytes),
      });
      item.sent = offset;
      attempt = 0;
    } catch (error) {
      attempt += 1;
      const decision = chunkRetry(failure(error), attempt);
      if (!decision.retry) throw error;
      if (decision.resume !== null) {
        offset = decision.resume;
        item.sent = offset;
      }
    }
  }
  if (item.removed) return false;
  await invoke("chat_attachment_complete", { conversationId, attachmentId: item.id });
  item.state = "ready";
  return true;
}
