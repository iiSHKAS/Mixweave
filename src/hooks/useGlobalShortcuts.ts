import { useEffect } from "react";
import { register, unregister } from "@tauri-apps/plugin-global-shortcut";
import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { useMixerStore } from "../store/mixer";
import { DEFAULT_SHORTCUTS, useShortcutSettings, type ShortcutAction } from "../store/shortcuts";
import {
  allChannelShortcutEntries,
  parseChannelShortcutActionId,
  useChannelShortcuts,
} from "../store/channelShortcuts";
import type { ShortcutOsdPayload } from "../store/shortcutOsd";
import { MASTER_BUS, MAX_MIC_GAIN, MAX_VOLUME } from "../types";
import { getVolumeCeiling } from "../store/volumeRange";
import { currentLook } from "../store/theme";
import { readOsdStyle } from "../store/osdStyle";
import { readOsdPosition } from "../store/osdPosition";
import { translate } from "../i18nCore";

/** Mirrors the Rust enum in commands::linux_shortcuts::ShortcutBackend. */
export type ShortcutBackend = "x11" | "gnome_wayland" | "unsupported";

const KNOWN_ACTIONS = new Set(Object.keys(DEFAULT_SHORTCUTS));
const CHANNEL_VOLUME_STEP = 5;

// Set from useGlobalShortcuts()'s own detect_shortcut_backend call so the
// plain dispatch functions below (outside any component, called straight
// from an event) can tell whether to also nudge the GNOME Shell extension,
// without re-detecting on every single keypress.
let lastDetectedBackend: ShortcutBackend = "x11";

function isShortcutAction(value: string): value is ShortcutAction {
  return KNOWN_ACTIONS.has(value);
}

let restartPending = false;

function currentTranslation(key: Parameters<typeof translate>[1], variables: Parameters<typeof translate>[2] = {}) {
  return translate(document.documentElement.lang || "en", key, variables);
}

export function restartApplication() {
  if (restartPending) return;
  restartPending = true;
  void invoke("restart_app").catch((cause) => {
    restartPending = false;
    useMixerStore.setState({ error: currentTranslation("errors.restartFailed", { cause: String(cause) }) });
  });
}

function steppedVolume(current: number, kind: "volume_up" | "volume_down", max: number): number {
  const delta = kind === "volume_up" ? CHANNEL_VOLUME_STEP : -CHANNEL_VOLUME_STEP;
  return Math.max(0, Math.min(max, current + delta));
}

function busDisplayLabel(bus: { name: string; label: string }): string {
  return bus.name === MASTER_BUS ? currentTranslation("mixer.group.master") : bus.label;
}

/** Broadcasts to every window - the dedicated X11 overlay window and/or the
 * main window's in-window fallback both just listen for this same event
 * (see ShortcutOsd.tsx) - plus, on GNOME/Wayland, nudges the GNOME Shell
 * extension that draws the real system-wide popup there (see
 * gnome_osd_extension.rs). A failure on that second path is expected right
 * after a first install, before GNOME has noticed the new extension, so
 * it's swallowed rather than surfaced as an app error. */
export function showOsd(payload: ShortcutOsdPayload) {
  void emit("shortcut-osd", payload);
  if (lastDetectedBackend === "gnome_wayland") {
    void invoke("show_gnome_osd", {
      label: payload.label,
      volumePercent: payload.volumePercent,
      max: payload.max,
      muted: payload.muted,
      theme: currentLook() === "dark" ? "dark" : "light",
      style: readOsdStyle(),
      position: readOsdPosition(),
    }).catch(() => {});
  }
}

/** The popover behind the same "keyboard" button is shared by regular
 * channels, buses (Master and custom mixes), and mic strips - each backed
 * by a different slice of the mixer store, so dispatch has to check all
 * three by name rather than assuming "channel" means a playback channel.
 * A "stream_*" kind (Streamer Mode's independent Stream lane - see
 * `ChannelShortcutKind`) always resolves to the exact same channel/mic by
 * name, just acting on its Stream fields instead of its Personal ones; no
 * bus ever produces one (Master's Stream lane has no shortcuts button of
 * its own - "listen" there already covers every mic automatically), so the
 * bus branch is left as plain Personal-only. */
