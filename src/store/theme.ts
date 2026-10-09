import { create } from "zustand";

export type ThemeId = "original" | "dark" | "system";

export const THEMES = [
  { id: "original" as const, labelKey: "settings.theme.original" as const, swatch: ["#B4B6B8", "#FF6F39", "#25282C"] },
  { id: "dark" as const, labelKey: "settings.theme.dark" as const, swatch: ["#202327", "#FF6F39", "#EEF0F3"] },
  { id: "system" as const, labelKey: "settings.theme.system" as const, swatch: ["#B4B6B8", "#FF6F39", "#202327"] },
];

const STORAGE_KEY = "mixweave-theme";
const LEGACY_STORAGE_KEY = "sonux-theme";
const SINK_STORAGE_KEY = "sink-theme";
const DARK_MEDIA_QUERY = "(prefers-color-scheme: dark)";

/** "system" isn't a look of its own - it's resolved to whichever of the two
 * real themes the OS is currently reporting. */
function resolveTheme(theme: ThemeId): "original" | "dark" {
  return theme === "system"
    ? (window.matchMedia(DARK_MEDIA_QUERY).matches ? "dark" : "original")
    : theme;
}

/** The look actually on screen right now ("system" resolved). */
export function currentLook(): "original" | "dark" {
  return resolveTheme(useTheme.getState().theme);
}

/** Re-read the saved theme and apply it - for windows (the shortcut popup)
 * that stay open while the main window changes it. */
export function syncTheme() {
  apply(initial());
}

function apply(theme: ThemeId) {
  const root = document.documentElement;
  const resolved = resolveTheme(theme);
  if (resolved === "original") delete root.dataset.theme;
  else root.dataset.theme = resolved;
}

function initial(): ThemeId {
  const saved = localStorage.getItem(STORAGE_KEY) ?? localStorage.getItem(LEGACY_STORAGE_KEY) ?? localStorage.getItem(SINK_STORAGE_KEY);
  return saved === "dark" || saved === "original" || saved === "system" ? saved : "original";
}

interface ThemeState {
  theme: ThemeId;
  setTheme: (t: ThemeId) => void;
}

export const useTheme = create<ThemeState>((set) => ({
  theme: initial(),
  setTheme: (t) => {
    localStorage.setItem(STORAGE_KEY, t);
    apply(t);
    set({ theme: t });
  },
}));

/** Apply the persisted theme as early as possible (called from main), and
 * keep "Follow System" live for the rest of the session - re-resolving
 * whenever the OS preference flips, without the user having to reopen
 * Settings and re-pick it. */
export function bootTheme() {
  apply(initial());
  window.matchMedia(DARK_MEDIA_QUERY).addEventListener("change", () => {
    if (useTheme.getState().theme === "system") apply("system");
  });
}
