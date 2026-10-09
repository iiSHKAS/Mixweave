//! Global shortcuts on Wayland cannot be grabbed by an application directly
//! (the compositor mediates all global input for security reasons). On GNOME,
//! `tauri-plugin-global-shortcut` falls back to an X11/XWayland grab that only
//! fires while our own window has focus. GNOME's long-standing
//! "custom keybinding" mechanism (Settings > Keyboard > Custom Shortcuts) runs
//! an arbitrary command on a system-wide key press regardless of focus, and
//! has existed since GNOME 3 with no portal/version gate. We drive that
//! mechanism through `gsettings` and let GNOME re-invoke our own binary with
//! `--shortcut <action>`; `tauri-plugin-single-instance` hands that back to
//! the already-running instance instead of starting a second one.

use std::collections::HashMap;
use std::process::Command;

const MEDIA_KEYS_SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys";
const CUSTOM_KEYBINDING_SCHEMA: &str =
    "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding";
const KEYBINDINGS_KEY: &str = "custom-keybindings";
const PATH_PREFIX: &str = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/sonux-";
const MEDIA_KEYS_SERVICE: &str = "org.gnome.SettingsDaemon.MediaKeys.service";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutBackend {
    /// X11 (or XWayland-hosted session): the existing plugin grab already
    /// works system-wide, nothing to change.
    X11,
    /// Wayland on GNOME: drive shortcuts through `gsettings` custom
    /// keybindings instead of the plugin grab.
    GnomeWayland,
    /// Wayland on a desktop we have no native fallback for yet: keep the
    /// existing focus-only behavior rather than fail loudly.
    Unsupported,
}

pub(crate) fn is_wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
}

pub(crate) fn is_gnome() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .map(|value| value.to_ascii_lowercase().contains("gnome"))
        .unwrap_or(false)
}

