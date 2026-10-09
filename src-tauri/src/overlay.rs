//! The keyboard-shortcut OSD (mute/volume popup) needs to sit on top of
//! whatever the user is actually looking at - typically a fullscreen game,
//! not Mixweave's own window. A window Mixweave's own webview draws into can only
//! ever appear inside that webview's window; showing it "outside" the app
//! requires a second, dedicated OS window with no decorations, click-through
//! input, and an explicit "always on top" hint.
//!
//! X11 grants that hint through EWMH (`_NET_WM_STATE_ABOVE`), which GTK maps
//! to `gtk_window_set_keep_above` and which most window managers honor - a
//! reasonably reliable, general mechanism. Wayland deliberately gives
//! clients no protocol to demand it at all (stacking order and positioning
//! are the compositor's call, full stop); Mutter/GNOME does not implement
//! wlroots' `layer-shell` extension that some other compositors use for
//! exactly this. Getting a true system-wide overlay on Wayland means working
//! *with* a specific compositor - e.g. a GNOME Shell extension - not against
//! the protocol, and is tracked as later, per-desktop follow-up work. For
//! now this window is X11-only; GNOME/Wayland is covered instead by the
//! Shell extension in `gnome_osd_extension.rs`, and any other Wayland
//! desktop falls back to the in-window popup this already had (see
//! `detect_shortcut_backend` and `ShortcutOsd.tsx`).

use tauri::{App, WebviewUrl, WebviewWindowBuilder};

const OSD_WINDOW_LABEL: &str = "osd";

// Transparent, click-through canvas the OSD card slides around inside of.
// Spans the whole monitor (rather than just a strip near one edge) so the
// card's CSS position - any of the six corners/edges the user can pick in
// Settings > Popup Overlay - always lands inside the window instead of
// being clipped by it; see the `.pos-*` rules in globals.css.
const FALLBACK_WIDTH: f64 = 1920.0;
const FALLBACK_HEIGHT: f64 = 1080.0;

pub fn spawn(app: &App) {
    if crate::commands::linux_shortcuts::is_wayland() {
        return;
    }

    let mut x = 0.0;
    let mut y = 0.0;
    let mut width = FALLBACK_WIDTH;
    let mut height = FALLBACK_HEIGHT;
    if let Ok(Some(monitor)) = app.primary_monitor() {
        let scale = monitor.scale_factor();
        x = f64::from(monitor.position().x) / scale;
        y = f64::from(monitor.position().y) / scale;
        width = f64::from(monitor.size().width) / scale;
        height = f64::from(monitor.size().height) / scale;
    }

    let result = WebviewWindowBuilder::new(
        app,
        OSD_WINDOW_LABEL,
        WebviewUrl::App("index.html?osd=1".into()),
    )
    .title("Mixweave OSD")
    .inner_size(width, height)
    .position(x, y)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .visible_on_all_workspaces(true)
    .skip_taskbar(true)
    .resizable(false)
    .shadow(false)
    .focused(false)
    .visible(true)
    .build();

    match result {
        Ok(window) => {
            // Purely a notification - it must never steal a click meant for
            // whatever is underneath it.
            if let Err(error) = window.set_ignore_cursor_events(true) {
                eprintln!("mixweave: could not make the OSD window click-through: {error}");
            }
        }
        Err(error) => eprintln!("mixweave: could not create the OSD overlay window: {error}"),
    }
}
