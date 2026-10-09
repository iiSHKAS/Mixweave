import { useCallback, useState } from "react";
import type { ReactNode } from "react";
import { useMixerStore } from "../../store/mixer";
import { MASTER_BUS, STREAMER_MODE_BUS } from "../../types";
import { Ms, ICON_CHOICES } from "../Icons";
import { Modal } from "../Modal";
import { ChannelStrip } from "./ChannelStrip";
import { MicStrip } from "./MicStrip";
import { BusStrip } from "./StreamMixStrip";
import { useAppCardDrag } from "./useAppCardDrag";
import type { DragAppPayload } from "./useAppCardDrag";
import { useI18n } from "../../i18n";
import { applicationGroupKey } from "../../lib/appGroups";

// UI-side gate only; the backend enforces the real limit.
const MAX_CHANNELS = 10;

// Keep entrance state across unmounts so navigation does not replay the animation.
let mixerEntranceShown = false;

/** Signal-flow group: header row (icon, label, count, optional +) above
 * its strips - per the updated design. */
function MixGroup({
  icon,
  label,
  count,
  hint,
  onAdd,
  addTitle,
  kind,
  children,
}: Readonly<{
  icon: string;
  label: string;
  /** Omitted for Master/Mic, which never show a channel count. */
  count?: string;
  /** Hover explanation of what this group does. */
  hint: string;
  onAdd?: () => void;
  addTitle?: string;
  /** Which signal-flow group this is, for the Daylight theme's tinted panel. */
  kind: "master" | "channels" | "mic" | "mixes";
  children: ReactNode;
}>) {
  return (
    <div className={`mix-group mix-group-${kind}`}>
      <div className="group-head" title={hint}>
        <Ms name={icon} className="gh-icon" />
        <span className="gh-label">{label}</span>
        {count && <span className="gh-count">{count}</span>}
        {onAdd && (
          <div className="gh-add-wrap">
            <button type="button" className="gh-add" onClick={onAdd} title={addTitle}>
              <Ms name="add" />
            </button>
          </div>
        )}
      </div>
      <div
        className="group-strips"
        onWheel={(event) => {
          if (event.currentTarget.scrollWidth <= event.currentTarget.clientWidth || event.deltaY === 0) return;
          event.currentTarget.scrollLeft += event.deltaY;
          event.preventDefault();
        }}
      >
        {children}
      </div>
    </div>
  );
}