fn gsettings_available() -> bool {
    Command::new("gsettings")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[tauri::command]
pub fn detect_shortcut_backend() -> ShortcutBackend {
    if !is_wayland() {
        ShortcutBackend::X11
    } else if is_gnome() && gsettings_available() {
        ShortcutBackend::GnomeWayland
    } else {
        ShortcutBackend::Unsupported
    }
}

/// Relays a shortcut-driven mute/volume change to the GNOME Shell extension
/// (see gnome_osd_extension.rs) that draws the popup on Wayland/GNOME. The
/// frontend calls this best-effort alongside its "mixweave://shortcut" event
/// path; a failure here (extension not enabled yet, D-Bus unavailable) is
/// silently swallowed by the caller rather than surfaced as an app error,
/// since the mute/volume change itself already succeeded regardless.
#[tauri::command]
pub fn show_gnome_osd(
    label: String,
    volume_percent: i32,
    max: i32,
    muted: bool,
    theme: Option<String>,
    style: Option<String>,
    position: Option<String>,
) -> Result<(), String> {
    crate::gnome_osd_extension::show(
        &label,
        volume_percent,
        max,
        muted,
        theme.as_deref().unwrap_or("dark"),
        style.as_deref().unwrap_or("waves"),
        position.as_deref().unwrap_or("middle-right"),
    )
}

/// Translate Mixweave's own recorder syntax (`Ctrl+Alt+M`, matching what
/// `shortcutFromKeyboardEvent` in the frontend produces) into a GTK
/// accelerator string GNOME's `custom-keybinding` schema understands.
fn to_gtk_accelerator(shortcut: &str) -> Option<String> {
    if shortcut.is_empty() {
        return None;
    }
    let parts: Vec<&str> = shortcut.split('+').collect();
    let (modifiers, key) = parts.split_at(parts.len() - 1);
    let key = key.first()?;

    let mut accelerator = String::new();
    for modifier in modifiers {
        accelerator.push_str(match *modifier {
            "Ctrl" => "<Control>",
            "Alt" => "<Alt>",
            "Shift" => "<Shift>",
            "Super" => "<Super>",
            _ => return None,
        });
    }
    accelerator.push_str(&gtk_keysym(key)?);
    Some(accelerator)
}

fn gtk_keysym(key: &str) -> Option<String> {
    if let Some(fkey) = key.strip_prefix('F') {
        if !fkey.is_empty() && fkey.chars().all(|c| c.is_ascii_digit()) {
            return Some(key.to_string());
        }
    }
    if let Some(digit) = key.strip_prefix("Numpad") {
        if digit.len() == 1 && digit.chars().all(|c| c.is_ascii_digit()) {
            return Some(format!("KP_{digit}"));
        }
    }
    if key.chars().count() == 1 {
        let ch = key.chars().next()?;
        if ch.is_ascii_alphabetic() {
            return Some(ch.to_ascii_lowercase().to_string());
        }
        if ch.is_ascii_digit() {
            return Some(ch.to_string());
        }
    }
    let name = match key {
        "ArrowUp" => "Up",
        "ArrowDown" => "Down",
        "ArrowLeft" => "Left",
        "ArrowRight" => "Right",
        "Backquote" => "grave",
        "Backslash" => "backslash",
        "BracketLeft" => "bracketleft",
        "BracketRight" => "bracketright",
        "CapsLock" => "Caps_Lock",
        "Comma" => "comma",
        "End" => "End",
        "Enter" => "Return",
        "Equal" => "equal",
        "Escape" => "Escape",
        "Home" => "Home",
        "Insert" => "Insert",
        "Minus" => "minus",
        "PageDown" => "Page_Down",
        "PageUp" => "Page_Up",
        "Pause" => "Pause",
        "Period" => "period",
        "PrintScreen" => "Print",
        "Quote" => "apostrophe",
        "ScrollLock" => "Scroll_Lock",
        "Semicolon" => "semicolon",
        "Slash" => "slash",
        "Space" => "space",
        "Tab" => "Tab",
        "Backspace" => "BackSpace",
        "Delete" => "Delete",
        "AudioVolumeDown" => "AudioLowerVolume",
        "AudioVolumeMute" => "AudioMute",
        "AudioVolumeUp" => "AudioRaiseVolume",
        "MediaPause" => "AudioPause",
        "MediaPlay" => "AudioPlay",
        "MediaPlayPause" => "AudioPlay",
        "MediaStop" => "AudioStop",
        "MediaTrackNext" => "AudioNext",
        "MediaTrackPrevious" => "AudioPrev",
        "NumLock" => "Num_Lock",
        "NumpadAdd" => "KP_Add",
        "NumpadDecimal" => "KP_Decimal",
        "NumpadDivide" => "KP_Divide",
        "NumpadEnter" => "KP_Enter",
        "NumpadEqual" => "KP_Equal",
        "NumpadMultiply" => "KP_Multiply",
        "NumpadSubtract" => "KP_Subtract",
        _ => return None,
    };
    Some(name.to_string())
}

/// dconf path segments only tolerate `[A-Za-z0-9_-]`; anything else
/// (":" from a "channel:<name>:<kind>" action id, in practice) becomes "-".
fn sanitize_path_segment(action: &str) -> String {
    action
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

pub(crate) fn gvariant_string(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// GNOME splits a custom keybinding's `command` with shell-style parsing, so
/// a path containing a space (e.g. "/media/me/PC games/...") must be quoted
/// or it is launched as just its first word.
fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-+:@%".contains(c))
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn gvariant_string_array(values: &[String]) -> String {
    let items: Vec<String> = values.iter().map(|v| gvariant_string(v)).collect();
    format!("[{}]", items.join(", "))
}

fn run_gsettings(args: &[&str]) -> Result<(), String> {
    let output = Command::new("gsettings")
        .args(args)
        .output()
        .map_err(|error| format!("could not run gsettings: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if detail.is_empty() {
            format!("gsettings exited with {}", output.status)
        } else {
            detail
        })
    }
}

/// Parse `gsettings get`'s GVariant text output for an `as` (string array)
/// value. An empty array prints with an explicit type annotation
/// ("@as []") since there are no elements to infer the type from; a
/// non-empty array never carries that prefix.
fn parse_string_array(text: &str) -> Vec<String> {
    let text = text.trim();
    let list = text.strip_prefix("@as").map(str::trim).unwrap_or(text);
    let Some(inner) = list
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    else {
        return Vec::new();
    };
    inner
        .split(',')
        .map(|entry| entry.trim().trim_matches('\'').to_string())
        // Every real custom-keybinding entry is an absolute dconf path.
        // This also self-heals a dconf array corrupted by an older parser
        // that mishandled "@as []", by dropping whatever garbage it left
        // behind instead of preserving it forever as "some other app's
        // shortcut".
        .filter(|entry| entry.starts_with('/'))
        .collect()
}

fn current_custom_keybindings() -> Vec<String> {
    let output = Command::new("gsettings")
        .args(["get", MEDIA_KEYS_SCHEMA, KEYBINDINGS_KEY])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    parse_string_array(&String::from_utf8_lossy(&output.stdout))
}

fn binary_command() -> Result<String, String> {
    let executable = crate::launch_path::launch_path()
        .map_err(|error| format!("could not locate the Mixweave executable: {error}"))?;
    let path = executable.to_string_lossy().into_owned();
    let path = path
        .strip_suffix(" (deleted)")
        .map(str::to_string)
        .unwrap_or(path);
    Ok(path)
}

fn media_keys_daemon_active() -> bool {
    Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", MEDIA_KEYS_SERVICE])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Some GNOME/Ubuntu-based packaging ships this component marked
/// "started by systemd" in its autostart entry without any systemd unit or
/// target actually pulling it in at session start, so it never launches at
/// login even though starting it on demand always works fine. Nudge it
/// awake ourselves; a no-op if it is already running.
fn ensure_media_keys_daemon_running() {
    if media_keys_daemon_active() {
        return;
    }
    let _ = Command::new("systemctl")
        .args(["--user", "start", MEDIA_KEYS_SERVICE])
        .status();
}

/// Rebuild every Mixweave-owned GNOME custom keybinding from the bindings the
/// frontend currently holds. Rebuilding from scratch each time (rather than
/// diffing) makes this self-healing: a stale or hand-edited entry is wiped
/// and replaced on the very next call, e.g. right after Mixweave starts.
#[tauri::command]
pub fn sync_gnome_shortcuts(
    enabled: bool,
    bindings: HashMap<String, String>,
) -> Result<(), String> {
    if enabled {
        // The gsettings entries below are useless if nothing on the desktop
        // is actually listening for them. Only nudge the daemon awake for
        // this running Mixweave process (a plain `systemctl --user start`,
        // idempotent and with no lasting effect once Mixweave exits) - do NOT
        // install anything into autostart: on this desktop, that produced a
        // second, uncoordinated launch path for the same component that
        // raced the system's own one over the same global keybinding grab
        // and brought down the whole Wayland session.
        ensure_media_keys_daemon_running();
    }
    let binary = binary_command()?;
    let existing = current_custom_keybindings();
    let (ours, others): (Vec<String>, Vec<String>) = existing
        .into_iter()
        .partition(|path| path.starts_with(PATH_PREFIX));

    let mut next_paths = others;
    let mut apply_errors = Vec::new();

    if enabled {
        for (action, shortcut) in &bindings {
            let Some(accelerator) = to_gtk_accelerator(shortcut) else {
                continue;
            };
            // Per-channel actions look like "channel:sink_game:mute"; dconf
            // path segments only tolerate a restricted character set, so the
            // raw action id (unchanged) still goes to `--shortcut`, but the
            // storage path uses a sanitized copy.
            let path = format!("{PATH_PREFIX}{}/", sanitize_path_segment(action));
            let schema_arg = format!("{CUSTOM_KEYBINDING_SCHEMA}:{path}");
            let name = format!("Mixweave: {action}");
            let command = format!("{} --shortcut {action}", shell_quote(&binary));
            let result = run_gsettings(&["set", &schema_arg, "name", &gvariant_string(&name)])
                .and_then(|()| {
                    run_gsettings(&["set", &schema_arg, "command", &gvariant_string(&command)])
                })
                .and_then(|()| {
                    run_gsettings(&[
                        "set",
                        &schema_arg,
                        "binding",
                        &gvariant_string(&accelerator),
                    ])
                });
            match result {
                Ok(()) => next_paths.push(path),
                Err(error) => apply_errors.push(format!("{action}: {error}")),
            }
        }
    }

    run_gsettings(&[
        "set",
        MEDIA_KEYS_SCHEMA,
        KEYBINDINGS_KEY,
        &gvariant_string_array(&next_paths),
    ])?;

    for stale in ours {
        if !next_paths.contains(&stale) {
            let schema_arg = format!("{CUSTOM_KEYBINDING_SCHEMA}:{stale}");
            let _ = run_gsettings(&["reset-recursively", &schema_arg]);
        }
    }

    if apply_errors.is_empty() {
        Ok(())
    } else {
        Err(apply_errors.join("; "))
    }
}
