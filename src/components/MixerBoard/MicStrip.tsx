import { memo, useState } from "react";
import { useMixerStore } from "../../store/mixer";
import { useStreamerModeStore } from "../../store/streamerMode";
import { MAX_MIC_GAIN, streamMeterKey, streamMicNodeName, type MicConfig } from "../../types";
import { useVolumeCeiling } from "../../store/volumeRange";
import { Ms } from "../Icons";
import { Fader } from "./Fader";
import { VuMeter } from "./VuMeter";
import { StreamerLanes, bothLanesMuted } from "./StreamerLanes";
import { MicPresetMenu } from "../Mic/MicPresetMenu";
import { AppIcon } from "../AppList/AppIcon";
import { InputSelect } from "./InputSelect";
import { ConfirmModal } from "../ConfirmModal";
import { Popover } from "../Popover";
import { ChannelShortcutsPopover } from "./ChannelShortcutsPopover";
import { laneHasBindings, useChannelShortcuts } from "../../store/channelShortcuts";
import { useI18n } from "../../i18n";
import { useHoldAction } from "../../hooks/useHoldAction";

interface MicStripProps {
  config: MicConfig;
  staggerIndex: number;
  dragging: boolean;
  /** Takes the mic's node name rather than closing over it, so MixerBoard
   *  can pass the same stable callback to every mic strip - required for
   *  React.memo below to skip re-rendering strips other than the one
   *  whose gain just changed. */
  onGripDragStart: (event: React.DragEvent, nodeName: string) => void;
  onGripDragEnd: () => void;
  onStripDragOver: (event: React.DragEvent, nodeName: string) => void;
  onOpenSettings: (nodeName: string) => void;
}

/** Mic channel strip: fader = chain gain, meters = processed
 *  signal. Memoized alongside ChannelStrip so dragging one mic's gain
 *  doesn't re-render every mic strip on the board. */
