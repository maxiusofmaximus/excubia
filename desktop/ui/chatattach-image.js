// Shrinks a picture in the webview before it leaves the machine: longest side at
// most 2048 px, re-encoded as WebP. The original goes when the copy does not help.
import { WEBP_QUALITY, chooseUpload, refusal, reducedSize } from "./chatattach-model.js";

export async function preparePicture(file) {
  let bitmap;
  try {
    bitmap = await createImageBitmap(file);
  } catch {
    throw refusal("invalid_image");
  }
  const { width, height } = reducedSize(bitmap.width, bitmap.height);
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  canvas.getContext("2d").drawImage(bitmap, 0, 0, width, height);
  bitmap.close();
  const webp = await new Promise((resolve) => canvas.toBlob(resolve, "image/webp", WEBP_QUALITY));
  const choice = chooseUpload({ originalSize: file.size, convertedSize: webp ? webp.size : null });
  if (choice === "tooLarge") throw refusal("image_too_large");
  return choice === "converted" ? webp : file;
}
