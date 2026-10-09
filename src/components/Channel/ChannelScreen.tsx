import { useState } from "react";
import { useMixerStore } from "../../store/mixer";
import type { VirtualSink } from "../../types";
import { defaultEqConfig, MAX_VOLUME } from "../../types";
import { useVolumeCeiling } from "../../store/volumeRange";
import { VolumeRangeToggle } from "../VolumeRangeToggle";
import { volToDb } from "../../lib/audio";
import { HSlider } from "../AppList/HSlider";
import { EqEditor } from "../Eq/EqModal";
import { EqPresetMenu } from "../Eq/EqPresetMenu";
import { channelAccentClass, channelIcon, Ms } from "../Icons";
import { ChannelApps } from "../MixerBoard/ChannelApps";
import { OutputSelect } from "../MixerBoard/OutputSelect";
import { ChannelProcessing } from "./ChannelProcessing";
import { AudioTestControls } from "../AudioTestControls";
import { useI18n } from "../../i18n";
import { applicationGroupKey, groupSeenApps } from "../../lib/appGroups";

export function ChannelScreen({ channel }: Readonly<{ channel: VirtualSink }>) {
  const { t } = useI18n();
  const volumeCeiling = useVolumeCeiling(channel.name, MAX_VOLUME);
  const setChannelVolume = useMixerStore((state) => state.setChannelVolume);
  const toggleMute = useMixerStore((state) => state.toggleMute);
  const output = useMixerStore((state) => state.channelOutputs[channel.name] ?? null);
  const resolvedOutput = useMixerStore((state) => state.resolvedOutputs[channel.name] ?? null);
  const failover = useMixerStore((state) => state.channelFailover[channel.name] ?? true);
  const setChannelOutput = useMixerStore((state) => state.setChannelOutput);
  const setChannelFailover = useMixerStore((state) => state.setChannelFailover);
  const appStreams = useMixerStore((state) => state.appStreams);
  const seenApps = useMixerStore((state) => state.seenApps);
  const eqConfig =
    useMixerStore((state) => state.eqConfigs[channel.name] ?? null) ??
    defaultEqConfig();
  const setChannelEq = useMixerStore((state) => state.setChannelEq);
  const [managingApps, setManagingApps] = useState(false);
  const setError = (message: string) => useMixerStore.setState({ error: message });

  const assignedAppGroups = new Set(
    appStreams
      .filter((app) => app.assigned_sink === channel.name)
      .map(applicationGroupKey),
  );
  for (const app of groupSeenApps(seenApps)) {
    if (!app.ignored && app.assigned_sinks.includes(channel.name)) assignedAppGroups.add(app.group_key);
  }
  const appCount = assignedAppGroups.size;

  return (
    <div className={`content channel-page ${channelAccentClass(channel)}`}>
      <div className="screen-head channel-screen-head">
        <div className="channel-heading-icon">
          <Ms name={channelIcon(channel)} />
        </div>
        <div className="head-copy">
          <h1>{channel.label}</h1>
          <p className="head-sub">{t("channel.subtitle")}</p>
        </div>
        <div className="screen-head-actions">
          <button
            type="button"
            className={"select channel-head-control" + (channel.muted ? " channel-action-danger" : "")}
            onClick={() => void toggleMute(channel.name, !channel.muted)}
          >
            <Ms name={channel.muted ? "volume_off" : "volume_up"} />
            {t(channel.muted ? "channel.muted" : "channel.mute")}
          </button>
          <AudioTestControls kind="channel" sinkName={channel.name} />
        </div>
      </div>

      <div className="screen-scroll channel-scroll">
        <div className="section-label">{t("channel.section")}</div>
        <div className="card channel-controls-card">
          <div className="channel-control-block channel-preset-control">
            <div className="control-label">{t("channel.presets")}</div>
            <EqPresetMenu
              sinkName={channel.name}
              config={eqConfig}
              onApply={(config) => void setChannelEq(channel.name, config)}
              onError={setError}
            />
          </div>
          <div className="channel-control-block channel-volume-control">
            <div className="control-label-row">
              <div className="control-label">{t("channel.volume")}</div>
              <VolumeRangeToggle id={channel.name} compact />
            </div>
            <HSlider
              value={channel.volume_percent}
              max={volumeCeiling}
              ariaLabel={t("channel.volumeLabel", { channel: channel.label })}
              valueLabel={`${channel.volume_percent}% · ${volToDb(channel.volume_percent)}`}
              onChange={(value) => void setChannelVolume(channel.name, value)}
            />
          </div>
          <div className="channel-control-block">
            <div className="control-label">{t("channel.device")}</div>
            <OutputSelect
              value={output}
              resolved={resolvedOutput}
              failover={failover}
              onFailoverChange={(enabled) => void setChannelFailover(channel.name, enabled)}
              onChange={(selected) => void setChannelOutput(channel.name, selected)}
            />
          </div>
          <div className="channel-control-block channel-app-control">
            <div className="control-label">{t("channel.apps")}</div>
            <div style={{ position: "relative" }}>
              <button
                type="button"
                className="select channel-apps-trigger"
                title={t("channel.appsHint")}
                onClick={() => setManagingApps(true)}
              >
                <span>{t(appCount === 1 ? "channel.appsOne" : "channel.appsMany", { count: appCount })}</span>
                <Ms name="expand_more" />
              </button>
              <ChannelApps
                channel={channel}
                open={managingApps}
                onClose={() => setManagingApps(false)}
              />
            </div>
          </div>
        </div>

        <div className="card channel-eq-card">
          <EqEditor channel={channel} />
        </div>

        <div className="section-label">{t("channel.processing.section")}</div>
        <ChannelProcessing channel={channel} />
      </div>
    </div>
  );
}