export function MixerBoard({
  onOpenProfiles,
  onOpenMic,
}: Readonly<{
  onOpenProfiles: () => void;
  onOpenMic: () => void;
}>) {
  const { t } = useI18n();
  const channels = useMixerStore((s) => s.channels);
  const buses = useMixerStore((s) => s.buses);
  const appStreams = useMixerStore((s) => s.appStreams);
  const addChannel = useMixerStore((s) => s.addChannel);
  const micConfig = useMixerStore((s) => s.micConfig);
  const micConfigs = useMixerStore((s) => s.micConfigs);
  const multipleMics = useMixerStore((s) => s.multipleMics);
  const selectMic = useMixerStore((s) => s.selectMic);
  const moveMicChannel = useMixerStore((s) => s.moveMicChannel);
  const commitMicChannelOrder = useMixerStore((s) => s.commitMicChannelOrder);
  const backendNative = useMixerStore((s) => s.backendNative);

  const moveChannel = useMixerStore((s) => s.moveChannel);
  const commitChannelOrder = useMixerStore((s) => s.commitChannelOrder);
  const routeAppGroup = useMixerStore((s) => s.routeAppGroup);
  const setAppGroupAssignment = useMixerStore((s) => s.setAppGroupAssignment);

  const { beginDrag, ghostNode, dropTarget, draggingPayload } = useAppCardDrag(
    (payload: DragAppPayload, destination: string) => {
      if (payload.streamIndexes.length > 0) {
        void routeAppGroup(payload.streamIndexes, payload.identities, payload.desktopId, destination);
      } else {
        void setAppGroupAssignment(payload.identities, destination || null);
      }
    },
  );
  const dropTargetWell = dropTarget ? (dropTarget.dataset.appWell ?? null) : null;

  const [addingChannel, setAddingChannel] = useState(false);
  const [channelLabel, setChannelLabel] = useState("");
  const [channelIcon, setChannelIcon] = useState(ICON_CHOICES[0]);
  const [channelSpatial, setChannelSpatial] = useState(false);
  const [draggingChannel, setDraggingChannel] = useState<string | null>(null);
  const [draggingMic, setDraggingMic] = useState<string | null>(null);

  // Stable (name-parameterized) drag-reorder callbacks, shared by every
  // strip instead of a fresh closure per channel/mic per render - paired
  // with React.memo on ChannelStrip/MicStrip/BusStrip, this keeps a fader
  // drag on one strip from re-rendering every other strip on the board.
  const handleChannelGripDragStart = useCallback((e: React.DragEvent, name: string) => {
    setDraggingChannel(name);
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", name);
  }, []);
  const handleChannelGripDragEnd = useCallback(() => {
    setDraggingChannel(null);
    void commitChannelOrder();
  }, [commitChannelOrder]);
  const handleChannelStripDragOver = useCallback((e: React.DragEvent, name: string) => {
    if (draggingChannel && draggingChannel !== name) {
      e.preventDefault();
      moveChannel(draggingChannel, name);
    }
  }, [draggingChannel, moveChannel]);
  const handleMicGripDragStart = useCallback((e: React.DragEvent, nodeName: string) => {
    setDraggingMic(nodeName);
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", nodeName);
  }, []);
  const handleMicGripDragEnd = useCallback(() => {
    setDraggingMic(null);
    void commitMicChannelOrder();
  }, [commitMicChannelOrder]);
  const handleMicStripDragOver = useCallback((e: React.DragEvent, nodeName: string) => {
    if (draggingMic && draggingMic !== nodeName) {
      e.preventDefault();
      moveMicChannel(draggingMic, nodeName);
    }
  }, [draggingMic, moveMicChannel]);
  const handleMicOpenSettings = useCallback((nodeName: string) => {
    selectMic(nodeName);
    onOpenMic();
  }, [selectMic, onOpenMic]);

  const [skipEntrance] = useState(() => {
    const alreadyShown = mixerEntranceShown;
    mixerEntranceShown = true;
    return alreadyShown;
  });

  if (channels.length === 0) {
    return (
      <div className="content">
        <div className="empty-hint" style={{ margin: "auto" }}>
          {t("mixer.loading")}
        </div>
      </div>
    );
  }

  // The strip is a live mixer view: its count includes only identities that
  // currently own a PipeWire stream. Remembered/offline assignments remain
  // available in the channel membership popover and Applications screen.
  const identitiesByChannel = new Map<string, Set<string>>();
  for (const stream of appStreams) {
    const key = applicationGroupKey(stream);
    if (stream.assigned_sink) {
      const identities = identitiesByChannel.get(stream.assigned_sink) ?? new Set<string>();
      identities.add(key);
      identitiesByChannel.set(stream.assigned_sink, identities);
    }
  }
  const counts = new Map(
    Array.from(identitiesByChannel, ([channel, identities]) => [channel, identities.size]),
  );

  const closeChannelModal = () => {
    setAddingChannel(false);
    setChannelLabel("");
    setChannelIcon(ICON_CHOICES[0]);
    setChannelSpatial(false);
  };
  const createChannel = () => {
    const label = channelLabel.trim();
    if (!label) return;
    void addChannel(label, channelIcon, channelSpatial);
    closeChannelModal();
  };
  const masterBus = buses.find((bus) => bus.name === MASTER_BUS);
  // Streamer Mode is a real mix on the backend (so it persists, and OBS can
  // capture it), but it's not a user-manageable mix - it never appears in
  // the regular Mixes list, exactly like the master mix doesn't.
  const extraBuses = buses.filter(
    (bus) => bus.name !== MASTER_BUS && bus.name !== STREAMER_MODE_BUS,
  );
  // Continue the 55 ms stagger across groups for a single board-wide entrance.
  let stagger = -1;

  return (
    <div className="content">
      <div className="screen-scroll" style={{ padding: 0 }}>
        <div className="mix-scroll">
          <div className={"mix-canvas" + (skipEntrance ? " no-entrance-anim" : "")}>
          {backendNative !== false && masterBus && (
            <>
              <MixGroup
                icon="instant_mix"
                label={t("mixer.group.master")}
                hint={t("mixer.group.masterHint")}
                kind="master"
              >
                <BusStrip
                  bus={masterBus}
                  staggerIndex={++stagger}
                  onManageProfiles={onOpenProfiles}
                />
              </MixGroup>
              <div className="group-div" />
            </>
          )}

          <MixGroup
            icon="apps"
            label={t("mixer.group.channels")}
            count={`${channels.length}`}
            hint={t("mixer.group.channelsHint")}
            onAdd={channels.length < MAX_CHANNELS ? () => setAddingChannel(true) : undefined}
            addTitle={t("mixer.group.addChannel")}
            kind="channels"
          >
            {channels.map((channel) => (
              <ChannelStrip
                key={channel.name}
                channel={channel}
                appCount={counts.get(channel.name) ?? 0}
                staggerIndex={++stagger}
                dragging={draggingChannel === channel.name}
                onGripDragStart={handleChannelGripDragStart}
                onGripDragEnd={handleChannelGripDragEnd}
                onStripDragOver={handleChannelStripDragOver}
                beginDrag={beginDrag}
                draggingPayload={draggingPayload}
                dropTargetWell={dropTargetWell}
              />
            ))}
          </MixGroup>

          {micConfig && (
            <>
              <div className="group-div" />
              <MixGroup
                icon="mic"
                label={t("mixer.group.microphone")}
                hint={micConfig.enabled
                  ? t("mixer.group.microphoneHint")
                  : t("mixer.group.microphoneDisabledHint")}
                kind="mic"
              >
                {(multipleMics ? micConfigs : [micConfig]).map((mic) => (
                  <MicStrip
                    key={mic.node_name}
                    config={mic}
                    staggerIndex={++stagger}
                    dragging={draggingMic === mic.node_name}
                    onGripDragStart={handleMicGripDragStart}
                    onGripDragEnd={handleMicGripDragEnd}
                    onStripDragOver={handleMicStripDragOver}
                    onOpenSettings={handleMicOpenSettings}
                  />
                ))}
              </MixGroup>
            </>
          )}

          {/* Mixes need the native backend; hide them on the pactl
           * fallback instead of showing strips that can't work. */}
          {backendNative !== false && extraBuses.length > 0 && (
          <>
          <div className="group-div" />
          <MixGroup
            icon="podcasts"
            label={t("mixer.group.mixes")}
            count={`${extraBuses.length}`}
            hint={t("mixer.group.mixesHint")}
            kind="mixes"
          >
            {extraBuses.map((bus) => (
              <BusStrip
                key={bus.name}
                bus={bus}
                staggerIndex={++stagger}
                onManageProfiles={onOpenProfiles}
              />
            ))}
          </MixGroup>
          </>
          )}
          </div>
        </div>
      </div>
      {ghostNode}

      <Modal open={addingChannel} onClose={closeChannelModal} title={t("mixer.channel.create.title")}>
        <input
          className="menu-input"
          placeholder={t("mixer.channel.create.placeholder")}
          value={channelLabel}
          autoFocus
          maxLength={24}
          onChange={(e) => setChannelLabel(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") createChannel();
          }}
        />
        <div className="modal-label">{t("mixer.channel.create.icon")}</div>
        <div className="icon-grid">
          {ICON_CHOICES.map((choice) => (
            <button
              type="button"
              key={choice}
              className={"icon-cell" + (choice === channelIcon ? " sel" : "")}
              onClick={() => setChannelIcon(choice)}
              aria-label={choice}
            >
              <Ms name={choice} />
            </button>
          ))}
        </div>
        <div className="modal-label">{t("mixer.channel.create.format")}</div>
        <div className="channel-format-choices">
          <button
            type="button"
            className={"channel-format-choice" + (!channelSpatial ? " selected" : "")}
            onClick={() => setChannelSpatial(false)}
          >
            <Ms name="speaker_group" />
            <span><strong>{t("mixer.channel.create.stereo")}</strong><small>{t("mixer.channel.create.stereoHint")}</small></span>
          </button>
          <button
            type="button"
            className={"channel-format-choice" + (channelSpatial ? " selected" : "")}
            onClick={() => setChannelSpatial(true)}
          >
            <Ms name="spatial_audio" />
            <span><strong>{t("mixer.channel.create.spatial")}</strong><small>{t("mixer.channel.create.spatialHint")}</small></span>
          </button>
        </div>
        <div className="modal-btns">
          <button type="button" className="modal-btn primary" onClick={createChannel} disabled={!channelLabel.trim()}>
            {t("mixer.channel.create.action")}
          </button>
          <button type="button" className="modal-btn" onClick={closeChannelModal}>
            {t("common.action.cancel")}
          </button>
        </div>
      </Modal>
    </div>
  );
}
