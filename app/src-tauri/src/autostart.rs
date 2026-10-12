//! Login entry for "show when Claude opens", written with the `auto-launch`
//! crate (Windows registry `Run` key, macOS LaunchAgent, Linux XDG autostart).
//!
//! - Windows: current user only (never HKLM), path in double quotes.
//! - The entry runs this executable with `--standby` and nothing else.
//! - Debug builds never write it, so `pnpm tauri dev` cannot overwrite the
//!   entry of an installed copy.

use auto_launch::{AutoLaunch, AutoLaunchBuilder};

use crate::cli;

/// Argument of the login entry: start with the window hidden.
pub const STANDBY_ARG: &str = "--standby";
/// Name of the entry (registry value / plist / .desktop file).
const ENTRY_NAME: &str = "PET-CREDITS";

fn build() -> Result<AutoLaunch, String> {
    let exe = cli::current_exe_clean().map_err(|_| "no executable path".to_string())?;
    #[cfg_attr(not(target_os = "linux"), allow(unused_mut))]
    let mut path = exe.to_string_lossy().into_owned();
    // Linux: inside an AppImage the real file is the AppImage, not the temporary mount.
    #[cfg(target_os = "linux")]
    if let Some(img) = std::env::var_os("APPIMAGE") {
        path = img.to_string_lossy().into_owned();
    }
    if path.contains('"') {
        return Err("unsupported executable path".into());
    }
    // Windows and Linux write the path into a command line as is: quote it so a
    // folder with spaces works. macOS uses it as a file path: no quotes there.
    #[cfg(not(target_os = "macos"))]
    let path = format!("\"{path}\"");

    let mut b = AutoLaunchBuilder::new();
    b.set_app_name(ENTRY_NAME)
        .set_app_path(&path)
        .set_args(&[STANDBY_ARG])
        .set_macos_launch_mode(auto_launch::MacOSLaunchMode::LaunchAgent);
    #[cfg(windows)]
    b.set_windows_enable_mode(auto_launch::WindowsEnableMode::CurrentUser);
    b.build().map_err(|e| e.to_string())
}

/// Add (`true`) or remove (`false`) the login entry.
pub fn set(on: bool) -> Result<(), String> {
    if cfg!(debug_assertions) {
        eprintln!("pet-credits: login entry skipped in debug");
        return Ok(());
    }
    let launcher = build()?;
    if on {
        launcher.enable()
    } else {
        launcher.disable()
    }
    .map_err(|e| e.to_string())
}

/// The real state of the login entry. In debug builds nothing is ever written,
/// so the requested value is the truth (`requested`).
pub fn actual(requested: bool) -> bool {
    if cfg!(debug_assertions) {
        return requested;
    }
    build()
        .and_then(|l| l.is_enabled().map_err(|e| e.to_string()))
        .unwrap_or(false)
}
