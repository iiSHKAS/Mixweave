import { create } from "zustand";

export interface ShortcutOsdPayload {
  label: string;
  volumePercent: number;
  /** The control's own ceiling (150 for channels/buses, 200 for mic gain) -
   * the bar fills relative to this, matching how the real faders read,
   * while the number shown is still the plain value. */
  max: number;
  muted: boolean;
}

interface ShortcutOsdState extends ShortcutOsdPayload {
  /** Bumped on every show() so the OSD resets its auto-hide timer even when
   * the same channel is triggered again before the previous popup faded. */
  token: number;
  show: (payload: ShortcutOsdPayload) => void;
}

/** Fired by the keyboard-shortcut dispatch code (not a React component) to
 * tell the on-screen overlay what just changed. */
export const useShortcutOsd = create<ShortcutOsdState>((set, get) => ({
  label: "",
  volumePercent: 0,
  max: 100,
  muted: false,
  token: 0,
  show: (payload) => set({ ...payload, token: get().token + 1 }),
}));
