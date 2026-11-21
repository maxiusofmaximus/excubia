// One face per agent: its character's portrait for the agent's status, or the initial when
// no character matches. The portraits are square, the card included (ui/assets/portraits, made
// from the APK's v3 renders); the palette mirrors the APK
// (adjcard/mobile/lib/ui/widgets/agent_avatar.dart).

const CHARACTERS = {
  alfred: { background: "#f7d27a", foreground: "#b7791f" },
  bob: { background: "#9ee6d4", foreground: "#0f7f6c" },
  nicole: { background: "#cdb9ff", foreground: "#6d3fd1" },
  ines: { background: "#ffb3a8", foreground: "#c2413a" },
  hugo: { background: "#a9cbff", foreground: "#1e5db3" },
  sami: { background: "#cdeb9a", foreground: "#4d7f12" },
  dani: { background: "#ffb9da", foreground: "#b0286a" },
  conan: { background: "#c9d2de", foreground: "#334155" },
  ernesto: { background: "#e7c9a8", foreground: "#8a3e1f" },
};

export const CHARACTER_IDS = Object.keys(CHARACTERS);
export const CLIPS = ["idle", "working", "done", "error"];

const CLIP_FOR_STATUS = { working: "working", done: "done", blocked: "error" };

export function clipFor(status) {
  return CLIP_FOR_STATUS[status] || "idle";
}

// Idle has no still of its own: the neutral frame stands in, as in the APK.
export function stillFor(clip) {
  return clip === "idle" ? "neutral" : clip;
}

// 128 px portraits up to 64 px faces, 384 px above that.
export function renderSize(px) {
  return px <= 64 ? 128 : 384;
}

// The state a face shows: `thinking` while Alfred waits for a reply, else the status's still.
export function stateFor(status, look = null) {
  return look === "thinking" ? "thinking" : stillFor(clipFor(status));
}

// The file for one state at a size. Every state has both sizes; the welcome is the 384 square.
export function renderFile(character, state, px) {
  return `assets/portraits/${character}-${state}-${renderSize(px)}.webp`;
}

function fold(text) {
  return text.trim().toLowerCase().normalize("NFD").replace(/\p{Diacritic}/gu, "");
}

// The character behind an agent. A character id from the data wins and is never
// guessed from the name; without one, a segment of the visible title names it
// ("desconocido · Ernesto · dev", "Alfred (lead)").
export function characterFor(name, characterId) {
  if (characterId) return Object.hasOwn(CHARACTERS, characterId) ? characterId : null;
  const segments = String(name || "")
    .split("·")
    .map(fold);
  return (
    Object.keys(CHARACTERS).find((id) =>
      segments.some((segment) => segment === id || /^[\s,(]/.test(segment.slice(id.length)) && segment.startsWith(id)),
    ) || null
  );
}

export function initialOf(name) {
  const trimmed = String(name || "").trim();
  return trimmed ? trimmed[0].toUpperCase() : "?";
}

export function avatarSource(character, status, px, look = null) {
  return renderFile(character, stateFor(status, look), px);
}

// Alfred's eyes shut for this long, every 3 to 6 s (the APK's blinking portrait).
export const BLINK_HOLD_MS = 120;

export function blinkGapMs(random = Math.random) {
  return 3000 + Math.floor(random() * 3001);
}

function prefersReducedMotion() {
  return globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches === true;
}

// One chained timer per face. It stops on its next step once the face has left the page
// or its image has been replaced (a failed load).
function startBlink(el, img, open, shut) {
  const step = (closed) => {
    if (!el.isConnected || !img.isConnected) return;
    img.src = closed ? shut : open;
    el.blinkTimer = setTimeout(() => step(!closed), closed ? BLINK_HOLD_MS : blinkGapMs());
  };
  el.blinkTimer = setTimeout(() => step(true), blinkGapMs());
}

// Paints the face into an `.avatar` element. Without a character the initial sits
// on the hot panel from the stylesheet. A team hire passes `faceFromName: false`:
// its face is its character id only, so renaming it never changes the face.
// Alfred's idle face blinks, unless the system asks to reduce motion.
export function paintAvatar(
  el,
  { name, characterId, status = "idle", look = null, px = 34, faceFromName = true },
) {
  const character = characterFor(faceFromName ? name : "", characterId);
  const profile = character ? CHARACTERS[character] : null;
  el.style.setProperty("--avatar-px", `${px}px`);
  el.style.setProperty("--avatar-bg", profile ? profile.background : "");
  el.style.setProperty("--avatar-fg", profile ? profile.foreground : "");
  el.setAttribute("aria-hidden", "true");
  clearTimeout(el.blinkTimer);
  if (!character) {
    el.textContent = initialOf(name);
    return el;
  }
  const state = stateFor(status, look);
  const open = renderFile(character, state, px);
  const img = document.createElement("img");
  img.alt = "";
  img.decoding = "async";
  img.src = open;
  img.addEventListener("error", () => el.replaceChildren(initialOf(name)));
  el.replaceChildren(img);
  if (character === "alfred" && state === "neutral" && !prefersReducedMotion()) {
    const shut = renderFile(character, "blink", px);
    new Image().src = shut;
    startBlink(el, img, open, shut);
  }
  return el;
}

export function avatarElement(options) {
  const el = document.createElement("span");
  el.className = "avatar";
  return paintAvatar(el, options);
}
