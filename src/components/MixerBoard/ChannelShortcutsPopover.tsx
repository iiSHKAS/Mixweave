import { useState } from "react";
import {
  DELEGATED_TO_GLOBAL,
  useChannelShortcuts,
  type ChannelShortcutKind,
} from "../../store/channelShortcuts";
import { useShortcutSettings, type ShortcutAction } from "../../store/shortcuts";
import { useI18n, type TranslationKey } from "../../i18n";
import { ShortcutRecorderInput } from "../ShortcutRecorderInput";
import { IconButton } from "../IconButton";
import { ToggleRow } from "../Toggle";
import { Ms } from "../Icons";
import { VolumeRangeToggle } from "../VolumeRangeToggle";

const PERSONAL_ROWS: { kind: ChannelShortcutKind; label: TranslationKey; icon: string }[] = [
  { kind: "mute", label: "channel.shortcuts.mute", icon: "volume_off" },
  { kind: "volume_up", label: "channel.shortcuts.volumeUp", icon: "volume_up" },
  { kind: "volume_down", label: "channel.shortcuts.volumeDown", icon: "volume_down" },
];

/** Same three actions, bound independently for the Stream lane - see
 * `ChannelShortcutKind`'s doc comment. Reuses the Personal rows' labels
 * (the popover's own title already says which lane it is). */
const STREAM_ROWS: { kind: ChannelShortcutKind; label: TranslationKey; icon: string }[] = [
  { kind: "stream_mute", label: "channel.shortcuts.mute", icon: "volume_off" },
  { kind: "stream_volume_up", label: "channel.shortcuts.volumeUp", icon: "volume_up" },
  { kind: "stream_volume_down", label: "channel.shortcuts.volumeDown", icon: "volume_down" },
];

/** The other half of DELEGATED_TO_GLOBAL: which global action a delegated
 * (channel, kind) pair actually edits. Game and Chat's mute is the older
 * global toggle_game/toggle_chat action, not a distinct one - this row
 * edits that global binding directly, one stored value shown and changed
 * the same way from either screen, instead of a second copy that can drift
 * out of sync and confuse the user about which one is actually live. */
const GLOBAL_ACTION_FOR: Partial<Record<string, ShortcutAction>> = {
  sink_game: "toggle_game",
  sink_chat: "toggle_chat",
};

/** Per-channel mute/volume-up/volume-down recorder, opened from the small
 * keyboard icon on each mixer strip. Shares the global "Enable shortcuts"
 * switch in Settings; this popover only manages the key bindings.
 * `lane` picks Streamer Mode's independent Personal/Stream bindings, and
 * disambiguates the title between the two triggers sitting right next to
 * each other there. Left out entirely (not just defaulted) by the normal
 * single-fader strip, which only ever has one set to bind and so keeps the
 * plain, undisambiguated title. */
export function ChannelShortcutsPopover({
  channelName,
  channelLabel,
  lane,
}: Readonly<{ channelName: string; channelLabel: string; lane?: "personal" | "stream" }>) {
  const { t } = useI18n();
  const rows = lane === "stream" ? STREAM_ROWS : PERSONAL_ROWS;
  const bindings = useChannelShortcuts((state) => state.getBindings(channelName));
  const setBinding = useChannelShortcuts((state) => state.setBinding);
  const globalBindings = useShortcutSettings((state) => state.bindings);
  const setGlobalBindings = useShortcutSettings((state) => state.setBindings);
  const enabled = useShortcutSettings((state) => state.enabled);
  const setEnabled = useShortcutSettings((state) => state.setEnabled);
  const [recording, setRecording] = useState<ChannelShortcutKind | null>(null);

  const globalAction = (kind: ChannelShortcutKind) =>
    DELEGATED_TO_GLOBAL[channelName] === kind ? GLOBAL_ACTION_FOR[channelName] : undefined;

  const displayValue = (kind: ChannelShortcutKind) => {
    const action = globalAction(kind);
    return action ? globalBindings[action] : bindings[kind];
  };

  const captureValue = (kind: ChannelShortcutKind, shortcut: string) => {
    const action = globalAction(kind);
    if (action) setGlobalBindings({ ...globalBindings, [action]: shortcut });
    else setBinding(channelName, kind, shortcut);
  };

  return (
    <div className="channel-shortcuts-popover">
      <div className="trigger-hint">
        {t(
          lane === "stream"
            ? "channel.shortcuts.titleStream"
            : lane === "personal"
              ? "channel.shortcuts.titlePersonal"
              : "channel.shortcuts.title",
          { channel: channelLabel },
        )}
      </div>

      {!enabled && (
        <>
          <ToggleRow
            icon="keyboard"
            title={t("settings.shortcuts.enable.title")}
            sub={t("channel.shortcuts.disabledHint")}
            on={enabled}
            onToggle={() => setEnabled(!enabled)}
          />
          <div className="menu-sep" />
        </>
      )}

      {rows.map(({ kind, label, icon }) => {
        const value = displayValue(kind);
        return (
          <label className="row shortcut-row" key={kind}>
            <div className="ricon">
              <Ms name={icon} />
            </div>
            <div className="rmain">
              <div className="rtitle">{t(label)}</div>
            </div>
            <ShortcutRecorderInput
              value={value}
              recording={recording === kind}
              idleAriaLabel={t("settings.shortcuts.label", { label: t(label) })}
              recordingAriaLabel={t("settings.shortcuts.recordingLabel", { label: t(label) })}
              recordingLabel={t("settings.shortcuts.recording")}
              placeholder={t("settings.shortcuts.placeholder")}
              title={t("settings.shortcuts.inputHint")}
              onStartRecording={() => setRecording(kind)}
              onStopRecording={() => setRecording((current) => (current === kind ? null : current))}
              onCapture={(shortcut) => captureValue(kind, shortcut)}
            />
            {value && (
              <IconButton
                icon="backspace"
                title={t("channel.shortcuts.clear", { label: t(label) })}
                onClick={() => captureValue(kind, "")}
              />
            )}
          </label>
        );
      })}

      <div className="menu-sep" />
      <VolumeRangeToggle id={channelName} />
    </div>
  );
}
