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
    // Storage can be unavailable; the choice then lasts for this session only.
  }
}

interface VolumeRangeState {
  overrides: Overrides;
  setAllowed: (id: string, allowed: boolean) => void;
}

export const useVolumeRange = create<VolumeRangeState>((set, get) => ({
  overrides: load(),
  setAllowed: (id, allowed) => {
    const overrides = { ...get().overrides, [id]: allowed };
    save(overrides);
    set({ overrides });
  },
}));

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

/** A strip already running above 100 % (a setup saved before this option
 * existed, or a profile that was) keeps its slider range: allow amplification
 * on exactly those strips, unless the user has explicitly chosen otherwise.
 * Everything else defaults to 100 %. Safe to call on every state change. */
export function allowBoostWhereInUse(strips: ReadonlyArray<{ id: string; levels: readonly number[] }>) {
  const { overrides } = useVolumeRange.getState();
  let next: Overrides | null = null;
  for (const { id, levels } of strips) {
    if (id in overrides || MASTER_STRIP_IDS.includes(id) || !levels.some((level) => level > UNITY_VOLUME)) continue;
    next ??= { ...overrides };
    next[id] = true;
  }
  if (!next) return;
  save(next);
  useVolumeRange.setState({ overrides: next });
}
