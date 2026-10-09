//! Installs and enables the bundled GNOME Shell extension
//! (`resources/gnome-extension/`) that draws the shortcut OSD on GNOME
//! Wayland, and relays "show the popup" calls to it over the session D-Bus.
//! See that extension's own `extension.js` for why a Shell extension is the
//! mechanism at all, rather than a second Mixweave window (as used on X11 -
//! see `overlay.rs`).

use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

use crate::commands::linux_shortcuts::{gvariant_string, is_gnome, is_wayland};

const EXTENSION_UUID: &str = "sonux-osd@sonux.dev";
const EXTENSION_JS: &str = include_str!("../resources/gnome-extension/extension.js");
const EXTENSION_METADATA: &str = include_str!("../resources/gnome-extension/metadata.json");
const EXTENSION_STYLESHEET: &str = include_str!("../resources/gnome-extension/stylesheet.css");

/// The files that make up the installed extension.
fn bundled_files() -> [(&'static str, &'static str); 3] {
    [
        ("extension.js", EXTENSION_JS),
        ("metadata.json", EXTENSION_METADATA),
        ("stylesheet.css", EXTENSION_STYLESHEET),
    ]
}

/// Names of the files in `files` whose installed copy in `dir` is missing or
/// differs from the bundled one.
fn stale_files(dir: &std::path::Path, files: &[(&'static str, &str)]) -> Vec<&'static str> {
    files
        .iter()
        .filter(|(name, contents)| {
            std::fs::read_to_string(dir.join(name)).map_or(true, |existing| existing != *contents)
        })
        .map(|(name, _)| *name)
        .collect()
}

const OSD_DBUS_DEST: &str = "dev.sonux.Osd";
const OSD_DBUS_PATH: &str = "/dev/sonux/Osd";
const OSD_DBUS_METHOD: &str = "dev.sonux.Osd.Show";
/// Theme plus which popup look to draw.
const OSD_DBUS_METHOD_STYLED: &str = "dev.sonux.Osd.ShowStyled";
/// Newer method that also carries the app theme. An extension loaded before
/// it existed (GNOME keeps the old code until the next login) lacks it, so
/// the caller falls back to the original `Show`.
const OSD_DBUS_METHOD_THEMED: &str = "dev.sonux.Osd.ShowThemed";
/// Newest method: also carries which screen corner/edge to show at. Same
/// fallback story - an extension loaded before it existed answers with an
/// error, and `ShowStyled` still works there (at its fixed default spot).
const OSD_DBUS_METHOD_POSITIONED: &str = "dev.sonux.Osd.ShowPositioned";

/// Writes the bundled extension into place and enables it, so trying the
/// shortcut popup never requires installing anything by hand. A no-op once
/// the files already match and the extension is already enabled - safe to
/// call on every startup.
pub fn ensure_installed() {
    let (wayland, gnome) = (is_wayland(), is_gnome());
    if !(wayland && gnome) {
        eprintln!(
            "mixweave: GNOME OSD extension skipped (wayland={wayland}, gnome={gnome}) - X11 uses the overlay window instead"
        );
        return;
    }
    let Some(data_home) = dirs::data_dir() else {
        eprintln!(
            "mixweave: GNOME OSD extension skipped - could not resolve the XDG data directory"
        );
        return;
    };
    let extension_dir = data_home
        .join("gnome-shell/extensions")
        .join(EXTENSION_UUID);
    eprintln!(
        "mixweave: installing GNOME OSD extension into {}",
        extension_dir.display()
    );

    // Compare every file, not just extension.js: a change to the stylesheet
    // alone (new looks) must reach the installed copy too.
    let stale = stale_files(&extension_dir, &bundled_files());
    if stale.is_empty() {
        eprintln!("mixweave: GNOME OSD extension files already up to date");
    } else {
        if let Err(error) = std::fs::create_dir_all(&extension_dir) {
            eprintln!("mixweave: could not create the GNOME OSD extension directory: {error}");
            return;
        }
        for (name, contents) in bundled_files() {
            if !stale.contains(&name) {
                continue;
            }
            if let Err(error) = std::fs::write(extension_dir.join(name), contents) {
                eprintln!("mixweave: could not write the GNOME OSD extension's {name}: {error}");
                return;
            }
        }
        eprintln!(
            "mixweave: wrote GNOME OSD extension files: {}",
            stale.join(", ")
        );
    }

    // GNOME Shell caches an extension's imported JS module for the entire
    // lifetime of the Shell process - once `extension.js` has been imported
    // once, no disable/enable cycle can make it re-read the file from disk,
    // only a full session restart can (a hard GJS/ES-module limitation, not
    // a bug in this code). So changed *code* in an already-ACTIVE extension
    // needs the next login. The stylesheet is different: Shell re-reads it
    // every time the extension is enabled, so a stylesheet-only change is
    // applied here by turning the extension off and on again.
    let code_changed = stale.contains(&"extension.js");
    let style_only_changed = !stale.is_empty() && !code_changed;
    if let Some(state) = extension_state() {
        if state.state == "ACTIVE" {
            if code_changed {
                eprintln!("mixweave: GNOME OSD extension code changed - it takes effect at the next login");
            } else if style_only_changed {
                eprintln!(
                    "mixweave: reloading the GNOME OSD extension to pick up its new stylesheet"
                );
                let _ = Command::new("gnome-extensions")
                    .args(["disable", EXTENSION_UUID])
                    .output();
                sleep(Duration::from_millis(600));
                let _ = Command::new("gnome-extensions")
                    .args(["enable", EXTENSION_UUID])
                    .output();
            } else {
                eprintln!("mixweave: GNOME OSD extension is already ACTIVE");
            }
            return;
        }
        eprintln!(
            "mixweave: GNOME OSD extension state is {} (enabled={}) before enabling",
            state.state, state.enabled
        );
    }

    eprintln!("mixweave: enabling GNOME OSD extension");
    match Command::new("gnome-extensions")
        .args(["enable", EXTENSION_UUID])
        .output()
    {
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !output.status.success() || !stderr.trim().is_empty() {
                eprintln!(
                    "mixweave: `gnome-extensions enable` exited with {} - stderr: {}",
                    output.status,
                    stderr.trim()
                );
            }
        }
        Err(error) => {
            eprintln!("mixweave: could not run `gnome-extensions enable` (is gnome-extensions installed?): {error}");
            return;
        }
    }

    // Enabling only flips a gsettings key; GNOME Shell picks that change up
    // through an async `changed::enabled-extensions` signal handler, which
    // then imports the module (on a cold first run, genuinely slow: disk
    // I/O plus JS parsing/eval) and calls its `enable()` - all after this
    // command has already returned. Poll the real state instead of trusting
    // the exit code, and give that chain generous time before giving up.
    for attempt in 1..=15 {
        sleep(Duration::from_millis(400));
        match extension_state() {
            Some(state) if state.state == "ACTIVE" => {
                eprintln!("mixweave: GNOME OSD extension is ACTIVE (attempt {attempt}/15)");
                return;
            }
            Some(state) => {
                eprintln!(
                    "mixweave: GNOME OSD extension state is {} (enabled={}) after enabling (attempt {attempt}/15)",
                    state.state, state.enabled
                );
            }
            None => {
                eprintln!("mixweave: could not read the GNOME OSD extension's state after enabling (attempt {attempt}/15)");
            }
        }
    }
    eprintln!(
        "mixweave: GNOME OSD extension did not reach ACTIVE after 15 attempts - run \
         `journalctl --user -b | grep sonux-osd` for its own enable()/Show() log"
    );
}

struct ExtensionInfo {
    state: String,
    enabled: bool,
}

/// Parses the `State: ...` and `Enabled: ...` lines out of
/// `gnome-extensions info <uuid>`. `state` reflects whether the Shell has
/// actually activated the extension in this process (e.g. "ACTIVE",
/// "INACTIVE", "ERROR", "INITIALIZED", "OUT OF DATE"); `enabled` only
/// reflects whether the `enabled-extensions` gsettings key currently lists
/// it - the two can disagree while the async enable chain above is still
/// catching up, which is exactly what makes both worth logging separately.
fn extension_state() -> Option<ExtensionInfo> {
    let output = Command::new("gnome-extensions")
        .args(["info", EXTENSION_UUID])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let state = stdout
        .lines()
        .find_map(|line| line.trim().strip_prefix("State:"))
        .map(|state| state.trim().to_string())?;
    let enabled = stdout
        .lines()
        .find_map(|line| line.trim().strip_prefix("Enabled:"))
        .map(|value| value.trim().eq_ignore_ascii_case("yes"))
        .unwrap_or(false);
    Some(ExtensionInfo { state, enabled })
}

/// Relays a shortcut-driven mute/volume change to the extension's `Show`
/// D-Bus method. A no-op (from the caller's perspective, since the
/// frontend only calls this best-effort) if the extension isn't currently
/// enabled - e.g. right after a first install that still needs GNOME to
/// notice it.
pub fn show(
    label: &str,
    volume_percent: i32,
    max: i32,
    muted: bool,
    theme: &str,
    style: &str,
    position: &str,
) -> Result<(), String> {
    let call = |method: &str, extra: Vec<String>| -> Result<(), String> {
        let mut args = vec![
            "call".to_string(),
            "--session".to_string(),
            "--dest".to_string(),
            OSD_DBUS_DEST.to_string(),
            "--object-path".to_string(),
            OSD_DBUS_PATH.to_string(),
            "--method".to_string(),
            method.to_string(),
            gvariant_string(label),
            volume_percent.to_string(),
            max.to_string(),
            if muted { "true" } else { "false" }.to_string(),
        ];
        args.extend(extra);
        let output = Command::new("gdbus")
            .args(&args)
            .output()
            .map_err(|error| format!("could not run gdbus: {error}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    };

    let theme = if theme == "light" { "light" } else { "dark" };
    let style = match style {
        "segments" | "fader" | "waves" => style,
        _ => "waves",
    };
    let position = match position {
        "top-right" | "middle-right" | "bottom-right" | "top-left" | "middle-left"
        | "bottom-left" => position,
        _ => "middle-right",
    };
    // Newest method first; an extension loaded before it existed (GNOME keeps
    // the old code until the next login) answers with an error, and the older
    // methods still work there.
    if call(
        OSD_DBUS_METHOD_POSITIONED,
        vec![
            gvariant_string(theme),
            gvariant_string(style),
            gvariant_string(position),
        ],
    )
    .is_ok()
    {
        return Ok(());
    }
    if call(
        OSD_DBUS_METHOD_STYLED,
        vec![gvariant_string(theme), gvariant_string(style)],
    )
    .is_ok()
    {
        return Ok(());
    }
    if call(OSD_DBUS_METHOD_THEMED, vec![gvariant_string(theme)]).is_ok() {
        return Ok(());
    }
    call(OSD_DBUS_METHOD, Vec::new()).map_err(|detail| {
        eprintln!("mixweave: GNOME OSD extension did not respond: {detail}");
        detail
    })
}
