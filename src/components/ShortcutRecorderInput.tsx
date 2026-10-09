import { shortcutFromKeyboardEvent } from "../store/shortcuts";

interface ShortcutRecorderInputProps {
  value: string;
  recording: boolean;
  idleAriaLabel: string;
  recordingAriaLabel: string;
  recordingLabel: string;
  placeholder: string;
  title: string;
  onStartRecording: () => void;
  onStopRecording: () => void;
  onCapture: (shortcut: string) => void;
}

/** Click-then-press-keys recorder shared by the global shortcuts screen and
 * any per-channel shortcut popover; behavior must stay identical everywhere
 * it appears. */
export function ShortcutRecorderInput({
  value,
  recording,
  idleAriaLabel,
  recordingAriaLabel,
  recordingLabel,
  placeholder,
  title,
  onStartRecording,
  onStopRecording,
  onCapture,
}: Readonly<ShortcutRecorderInputProps>) {
  return (
    <input
      className={`shortcut-input${recording ? " recording" : ""}`}
      value={recording ? recordingLabel : value}
      placeholder={placeholder}
      spellCheck={false}
      readOnly
      title={title}
      aria-label={recording ? recordingAriaLabel : idleAriaLabel}
      onFocus={onStartRecording}
      onBlur={onStopRecording}
      onKeyDown={(event) => {
        event.preventDefault();
        if (!event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey && event.key === "Escape") {
          event.currentTarget.blur();
          return;
        }
        const shortcut = shortcutFromKeyboardEvent(event);
        if (shortcut === null) return;
        onCapture(shortcut);
        event.currentTarget.blur();
      }}
    />
  );
}
