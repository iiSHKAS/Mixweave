import { memo, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { useMixerStore } from "../../store/mixer";
import { useStreamerModeStore } from "../../store/streamerMode";
import type { BusDef } from "../../types";
import { busMembers, MASTER_BUS, STREAMER_MODE_BUS, MAX_VOLUME } from "../../types";
import { useVolumeCeiling } from "../../store/volumeRange";
import { volToDb } from "../../lib/audio";
import { channelIcon, Ms } from "../Icons";
import { ConfirmModal } from "../ConfirmModal";
import { MenuCheckItem } from "../MenuItem";
import { Popover } from "../Popover";
import { Fader } from "./Fader";
import { VuMeter } from "./VuMeter";
import { StreamerLanes, bothLanesMuted } from "./StreamerLanes";
import { ChannelShortcutsPopover } from "./ChannelShortcutsPopover";
import { useChannelShortcuts } from "../../store/channelShortcuts";
import { HardwareDevices } from "./HardwareDevices";
import { ProfileMenu } from "../TitleBar/ProfileMenu";
import { useI18n } from "../../i18n";

/**
 * A mix (record bus): aggregates the chosen channels into a capturable
 * source. The label is exactly the device name recorders display - rename
 * it and OBS sees the new name. Volume/mute shape what recorders hear, not
 * what you hear - except for the master mix, which is the exception: its
 * volume/mute instead rescale and silence every channel's own live output
 * (backend: `push_master_gain_to_channels`), so it's the true overall
 * listening volume, not just a recording tap.
 */
/** Compact "what this mix carries" label for the membership button.
 *  Memoized alongside ChannelStrip/MicStrip so a channel's fader drag
 *  doesn't re-render Master or the other mixes too. */
function BusStripBase({
  bus,
  staggerIndex,
  onManageProfiles,
}: Readonly<{
  bus: BusDef;
  staggerIndex: number;
  onManageProfiles: () => void;
}>) {
  const { t } = useI18n();
  const volumeCeiling = useVolumeCeiling(bus.name, MAX_VOLUME);
  // A flattened primitive key per channel (name/label/icon only, no volume)
  // instead of the raw `channels` array - keeps this bus strip from
  // re-rendering every time a channel's fader moves.
  const channelKeys = useMixerStore(useShallow((s) => s.channels
    .map((c) => `${c.name}\u0000${c.label}\u0000${channelIcon(c)}`)));
  const channels = channelKeys.map((key) => {
    const [name, label, icon] = key.split("\u0000");
    return { name, label, icon };
  });
  const setBusMembers = useMixerStore((s) => s.setBusMembers);
  const setBusExclude = useMixerStore((s) => s.setBusExclude);
  const renameBus = useMixerStore((s) => s.renameBus);
  const removeBus = useMixerStore((s) => s.removeBus);
  const monitoring = useMixerStore((s) => s.monitors[bus.name] ?? false);
  const streamMonitoring = useMixerStore((s) => s.monitors[STREAMER_MODE_BUS] ?? false);
  const toggleMonitor = useMixerStore((s) => s.toggleMonitor);
  const setBusVolume = useMixerStore((s) => s.setBusVolume);
  const setBusMute = useMixerStore((s) => s.setBusMute);

  const [managing, setManaging] = useState(false);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const shortcutBindings = useChannelShortcuts((s) => s.getBindings(bus.name));
  const hasShortcuts = Object.values(shortcutBindings).some(Boolean);

  // The master mix always exists, carries every channel, and is the one mix
  // whose volume/mute are the true overall listening controls (see the
  // component doc comment above).
  const isMaster = bus.name === MASTER_BUS;
  const displayLabel = isMaster ? t("mixer.group.master") : bus.label;
  const memberLabel = () => {
    if (!bus.exclude) return t(carried.length === 1 ? "mixer.mix.channelOne" : "mixer.mix.channelMany", { count: carried.length });
    if (carried.length === allNames.length) return t("mixer.mix.allChannels");
    return t("mixer.mix.allBut", { count: allNames.length - carried.length });
  };

  // Volume/mute live on the persisted bus, so they survive remounts, profile
  // switches, and restarts (the backend re-applies them to the fresh node).
  const volume = bus.volume_percent;
  const muted = bus.muted;

  // Only Master uses two lanes; custom mixes keep a single fader. The Stream
  // lane controls the independent master send via persistence::buses::streamer_gain.
  const streamerMode = useStreamerModeStore((s) => s.enabled);
  const streamerModeBus = useMixerStore((s) => s.buses.find((b) => b.name === STREAMER_MODE_BUS));
  const streamVolume = streamerModeBus?.volume_percent ?? 100;
  const streamMuted = streamerModeBus?.muted ?? false;
  const cardMuted = isMaster && streamerMode ? bothLanesMuted(muted, streamMuted) : muted;

  const applyVolume = (v: number) => void setBusVolume(bus.name, v);
  const toggleMute = () => void setBusMute(bus.name, !muted);
  const commitRename = () => {
    setEditing(false);
    const label = draft.trim();
    if (label && label !== bus.label) void renameBus(bus.name, label);
  };
  // What this mix actually carries (mode-aware).
  const allNames = channels.map((c) => c.name);
  const carried = busMembers(bus, allNames);

  const toggleMember = (channelName: string) => {
    const next = carried.includes(channelName)
      ? carried.filter((c) => c !== channelName)
      : [...carried, channelName];
    void setBusMembers(bus.name, next);
  };

  return (
    <div
      className={
        "strip bus-strip" +
        (isMaster ? " master-strip" : "") +
        (isMaster && streamerMode ? " streamer" : "") +
        (cardMuted ? " muted" : "")
      }
      style={{ ["--stagger" as string]: staggerIndex }}
    >
      {!isMaster && (
        <button
          type="button"
          className="strip-x"
          aria-label={t("mixer.mix.deleteNamed", { mix: bus.label })}
          title={t("mixer.mix.delete")}
          onClick={() => setConfirmingDelete(true)}
        >
          <Ms name="close" />
        </button>
      )}

      <div className="strip-head">
        <div className="strip-title-row">
          <div className="strip-icon strip-icon-bus">
            <Ms name={isMaster ? "speaker" : "radio_button_checked"} />
          </div>
          {isMaster ? (
            <div className="strip-name">{displayLabel}</div>
          ) : editing ? (
            <input
              className="menu-input strip-name-input"
              value={draft}
              autoFocus
              maxLength={24}
              onChange={(e) => setDraft(e.target.value)}
              onBlur={commitRename}
              onKeyDown={(e) => {
                if (e.key === "Enter") commitRename();
                if (e.key === "Escape") setEditing(false);
              }}
            />
          ) : (
            <div
              className="strip-name strip-name-editable"
              title={t("mixer.mix.renameHint")}
              onDoubleClick={() => {
                setDraft(bus.label);
                setEditing(true);
              }}
            >
              {displayLabel}
            </div>
          )}
        </div>
        {isMaster ? (
          <div className="strip-meta" title={t("mixer.mix.masterHint")}>
            {t("mixer.mix.allChannels")}
          </div>
        ) : (
          <div style={{ position: "relative" }}>
            <button
              type="button"
              className="strip-meta strip-meta-btn"
              title={t("mixer.mix.chooseChannels")}
              onClick={() => setManaging(true)}
            >
              {memberLabel()}
              <Ms name="expand_more" style={{ fontSize: 13 }} />
            </button>
            <Popover
              open={managing}
              onClose={() => setManaging(false)}
              side="bottom"
              align="center"
              style={{ minWidth: 220 }}
            >
              {channels.map((c) => (
                <MenuCheckItem
                  key={c.name}
                  checked={carried.includes(c.name)}
                  onClick={() => toggleMember(c.name)}
                >
                  <span className="menu-item-label">{c.label}</span>
                </MenuCheckItem>
              ))}
              <div className="menu-div" />
              <MenuCheckItem
                checked={bus.exclude}
                title={t("mixer.mix.autoIncludeHint")}
                onClick={() => void setBusExclude(bus.name, !bus.exclude)}
              >
                <span className="menu-item-label">{t("mixer.mix.autoInclude")}</span>
              </MenuCheckItem>
            </Popover>
          </div>
        )}
      </div>

      {!(isMaster && streamerMode) && (
        <div className="strip-preset-slot">
          {isMaster ? (
            <ProfileMenu compact onManageProfiles={onManageProfiles} />
          ) : (
            <div className="strip-preset-static">
              <Ms name="podcasts" />
              <span>{t("mixer.mix.routing")}</span>
            </div>
          )}
        </div>
      )}

      {isMaster && streamerMode ? (
        <StreamerLanes
          displayName={displayLabel}
          maxVolume={volumeCeiling}
          personalVuSource={bus.name}
          streamVuSource={STREAMER_MODE_BUS}
          personal={{
            value: volume,
            muted,
            onVolumeChange: applyVolume,
            onMuteToggle: toggleMute,
          }}
          stream={{
            value: streamVolume,
            muted: streamMuted,
            onVolumeChange: (v) => void setBusVolume(STREAMER_MODE_BUS, v),
            onMuteToggle: () => void setBusMute(STREAMER_MODE_BUS, !streamMuted),
          }}
          personalSecondaryAction={false}
          streamSecondaryAction={
            <button
              type="button"
              className={"sbtn" + (streamMonitoring ? " on-mon" : "")}
              onClick={() => void toggleMonitor(STREAMER_MODE_BUS)}
              aria-pressed={streamMonitoring}
              title={t("streamer.streamMonitor", { name: displayLabel })}
            >
              <Ms name="headphones" />
            </button>
          }
        />
      ) : (
        <>
          <div className="strip-readout">
            <div className="ro-value">
              <span className="ro-num">{volume}</span>
              <span className="ro-pct">%</span>
            </div>
            <div className="db">{volToDb(volume)}</div>
          </div>

          <div className="strip-body">
            <div className="channel-fader">
              <Fader
                value={volume}
                max={volumeCeiling}
                ariaLabel={isMaster ? t("mixer.mix.masterVolume") : t("mixer.mix.volume", { mix: bus.label })}
                onChange={applyVolume}
              />
              <VuMeter source={bus.name} enabled={!muted} />
            </div>
          </div>

          <div className="strip-btns">
            <button
              type="button"
              className={"sbtn" + (muted ? " on-mute" : "")}
              onClick={toggleMute}
              aria-pressed={muted}
              title={t(isMaster
                ? (muted ? "mixer.mix.masterUnmute" : "mixer.mix.masterMute")
                : (muted ? "mixer.mix.unmute" : "mixer.mix.mute"))}
            >
              <Ms name={muted ? "volume_off" : "volume_up"} />
            </button>
            {!isMaster && (
              <button
                type="button"
                className={"sbtn" + (monitoring ? " on-mon" : "")}
                onClick={() => void toggleMonitor(bus.name)}
                aria-pressed={monitoring}
                title={t("mixer.mix.monitor")}
              >
                <Ms name="headphones" />
              </button>
            )}
            <div className="sbtn-anchor">
              <button
                type="button"
                className={"sbtn" + (hasShortcuts ? " on-eq" : "")}
                onClick={() => setShortcutsOpen((open) => !open)}
                aria-pressed={shortcutsOpen}
                title={t("channel.shortcuts.button", { channel: displayLabel })}
              >
                <Ms name="keyboard" />
              </button>
              <Popover
                open={shortcutsOpen}
                onClose={() => setShortcutsOpen(false)}
                side="bottom"
                align="end"
                style={{ minWidth: 340 }}
              >
                <ChannelShortcutsPopover channelName={bus.name} channelLabel={displayLabel} />
              </Popover>
            </div>
          </div>
        </>
      )}

      {isMaster && <HardwareDevices />}

      {!isMaster && (
        <div
          className="strip-apps strip-apps-passive"
          aria-label={t("mixer.mix.carriedChannels", { mix: bus.label })}
        >
          <div className="strip-apps-label">{t("mixer.group.channels")}</div>
          {carried.map((name) => {
            const channel = channels.find((candidate) => candidate.name === name);
            return (
              <div className="strip-app-chip" key={name}>
                <span className="strip-app-icon"><Ms name="audio_file" /></span>
                <span className="strip-app-name">{channel?.label ?? name}</span>
              </div>
            );
          })}
        </div>
      )}

      {!isMaster && (
        <div
          className="strip-route"
          title={t("mixer.mix.sourceHint", { mix: bus.label })}
        />
      )}

      <ConfirmModal
        open={confirmingDelete}
        onClose={() => setConfirmingDelete(false)}
        title={t("mixer.mix.deleteTitle", { mix: bus.label })}
        confirmLabel={t("mixer.mix.delete")}
        onConfirm={() => void removeBus(bus.name)}
      >
        {t("mixer.mix.deleteBody", { mix: bus.label })}
      </ConfirmModal>
    </div>
  );
}

export const BusStrip = memo(BusStripBase);
