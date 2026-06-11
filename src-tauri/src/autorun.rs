//! Cross‑platform autorun management.
//!
//! This module exposes a small API to enable, disable and query
//! application autostart status on supported operating systems.  On
//! Windows the implementation writes to the per‑user Run registry key.
//! On Linux the implementation generates a freedesktop `.desktop`
//! entry and a user systemd service.  Both approaches allow the
//! application to start automatically when the user logs in or the
//! system boots.  A boot‑sync mode is provided to refresh the
//! currently active preset without showing a GUI.  On other
//! operating systems the functions are no‑ops.

use std::fs::{self, File};
use std::io::Write;
#[cfg(target_os = "linux")]
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use crate::state::{AppError, AppState};


#[cfg(target_os = "linux")]
fn quote_exec_path(path: &Path) -> String {
    let escaped = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// Returns true if autorun is currently enabled for this user.
pub fn status() -> Result<bool, AppError> {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WOW64_64KEY};
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key = hkcu
            .open_subkey_with_flags("Software\\Microsoft\\Windows\\CurrentVersion\\Run", KEY_READ | KEY_WOW64_64KEY)
            .map_err(|e| AppError::Message(format!("Failed to open Run registry key: {e}")))?;
        let exe = std::env::current_exe()
            .map_err(|e| AppError::Message(format!("Failed to resolve current executable: {e}")))?;
        let name = exe
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("smart_eq_preset_switcher");
        match key.get_value::<String, _>(name) {
            Ok(value) => {
                // Consider it enabled if the path matches our binary and
                // contains a GUI flag.
                Ok(value.contains(exe.to_string_lossy().as_ref()))
            }
            Err(_) => Ok(false),
        }
    }
    #[cfg(target_os = "linux")]
    {
        // Check if the autostart desktop entry exists and if the systemd
        // service is enabled.
        let autostart = autostart_desktop_path();
        if !autostart.exists() {
            return Ok(false);
        }
        // If systemctl is present, use it to determine whether the user
        // service is enabled.  A failure falls back to the presence of
        // the .service file.
        if let Ok(output) = Command::new("systemctl")
            .args(["--user", "is-enabled", service_name()])
            .output()
        {
            if output.status.success() {
                return Ok(true);
            }
            // Some distributions report "disabled" on stderr; treat any
            // non‑enabled state as disabled.
            return Ok(false);
        }
        // Fallback: check for service file.
        Ok(service_path().exists())
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        Ok(false)
    }
}

/// Enables autorun for the current user.  On Windows this adds a Run
/// registry entry.  On Linux this writes a freedesktop autostart
/// desktop entry and a systemd user service, enabling it if
/// `systemctl` is available.  It also attempts to enable user
/// lingering so the service can run at boot without a login session.
pub fn enable() -> Result<(), AppError> {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE, KEY_WOW64_64KEY};
        use winreg::RegKey;
        let exe = std::env::current_exe()
            .map_err(|e| AppError::Message(format!("Failed to resolve current executable: {e}")))?;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu
            .create_subkey_with_flags(
                "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                KEY_SET_VALUE | KEY_WOW64_64KEY,
            )
            .map_err(|e| AppError::Message(format!("Failed to open Run registry key: {e}")))?;
        let name = exe
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("smart_eq_preset_switcher");
        let cmd = format!("\"{}\" --tray", exe.to_string_lossy());
        key.set_value(name, &cmd)
            .map_err(|e| AppError::Message(format!("Failed to set registry value: {e}")))?
        ;
        let _ = key.delete_value("smart_equalizer_apo_presets_manager");
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        let exe = std::env::current_exe()
            .map_err(|e| AppError::Message(format!("Failed to resolve current executable: {e}")))?;
        // Write the autostart desktop file.
        let desktop = autostart_desktop_path();
        if let Some(parent) = desktop.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| AppError::Message(format!("Failed to create autostart directory: {e}")))?;
        }
        let mut file = File::create(&desktop)
            .map_err(|e| AppError::Message(format!("Failed to create autostart file: {e}")))?;
        let quoted_exe = quote_exec_path(&exe);
        let content = format!(
            "[Desktop Entry]\nType=Application\nName=SmartEQPresetSwitcher\nComment=Start SmartEQPresetSwitcher at login\nExec={quoted_exe} --tray\nHidden=false\nX-GNOME-Autostart-enabled=true\nNoDisplay=true\n"
        );
        file.write_all(content.as_bytes())
            .map_err(|e| AppError::Message(format!("Failed to write autostart file: {e}")))?;

        // Write the systemd service file.
        let service = service_path();
        if let Some(parent) = service.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| AppError::Message(format!("Failed to create systemd directory: {e}")))?;
        }
        let mut file = File::create(&service)
            .map_err(|e| AppError::Message(format!("Failed to create systemd service: {e}")))?;
        let service_content = format!(
            "[Unit]\nDescription=SmartEQPresetSwitcher Boot Sync\n\n[Service]\nExecStart={quoted_exe} --boot-sync\nRestart=no\n\n[Install]\nWantedBy=default.target\n"
        );
        file.write_all(service_content.as_bytes())
            .map_err(|e| AppError::Message(format!("Failed to write systemd service: {e}")))?;

        // Try to reload and enable the service using systemctl if available.
        if Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .status()
            .is_ok()
        {
            let _ = Command::new("systemctl")
                .args(["--user", "enable", service_name()])
                .status();
            let _ = Command::new("systemctl")
                .args(["--user", "start", service_name()])
                .status();
        }
        // Attempt to enable user lingering so the service runs without an
        // interactive login.  This may fail silently on systems without
        // logind.
        let _ = Command::new("loginctl")
            .args(["enable-linger", &whoami()])
            .status();
        Ok(())
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        Ok(())
    }
}

