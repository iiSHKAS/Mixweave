import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface UpdateStatus {
  phase: "idle" | "checking" | "current" | "downloading" | "installing" | "installed" | "error";
  version: string | null;
  downloaded: number;
  total: number | null;
  error: string | null;
}
interface UpdateInfo { enabled: boolean; supported: boolean; configured: boolean; status: UpdateStatus }
interface UpdateStore {
  info: UpdateInfo | null;
  actionError: string | null;
  pending: boolean;
  refresh: () => Promise<void>;
  toggle: (enabled: boolean) => Promise<void>;
  check: () => Promise<void>;
}
let statusRevision = 0;
let latestStatus: UpdateStatus | undefined;
export const useUpdates = create<UpdateStore>((set, get) => ({
  info: null, actionError: null, pending: false,
  refresh: async () => {
    const revision = statusRevision;
    try {
      const info = await invoke<UpdateInfo>("get_update_status");
      // A slow snapshot must not overwrite a newer completion/progress event.
      if (statusRevision !== revision && latestStatus) info.status = latestStatus;
      set({ info });
    }
    catch (error) { set({ actionError: String(error) }); }
  },
  toggle: async (enabled) => {
    set({ pending: true, actionError: null });
    try { await invoke("set_auto_update", { enabled }); await get().refresh(); }
    catch (error) { set({ actionError: String(error) }); }
    finally { set({ pending: false }); }
  },
  check: async () => {
    set({ pending: true, actionError: null });
    try { await invoke("check_and_install_update"); }
    catch (error) { set({ actionError: String(error) }); }
    finally { await get().refresh(); set({ pending: false }); }
  },
}));
export async function subscribeUpdates() {
  const unlisten = await listen<UpdateStatus>("mixweave://update", ({ payload }) => {
    statusRevision += 1;
    latestStatus = payload;
    const info = useUpdates.getState().info;
    if (info) useUpdates.setState({ info: { ...info, status: payload } });
    else void useUpdates.getState().refresh();
  });
  await useUpdates.getState().refresh();
  return unlisten;
}
