import { create } from "zustand";

/** Screen corner/edge the volume/mute popup appears at. */
export type OsdPosition =
  | "top-right"
  | "middle-right"
  | "bottom-right"
  | "top-left"
  | "middle-left"
  | "bottom-left";

// Translation keys use camelCase segments (the app's convention - see
// TRANSLATION_KEY_PATTERN in i18nCore.ts, which rejects hyphens), unlike the
// hyphenated `id`s above that also double as the GNOME extension's D-Bus
// argument and CSS class suffix.
type OsdPositionLabelKey =
  | "settings.osd.position.topRight"
  | "settings.osd.position.middleRight"
  | "settings.osd.position.bottomRight"
  | "settings.osd.position.topLeft"
  | "settings.osd.position.middleLeft"
  | "settings.osd.position.bottomLeft";

export const OSD_POSITIONS: readonly { id: OsdPosition; labelKey: OsdPositionLabelKey }[] = [
  { id: "top-right", labelKey: "settings.osd.position.topRight" },
  { id: "middle-right", labelKey: "settings.osd.position.middleRight" },
  { id: "bottom-right", labelKey: "settings.osd.position.bottomRight" },
  { id: "top-left", labelKey: "settings.osd.position.topLeft" },
  { id: "middle-left", labelKey: "settings.osd.position.middleLeft" },
  { id: "bottom-left", labelKey: "settings.osd.position.bottomLeft" },
];

/** Matches the GNOME extension's original fixed placement, so anyone who
 * never touches this setting sees no change. */
export const DEFAULT_OSD_POSITION: OsdPosition = "middle-right";
const STORAGE_KEY = "mixweave-osd-position";

function isOsdPosition(value: unknown): value is OsdPosition {
  return OSD_POSITIONS.some((option) => option.id === value);
}

/** Read straight from storage: the popup can live in a separate window that
 * must pick up a change made in the main window. */
export function readOsdPosition(): OsdPosition {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    return isOsdPosition(saved) ? saved : DEFAULT_OSD_POSITION;
  } catch {
    return DEFAULT_OSD_POSITION;
  }
}

interface OsdPositionState {
  position: OsdPosition;
  setPosition: (position: OsdPosition) => void;
}

export const useOsdPosition = create<OsdPositionState>((set) => ({
  position: readOsdPosition(),
  setPosition: (position) => {
    try {
      localStorage.setItem(STORAGE_KEY, position);
    } catch {
      // The choice then lasts for this session only.
    }
    set({ position });
  },
}));