/// Disables autorun for the current user.  It removes the Run
/// registry entry or the autostart files as appropriate.
pub fn disable() -> Result<(), AppError> {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE, KEY_WOW64_64KEY};
        use winreg::RegKey;
        let exe = std::env::current_exe()
            .map_err(|e| AppError::Message(format!("Failed to resolve current executable: {e}")))?;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key = hkcu
            .open_subkey_with_flags(
                "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                KEY_SET_VALUE | KEY_WOW64_64KEY,
            )
            .map_err(|e| AppError::Message(format!("Failed to open Run registry key: {e}")))?;
        let name = exe
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("smart_eq_preset_switcher");
        let _ = key.delete_value(name);
        let _ = key.delete_value("smart_equalizer_apo_presets_manager");
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        // Remove autostart desktop file.
        let _ = fs::remove_file(autostart_desktop_path());
        // Disable and remove systemd service.
        if Command::new("systemctl")
            .args(["--user", "disable", service_name()])
            .status()
            .is_ok()
        {
            let _ = Command::new("systemctl")
                .args(["--user", "stop", service_name()])
                .status();
        }
        let _ = fs::remove_file(service_path());
        Ok(())
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        Ok(())
    }
}

/// Performs a boot‑synchronization of the application state without
/// launching a GUI.  This function initializes the application state
/// and writes the active preset to the appropriate configuration
/// location.  On Windows this updates the Equalizer APO config.  On
/// Linux this updates the PipeWire parametric EQ and filter chain
/// configurations.  It exits silently if no active preset is found.
pub fn boot_sync() -> Result<(), AppError> {
    // Initialize state and write the active preset configuration.  Any
    // logging happens via the existing logging subsystem.
    let state = AppState::initialize()?;
    {
        let mut guard = state.lock()?;
        // Write the active configuration.  This will noop if there is no
        // active preset.
        let _ = guard.write_active_config();
    }
    #[cfg(target_os = "linux")]
    {
        // On Linux also update the PipeWire/EasyEffects configuration.
        if let Err(error) = crate::linux_eq::export_active_preset() {
            // Log the error but do not fail the boot sync entirely.
            crate::logging::append_log_line("ERROR", format!("Linux EQ export failed: {error}"));
        }
    }
    Ok(())
}

// Helpers for Linux implementation.

#[cfg(target_os = "linux")]
fn autostart_desktop_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("autostart")
        .join("smart-eq-preset-switcher.desktop")
}

#[cfg(target_os = "linux")]
fn service_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("systemd/user")
        .join(format!("{}.service", service_name()))
}

#[cfg(target_os = "linux")]
fn service_name() -> &'static str {
    "smart-eq-preset-switcher"
}

#[cfg(target_os = "linux")]
fn whoami() -> String {
    std::env::var("USER").unwrap_or_else(|_| "unknown".into())
}