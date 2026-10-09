import { memo, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { useMixerStore } from "../../store/mixer";
import { useStreamerModeStore } from "../../store/streamerMode";
import type { AppIdentity, VirtualSink } from "../../types";
import { defaultEqConfig, MAX_VOLUME, streamMeterKey } from "../../types";
import { useVolumeCeiling } from "../../store/volumeRange";
import { channelAccentClass, channelIcon, Ms, ICON_CHOICES } from "../Icons";
import { ConfirmModal } from "../ConfirmModal";
import { Popover } from "../Popover";
import { volToDb } from "../../lib/audio";
import { AppChip } from "./AppChip";
import { ChannelApps } from "./ChannelApps";
import { Fader } from "./Fader";
import { OutputSelect } from "./OutputSelect";
import { VuMeter } from "./VuMeter";
import { StreamerLanes, bothLanesMuted } from "./StreamerLanes";
import { EqPresetMenu } from "../Eq/EqPresetMenu";
import { useI18n } from "../../i18n";
import { applicationGroupKey, groupSeenApps } from "../../lib/appGroups";
import { laneHasBindings, useChannelShortcuts } from "../../store/channelShortcuts";
import { ChannelShortcutsPopover } from "./ChannelShortcutsPopover";
import type { DragAppPayload } from "./useAppCardDrag";

interface DraggableApp {
  key: string;
  name: string;
  iconPath: string | null;
  active: boolean;
  streamIndexes: number[];
  identities: AppIdentity[];
  desktopId: string | null;
  onChannel: boolean;
}

interface ChannelStripProps {
  channel: VirtualSink;
  appCount: number;
  /** Entrance-animation stagger index; cards are spaced 55 ms apart. */
  staggerIndex: number;
  /** Drag-reorder wiring (owned by MixerBoard). Takes the channel name
   *  rather than closing over it, so MixerBoard can pass the same stable
   *  callback to every strip instead of a fresh closure per channel per
   *  render - required for React.memo below to actually skip re-rendering
   *  strips other than the one whose volume just changed. */
  dragging: boolean;
  onGripDragStart: (e: React.DragEvent, name: string) => void;
  onGripDragEnd: () => void;
  onStripDragOver: (e: React.DragEvent, name: string) => void;
  /** Shared pointer-driven app-card drag (owned by MixerBoard). */
  beginDrag: (
    event: React.PointerEvent<HTMLElement>,
    payload: DragAppPayload,
    onTap: (payload: DragAppPayload) => void,
  ) => void;
  draggingPayload: DragAppPayload | null;
  dropTargetWell: string | null;
}

/** Memoized: with MixerBoard passing stable per-channel props (see
 *  ChannelStripProps above), this lets dragging one channel's fader
 *  re-render only that strip instead of every strip on the board. */
function ChannelStripBase({
  channel,
  appCount,
  staggerIndex,
  dragging,
  onGripDragStart,
  onGripDragEnd,
  onStripDragOver,
  beginDrag,
  draggingPayload,
  dropTargetWell,
}: Readonly<ChannelStripProps>) {
  const { t } = useI18n();
  const volumeCeiling = useVolumeCeiling(channel.name, MAX_VOLUME);
  const setChannelVolume = useMixerStore((s) => s.setChannelVolume);
  const toggleMute = useMixerStore((s) => s.toggleMute);
  const output = useMixerStore((s) => s.channelOutputs[channel.name] ?? null);
  const resolvedOutput = useMixerStore((s) => s.resolvedOutputs[channel.name] ?? null);
  const failover = useMixerStore((s) => s.channelFailover[channel.name] ?? true);
  const setChannelOutput = useMixerStore((s) => s.setChannelOutput);
  const setChannelFailover = useMixerStore((s) => s.setChannelFailover);
  const renameChannel = useMixerStore((s) => s.renameChannel);
  const removeChannel = useMixerStore((s) => s.removeChannel);
  const setChannelIcon = useMixerStore((s) => s.setChannelIcon);
  // Selecting just the count (and, below, a flattened primitive key per
  // other channel) instead of the raw `channels` array keeps this strip
  // from re-rendering every time ANY channel's volume changes - a plain
  // `useMixerStore((s) => s.channels)` here would return a new array
  // reference on every fader drag tick across the whole board.
  const channelCount = useMixerStore((s) => s.channels.length);
  const otherChannelKeys = useMixerStore(useShallow((s) => s.channels
    .filter((c) => c.name !== channel.name)
    .map((c) => `${c.name}\u0000${c.label}\u0000${channelIcon(c)}`)));
  const appStreams = useMixerStore((s) => s.appStreams);
  const seenApps = useMixerStore((s) => s.seenApps);
  const routeAppGroup = useMixerStore((s) => s.routeAppGroup);
  const setAppGroupAssignment = useMixerStore((s) => s.setAppGroupAssignment);
  const eqConfig = useMixerStore((s) => s.eqConfigs[channel.name] ?? null) ?? defaultEqConfig();
  const setChannelEq = useMixerStore((s) => s.setChannelEq);
  const setChannelStreamVolume = useMixerStore((s) => s.setChannelStreamVolume);
  const toggleChannelStreamMute = useMixerStore((s) => s.toggleChannelStreamMute);
  const accentClass = ` ${channelAccentClass(channel)}`;
  const streamerMode = useStreamerModeStore((s) => s.enabled);
  const cardMuted = streamerMode
    ? bothLanesMuted(channel.muted, channel.stream_send_muted)
    : channel.muted;

  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const [pickingIcon, setPickingIcon] = useState(false);
  const [managingApps, setManagingApps] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  // Streamer Mode's two lanes each get their own shortcuts trigger (kept in
  // the same below-the-slider spot every other track uses - see
  // StreamerLanes) and their own independent bindings (see
  // ChannelShortcutKind) - but only one popover can be open at a time.
  const [streamerShortcutsAnchor, setStreamerShortcutsAnchor] = useState<"personal" | "stream" | null>(null);
  const channelShortcutBindings = useChannelShortcuts((s) => s.getBindings(channel.name));
  const hasChannelShortcuts = laneHasBindings(channelShortcutBindings, "personal");

  const commitRename = () => {
    setEditing(false);
    const label = draft.trim();
    if (label && label !== channel.label) {
      void renameChannel(channel.name, label);
    }
  };

  const appsByKey = new Map<string, DraggableApp>();
  for (const app of appStreams) {
    const key = applicationGroupKey(app);
    const existing = appsByKey.get(key);
    if (existing) {
      existing.streamIndexes.push(app.index);
      existing.active ||= app.active;
      existing.desktopId ??= app.desktop_id;
      existing.onChannel ||= app.assigned_sink === channel.name;
      if (!existing.identities.some((identity) => (
        identity.match_prop === app.match_prop && identity.match_value === app.match_value
      ))) existing.identities.push({ match_prop: app.match_prop, match_value: app.match_value });
    } else {
      appsByKey.set(key, {
        key,
        name: app.alias ?? app.app_name,
        iconPath: app.icon_path,
        active: app.active,
        streamIndexes: [app.index],
        identities: [{ match_prop: app.match_prop, match_value: app.match_value }],
        desktopId: app.desktop_id,
        onChannel: app.assigned_sink === channel.name,
      });
    }
  }
  for (const history of groupSeenApps(seenApps)) {
    const existing = appsByKey.get(history.group_key);
    if (!existing) continue;
    for (const identity of history.identities) {
      if (!existing.identities.some((candidate) => (
        candidate.match_prop === identity.match_prop && candidate.match_value === identity.match_value
      ))) existing.identities.push(identity);
    }
    existing.name = history.alias ?? existing.name;
    existing.iconPath ??= history.icon_path;
  }
  const apps = Array.from(appsByKey.values()).filter((app) => app.onChannel);
  apps.sort((a, b) => Number(b.active) - Number(a.active) || a.name.localeCompare(b.name));

  const moveApp = (app: DraggableApp, destination: string) => {
    if (app.streamIndexes.length > 0) {
      void routeAppGroup(app.streamIndexes, app.identities, app.desktopId, destination);
    } else {
      void setAppGroupAssignment(app.identities, destination || null);
    }
  };

  const destinations = [
    ...otherChannelKeys.map((key) => {
      const [name, label, icon] = key.split("\u0000");
      return { name, label, icon };
    }),
    { name: "", label: t("common.systemDefault"), icon: "speaker_group" },
  ];

  const isDropTarget = dropTargetWell === channel.name;

  return (
    <div
      className={
        "strip channel-strip" +
        accentClass +
        (streamerMode ? " streamer" : "") +
        (cardMuted ? " muted" : "") +
        (dragging ? " dragging" : "")
      }
      style={{ ["--stagger" as string]: staggerIndex }}
      onDragOver={(e) => onStripDragOver(e, channel.name)}
    >
      {channelCount > 1 && (
        <span
          className="strip-grip"
          draggable
          title={t("mixer.dragReorder")}
          onDragStart={(e) => onGripDragStart(e, channel.name)}
          onDragEnd={onGripDragEnd}
        >
          <Ms name="drag_indicator" />
        </span>
      )}
      {channelCount > 1 && (
        <button
          type="button"
          className="strip-x"
          aria-label={t("mixer.channel.deleteNamed", { channel: channel.label })}
          title={t("mixer.channel.delete")}
          onClick={() => setConfirmingDelete(true)}
        >
          <Ms name="close" />
        </button>
      )}

      <div className="strip-head">
        <div className="strip-title-row">
          <div style={{ position: "relative" }}>
            <button
              type="button"
              className="strip-icon strip-icon-btn"
              title={t("mixer.channel.changeIcon")}
              aria-label={t("mixer.channel.changeIconNamed", { channel: channel.label })}
              onClick={() => setPickingIcon(true)}
            >
              <Ms name={channelIcon(channel)} />
            </button>
            <Popover
              open={pickingIcon}
              onClose={() => setPickingIcon(false)}
              side="bottom"
              align="center"
              style={{ minWidth: 196 }}
            >
              <div className="icon-grid">
                {ICON_CHOICES.map((icon) => (
                  <button
                    type="button"
                    key={icon}
                    className={"icon-cell" + (channelIcon(channel) === icon ? " sel" : "")}
                    onClick={() => {
                      setPickingIcon(false);
                      void setChannelIcon(channel.name, icon);
                    }}
                  >
                    <Ms name={icon} />
                  </button>
                ))}
              </div>
            </Popover>
          </div>
          {editing ? (
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
              title={t("mixer.renameHint")}
              onDoubleClick={() => {
                setDraft(channel.label);
                setEditing(true);
              }}
            >
              {channel.label}
            </div>
          )}
        </div>
      </div>

      {!streamerMode && (
        <div className="strip-preset-slot">
          <EqPresetMenu
            compact
            sinkName={channel.name}
            config={eqConfig}
            onApply={(config) => void setChannelEq(channel.name, config)}
            onError={(error) => useMixerStore.setState({ error })}
          />
        </div>
      )}

      {streamerMode ? (
        <StreamerLanes
          displayName={channel.label}
          maxVolume={volumeCeiling}
          personalVuSource={channel.name}
          streamVuSource={streamMeterKey(channel.name)}
          personal={{
            value: channel.volume_percent,
            muted: channel.muted,
            onVolumeChange: (v) => void setChannelVolume(channel.name, v),
            onMuteToggle: () => void toggleMute(channel.name, !channel.muted),
          }}
          stream={{
            value: channel.stream_send_volume_percent,
            muted: channel.stream_send_muted,
            onVolumeChange: (v) => void setChannelStreamVolume(channel.name, v),
            onMuteToggle: () => void toggleChannelStreamMute(channel.name, !channel.stream_send_muted),
          }}
          personalSecondaryAction={
            <div className="sbtn-anchor">
              <button
                type="button"
                className={"sbtn" + (laneHasBindings(channelShortcutBindings, "personal") ? " on-eq" : "")}
                onClick={() => setStreamerShortcutsAnchor((open) => (open === "personal" ? null : "personal"))}
                aria-pressed={streamerShortcutsAnchor === "personal"}
                title={t("channel.shortcuts.buttonPersonal", { channel: channel.label })}
              >
                <Ms name="keyboard" />
              </button>
              <Popover
                open={streamerShortcutsAnchor === "personal"}
                onClose={() => setStreamerShortcutsAnchor(null)}
                side="bottom"
                align="end"
                style={{ minWidth: 340 }}
              >
                <ChannelShortcutsPopover channelName={channel.name} channelLabel={channel.label} lane="personal" />
              </Popover>
            </div>
          }
          streamSecondaryAction={
            <div className="sbtn-anchor">
              <button
                type="button"
                className={"sbtn" + (laneHasBindings(channelShortcutBindings, "stream") ? " on-eq" : "")}
                onClick={() => setStreamerShortcutsAnchor((open) => (open === "stream" ? null : "stream"))}
                aria-pressed={streamerShortcutsAnchor === "stream"}
                title={t("channel.shortcuts.buttonStream", { channel: channel.label })}
              >
                <Ms name="keyboard" />
              </button>
              <Popover
                open={streamerShortcutsAnchor === "stream"}
                onClose={() => setStreamerShortcutsAnchor(null)}
                side="bottom"
                align="end"
                style={{ minWidth: 340 }}
              >
                <ChannelShortcutsPopover channelName={channel.name} channelLabel={channel.label} lane="stream" />
              </Popover>
            </div>
          }
        />
      ) : (
        <>
          <div className="strip-readout">
            <div className="ro-value">
              <span className="ro-num">{channel.volume_percent}</span>
              <span className="ro-pct">%</span>
            </div>
            <div className="db">{volToDb(channel.volume_percent)}</div>
          </div>

          <div className="strip-body">
            <div className="channel-fader">
              <Fader
                value={channel.volume_percent}
                max={volumeCeiling}
                ariaLabel={t("channel.volumeLabel", { channel: channel.label })}
                onChange={(v) => void setChannelVolume(channel.name, v)}
              />
              <VuMeter source={channel.name} enabled={!channel.muted} />
            </div>
          </div>

          <div className="strip-btns">
            <button
              type="button"
              className={"sbtn" + (channel.muted ? " on-mute" : "")}
              onClick={() => void toggleMute(channel.name, !channel.muted)}
              aria-pressed={channel.muted}
              title={t(channel.muted ? "mixer.unmute" : "channel.mute")}
            >
              <Ms name={channel.muted ? "volume_off" : "volume_up"} />
            </button>
            <div className="sbtn-anchor">
              <button
                type="button"
                className={"sbtn" + (hasChannelShortcuts ? " on-eq" : "")}
                onClick={() => setShortcutsOpen((open) => !open)}
                aria-pressed={shortcutsOpen}
                title={t("channel.shortcuts.button", { channel: channel.label })}
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
                <ChannelShortcutsPopover channelName={channel.name} channelLabel={channel.label} />
              </Popover>
            </div>
          </div>
        </>
      )}

      <div style={{ position: "relative", display: "flex", flexDirection: "column", flex: "1 1 auto", minHeight: 0 }}>
        <div className={"strip-apps" + (isDropTarget ? " drop-target" : "")} data-app-well={channel.name}>
          <button
            type="button"
            className="strip-apps-label"
            aria-label={t("mixer.channel.routedApps", { channel: channel.label })}
            title={t("channel.appsHint")}
            onClick={() => setManagingApps(true)}
          >
            {t("onboarding.flow.apps")}
            <span className="strip-apps-count">
              {t(appCount === 1 ? "channel.appsOne" : "channel.appsMany", { count: appCount })}
            </span>
          </button>
          {apps.length === 0 ? (
            <div className="strip-apps-empty">
              <Ms name="download" />
              <span>{t("mixer.channel.dropApps")}</span>
            </div>
          ) : (
            apps.map((app) => (
              <AppChip
                key={app.key}
                app={app}
                originChannel={channel.name}
                beginDrag={beginDrag}
                isDraggingSource={draggingPayload?.key === app.key}
                destinations={destinations}
                onMove={(destination) => moveApp(app, destination)}
              />
            ))
          )}
          {isDropTarget && <div className="drop-release-text">{t("mixer.channel.releaseToMove")}</div>}
        </div>
        <ChannelApps
          channel={channel}
          open={managingApps}
          onClose={() => setManagingApps(false)}
        />
      </div>

      {!streamerMode && (
        <OutputSelect
          compact
          value={output}
          resolved={resolvedOutput}
          failover={failover}
          onFailoverChange={(enabled) => void setChannelFailover(channel.name, enabled)}
          onChange={(o) => void setChannelOutput(channel.name, o)}
        />
      )}

      <ConfirmModal
        open={confirmingDelete}
        onClose={() => setConfirmingDelete(false)}
        title={t("mixer.channel.deleteTitle", { channel: channel.label })}
        confirmLabel={t("mixer.channel.delete")}
        onConfirm={() => void removeChannel(channel.name)}
      >
        {t("mixer.channel.deleteBody")}
      </ConfirmModal>
    </div>
  );
}

export const ChannelStrip = memo(ChannelStripBase);
