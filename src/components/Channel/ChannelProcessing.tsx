import { invoke } from "@tauri-apps/api/core";
import { useMixerStore } from "../../store/mixer";
import type { EqConfig, VirtualSink } from "../../types";
import { defaultEqConfig } from "../../types";
import { DspSlider } from "../Mic/DspSlider";
import { ProcessingHead } from "../ProcessingHead";
import { Ms } from "../Icons";
import { ProcessingInfo } from "../ProcessingInfo";
import { useI18n, type TranslationKey } from "../../i18n";

function ModeButton({
  mode,
  current,
  icon,
  title,
  onSelect,
}: Readonly<{
  mode: EqConfig["playback_mode"];
  current: EqConfig["playback_mode"];
  icon: string;
  title: string;
  onSelect: (mode: EqConfig["playback_mode"]) => void;
}>) {
  return (
    <button
      type="button"
      className={"channel-mode-button" + (mode === current ? " active" : "")}
      aria-pressed={mode === current}
      onClick={() => onSelect(mode)}
    >
      <Ms name={icon} />
      {title}
    </button>
  );
}

export function ChannelProcessing({ channel }: Readonly<{ channel: VirtualSink }>) {
  const { t } = useI18n();
  const config =
    useMixerStore((s) => s.eqConfigs[channel.name] ?? null) ?? defaultEqConfig();
  const setChannelEq = useMixerStore((s) => s.setChannelEq);
  const isVoice = channel.name === "sink_chat";
  const supportsSpatial = channel.name === "sink_game"
    || channel.name === "sink_media"
    || channel.name.startsWith("sink_spatial_");
  const apply = (patch: Partial<EqConfig>) =>
    void setChannelEq(channel.name, { ...config, ...patch });

  const modeSwitch = (
    <div className="channel-mode-switch" role="group" aria-label={t("processing.outputMode")}>
      <ModeButton
        mode="headphones"
        current={config.playback_mode}
        icon="headphones"
        title={t("processing.headphones")}
        onSelect={(playback_mode) => apply({ playback_mode })}
      />
      <ModeButton
        mode="speakers"
        current={config.playback_mode}
        icon="speaker"
        title={t("processing.speakers")}
        onSelect={(playback_mode) => apply({ playback_mode })}
      />
    </div>
  );
  const tuningLocked = !config.spatial_enabled;
  const tuning = Math.round(config.spatial_tuning * 100);
  const tuningWord = tuning <= 35
    ? t("processing.spatial.precision")
    : tuning >= 65 ? t("processing.spatial.immersion") : t("processing.spatial.balanced");

  return (
    <div className={"channel-processing-grid" + (supportsSpatial ? " has-spatial" : "") + (isVoice ? " voice-grid" : "")}>
      {supportsSpatial ? (
        <div className="card channel-processing-card spatial-processing-card">
          <ProcessingHead
            icon="surround_sound"
            tone="accent"
            title={t("processing.spatial.title")}
            subtitle={t("processing.spatial.subtitle")}
            actions={modeSwitch}
            on={config.spatial_enabled}
            onToggle={() => apply({ spatial_enabled: !config.spatial_enabled })}
          />

          <div className={"spatial-body" + (!config.spatial_enabled ? " inactive" : "")}>
            <div className="spatial-panel">
              <div className="spatial-front">{t("processing.spatial.front")}</div>
              <div className="spatial-stage" aria-label={t("processing.spatial.stageLabel")}>
                <div className="spatial-listener">
                  <Ms name={config.playback_mode === "headphones" ? "headphones" : "speaker"} />
                  <span>{t("processing.spatial.you")}</span>
                </div>
                {[
                  ["FL", "processing.spatial.frontLeft", "fl"],
                  ["FC", "processing.spatial.frontCentre", "fc"],
                  ["FR", "processing.spatial.frontRight", "fr"],
                  ["SL", "processing.spatial.sideLeft", "sl"],
                  ["SR", "processing.spatial.sideRight", "sr"],
                  ["RL", "processing.spatial.rearLeft", "rl"],
                  ["LFE", "processing.spatial.subwoofer", "lfe"],
                  ["RR", "processing.spatial.rearRight", "rr"],
                ].map(([id, title, position]) => (
                  <button
                    key={id}
                    type="button"
                    className={`spatial-speaker ${position}`}
                    title={t("processing.spatial.test", { speaker: t(title as TranslationKey) })}
                    aria-label={t("processing.spatial.test", { speaker: t(title as TranslationKey) })}
                    disabled={!config.spatial_enabled}
                    onClick={() => void invoke("test_spatial_channel", { sinkName: channel.name, channel: id })}
                  >
                    <Ms name={id === "LFE" ? "speaker" : "volume_up"} />
                    <span>{id}</span>
                  </button>
                ))}
              </div>
              <div className="spatial-hint"><i aria-hidden="true" />{t("processing.spatial.hint")}</div>
            </div>

            <div className={"spatial-side" + (tuningLocked ? " locked" : "")}>
              <div className="spatial-side-head">
                <div className="spatial-side-title">{t("processing.spatial.tuningHeading")}</div>
                <button
                  type="button"
                  className="text-action"
                  disabled={tuningLocked}
                  onClick={() => apply({ spatial_tuning: 0.5, spatial_distance: 0.5 })}
                >
                  <Ms name="restart_alt" />
                  {t("common.action.reset")}
                </button>
              </div>
              <DspSlider
                stacked
                label={t("processing.spatial.performance")}
                startLabel={t("processing.spatial.precision")}
                endLabel={t("processing.spatial.immersion")}
                pillText={tuningWord}
                min={0}
                max={100}
                step={1}
                value={tuning}
                defaultValue={50}
                unit=""
                disabled={tuningLocked}
                onChange={(value) => apply({ spatial_tuning: value / 100 })}
              />
              <DspSlider
                stacked
                label={t("processing.spatial.distance")}
                startLabel={t("processing.spatial.near")}
                endLabel={t("processing.spatial.far")}
                min={0}
                max={100}
                step={1}
                value={Math.round(config.spatial_distance * 100)}
                defaultValue={50}
                unit=""
                disabled={tuningLocked}
                onChange={(value) => apply({ spatial_distance: value / 100 })}
              />
              <div className="spatial-note">
                <ProcessingInfo label={t("processing.spatial.title")} text={t("processing.spatial.info")} />
                <span>{t(config.playback_mode === "headphones" ? "processing.spatial.noteHeadphones" : "processing.spatial.noteSpeakers")}</span>
              </div>
            </div>
          </div>
        </div>
      ) : (
        <div className="card channel-processing-card channel-output-card">
          <ProcessingHead
            icon="headphones"
            title={t("processing.outputMode")}
            subtitle={t("processing.outputMode.subtitle")}
            info={<ProcessingInfo label={t("processing.outputMode")} text={t("processing.outputInfo")} />}
            actions={modeSwitch}
          />
        </div>
      )}

      {isVoice ? (
        <>
          <div className="card channel-processing-card">
            <ProcessingHead
              icon="noise_control_off"
              title={t("processing.noiseGate")}
              subtitle={t("processing.noiseGate.subtitle")}
              info={<ProcessingInfo label={t("processing.noiseGate")} text={t("processing.noiseGate.info")} />}
              on={config.gate_enabled}
              onToggle={() => apply({ gate_enabled: !config.gate_enabled })}
            />
            <DspSlider
              label={t("processing.threshold")}
              min={-80}
              max={-10}
              step={1}
              unit=" dB"
              value={config.gate_threshold_db}
              defaultValue={-48}
              disabled={!config.gate_enabled}
              onChange={(gate_threshold_db) => apply({ gate_threshold_db })}
            />
          </div>

          <div className="card channel-processing-card">
            <ProcessingHead
              icon="compress"
              title={t("processing.compressor")}
              subtitle={t("processing.compressor.subtitle")}
              info={<ProcessingInfo label={t("processing.compressor")} text={t("processing.compressor.info")} />}
              on={config.comp_enabled}
              onToggle={() => apply({ comp_enabled: !config.comp_enabled })}
            />
            <DspSlider
              label={t("processing.threshold")}
              min={-60}
              max={0}
              step={1}
              unit=" dB"
              value={config.comp_threshold_db}
              defaultValue={-18}
              disabled={!config.comp_enabled}
              onChange={(comp_threshold_db) => apply({ comp_threshold_db })}
            />
            <DspSlider
              label={t("processing.strength")}
              min={1}
              max={10}
              step={0.5}
              unit=":1"
              value={config.comp_ratio}
              defaultValue={3}
              disabled={!config.comp_enabled}
              onChange={(comp_ratio) => apply({ comp_ratio })}
            />
            <DspSlider
              label={t("processing.volumeBoost")}
              min={-12}
              max={12}
              step={0.5}
              unit=" dB"
              value={config.boost_db}
              defaultValue={0}
              onChange={(boost_db) => apply({ boost_db })}
            />
          </div>
        </>
      ) : (
        <div className="card channel-processing-card">
          <ProcessingHead
            icon="volume_up"
            title={t("processing.smartVolume.title")}
            subtitle={t("processing.smartVolume.subtitle")}
            info={<ProcessingInfo label={t("processing.smartVolume.infoLabel")} text={t("processing.smartVolume.info")} />}
            on={config.comp_enabled}
            onToggle={() => apply({ comp_enabled: !config.comp_enabled })}
          />
          <DspSlider
            label={t("processing.level")}
            min={1}
            max={10}
            step={0.5}
            unit=""
            value={config.comp_ratio}
            defaultValue={3}
            disabled={!config.comp_enabled}
            onChange={(comp_ratio) => apply({ comp_ratio })}
          />
          <DspSlider
            label={t("processing.volumeBoost")}
            min={-12}
            max={12}
            step={0.5}
            unit=" dB"
            value={config.boost_db}
            defaultValue={0}
            onChange={(boost_db) => apply({ boost_db })}
          />
        </div>
      )}

      <div className="card channel-processing-card channel-limiter-card">
        <ProcessingHead
          icon="vertical_align_top"
          title={t("processing.limiter")}
          subtitle={t("processing.limiter.subtitle")}
          info={<ProcessingInfo label={t("processing.limiter")} text={t("processing.limiter.info")} />}
          on={config.limiter_enabled}
          onToggle={() => apply({ limiter_enabled: !config.limiter_enabled })}
        />
        <DspSlider
          label={t("processing.ceiling")}
          min={-12}
          max={0}
          step={0.5}
          unit=" dB"
          value={config.limiter_ceiling_db}
          defaultValue={-1}
          disabled={!config.limiter_enabled}
          onChange={(limiter_ceiling_db) => apply({ limiter_ceiling_db })}
        />
        <div className="processing-note">{t("processing.limiter.note")}</div>
      </div>
    </div>
  );
}
