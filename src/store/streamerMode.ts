import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

interface StreamerModeState {
  enabled: boolean;
  /** Sets `enabled` from the persisted backend value at startup, without
   *  writing it back (see `synchronizeStartupState`). */
  hydrate: (enabled: boolean) => void;
  /** The user's own toggle: creates/destroys the Streamer Mode mix's live
   *  node (`set_streamer_mode_enabled`) and persists the choice so it
   *  survives a restart. While off, that node does not exist at all, so it
   *  can never appear in any device picker. Reverts and sets `error` on
   *  failure - the caller surfaces `error` however it displays errors,
   *  kept out of this store to avoid a mixer.ts <-> streamerMode.ts import
   *  cycle. */
  setEnabled: (enabled: boolean) => Promise<void>;
  error: string | null;
}

export const useStreamerModeStore = create<StreamerModeState>((set, get) => ({
  enabled: false,
  hydrate: (enabled) => set({ enabled }),
  setEnabled: async (enabled) => {
    const previous = get().enabled;
    set({ enabled, error: null });
    try {
      await invoke("set_streamer_mode_enabled", { enabled });
    } catch (e) {
      set({ enabled: previous, error: String(e) });
    }
  },
  error: null,
}));
