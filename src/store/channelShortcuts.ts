import { create } from "zustand";

/** The "stream_" kinds are Streamer Mode's independent Stream lane - a
 * genuinely separate binding from its Personal counterpart (`mute` etc.),
 * not just a different label on the same one, so a channel or mic can have
 * both a "mute Personal" and a "mute Stream" hotkey at once. */
export type ChannelShortcutKind =
  | "mute"
  | "volume_up"
  | "volume_down"
  | "stream_mute"
  | "stream_volume_up"
  | "stream_volume_down";

export interface ChannelShortcutBindings {
  mute: string;
  volume_up: string;
  volume_down: string;
  stream_mute: string;
  stream_volume_up: string;
  stream_volume_down: string;
}

const EMPTY_CHANNEL_BINDINGS: ChannelShortcutBindings = {
  mute: "",
  volume_up: "",
  volume_down: "",
  stream_mute: "",
  stream_volume_up: "",
  stream_volume_down: "",
};

const STORAGE_KEY = "mixweave-channel-shortcuts";
const LEGACY_STORAGE_KEY = "sonux-channel-shortcuts";
const ACTION_KIND_PATTERN =
  /^channel:(.+):(mute|volume_up|volume_down|stream_mute|stream_volume_up|stream_volume_down)$/;

/** Game and Chat's mute is the older global toggle_game/toggle_chat action,
 * not a distinct per-channel one - ChannelShortcutsPopover edits that global
 * binding directly for these two so there is exactly one stored value,
 * shown and changed the same way from either screen. Excluded here too so a
 * leftover value from before that UI change (or any other stray write)
 * never gets registered a second time under its own "channel:..." id. */
export const DELEGATED_TO_GLOBAL: Partial<Record<string, ChannelShortcutKind>> = {
  sink_game: "mute",
  sink_chat: "mute",
};

function isDelegatedToGlobal(channel: string, kind: ChannelShortcutKind): boolean {
  return DELEGATED_TO_GLOBAL[channel] === kind;
}

/** Encodes a per-channel shortcut as the single opaque action string that
 * flows through the plugin registration loop, the Wayland/GNOME gsettings
 * sync, and the "mixweave://shortcut" event payload. */
export function channelShortcutActionId(channel: string, kind: ChannelShortcutKind): string {
  return `channel:${channel}:${kind}`;
}

export function parseChannelShortcutActionId(
  action: string,
): { channel: string; kind: ChannelShortcutKind } | null {
  const match = ACTION_KIND_PATTERN.exec(action);
  if (!match) return null;
  return { channel: match[1], kind: match[2] as ChannelShortcutKind };
}

type StoredState = Record<string, ChannelShortcutBindings>;

function readSettings(): StoredState {
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? localStorage.getItem(LEGACY_STORAGE_KEY) ?? "null") as
      | Partial<Record<string, Partial<ChannelShortcutBindings>>>
      | null;
    if (!saved || typeof saved !== "object") return {};
    const result: StoredState = {};
    for (const [channel, bindings] of Object.entries(saved)) {
      if (!bindings || typeof bindings !== "object") continue;
      result[channel] = {
        mute: typeof bindings.mute === "string" ? bindings.mute : "",
        volume_up: typeof bindings.volume_up === "string" ? bindings.volume_up : "",
        volume_down: typeof bindings.volume_down === "string" ? bindings.volume_down : "",
        // Missing in settings saved before the Stream lane got its own
        // bindings - defaults to unbound, same as any other fresh kind.
        stream_mute: typeof bindings.stream_mute === "string" ? bindings.stream_mute : "",
        stream_volume_up: typeof bindings.stream_volume_up === "string" ? bindings.stream_volume_up : "",
        stream_volume_down: typeof bindings.stream_volume_down === "string" ? bindings.stream_volume_down : "",
      };
    }
    return result;
  } catch {
    return {};
  }
}

function save(state: StoredState) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
}

interface ChannelShortcutsState {
  byChannel: StoredState;
  getBindings: (channel: string) => ChannelShortcutBindings;
  setBinding: (channel: string, kind: ChannelShortcutKind, shortcut: string) => void;
}

/** Whether one lane (Personal or Stream) has any of its three shortcuts
 * bound - for highlighting just that lane's trigger button, not the whole
 * channel's, now that the two are independent. */
export function laneHasBindings(bindings: ChannelShortcutBindings, lane: "personal" | "stream"): boolean {
  return lane === "stream"
    ? Boolean(bindings.stream_mute || bindings.stream_volume_up || bindings.stream_volume_down)
    : Boolean(bindings.mute || bindings.volume_up || bindings.volume_down);
}

export const useChannelShortcuts = create<ChannelShortcutsState>((set, get) => ({
  byChannel: readSettings(),
  getBindings: (channel) => get().byChannel[channel] ?? EMPTY_CHANNEL_BINDINGS,
  setBinding: (channel, kind, shortcut) => {
    const current = get().byChannel[channel] ?? EMPTY_CHANNEL_BINDINGS;
    const next: StoredState = {
      ...get().byChannel,
      [channel]: { ...current, [kind]: shortcut.trim() },
    };
    save(next);
    set({ byChannel: next });
  },
}));

/** Flattened (actionId, shortcut) pairs across every channel, for merging
 * into the global registration loop. Empty bindings are already excluded. */
export function allChannelShortcutEntries(byChannel: StoredState): [string, string][] {
  const entries: [string, string][] = [];
  for (const [channel, bindings] of Object.entries(byChannel)) {
    for (const kind of [
      "mute",
      "volume_up",
      "volume_down",
      "stream_mute",
      "stream_volume_up",
      "stream_volume_down",
    ] as const) {
      if (isDelegatedToGlobal(channel, kind)) continue;
      const shortcut = bindings[kind];
      if (shortcut) entries.push([channelShortcutActionId(channel, kind), shortcut]);
    }
  }
  return entries;
}
