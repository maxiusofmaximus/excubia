// Copies the avatar renders the desktop UI shows from the APK assets into ui/assets.
//   node scripts/sync-avatars.mjs [apk-assets-dir]
// Default source: ../apk/mobile/assets (the sibling worktree). Only the files that
// avatar.js can ask for are copied: the 4 head clips and their stills, 64 and 128 px.
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { CHARACTER_IDS, CLIPS, stillFor } from "../ui/avatar.js";

const desktop = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const source = resolve(process.argv[2] || join(desktop, "..", "apk", "mobile", "assets"));
const target = join(desktop, "ui", "assets");
const SIZES = [64, 128];

let copied = 0;
for (const id of CHARACTER_IDS) {
  mkdirSync(join(target, "agents_anim", id), { recursive: true });
  mkdirSync(join(target, "agents"), { recursive: true });
  for (const size of SIZES) {
    for (const clip of CLIPS) {
      const head = `${clip}-head-${size}.webp`;
      copyFileSync(join(source, "agents_anim", id, head), join(target, "agents_anim", id, head));
      copied += 1;
      const still = `${id}-${stillFor(clip)}-${size}.png`;
      copyFileSync(join(source, "agents", still), join(target, "agents", still));
      copied += 1;
    }
  }
}
console.log(`avatares copiados desde ${source}: ${copied} ficheros`);
