//! The path a user would launch Mixweave by.
//!
//! Inside an AppImage, `std::env::current_exe()` is a file in a temporary
//! FUSE mount (`/tmp/.mount_XXXX/usr/bin/mixweave`) that changes on every
//! launch and disappears when the app exits. Anything that must outlive the
//! process (the autostart unit, GNOME keybinding commands, a restart) has to
//! use the AppImage file itself, which the runtime exports as `$APPIMAGE`.

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;

/// The AppImage file when running from one, otherwise the executable.
pub fn launch_path() -> io::Result<PathBuf> {
    launch_path_from(std::env::var_os("APPIMAGE"), std::env::current_exe)
}

fn launch_path_from(
    appimage: Option<OsString>,
    current_exe: impl FnOnce() -> io::Result<PathBuf>,
) -> io::Result<PathBuf> {
    match appimage {
        Some(path) if !path.is_empty() => Ok(PathBuf::from(path)),
        _ => current_exe(),
    }
}
