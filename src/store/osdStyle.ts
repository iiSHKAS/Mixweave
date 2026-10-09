import { create } from "zustand";

/** Looks of the volume/mute popup shown by the keyboard shortcuts. */
export type OsdStyle = "segments" | "fader" | "waves";

export const OSD_STYLES: readonly { id: OsdStyle; labelKey: "settings.osd.segments" | "settings.osd.fader" | "settings.osd.waves" }[] = [
  { id: "segments", labelKey: "settings.osd.segments" },
  { id: "fader", labelKey: "settings.osd.fader" },
  { id: "waves", labelKey: "settings.osd.waves" },
];

export const DEFAULT_OSD_STYLE: OsdStyle = "waves";
const STORAGE_KEY = "mixweave-osd-style";

function isOsdStyle(value: unknown): value is OsdStyle {
  return value === "segments" || value === "fader" || value === "waves";
}

/** Read straight from storage: the popup can live in a separate window that
 * must pick up a change made in the main window. */
export function readOsdStyle(): OsdStyle {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    return isOsdStyle(saved) ? saved : DEFAULT_OSD_STYLE;
  } catch {
    return DEFAULT_OSD_STYLE;
  }
}

interface OsdStyleState {
  style: OsdStyle;
  setStyle: (style: OsdStyle) => void;
}

export const useOsdStyle = create<OsdStyleState>((set) => ({
  style: readOsdStyle(),
  setStyle: (style) => {
    try {
      localStorage.setItem(STORAGE_KEY, style);
    } catch {
      // The choice then lasts for this session only.
    }
    set({ style });
  },
}));