function MicStripBase({
  config: micConfig,
  staggerIndex,
  dragging,
  onGripDragStart,
  onGripDragEnd,
  onStripDragOver,
  onOpenSettings,
}: Readonly<MicStripProps>) {
  const { t } = useI18n();
  const gainCeiling = useVolumeCeiling(micConfig.node_name, MAX_MIC_GAIN);
  const setMicChannelConfig = useMixerStore((s) => s.setMicChannelConfig);
  const setMicConfig = (patch: Partial<MicConfig>) => setMicChannelConfig(micConfig.node_name, patch);
  const monitoring = useMixerStore((s) => s.monitors[micConfig.node_name] ?? false);
  const toggleMonitor = useMixerStore((s) => s.toggleMonitor);
  const hold = useHoldAction(
    () => void setMicConfig({ muted: !micConfig.muted }),
    () => void toggleMonitor(micConfig.node_name),
  );
  const holdTitle = t("mixer.microphone.holdToListen");
  const streamMicNode = streamMicNodeName(micConfig.node_name);
  const streamMonitoring = useMixerStore((s) => s.monitors[streamMicNode] ?? false);
  const personalHold = {
    onMuteHold: () => void toggleMonitor(micConfig.node_name),
    holdActive: monitoring,
    holdIcon: "headphones",
    holdTitle,
  };
  const streamHold = {
    onMuteHold: () => void toggleMonitor(streamMicNode),
    holdActive: streamMonitoring,
    holdIcon: "headphones",
    holdTitle,
  };
  const removeMicChannel = useMixerStore((s) => s.removeMicChannel);
  const allMicClients = useMixerStore((s) => s.micClients);
  const micClients = allMicClients.filter((client) => client.mic_node === micConfig.node_name);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  // Streamer Mode's two lanes each get their own shortcuts trigger (kept in
  // the same below-the-slider spot every other track uses - see
  // StreamerLanes) and their own independent bindings (see
  // ChannelShortcutKind) - but only one popover can be open at a time.
  const [streamerShortcutsAnchor, setStreamerShortcutsAnchor] = useState<"personal" | "stream" | null>(null);
  const shortcutBindings = useChannelShortcuts((s) => s.getBindings(micConfig.node_name));
  const hasShortcuts = laneHasBindings(shortcutBindings, "personal");
  const secondary = micConfig.node_name !== "sink_mic";
  const streamerMode = useStreamerModeStore((s) => s.enabled);
  const cardMuted = streamerMode
    ? bothLanesMuted(micConfig.muted, micConfig.stream_send_muted)
    : micConfig.muted;

  const commitRename = () => {
    setEditing(false);
    const label = draft.trim();
    if (label && label !== micConfig.output_label)
      void setMicConfig({ output_label: label });
  };

  return (
      <div
        className={"strip input-strip strip-accent-mic" + (streamerMode ? " streamer" : "") + (cardMuted ? " muted" : "") + (!micConfig.enabled ? " mic-strip-disabled" : "") + (dragging ? " dragging" : "")}
        style={{ ["--stagger" as string]: staggerIndex }}
        onDragOver={(e) => onStripDragOver(e, micConfig.node_name)}
      >
      {secondary && (
        <span
          className="strip-grip"
          draggable
          title={t("mixer.dragReorder")}
          onDragStart={(e) => onGripDragStart(e, micConfig.node_name)}
          onDragEnd={onGripDragEnd}
        >
          <Ms name="drag_indicator" />
        </span>
      )}
      {secondary && (
        <button
          type="button"
          className="strip-x"
          aria-label={t("microphone.delete.action")}
          title={t("microphone.delete.action")}
          onClick={() => setConfirmingDelete(true)}
        >
          <Ms name="close" />
        </button>
      )}
      <div className="strip-head">
        <div className="strip-title-row">
          <div className="strip-icon strip-icon-mic">
            <Ms name={micConfig.enabled ? "mic" : "mic_off"} />
          </div>
          {editing ? (
            <input
              className="menu-input strip-name-input"
              value={draft}
              autoFocus
              maxLength={32}
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
              title={t("mixer.microphone.renameHint")}
              onDoubleClick={() => {
                if (!micConfig.enabled) return;
                setDraft(micConfig.output_label);
                setEditing(true);
              }}
            >
              {micConfig.output_label}
            </div>
          )}
        </div>
        <div className="strip-meta">{t(micConfig.enabled ? "mixer.microphone.capture" : "mixer.microphone.disabled")}</div>
      </div>

      {!streamerMode && (
        <div className="strip-preset-slot">
          <MicPresetMenu
            compact
            config={micConfig}
            onApply={(patch) => void setMicConfig(patch)}
          />
        </div>
      )}

      {streamerMode ? (
        <StreamerLanes
          displayName={micConfig.output_label}
          maxVolume={gainCeiling}
          personalVuSource={micConfig.node_name}
          streamVuSource={streamMeterKey(micConfig.node_name)}
          vuMono
          personal={{
            value: micConfig.gain_percent,
            muted: micConfig.muted,
            onVolumeChange: (v) => void setMicConfig({ gain_percent: v }),
            onMuteToggle: () => void setMicConfig({ muted: !micConfig.muted }),
            ...personalHold,
          }}
          stream={{
            value: micConfig.stream_send_gain_percent,
            muted: micConfig.stream_send_muted,
            onVolumeChange: (v) => void setMicConfig({ stream_send_gain_percent: v }),
            onMuteToggle: () => void setMicConfig({ stream_send_muted: !micConfig.stream_send_muted }),
            ...streamHold,
          }}
          personalSecondaryAction={
            <div className="sbtn-anchor">
              <button
                type="button"
                className={"sbtn" + (laneHasBindings(shortcutBindings, "personal") ? " on-eq" : "")}
                onClick={() => setStreamerShortcutsAnchor((open) => (open === "personal" ? null : "personal"))}
                aria-pressed={streamerShortcutsAnchor === "personal"}
                title={t("channel.shortcuts.buttonPersonal", { channel: micConfig.output_label })}
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
                <ChannelShortcutsPopover channelName={micConfig.node_name} channelLabel={micConfig.output_label} lane="personal" />
              </Popover>
            </div>
          }
          streamSecondaryAction={
            <div className="sbtn-anchor">
              <button
                type="button"
                className={"sbtn" + (laneHasBindings(shortcutBindings, "stream") ? " on-eq" : "")}
                onClick={() => setStreamerShortcutsAnchor((open) => (open === "stream" ? null : "stream"))}
                aria-pressed={streamerShortcutsAnchor === "stream"}
                title={t("channel.shortcuts.buttonStream", { channel: micConfig.output_label })}
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
                <ChannelShortcutsPopover channelName={micConfig.node_name} channelLabel={micConfig.output_label} lane="stream" />
              </Popover>
            </div>
          }
        />
      ) : (
        <>
          <div className="strip-readout">
            <div className="ro-value">
              <span className="ro-num">{micConfig.gain_percent}</span>
              <span className="ro-pct">%</span>
            </div>
            <div className="db">{t("microphone.gain")}</div>
          </div>

          <div className="strip-body">
            <div className="channel-fader">
              <Fader
                value={micConfig.gain_percent}
                max={gainCeiling}
                ariaLabel={t("microphone.gainLabel", { microphone: micConfig.output_label })}
                onChange={(v) => void setMicConfig({ gain_percent: v })}
              />
              <VuMeter
                source={micConfig.node_name}
                enabled={micConfig.enabled && !micConfig.muted}
                mono
              />
            </div>
          </div>

          <div className="strip-btns">
            <button
              type="button"
              className={"sbtn" + (monitoring ? " on-mon" : micConfig.muted ? " on-mute" : "")}
              {...hold}
              aria-pressed={micConfig.muted}
              title={`${t(micConfig.muted ? "mixer.microphone.unmute" : "mixer.microphone.mute")}\n${holdTitle}`}
            >
              <Ms name={monitoring ? "headphones" : micConfig.muted ? "mic_off" : "mic"} />
            </button>
            <div className="sbtn-anchor">
              <button
                type="button"
                className={"sbtn" + (hasShortcuts ? " on-eq" : "")}
                onClick={() => setShortcutsOpen((open) => !open)}
                aria-pressed={shortcutsOpen}
                title={t("channel.shortcuts.button", { channel: micConfig.output_label })}
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
                <ChannelShortcutsPopover channelName={micConfig.node_name} channelLabel={micConfig.output_label} />
              </Popover>
            </div>
          </div>
        </>
      )}

      {!micConfig.enabled ? (
        <button type="button" className="strip-apps mic-disabled-settings" onClick={() => onOpenSettings(micConfig.node_name)}>
          <Ms name="settings_voice" />
          <strong>{t("mixer.microphone.openSettings")}</strong>
          <small>{t("mixer.microphone.enableHint")}</small>
        </button>
      ) : (
        <div className="strip-apps strip-apps-passive" aria-label={t("mixer.microphone.clients")}>
          <div className="strip-apps-label">{t("onboarding.flow.apps")}</div>
          {micClients.length === 0 ? (
            <div className="strip-apps-empty">
              <Ms name="mic" />
              <span>{t("mixer.microphone.noClients")}</span>
            </div>
          ) : (
          micClients.map((client) => (
            <div
              className={"strip-app-chip" + (client.active ? " active" : "")}
              key={`${client.mic_node}\0${client.match_prop}\0${client.match_value}`}
              title={t("mixer.microphone.clientRecording", { application: client.app_name })}
            >
              <span className="strip-app-icon">
                <AppIcon iconPath={client.icon_path} />
              </span>
              <span className="strip-app-name">{client.app_name}</span>
              {client.active && <span className="strip-app-live" title={t("mixer.microphone.recording")} />}
            </div>
          ))) }
        </div>
      )}

      {!streamerMode && (
        <InputSelect
          value={micConfig.input_device}
          onChange={(inputDevice) => void setMicConfig({ input_device: inputDevice })}
        />
      )}
      <ConfirmModal
        open={confirmingDelete}
        onClose={() => setConfirmingDelete(false)}
        title={t("microphone.delete.title", { microphone: micConfig.output_label })}
        confirmLabel={t("microphone.delete.action")}
        onConfirm={() => void removeMicChannel(micConfig.node_name)}
      >
        {t("microphone.delete.body")}
      </ConfirmModal>
      </div>
  );
}

export const MicStrip = memo(MicStripBase);
