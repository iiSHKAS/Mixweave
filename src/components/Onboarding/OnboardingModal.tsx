import { useEffect, useRef, useState } from "react";
import { useMixerStore } from "../../store/mixer";
import { useI18n, type TranslationKey } from "../../i18n";
import { Ms, channelAccentClass, channelIcon } from "../Icons";
import { Modal } from "../Modal";
import { Fader } from "../MixerBoard/Fader";
import { DspSlider } from "../Mic/DspSlider";
import { Toggle } from "../Toggle";
import { volToDb } from "../../lib/audio";
import { UNITY_VOLUME } from "../../store/volumeRange";

type PreviewKind = "mixer" | "apps" | "profiles" | "microphone";

interface Step {
  icon: string;
  title: TranslationKey;
  body: TranslationKey;
  preview: PreviewKind;
}

/** Real mixer-board markup (.mix-group/.strip/.Fader), fed demo state local
 * to this modal - so the tour's first preview is a genuine, playable mixer
 * rather than a lookalike drawing, and dragging it costs nothing since
 * nothing here touches the real store. */
function MixerPreview() {
  const { t } = useI18n();
  const demo = [
    { name: "sink_game", label: t("onboarding.preview.game") },
    { name: "sink_chat", label: t("onboarding.preview.chat") },
    { name: "sink_media", label: t("onboarding.preview.media") },
  ];
  const [volumes, setVolumes] = useState<Record<string, number>>({ sink_game: 72, sink_chat: 48, sink_media: 61 });

  return (
    <div className="mix-group mix-group-channels ob-mixer-demo">
      <div className="group-head">
        <Ms name="grid_view" className="gh-icon" />
        <span className="gh-label">{t("mixer.group.channels")}</span>
      </div>
      <div className="group-strips">
        {demo.map((channel) => {
          const value = volumes[channel.name];
          return (
            <div className={"strip channel-strip " + channelAccentClass(channel)} key={channel.name}>
              <div className="strip-head">
                <div className="strip-title-row">
                  <span className="strip-icon"><Ms name={channelIcon(channel)} /></span>
                  <div className="strip-name">{channel.label}</div>
                </div>
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
                    max={UNITY_VOLUME}
                    ariaLabel={channel.label}
                    onChange={(v) => setVolumes((prev) => ({ ...prev, [channel.name]: v }))}
                  />
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** Real channel-strip app well (.strip-apps/.strip-app-chip), showing one
 * app parked on the wrong channel next to the one it belongs on. */
function AppsPreview() {
  const { t } = useI18n();
  return (
    <div className="ob-apps-demo">
      <div className={"strip channel-strip " + channelAccentClass({ name: "sink_chat" })}>
        <div className="strip-head">
          <div className="strip-title-row">
            <span className="strip-icon"><Ms name={channelIcon({ name: "sink_chat" })} /></span>
            <div className="strip-name">{t("onboarding.preview.chat")}</div>
          </div>
        </div>
        <div className="strip-apps">
          <div className="strip-apps-label">
            {t("onboarding.flow.apps")}
            <span className="strip-apps-count">{t("channel.appsMany", { count: 0 })}</span>
          </div>
          <div className="strip-apps-empty">
            <Ms name="download" />
            <span>{t("mixer.channel.dropApps")}</span>
          </div>
        </div>
      </div>

      <Ms name="arrow_forward" className="ob-apps-arrow" />

      <div className={"strip channel-strip " + channelAccentClass({ name: "sink_media" })}>
        <div className="strip-head">
          <div className="strip-title-row">
            <span className="strip-icon"><Ms name={channelIcon({ name: "sink_media" })} /></span>
            <div className="strip-name">{t("onboarding.preview.media")}</div>
          </div>
        </div>
        <div className="strip-apps">
          <div className="strip-apps-label">
            {t("onboarding.flow.apps")}
            <span className="strip-apps-count">{t("channel.appsOne", { count: 1 })}</span>
          </div>
          <div className="strip-app-chip">
            <span className="strip-app-icon"><Ms name="language" /></span>
            <span className="strip-app-name">{t("onboarding.preview.browser")}</span>
            <span className="strip-app-live" title={t("mixer.running")} />
            <Ms name="drag_indicator" className="strip-app-grip" />
          </div>
        </div>
      </div>

      <div className="ob-hint-line"><Ms name="check_circle" /> {t("onboarding.preview.routeRemembered")}</div>
    </div>
  );
}

/** Real profile-library markup (.profile-library-row/.profile-application-rules). */
function ProfilesPreview() {
  const { t } = useI18n();
  const profiles = [
    { name: t("onboarding.preview.gaming"), active: true },
    { name: t("onboarding.preview.everyday"), active: false },
    { name: t("onboarding.preview.streaming"), active: false },
  ];
  return (
    <div className="ob-profiles-demo">
      <div className="profile-automation-list">
        {profiles.map((profile) => (
          <div className={"profile-library-row" + (profile.active ? " selected" : "")} key={profile.name}>
            <span className="profile-library-select">
              <Ms name={profile.active ? "check" : "bookmark"} />
              <span><strong>{profile.name}</strong></span>
            </span>
          </div>
        ))}
      </div>
      <div className="card ob-profile-detail">
        <div className="trigger-hint">{t("profiles.applications.description", { profile: profiles[0].name })}</div>
        <div className="profile-application-rules">
          <div>
            <Ms name="deployed_code" />
            <span className="profile-rule-copy">
              {/* An application (not a channel or the profile): a well-known game, never translated. */}
              <strong>Counter-Strike 2</strong>
            </span>
            <span className="ob-profile-auto"><Ms name="bolt" />{t("onboarding.preview.autoSwitch")}</span>
          </div>
        </div>
        <div className="ob-hint-line"><Ms name="check" /> {t("onboarding.preview.profileSaved")}</div>
      </div>
    </div>
  );
}

/** Real mic-processing markup (.mic-processing-card/Toggle/DspSlider), with
 * a couple of local sliders the visitor can actually try. */
function MicrophonePreview() {
  const { t } = useI18n();
  const [gateOn, setGateOn] = useState(true);
  const [gateThreshold, setGateThreshold] = useState(-42);
  const [compOn, setCompOn] = useState(true);
  const [compThreshold, setCompThreshold] = useState(-18);

  return (
    <div className="ob-mic-demo">
      <div className="card mic-processing-card">
        <div className="processing-card-head">
          <div className="processing-toggle-title">
            <Toggle on={gateOn} onClick={() => setGateOn((v) => !v)} />
            <div className="rtitle">{t("processing.noiseGate")}</div>
          </div>
        </div>
        <DspSlider
          label={t("processing.threshold")}
          min={-80}
          max={-10}
          step={1}
          unit=" dB"
          value={gateThreshold}
          defaultValue={-42}
          disabled={!gateOn}
          onChange={setGateThreshold}
        />
      </div>
      <div className="card mic-processing-card">
        <div className="processing-card-head">
          <div className="processing-toggle-title">
            <Toggle on={compOn} onClick={() => setCompOn((v) => !v)} />
            <div className="rtitle">{t("processing.compressor")}</div>
          </div>
        </div>
        <DspSlider
          label={t("processing.threshold")}
          min={-60}
          max={0}
          step={1}
          unit=" dB"
          value={compThreshold}
          defaultValue={-18}
          disabled={!compOn}
          onChange={setCompThreshold}
        />
      </div>
      <div className="ob-hint-line"><Ms name="headphones" /> {t("onboarding.preview.readyInApps")}</div>
    </div>
  );
}

function StepPreview({ kind }: Readonly<{ kind: PreviewKind }>) {
  switch (kind) {
    case "mixer": return <MixerPreview />;
    case "apps": return <AppsPreview />;
    case "profiles": return <ProfilesPreview />;
    case "microphone": return <MicrophonePreview />;
  }
}

const STEPS: Step[] = [
  { icon: "graphic_eq", title: "onboarding.setup.title", body: "onboarding.setup.body", preview: "mixer" },
  { icon: "grid_view", title: "onboarding.apps.title", body: "onboarding.apps.body", preview: "apps" },
  { icon: "bookmarks", title: "onboarding.profiles.title", body: "onboarding.profiles.body", preview: "profiles" },
  { icon: "mic", title: "onboarding.microphone.title", body: "onboarding.microphone.body", preview: "microphone" },
];

function ObProgress({ step, onSelect }: Readonly<{ step: number; onSelect: (step: number) => void }>) {
  const { t } = useI18n();
  const total = STEPS.length + 1;
  return (
    <div className="ob-progress">
      <span>{t("onboarding.progress", { current: step + 1, total })}</span>
      <div className="ob-dots">
        {[...STEPS, null].map((item, index) => (
          <button
            type="button"
            key={item?.title ?? "choice"}
            className={index === step ? "on" : ""}
            aria-label={t("onboarding.progressGoTo", { current: index + 1, total })}
            aria-current={index === step ? "step" : undefined}
            onClick={() => onSelect(index)}
          />
        ))}
      </div>
    </div>
  );
}

/** First-run orientation: four live, playable previews built from the app's
 * own components, then a starting-setup choice. */
export function OnboardingModal() {
  const { t } = useI18n();
  const show = useMixerStore((state) => state.showOnboarding);
  const replay = useMixerStore((state) => state.onboardingReplay);
  const finishOnboarding = useMixerStore((state) => state.finishOnboarding);
  const [step, setStep] = useState(0);
  const stepHeading = useRef<HTMLHeadingElement>(null);

  useEffect(() => {
    if (show) setStep(0);
  }, [show]);

  useEffect(() => {
    if (show) stepHeading.current?.focus();
  }, [show, step]);

  const last = step === STEPS.length;
  const current = STEPS[step];

  return (
    <Modal
      open={show}
      onClose={() => void finishOnboarding(false)}
      title={t("onboarding.dialogLabel")}
      className="ob-modal"
      dismissible={replay}
    >
      {last ? (
        <div className="ob-final">
          <div className="ob-final-mark"><Ms name={replay ? "check" : "dashboard_customize"} /></div>
          <div className="ob-copy">
            <h2 ref={stepHeading} tabIndex={-1}>{t(replay ? "onboarding.replay.title" : "onboarding.choice.title")}</h2>
            <p>{t(replay ? "onboarding.replay.body" : "onboarding.choice.body")}</p>
          </div>
          {!replay && (
            <div className="ob-choices">
              <button type="button" className="ob-choice" onClick={() => void finishOnboarding(false)}>
                <Ms name="dashboard" />
                <span><strong>{t("onboarding.choice.ready.title")}</strong><small>{t("onboarding.choice.ready.body")}</small></span>
                <Ms name="arrow_forward" className="ob-direction-arrow" />
              </button>
              <button type="button" className="ob-choice" onClick={() => void finishOnboarding(true)}>
                <Ms name="add_box" />
                <span><strong>{t("onboarding.choice.custom.title")}</strong><small>{t("onboarding.choice.custom.body")}</small></span>
                <Ms name="arrow_forward" className="ob-direction-arrow" />
              </button>
            </div>
          )}
        </div>
      ) : (
        <>
          <div className="ob-copy">
            <span className="ob-step-icon"><Ms name={current.icon} /></span>
            <div>
              <h2 ref={stepHeading} tabIndex={-1}>{t(current.title)}</h2>
              <p>{t(current.body)}</p>
            </div>
          </div>
          <div className="ob-preview" key={current.preview}>
            <StepPreview kind={current.preview} />
          </div>
        </>
      )}
      <div className="ob-foot">
        <button
          type="button"
          className="modal-btn"
          onClick={() => step > 0 ? setStep(step - 1) : void finishOnboarding(false)}
        >
          {t(step > 0 ? "common.action.back" : "common.action.skip")}
        </button>
        <ObProgress step={step} onSelect={setStep} />
        {last && replay ? (
          <button type="button" className="modal-btn primary" onClick={() => void finishOnboarding(false)}>
            {t("common.action.done")}
          </button>
        ) : last ? <span className="ob-foot-spacer" /> : (
          <button type="button" className="modal-btn primary" onClick={() => setStep(step + 1)}>
            {t("common.action.next")}
          </button>
        )}
      </div>
    </Modal>
  );
}