function runChannelShortcut(action: string): boolean {
  const parsed = parseChannelShortcutActionId(action);
  if (!parsed) return false;
  const mixer = useMixerStore.getState();
  const stream = parsed.kind.startsWith("stream_");
  const baseKind = (stream ? parsed.kind.slice("stream_".length) : parsed.kind) as
    | "mute"
    | "volume_up"
    | "volume_down";
  const osdLabel = (base: string) => (stream ? `${base} • ${currentTranslation("streamer.stream")}` : base);

  const channel = mixer.channels.find((candidate) => candidate.name === parsed.channel);
  if (channel) {
    let muted = stream ? channel.stream_send_muted : channel.muted;
    let volumePercent = stream ? channel.stream_send_volume_percent : channel.volume_percent;
    if (baseKind === "mute") {
      muted = !muted;
      if (stream) void mixer.toggleChannelStreamMute(channel.name, muted);
      else void mixer.toggleMute(channel.name, muted);
    } else {
      volumePercent = steppedVolume(volumePercent, baseKind, getVolumeCeiling(channel.name, MAX_VOLUME));
      if (stream) void mixer.setChannelStreamVolume(channel.name, volumePercent);
      else void mixer.setChannelVolume(channel.name, volumePercent);
    }
    showOsd({ label: osdLabel(channel.label), volumePercent, max: getVolumeCeiling(channel.name, MAX_VOLUME), muted });
    return true;
  }

  const bus = mixer.buses.find((candidate) => candidate.name === parsed.channel);
  if (bus) {
    let muted = bus.muted;
    let volumePercent = bus.volume_percent;
    if (baseKind === "mute") {
      muted = !muted;
      void mixer.setBusMute(bus.name, muted);
    } else {
      volumePercent = steppedVolume(volumePercent, baseKind, getVolumeCeiling(bus.name, MAX_VOLUME));
      void mixer.setBusVolume(bus.name, volumePercent);
    }
    showOsd({ label: busDisplayLabel(bus), volumePercent, max: getVolumeCeiling(bus.name, MAX_VOLUME), muted });
    return true;
  }

  const mic = mixer.micConfigs.find((candidate) => candidate.node_name === parsed.channel);
  if (mic) {
    let muted = stream ? mic.stream_send_muted : mic.muted;
    let volumePercent = stream ? mic.stream_send_gain_percent : mic.gain_percent;
    if (baseKind === "mute") {
      muted = !muted;
      void mixer.setMicChannelConfig(mic.node_name, stream ? { stream_send_muted: muted } : { muted });
    } else {
      volumePercent = steppedVolume(volumePercent, baseKind, getVolumeCeiling(mic.node_name, MAX_MIC_GAIN));
      void mixer.setMicChannelConfig(
        mic.node_name,
        stream ? { stream_send_gain_percent: volumePercent } : { gain_percent: volumePercent },
      );
    }
    showOsd({ label: osdLabel(mic.output_label), volumePercent, max: getVolumeCeiling(mic.node_name, MAX_MIC_GAIN), muted });
    return true;
  }

  return true;
}

function runShortcut(action: string) {
  if (runChannelShortcut(action)) return;
  if (!isShortcutAction(action)) return;

  if (action === "restart_app") {
    restartApplication();
    return;
  }
  const mixer = useMixerStore.getState();
  if (action === "toggle_mic") {
    if (mixer.micConfig) {
      const muted = !mixer.micConfig.muted;
      void mixer.setMicConfig({ muted });
      showOsd({
        label: mixer.micConfig.output_label,
        volumePercent: mixer.micConfig.gain_percent,
        max: getVolumeCeiling(mixer.micConfig.node_name, MAX_MIC_GAIN),
        muted,
      });
    }
    return;
  }

  const name = action === "toggle_game" ? "sink_game" : "sink_chat";
  const channel = mixer.channels.find((candidate) => candidate.name === name);
  if (channel) {
    const muted = !channel.muted;
    void mixer.toggleMute(channel.name, muted);
    showOsd({ label: channel.label, volumePercent: channel.volume_percent, max: getVolumeCeiling(channel.name, MAX_VOLUME), muted });
  }
}

