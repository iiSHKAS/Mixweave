import { useI18n } from "../i18n";
import { useMixerStore } from "../store/mixer";
import { UNITY_VOLUME, useVolumeRange } from "../store/volumeRange";
import { MAX_MIC_GAIN, MAX_VOLUME } from "../types";
import { Toggle, ToggleRow } from "./Toggle";

/** Pulls a strip's levels back to 100 % (used when amplification is turned
 * off while it is above that, so the slider and the real level agree). */
function clampToUnity(id: string) {
  const mixer = useMixerStore.getState();
  const cap = (level: number) => Math.min(level, UNITY_VOLUME);
  const channel = mixer.channels.find((candidate) => candidate.name === id);
  if (channel) {
    if (channel.volume_percent > UNITY_VOLUME) void mixer.setChannelVolume(id, cap(channel.volume_percent));
    if (channel.stream_send_volume_percent > UNITY_VOLUME) {
      void mixer.setChannelStreamVolume(id, cap(channel.stream_send_volume_percent));
    }
    return;
  }
  const bus = mixer.buses.find((candidate) => candidate.name === id);
  if (bus) {
    if (bus.volume_percent > UNITY_VOLUME) void mixer.setBusVolume(id, cap(bus.volume_percent));
    return;
  }
  const mic = mixer.micConfigs.find((candidate) => candidate.node_name === id);
  if (mic) {
    const patch: { gain_percent?: number; stream_send_gain_percent?: number } = {};
    if (mic.gain_percent > UNITY_VOLUME) patch.gain_percent = cap(mic.gain_percent);
    if (mic.stream_send_gain_percent > UNITY_VOLUME) patch.stream_send_gain_percent = cap(mic.stream_send_gain_percent);
    if (Object.keys(patch).length > 0) void mixer.setMicChannelConfig(id, patch);
  }
}

/** "Allow above 100 %" switch for one strip's slider. */
export function VolumeRangeToggle({ id, compact = false }: Readonly<{ id: string; compact?: boolean }>) {
  const { t } = useI18n();
  const mic = useMixerStore((state) => state.micConfigs.some((candidate) => candidate.node_name === id));
  const allowed = useVolumeRange((state) => state.overrides[id] === true);
  const setAllowed = useVolumeRange((state) => state.setAllowed);
  const hardMax = mic ? MAX_MIC_GAIN : MAX_VOLUME;
  const toggle = () => {
    setAllowed(id, !allowed);
    if (allowed) clampToUnity(id);
  };
  if (compact) {
    return (
      <label className="range-chip" title={t("volumeRange.description", { max: hardMax })}>
        <span>{t("volumeRange.short")}</span>
        <Toggle on={allowed} onClick={toggle} />
      </label>
    );
  }
  return (
    <ToggleRow
      icon="volume_up"
      title={t("volumeRange.title")}
      sub={t("volumeRange.description", { max: hardMax })}
      on={allowed}
      onToggle={toggle}
    />
  );
}
