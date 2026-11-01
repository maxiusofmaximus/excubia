// One face per agent: its character's animated head for the agent's status, the
// still frame under reduced motion, or the initial when no character matches.
// Only the head acts; the body is in the render and never moves (APK rule).
// The palette and clips mirror apk/mobile/lib/ui/widgets/agent_avatar.dart.

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

// 64 px renders up to 64 px faces, 128 px above that.
export function renderSize(px) {
  return px <= 64 ? 64 : 128;
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
      segments.some((segment) => segment === id || segment.startsWith(`${id} `)),
    ) || null
  );
}

export function initialOf(name) {
  const trimmed = String(name || "").trim();
  return trimmed ? trimmed[0].toUpperCase() : "?";
}

export function avatarSource(character, status, px, reduced) {
  const clip = clipFor(status);
  const size = renderSize(px);
  if (reduced) return `assets/agents/${character}-${stillFor(clip)}-${size}.png`;
  return `assets/agents_anim/${character}/${clip}-head-${size}.webp`;
}

function prefersReducedMotion() {
  return globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches === true;
}

// Paints the face into an `.avatar` element. Without a character the initial sits
// on the brass disc from the stylesheet. A team hire passes `faceFromName: false`:
// its face is its character id only, so renaming it never changes the face.
export function paintAvatar(el, { name, characterId, status = "idle", px = 34, faceFromName = true }) {
  const character = characterFor(faceFromName ? name : "", characterId);
  const profile = character ? CHARACTERS[character] : null;
  el.style.setProperty("--avatar-px", `${px}px`);
  el.style.setProperty("--avatar-bg", profile ? profile.background : "");
  el.style.setProperty("--avatar-fg", profile ? profile.foreground : "");
  el.setAttribute("aria-hidden", "true");
  if (!character) {
    el.textContent = initialOf(name);
    return el;
  }
  const img = document.createElement("img");
  img.alt = "";
  img.decoding = "async";
  img.src = avatarSource(character, status, px, prefersReducedMotion());
  img.addEventListener("error", () => el.replaceChildren(initialOf(name)));
  el.replaceChildren(img);
  return el;
}

export function avatarElement(options) {
  const el = document.createElement("span");
  el.className = "avatar";
  return paintAvatar(el, options);
}
