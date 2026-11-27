// GumballZ Get - Modern cross-platform download manager
// Copyright (C) 2026 Nguyen Quoc Hung (GumballZ) <https://github.com/nughnguyen/gumballz-get>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn check_portable_mode() {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if dir.join("portable.txt").exists() || dir.join(".portable").exists() {
                let data_dir = dir.join("data");
                let _ = std::fs::create_dir_all(&data_dir);
                std::env::set_var("GUMBALLZ_PORTABLE", "1");
                std::env::set_var("GUMBALLZ_DATA_DIR", data_dir.to_string_lossy().to_string());

                #[cfg(windows)]
                if std::env::var("WEBVIEW2_USER_DATA_FOLDER").is_err() {
                    let webview_dir = data_dir.join("webview");
                    let _ = std::fs::create_dir_all(&webview_dir);
                    std::env::set_var(
                        "WEBVIEW2_USER_DATA_FOLDER",
                        webview_dir.to_string_lossy().to_string(),
                    );
                }

                // Older versions resolved the settings store against Tauri's
                // own AppData dir, so portable users' settings landed in the
                // OS profile. Adopt that file once if the portable dir has none.
                let portable_settings = data_dir.join("settings.json");
                if !portable_settings.exists() {
                    if let Some(base) = dirs::data_dir() {
                        let modern_settings =
                            base.join("com.gumballz.get").join("settings.json");
                        if modern_settings.exists() {
                            let _ = std::fs::copy(&modern_settings, &portable_settings);
                        }
                    }
                }
            }
        }
    }
}

fn setup_environment() {
    std::env::remove_var("PYTHONHOME");
    std::env::remove_var("PYTHONPATH");

    if let Some(bin_dir) = gumballz_lib::core::paths::app_data_dir().map(|d| d.join("bin")) {
        let sep = if cfg!(windows) { ";" } else { ":" };
        let current = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}{}{}", bin_dir.display(), sep, current));
    }

    std::env::set_var("PYTHONIOENCODING", "utf-8");
    std::env::set_var("PYTHONUTF8", "1");
    std::env::set_var("PYTHONLEGACYWINDOWSSTDIO", "0");

    #[cfg(target_os = "linux")]
    {
        let is_wayland = std::env::var("XDG_SESSION_TYPE").as_deref() == Ok("wayland")
            || std::env::var_os("WAYLAND_DISPLAY").is_some();

        if std::env::var_os("APPIMAGE").is_some()
            && std::env::var_os("GDK_BACKEND").is_none()
            && is_wayland
        {
            std::env::set_var("GDK_BACKEND", "x11");
        }
    }

    // WebKit plays media through GStreamer, and the AppImage bundles
    // GStreamer's core libraries — so the bundled libgstreamer looks for the
    // playback elements (appsink, autoaudiosink, decodebin) inside the
    // AppImage's own gstreamer-1.0 directory, which linuxdeploy does not fill,
    // and never scans the system's. Anything that starts a media pipeline
    // (the study course player) then dies on a NULL GObject and blanks the
    // page. Point the scan at the system plugin directories; they are
    // ABI-compatible because the bundled libraries are the same gst the
    // system plugins were built against. GST_PLUGIN_PATH adds to the scan
    // instead of replacing it, and an explicit user choice always wins.
    //
    // This is only a FALLBACK: the authoritative fix lives in the AppRun hook
    // (scripts/linux/linuxdeploy-plugin-gtk.sh), which sets the variable
    // before exec. Setting it this late races WebKit's early GStreamer init
    // through GLib's cached getenv and loses nondeterministically.
    #[cfg(target_os = "linux")]
    if std::env::var_os("APPIMAGE").is_some() && std::env::var_os("GST_PLUGIN_PATH").is_none() {
        let mut dirs: Vec<String> = Vec::new();
        if let Some(appdir) = std::env::var_os("APPDIR") {
            dirs.push(format!(
                "{}/usr/lib/gstreamer-1.0",
                appdir.to_string_lossy()
            ));
        }
        for dir in ["/usr/lib/gstreamer-1.0", "/usr/lib64/gstreamer-1.0"] {
            if std::path::Path::new(dir).is_dir() {
                dirs.push(dir.to_string());
            }
        }
        if dirs.len() > 1 {
            std::env::set_var("GST_PLUGIN_PATH", dirs.join(":"));
        }
    }
}

/// Uses an unpacked WebView2 Fixed Version Runtime next to the executable.
///
/// Issue #218. Tauri only sets `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` when the
/// bundle is compiled in `fixedRuntime` mode (`app.rs`), and never clears it — so
/// setting it here works, unlike `WEBVIEW2_USER_DATA_FOLDER`, which Tauri
/// would overwrite (see `core/portable.rs`). Does not touch it if the user
/// already set it.
#[cfg(windows)]
fn check_fixed_webview_runtime() {
    if std::env::var_os("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER").is_some() {
        return;
    }
    let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|p| p.to_path_buf()))
    else {
        return;
    };
    if let Some(runtime) = gumballz_lib::core::portable::find_fixed_runtime(&dir) {
        std::env::set_var("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER", &runtime);
    }
}

/// Startup errors must NOT die silently anymore (issue v1.0.0: opened the setup
/// exe and saw no window, no tray icon, no error message).
///
/// Cause: `run()` uses `.expect("error while building tauri
/// application")` — if window creation fails (e.g. machine has no WebView2
/// Runtime) the app panics. Since `windows_subsystem = "windows"` there is no
/// console to show the error, the process vanishes silently.
///
/// This hook, Windows only: writes the error details to
/// `%TEMP%\gumballz-get-startup-error.txt` then shows an English dialog so the
/// user knows the reason + how to fix it, instead of quitting silently.
#[cfg(windows)]
fn install_startup_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let message = info
            .payload_as_str()
            .unwrap_or("unknown error")
            .to_string();
        let detail = match info.location() {
            Some(loc) => format!("{message} ({}:{}:{})", loc.file(), loc.line(), loc.column()),
            None => message.clone(),
        };

        // Log first: if the dialog gets dismissed, the file is still there to look at.
        let log_path = std::env::temp_dir().join("gumballz-get-startup-error.txt");
        let _ = std::fs::write(&log_path, &detail);

        // Only show the dialog when the panic is on the main thread (app startup). A
        // panic on a worker thread (e.g. the startup-check thread) only writes the log,
        // so as not to bother the user while the app still runs fine.
        if std::thread::current().name() != Some("main") {
            return;
        }

        let hint = if detail.contains("WebView2") {
            "This machine has no WebView2 Runtime, so the app could not open a window. \
             Install Microsoft Edge WebView2 Runtime and reopen the app."
        } else {
            "Send the error file below to GumballZ for support."
        };
        let text = format!(
            "GumballZ Get hit an error while starting and could not open.\n\n{hint}\n\nError details:\n{detail}\n\nError file: {}",
            log_path.display()
        );
        show_error_dialog(&text);
    }));
}

#[cfg(windows)]
fn show_error_dialog(text: &str) {
    use windows::{
        core::PCWSTR,
        Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK},
    };
    fn to_wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }
    let title = to_wide("GumballZ Get could not open");
    let body = to_wide(text);
    unsafe {
        MessageBoxW(
            None,
            PCWSTR::from_raw(body.as_ptr()),
            PCWSTR::from_raw(title.as_ptr()),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn main() {
    #[cfg(windows)]
    install_startup_panic_hook();
    check_portable_mode();
    #[cfg(windows)]
    check_fixed_webview_runtime();
    setup_environment();
    gumballz_lib::run()
}
