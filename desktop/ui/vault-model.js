// «Protege tu historial», free of the DOM so the tests can run it. The mouse pool becomes
// packed records the host expects: i32 x LE, i32 y LE, u64 micros LE (16 bytes each). The
// recovery challenge asks for three words of the phrase; nothing here keeps the phrase.

import { t } from "./i18n.js";

export const SAMPLES_NEEDED = 2048;
// shortcut: the host route takes 64 KiB; 3 000 records (48 000 bytes, 64 000 base64 chars) fit
// it. Past that the pool stops growing, which is more than the 2 048 the host counts.
export const SAMPLES_MAX = 3000;
const RECORD_BYTES = 16;
const PHRASE_WORDS = 24;
const CHALLENGE_WORDS = 3;

export function createCollector(needed = SAMPLES_NEEDED) {
  let samples = [];
  let previous = null;

  return {
    add(x, y, micros) {
      const point = { x: Math.round(x) | 0, y: Math.round(y) | 0 };
      if (samples.length >= SAMPLES_MAX) return;
      // As the host does: a sample equal to the previous one in x and y does not count.
      if (previous && previous.x === point.x && previous.y === point.y) return;
      previous = point;
      samples.push({ ...point, micros: Math.max(0, Math.round(micros)) });
    },
    count() {
      return samples.length;
    },
    progress() {
      return Math.min(1, samples.length / needed);
    },
    isEnough() {
      return samples.length >= needed;
    },
    pack() {
      const buffer = new ArrayBuffer(samples.length * RECORD_BYTES);
      const view = new DataView(buffer);
      samples.forEach((sample, index) => {
        const at = index * RECORD_BYTES;
        view.setInt32(at, sample.x, true);
        view.setInt32(at + 4, sample.y, true);
        view.setBigUint64(at + 8, BigInt(sample.micros), true);
      });
      return toBase64(new Uint8Array(buffer));
    },
    // Drops the samples once the key exists, so none stays in memory.
    clear() {
      samples = [];
      previous = null;
    },
  };
}

// Built in slices: spreading a long array into String.fromCharCode would overflow the stack.
function toBase64(bytes) {
  let binary = "";
  for (let at = 0; at < bytes.length; at += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(at, at + 0x8000));
  }
  return btoa(binary);
}

// Three different positions (1..24), in order, for the user to rewrite. `seed` makes the
// choice repeatable in tests; the screen leaves it out.
export function confirmChallenge(phrase, seed = Math.floor(Math.random() * 0x100000000)) {
  if (splitWords(phrase).length !== PHRASE_WORDS) throw new Error(t("vault.err_phrase_length"));
  let state = seed >>> 0;
  const next = () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  const picked = new Set();
  while (picked.size < CHALLENGE_WORDS) {
    picked.add(1 + Math.floor(next() * PHRASE_WORDS));
  }
  return [...picked].sort((a, b) => a - b);
}

// Compares the typed words with the phrase's, ignoring case and extra spaces.
export function checkChallenge(phrase, positions, answers) {
  const words = splitWords(phrase);
  if (words.length !== PHRASE_WORDS || positions.length !== answers.length) return false;
  return positions.every((position, index) => {
    const expected = words[position - 1];
    return expected !== undefined && expected === normalizeWord(answers[index]);
  });
}

export function splitWords(phrase) {
  return normalizeWord(phrase).split(" ").filter(Boolean);
}

function normalizeWord(text) {
  return String(text ?? "").replace(/\s+/g, " ").trim().toLowerCase();
}

// The words the screen shows for a refused protect call; `error` is the typed failure
// from the desktop bridge (`code` for a vault refusal).
const PROTECT_ERRORS = {
  vault_samples_insufficient: t("vault.err_samples"),
  vault_already_protected: t("vault.err_already"),
};
const PROTECT_FALLBACK = t("vault.err_protect_fallback");

export function protectErrorText(error) {
  return PROTECT_ERRORS[error?.code] || PROTECT_FALLBACK;
}


export function finishErrorText(error) {
  return error?.code === "vault_no_backup" ? t("vault.err_no_backup") : t("vault.err_finish");
}
