import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useMixerStore } from "../../store/mixer";
import { Ms } from "../Icons";
import { Popover } from "../Popover";
import { useI18n } from "../../i18n";

interface HardwareDevice {
  name: string;
  description: string;
  kind: "output" | "input";
  volume_percent: number;
  muted: boolean;
  is_default: boolean;
}

const POLL_MS = 3000;

/**
 * Master-strip device panel (fills the bottom well): pick the physical output/input and see/adjust each
 * device's real hardware level, so a quiet device (e.g. stuck at 40%) is visible.
 */
export function HardwareDevices() {
  const { t } = useI18n();
  const [openKind, setOpenKind] = useState<"output" | "input" | null>(null);
  const [devices, setDevices] = useState<HardwareDevice[]>([]);
  const [failed, setFailed] = useState(false);
  const channels = useMixerStore((s) => s.channels);
  const channelOutputs = useMixerStore((s) => s.channelOutputs);
  const resolvedOutputs = useMixerStore((s) => s.resolvedOutputs);
  const micConfig = useMixerStore((s) => s.micConfig);
  const setAllOutputs = useMixerStore((s) => s.setAllOutputs);
  const setMicConfig = useMixerStore((s) => s.setMicConfig);

  const refresh = useCallback(async () => {
    try {
      setDevices(await invoke<HardwareDevice[]>("get_hardware_devices"));
      setFailed(false);
    } catch {
      setFailed(true);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const id = window.setInterval(() => { if (!document.hidden) void refresh(); }, POLL_MS);
    return () => window.clearInterval(id);
  }, [refresh]);

  const patch = (name: string, change: Partial<HardwareDevice>) =>
    setDevices((all) => all.map((d) => (d.name === name ? { ...d, ...change } : d)));

  const setVolume = (d: HardwareDevice, percent: number) => {
    patch(d.name, { volume_percent: percent });
    void invoke("set_hardware_volume", { kind: d.kind, name: d.name, percent }).catch(() => void refresh());
  };
  const toggleMute = (d: HardwareDevice) => {
    patch(d.name, { muted: !d.muted });
    void invoke("set_hardware_mute", { kind: d.kind, name: d.name, muted: !d.muted }).catch(() => void refresh());
  };

  const outputActive = (d: HardwareDevice) =>
    channels.length > 0 &&
    channels.every((c) => {
      const chosen = channelOutputs[c.name] ?? null;
      return chosen === null ? (resolvedOutputs[c.name] ?? null) === d.name : chosen === d.name;
    });
  const inputActive = (d: HardwareDevice) =>
    micConfig ? (micConfig.input_device ?? null) === d.name || (micConfig.input_device == null && d.is_default) : false;

  const row = (d: HardwareDevice, active: boolean, closePicker?: () => void) => {
    const kind = d.kind;
    return (
      <div key={d.name} className={"hw-row" + (active ? " active" : "")}>
        <button
          type="button"
          className="hw-pick"
          aria-pressed={active}
          title={t(kind === "output" ? "mixer.hardware.useOutput" : "mixer.hardware.useInput")}
          onClick={() => {
            if (kind === "output") void setAllOutputs(d.name);
            else void setMicConfig({ input_device: d.name });
            closePicker?.();
          }}
        >
          <Ms name={active ? "radio_button_checked" : "radio_button_unchecked"} />
          <span className="hw-name">{d.description}</span>
        </button>
        <div className="hw-level">
          <button
            type="button"
            className="hw-mute"
            aria-label={t(d.muted ? "mixer.hardware.unmute" : "mixer.hardware.mute", { device: d.description })}
            onClick={() => toggleMute(d)}
          >
            <Ms name={d.muted ? (kind === "output" ? "volume_off" : "mic_off") : kind === "output" ? "volume_up" : "mic"} />
          </button>
          <input
            type="range"
            min={0}
            max={100}
            value={Math.min(100, d.volume_percent)}
            aria-label={t("mixer.hardware.level", { device: d.description })}
            onChange={(e) => setVolume(d, Number(e.target.value))}
          />
          <span className={"hw-pct" + (d.volume_percent < 100 && !d.muted ? " low" : "")} dir="ltr">{d.volume_percent}%</span>
        </div>
      </div>
    );
  };

  // One compact box per kind: the selected (else default) device with its
  // level; the header opens the full list to switch device or tune the others.
  const section = (kind: "output" | "input") => {
    const isActive = kind === "output" ? outputActive : inputActive;
    const rank = (d: HardwareDevice) => (isActive(d) ? 0 : d.is_default ? 1 : 2);
    const list = devices.filter((d) => d.kind === kind).sort((a, b) => rank(a) - rank(b));
    const label = t(`mixer.hardware.${kind}s`);
    const open = openKind === kind;
    return (
      <div className="strip-apps hw-inline" aria-label={label}>
        <button
          type="button"
          className="hw-heading"
          title={t("mixer.hardware.title")}
          aria-expanded={open}
          onClick={() => setOpenKind(open ? null : kind)}
        >
          <Ms name={kind === "output" ? "speaker" : "mic"} />
          <span className="hw-heading-label">{label}</span>
          {list.length > 1 && <span className="hw-count">{list.length}</span>}
          <Ms name="expand_more" />
        </button>
        {list.length === 0 ? <div className="hw-empty">{t("mixer.hardware.none")}</div> : row(list[0], isActive(list[0]))}
        <Popover open={open} onClose={() => setOpenKind(null)} side="top" align="center" style={{ minWidth: 280 }}>
          <div className="hw-panel">{list.map((d) => row(d, isActive(d), () => setOpenKind(null)))}</div>
        </Popover>
      </div>
    );
  };

  return (
    <div className="hw-stack" title={failed ? t("mixer.hardware.error") : undefined}>
      {section("output")}
      {section("input")}
    </div>
  );
}
