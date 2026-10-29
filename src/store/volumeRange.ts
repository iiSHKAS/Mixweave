import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";
import { MASTER_BUS, STREAMER_MODE_BUS } from "../types";

/** Unity level: what every slider tops out at unless amplification is allowed. */
export const UNITY_VOLUME = 100;

const STORAGE_KEY = "mixweave-volume-range";

/** Per-strip choice to let a slider amplify past 100 %, keyed by the strip's
 * node name (channel, mix/master bus, or microphone). A strip's Personal and
 * Stream lanes share it. Absent means "not allowed". */
type Overrides = Record<string, boolean>;

/** Master (and its Streamer Mode twin) always starts at a 100 % cap: it scales
 * every channel, so amplifying it is opt-in even for setups that ran above it. */
export const MASTER_STRIP_IDS: readonly string[] = [MASTER_BUS, STREAMER_MODE_BUS];
const MASTER_RESET_KEY = "mixweave-volume-range-master-default";

function load(): Overrides {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return {};
    const overrides: Overrides = Object.fromEntries(
      Object.entries(parsed).filter(([, value]) => typeof value === "boolean"),
    );
    // One time: drop any Master range that an earlier "keep what is in use"
    // pass switched on, so Master is back at the 100 % default.
    if (!localStorage.getItem(MASTER_RESET_KEY)) {
      localStorage.setItem(MASTER_RESET_KEY, "1");
      for (const id of MASTER_STRIP_IDS) delete overrides[id];
      localStorage.setItem(STORAGE_KEY, JSON.stringify(overrides));
    }
    return overrides;
  } catch {
    return {};
  }
}

function save(overrides: Overrides) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(overrides));
  } catch {
    // The backend remains authoritative when the local cache is unavailable.
  }
}

interface VolumeRangeState {
  overrides: Overrides;
  setAllowed: (id: string, allowed: boolean) => Promise<void>;
}

export const useVolumeRange = create<VolumeRangeState>((set) => ({
  overrides: load(),
  setAllowed: async (id, allowed) => {
    const overrides = await invoke<Overrides>("sync_volume_ranges", { id, allowed });
    save(overrides);
    set({ overrides });
  },
}));

export async function initializeVolumeRanges() {
  const overrides = await invoke<Overrides>("sync_volume_ranges", { legacy: load() });
  save(overrides);
  useVolumeRange.setState({ overrides });
}

/** The slider maximum for a strip: `hardMax` when amplification is allowed,
 * otherwise 100 %. */
export function volumeCeiling(overrides: Overrides, id: string, hardMax: number): number {
  return overrides[id] ? hardMax : UNITY_VOLUME;
}

export function useVolumeCeiling(id: string, hardMax: number): number {
  return useVolumeRange((state) => volumeCeiling(state.overrides, id, hardMax));
}

export function getVolumeCeiling(id: string, hardMax: number): number {
  return volumeCeiling(useVolumeRange.getState().overrides, id, hardMax);
}
