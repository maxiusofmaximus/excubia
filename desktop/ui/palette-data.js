// The approved palettes and the two decisions shared by every entry point.
// It is a classic script on purpose: ui/palette-boot.js runs synchronously in
// <head>, before the first paint, and cannot import an ES module. ui/palettes.js
// imports this file for its side effect and exposes the same model with the
// storage and media-query helpers that need a module.

(function (global) {
  const DEFAULT_PALETTE = "noche";
  const STORAGE_KEY = "excubia-palette";

  const DARK_SHADOW = {
    "shadow-toast": "0 16px 40px rgba(0, 0, 0, 0.4)",
  };
  const LIGHT_SHADOW = {
    "shadow-toast": "0 16px 40px rgba(70, 58, 40, 0.18)",
  };

  function tokens(values, shadow) {
    return Object.freeze({ ...values, ...shadow });
  }

  const PALETTES = Object.freeze({
    noche: {
      id: "noche",
      name: "Noche",
      note: "Azul noche y latón. La de Excubia",
      scheme: "oscuro",
      tokens: tokens(
        {
          bg: "#0b1426",
          surface: "#101b33",
          raised: "#17274a",
          "raised-2": "#1d3159",
          line: "#1f2e50",
          "line-strong": "#6f84b3",
          text: "#efe6d2",
          "text-2": "#ded2b6",
          muted: "#9aa5bd",
          accent: "#f2a93b",
          "accent-text": "#f2a93b",
          "accent-rgb": "242, 169, 59",
          "accent-soft": "#3a2e17",
          "on-accent": "#1a1204",
          "on-attention": "#1a1204",
          blocked: "#ff7a62",
          working: "#8fb4e8",
          done: "#6fcf97",
          idle: "#9aa5bd",
          unknown: "#ded2b6",
          warning: "#f2a93b",
          sunken: "#081020",
        },
        DARK_SHADOW,
      ),
    },
    pergamino: {
      id: "pergamino",
      name: "Pergamino",
      note: "Papel cálido. La clara de Excubia",
      scheme: "claro",
      tokens: tokens(
        {
          bg: "#f6f0e1",
          surface: "#fbf7ec",
          raised: "#ece2c8",
          "raised-2": "#e4d8bc",
          line: "#e0d4b8",
          "line-strong": "#7a7265",
          text: "#1c2438",
          "text-2": "#34405e",
          muted: "#525b73",
          accent: "#f2a93b",
          "accent-text": "#8a5208",
          "accent-rgb": "242, 169, 59",
          "accent-soft": "#f6e4be",
          "on-accent": "#1a1204",
          "on-attention": "#ffffff",
          blocked: "#a8301c",
          working: "#2f5c9e",
          done: "#276b4e",
          idle: "#525b73",
          unknown: "#34405e",
          warning: "#8a5208",
          sunken: "#fbf7ec",
        },
        LIGHT_SHADOW,
      ),
    },
    carbon: {
      id: "carbon",
      name: "Carbón",
      note: "Gris oscuro neutro, acento azul acero",
      scheme: "oscuro",
      tokens: tokens(
        {
          bg: "#181818",
          surface: "#1f1f1f",
          raised: "#2a2a2a",
          "raised-2": "#333333",
          line: "#303030",
          "line-strong": "#808080",
          text: "#ececec",
          "text-2": "#d6d6d6",
          muted: "#a3a3a3",
          accent: "#7fa6d6",
          "accent-text": "#7fa6d6",
          "accent-rgb": "127, 166, 214",
          "accent-soft": "#27384b",
          "on-accent": "#0d1622",
          "on-attention": "#0d1622",
          blocked: "#ff8a7a",
          working: "#8fb7e8",
          done: "#7fd6a3",
          idle: "#a3a3a3",
          unknown: "#d6d6d6",
          warning: "#7fa6d6",
          sunken: "#121212",
        },
        DARK_SHADOW,
      ),
    },
    papel: {
      id: "papel",
      name: "Papel",
      note: "Blanco roto neutro, acento azul",
      scheme: "claro",
      tokens: tokens(
        {
          bg: "#fcfcfc",
          surface: "#f3f3f3",
          raised: "#e9e9e9",
          "raised-2": "#e0e0e0",
          line: "#dcdcdc",
          "line-strong": "#6b6b6b",
          text: "#141414",
          "text-2": "#333333",
          muted: "#5a5a5a",
          accent: "#2b6fb5",
          "accent-text": "#1f5c9e",
          "accent-rgb": "43, 111, 181",
          "accent-soft": "#d9e6f4",
          "on-accent": "#ffffff",
          "on-attention": "#ffffff",
          blocked: "#b03420",
          working: "#2f5c9e",
          done: "#276b4e",
          idle: "#5a5a5a",
          unknown: "#333333",
          warning: "#8a5208",
          sunken: "#f3f3f3",
        },
        LIGHT_SHADOW,
      ),
    },
    medianoche: {
      id: "medianoche",
      name: "Medianoche",
      note: "Azul profundo de bajo brillo",
      scheme: "oscuro",
      tokens: tokens(
        {
          bg: "#00183a",
          surface: "#001f4a",
          raised: "#0a2c5e",
          "raised-2": "#10346b",
          line: "#16396f",
          "line-strong": "#5b8bd0",
          text: "#e8f0ff",
          "text-2": "#d0e0fa",
          muted: "#93a8cc",
          accent: "#6fb1ff",
          "accent-text": "#6fb1ff",
          "accent-rgb": "111, 177, 255",
          "accent-soft": "#143a6e",
          "on-accent": "#001226",
          "on-attention": "#001226",
          blocked: "#ff8f7a",
          working: "#8cc4ff",
          done: "#74d6a0",
          idle: "#93a8cc",
          unknown: "#d0e0fa",
          warning: "#6fb1ff",
          sunken: "#001230",
        },
        DARK_SHADOW,
      ),
    },
    contraste: {
      id: "contraste",
      name: "Alto contraste",
      note: "Negro y blanco, máxima legibilidad",
      scheme: "oscuro",
      tokens: tokens(
        {
          bg: "#000000",
          surface: "#0a0a0a",
          raised: "#1a1a1a",
          "raised-2": "#262626",
          line: "#ffffff",
          "line-strong": "#c8c8c8",
          text: "#ffffff",
          "text-2": "#f0f0f0",
          muted: "#d0d0d0",
          accent: "#ffd400",
          "accent-text": "#ffd400",
          "accent-rgb": "255, 212, 0",
          "accent-soft": "#2b2b00",
          "on-accent": "#000000",
          "on-attention": "#000000",
          blocked: "#ff8080",
          working: "#80c8ff",
          done: "#7dffa8",
          idle: "#d0d0d0",
          unknown: "#f0f0f0",
          warning: "#ffd400",
          sunken: "#000000",
        },
        DARK_SHADOW,
      ),
    },
  });

  function defaultPrefs() {
    return { id: DEFAULT_PALETTE, followSystem: false };
  }

  // A plain object inherits `__proto__` and friends; only the six own keys are
  // palette ids, so a corrupted store cannot smuggle one of those in.
  function isPaletteId(id) {
    return typeof id === "string" && Object.hasOwn(PALETTES, id);
  }

  function resolvePalette(prefs, systemIsDark) {
    if (prefs && prefs.followSystem) return systemIsDark ? "noche" : "pergamino";
    return prefs && isPaletteId(prefs.id) ? prefs.id : DEFAULT_PALETTE;
  }

  // No stored value is not a failure: the first-run screen and the migration
  // both need to tell it apart from a broken store, which falls back to Noche.
  function loadPrefs() {
    try {
      const raw = global.localStorage.getItem(STORAGE_KEY);
      if (raw === null) return null;
      const data = JSON.parse(raw);
      if (!data || !isPaletteId(data.id)) return defaultPrefs();
      return { id: data.id, followSystem: Boolean(data.followSystem) };
    } catch {
      return defaultPrefs();
    }
  }

  function initialSetup({ stored, onboardingDone, systemIsDark }) {
    if (stored) return { prefs: stored, showPicker: false };
    if (!onboardingDone) return { prefs: null, showPicker: true };
    return {
      prefs: {
        id: resolvePalette({ followSystem: true }, systemIsDark),
        followSystem: true,
      },
      showPicker: false,
    };
  }

  function applyPalette(id) {
    const palette = isPaletteId(id) ? PALETTES[id] : PALETTES[DEFAULT_PALETTE];
    const root = global.document.documentElement;
    for (const [name, value] of Object.entries(palette.tokens)) {
      root.style.setProperty(`--${name}`, value);
    }
    root.dataset.palette = palette.id;
    root.style.colorScheme = palette.scheme === "claro" ? "light" : "dark";
  }

  function systemIsDark() {
    try {
      return global.matchMedia("(prefers-color-scheme: dark)").matches;
    } catch {
      return false;
    }
  }

  global.ExcubiaPalettes = Object.freeze({
    PALETTES,
    DEFAULT_PALETTE,
    STORAGE_KEY,
    isPaletteId,
    loadPrefs,
    resolvePalette,
    initialSetup,
    applyPalette,
    systemIsDark,
  });
})(globalThis);
