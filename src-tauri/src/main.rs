// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Tauri's AppImage GTK hook forces GDK_BACKEND=x11, even on Wayland.
    // XWayland then stretches the window on fractionally scaled monitors.
    // Select the native backend before GTK starts, with X11 as a fallback.
    // A separate override survives the AppImage hook for driver workarounds.
    #[cfg(target_os = "linux")]
    {
        if let Some(backend) = std::env::var_os("MIXWEAVE_GDK_BACKEND") {
            std::env::set_var("GDK_BACKEND", backend);
        } else if std::env::var_os("APPIMAGE").is_some()
            && std::env::var_os("WAYLAND_DISPLAY").is_some_and(|value| !value.is_empty())
        {
            std::env::set_var("GDK_BACKEND", "wayland,x11");
        }
    }

    // WebKitGTK's DMABUF renderer crashes with a Wayland protocol error
    // (Gdk Error 71) on some GPU/driver combinations. Disable it unless the
    // user has set the variable themselves.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    mixweave_lib::run()
}
