import type { ReactNode } from "react";
import { Ms } from "../Icons";
import { Fader } from "./Fader";
import { VuMeter } from "./VuMeter";
import { useI18n } from "../../i18n";
import { volToDb } from "../../lib/audio";
import { useHoldAction } from "../../hooks/useHoldAction";

/** One lane's volume/mute plus how to change them. Every card's two lanes
 *  are real backend-bound controls: Personal is the existing single-fader
 *  volume/mute, Stream is the independent send (see
 *  `commands::routing::push_channel_stream_controls` for channels/Master,
 *  `MicParams::stream_settings` for the mic). */
export interface LaneBinding {
  value: number;
  muted: boolean;
  onVolumeChange: (v: number) => void;
  onMuteToggle: () => void;
  /** Press-and-hold on the mute button; while `holdActive` its icon shows
   *  the held action (listening) instead of the mute state. */
  onMuteHold?: () => void;
  holdActive?: boolean;
  holdIcon?: string;
  holdTitle?: string;
}

interface StreamerLanesProps {
  displayName: string;
  maxVolume: number;
  /** Each lane meters its own independent signal - never the same source,
   *  or the Stream fader would look like it does nothing (see
   *  `streamMeterKey`). */
  personalVuSource: string;
  streamVuSource: string;
  vuMono?: boolean;
  personal: LaneBinding;
  stream: LaneBinding;
  /** Caller-supplied action beside mute: monitoring for Master or shortcuts
   *  for channels and microphones. */
  personalSecondaryAction?: ReactNode | false;
  streamSecondaryAction?: ReactNode | false;
}

/** Personal or Stream lane using the normal channel fader and meter markup. */
function Lane({
  kind,
  displayName,
  value,
  muted,
  maxVolume,
  vuSource,
  vuMono,
  onVolumeChange,
  onMuteToggle,
  onMuteHold,
  holdActive,
  holdIcon,
  holdTitle,
  secondaryAction,
}: Readonly<{
  kind: "personal" | "stream";
  displayName: string;
  value: number;
  muted: boolean;
  maxVolume: number;
  vuSource: string;
  vuMono?: boolean;
  onVolumeChange: (v: number) => void;
  onMuteToggle: () => void;
  onMuteHold?: () => void;
  holdActive?: boolean;
  holdIcon?: string;
  holdTitle?: string;
  secondaryAction?: ReactNode | false;
}>) {
  const { t } = useI18n();
  const hold = useHoldAction(onMuteToggle, () => onMuteHold?.());
  const label = t(kind === "personal" ? "streamer.personal" : "streamer.stream");
  const icon = kind === "personal" ? "headphones" : "podcasts";
  return (
    <div className={"lane" + (muted ? " muted" : "")}>
      <div className="lane-label">
        <Ms name={icon} />
        {label}
      </div>

      <div className="strip-readout">
        <div className="ro-value">
          <span className="ro-num">{value}</span>
          <span className="ro-pct">%</span>
        </div>
        <div className="db">{volToDb(value)}</div>
      </div>

      <div className="strip-body">
        <div className="channel-fader">
          <Fader
            value={value}
            max={maxVolume}
            ariaLabel={t(kind === "personal" ? "streamer.personalVolumeLabel" : "streamer.streamVolumeLabel", { name: displayName })}
            onChange={onVolumeChange}
          />
          <VuMeter source={vuSource} enabled={!muted} mono={vuMono} />
        </div>
      </div>

      <div className="strip-btns">
        <button
          type="button"
          className={"sbtn" + (holdActive ? " on-mon" : muted ? " on-mute" : "")}
          {...(onMuteHold ? hold : { onClick: onMuteToggle })}
          aria-pressed={muted}
          title={
            t(
              muted
                ? (kind === "personal" ? "streamer.personalUnmute" : "streamer.streamUnmute")
                : (kind === "personal" ? "streamer.personalMute" : "streamer.streamMute"),
              { name: displayName },
            ) + (holdTitle ? `\n${holdTitle}` : "")
          }
        >
          <Ms name={holdActive && holdIcon ? holdIcon : muted ? "volume_off" : "volume_up"} />
        </button>
        {secondaryAction ?? (
          <button
            type="button"
            className="sbtn"
            disabled
            title={t("streamer.shortcutsUnavailable")}
          >
            <Ms name="keyboard" />
          </button>
        )}
      </div>
    </div>
  );
}

/** Two full-size fader lanes with secondary actions aligned beside each mute button. */
export function StreamerLanes({
  displayName,
  maxVolume,
  personalVuSource,
  streamVuSource,
  vuMono,
  personal,
  stream,
  personalSecondaryAction,
  streamSecondaryAction,
}: Readonly<StreamerLanesProps>) {
  return (
    <div className="streamer-lanes">
      <Lane
        kind="personal"
        displayName={displayName}
        value={personal.value}
        muted={personal.muted}
        maxVolume={maxVolume}
        vuSource={personalVuSource}
        vuMono={vuMono}
        onVolumeChange={personal.onVolumeChange}
        onMuteToggle={personal.onMuteToggle}
        onMuteHold={personal.onMuteHold}
        holdActive={personal.holdActive}
        holdIcon={personal.holdIcon}
        holdTitle={personal.holdTitle}
        secondaryAction={personalSecondaryAction}
      />
      <Lane
        kind="stream"
        displayName={displayName}
        value={stream.value}
        muted={stream.muted}
        maxVolume={maxVolume}
        vuSource={streamVuSource}
        vuMono={vuMono}
        onVolumeChange={stream.onVolumeChange}
        onMuteToggle={stream.onMuteToggle}
        onMuteHold={stream.onMuteHold}
        holdActive={stream.holdActive}
        holdIcon={stream.holdIcon}
        holdTitle={stream.holdTitle}
        secondaryAction={streamSecondaryAction}
      />
    </div>
  );
}

/** Apply the shared outer-card mute style only when both lanes are muted. */
export function bothLanesMuted(personalMuted: boolean, streamMuted: boolean): boolean {
  return personalMuted && streamMuted;
}