export function useGlobalShortcuts() {
  const enabled = useShortcutSettings((state) => state.enabled);
  const bindings = useShortcutSettings((state) => state.bindings);
  const channelBindings = useChannelShortcuts((state) => state.byChannel);

  // GNOME re-invokes our own binary from a system-wide keybinding; the
  // running instance receives that as a "mixweave://shortcut" event (see
  // lib.rs's single-instance callback) instead of a plugin callback.
  useEffect(() => {
    let disposed = false;
    let disposeListener: (() => void) | undefined;
    void listen<string>("mixweave://shortcut", (event) => {
      if (!useShortcutSettings.getState().enabled) return;
      runShortcut(event.payload);
    }).then((unlisten) => {
      if (disposed) unlisten();
      else disposeListener = unlisten;
    });
    return () => {
      disposed = true;
      disposeListener?.();
    };
  }, []);

  useEffect(() => {
    if (!enabled) {
      // Best-effort: drop any GNOME custom keybindings from a previous
      // session so a disabled toggle actually stops the shortcuts.
      void invoke<ShortcutBackend>("detect_shortcut_backend")
        .then((backend) => backend === "gnome_wayland" && invoke("sync_gnome_shortcuts", { enabled: false, bindings: {} }))
        .catch(() => {});
      return;
    }

    const entries: [string, string][] = [
      ...(Object.entries(bindings) as [ShortcutAction, string][]).filter(([, shortcut]) => shortcut.length > 0),
      ...allChannelShortcutEntries(channelBindings),
    ];
    const normalized = entries.map(([, shortcut]) => shortcut.toLowerCase());
    if (new Set(normalized).size !== normalized.length) {
      useMixerStore.setState({ error: currentTranslation("errors.shortcutsDuplicate") });
      return;
    }

    let disposed = false;
    const registered: string[] = [];

    const setup = async () => {
      const backend = await invoke<ShortcutBackend>("detect_shortcut_backend").catch(
        (): ShortcutBackend => "x11",
      );
      if (disposed) return;
      lastDetectedBackend = backend;

      // Wayland/GNOME: the plugin's X11 grab only fires while our own
      // window has focus. Drive GNOME's own custom-keybinding mechanism
      // instead, which fires system-wide regardless of GNOME version.
      if (backend === "gnome_wayland") {
        try {
          await invoke("sync_gnome_shortcuts", {
            enabled,
            bindings: Object.fromEntries(entries),
          });
        } catch (cause) {
          if (!disposed) {
            useMixerStore.setState({
              error: currentTranslation("errors.shortcutsRegistration", { shortcuts: String(cause) }),
            });
          }
        }
        return;
      }

      const failed: string[] = [];
      for (const [action, shortcut] of entries) {
        try {
          await register(shortcut, (event) => {
            if (event.state === "Pressed") runShortcut(action);
          });
          if (disposed) {
            await unregister(shortcut);
            return;
          }
          registered.push(shortcut);
        } catch {
          failed.push(shortcut);
        }
      }
      if (!disposed && failed.length > 0) {
        useMixerStore.setState({
          error: currentTranslation("errors.shortcutsRegistration", { shortcuts: failed.join(", ") }),
        });
      }
    };

    void setup();
    return () => {
      disposed = true;
      if (registered.length > 0) void unregister(registered).catch(() => {});
    };
  }, [enabled, bindings, channelBindings]);
}
