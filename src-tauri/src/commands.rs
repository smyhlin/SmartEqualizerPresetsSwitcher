use std::{path::PathBuf, process::Command};
#[cfg(target_os = "windows")]
use std::{
    env, fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::{
    autoeq, current_runtime_settings,
    logging::{append_log_line, log_folder_path, read_log_snapshot, LogSnapshot},
    refresh_runtime, refresh_runtime_settings, set_autorun_enabled_state,
    state::{AppError, AppState, PresetLibrary},
};
#[cfg(target_os = "windows")]
use crate::state::{classify_elevation_failure, detect_installed_install_path, run_elevated_process};
#[cfg(all(test, not(target_os = "windows")))]
use std::path::Path;

#[cfg(target_os = "windows")]
const INSTALL_APO_SCRIPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/resources/scripts/install-apo.ps1"
));

#[cfg(target_os = "windows")]
fn escape_for_powershell(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(target_os = "windows")]
fn write_install_script() -> Result<PathBuf, AppError> {
    let token = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let script_path = env::temp_dir().join(format!("install-apo-{token}.ps1"));
    fs::write(&script_path, INSTALL_APO_SCRIPT)?;
    Ok(script_path)
}

#[cfg(target_os = "windows")]
fn installer_log_path() -> PathBuf {
    let token = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    env::temp_dir().join(format!("install-apo-{token}.log"))
}

#[cfg(any(target_os = "windows", test))]
fn selector_path_from_install_path(install_path: &Path) -> PathBuf {
    install_path.join("DeviceSelector.exe")
}

#[cfg(any(target_os = "windows", test))]
fn should_retry_with_windows_powershell(error: &AppError) -> bool {
    let message = error.to_string().to_lowercase();
    message.contains("pwsh.exe")
        && (message.contains("cannot find")
            || message.contains("does not exist")
            || message.contains("not recognized"))
}

#[cfg(target_os = "windows")]
fn run_installer_script(script_path: &Path, log_path: &Path) -> Result<(), AppError> {
    let arguments = vec![
        "-NoProfile".to_string(),
        "-NonInteractive".to_string(),
        "-ExecutionPolicy".to_string(),
        "Bypass".to_string(),
        "-File".to_string(),
        script_path.to_string_lossy().into_owned(),
        "-LogPath".to_string(),
        log_path.to_string_lossy().into_owned(),
    ];

    match run_elevated_process(Path::new("pwsh.exe"), &arguments) {
        Ok(()) => Ok(()),
        Err(error) if should_retry_with_windows_powershell(&error) => {
            append_log_line(
                "WARN",
                "pwsh.exe was unavailable for the installer; retrying with powershell.exe.",
            );
            run_elevated_process(Path::new("powershell.exe"), &arguments)
        }
        Err(error) => Err(error),
    }
}

#[cfg(target_os = "windows")]
fn append_installer_log_to_app(log_path: &Path) {
    match fs::read_to_string(log_path) {
        Ok(content) => {
            for line in content
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
            {
                append_log_line("INFO", format!("[Installer] {line}"));
            }
        }
        Err(error) => append_log_line(
            "WARN",
            format!(
                "Unable to read the installer log at '{}': {error}",
                log_path.display()
            ),
        ),
    }
}

#[cfg(target_os = "windows")]
fn installer_log_summary(log_path: &Path) -> Option<String> {
    let content = fs::read_to_string(log_path).ok()?;
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .last()
        .map(|line| line.to_string())
}

#[cfg(target_os = "windows")]
fn launch_device_selector(selector_path: &Path, working_directory: &Path) -> Result<(), AppError> {
    let command = format!(
        "Start-Process -FilePath '{}' -WorkingDirectory '{}' -Verb RunAs -ErrorAction Stop",
        escape_for_powershell(selector_path.as_os_str().to_string_lossy().as_ref()),
        escape_for_powershell(working_directory.as_os_str().to_string_lossy().as_ref())
    );
    let shells = ["pwsh.exe", "powershell.exe"];
    let mut last_error: Option<std::io::Error> = None;

    for shell in shells {
        let result = Command::new(shell)
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                command.as_str(),
            ])
            .output();

        match result {
            Ok(output) if output.status.success() => return Ok(()),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let fallback_message = format!(
                    "Failed to launch Device Selector from '{}'.",
                    selector_path.display()
                );
                return Err(classify_elevation_failure(
                    output.status.code(),
                    stdout.as_str(),
                    stderr.as_str(),
                    fallback_message,
                ));
            }
            Err(error) => {
                last_error = Some(error);
            }
        }
    }

    Err(last_error.map_or_else(
        || AppError::Message("Unable to start an elevated PowerShell shell.".to_string()),
        |error| error.into(),
    ))
}

#[cfg(target_os = "windows")]
fn selector_path_error(selector_path: &Path) -> AppError {
    AppError::Message(format!(
        "Device Selector was not found at '{}'. Reinstall Equalizer APO to repair the install.",
        selector_path.display()
    ))
}


#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EqBackendStatus {
    platform: String,
    state: String,
    backend_name: String,
    status_label: String,
    status_detail: String,
    active_group_name: Option<String>,
    active_preset_name: Option<String>,
    config_path: Option<String>,
    installed_config_path: Option<String>,
    active_export_path: Option<String>,
    pipewire_config_path: Option<String>,
    setup_action_label: String,
    detected_backend_label: Option<String>,
    detected_backend_detail: Option<String>,
    install_command: Option<String>,
    restart_command: Option<String>,
    setup_hint: Option<String>,
}

fn active_selection(snapshot: &PresetLibrary) -> (Option<String>, Option<String>) {
    for group in &snapshot.groups {
        if let Some(active) = &group.active_preset {
            return (Some(group.name.clone()), Some(active.clone()));
        }
    }

    (None, None)
}

#[cfg(target_os = "linux")]
fn linux_eq_paths(snapshot: &PresetLibrary) -> (PathBuf, PathBuf) {
    let active_export_path = PathBuf::from(&snapshot.app_data_dir)
        .join("linux-eq")
        .join("active-equalizerapo.txt");
    let pipewire_config_path = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pipewire/pipewire.conf.d/99-smart-eq-preset-switcher-parametric-eq.conf");

    (active_export_path, pipewire_config_path)
}

#[cfg(target_os = "linux")]
fn linux_install_command() -> String {
    if Command::new("pacman").arg("--version").output().is_ok() {
        return "sudo pacman -S --needed pipewire pipewire-pulse wireplumber easyeffects".into();
    }

    if Command::new("apt").arg("--version").output().is_ok()
        || Command::new("apt-get").arg("--version").output().is_ok()
    {
        return "sudo apt update && sudo apt install -y pipewire pipewire-pulse wireplumber easyeffects".into();
    }

    if Command::new("dnf").arg("--version").output().is_ok() {
        return "sudo dnf install pipewire pipewire-pulseaudio wireplumber easyeffects".into();
    }

    if Command::new("zypper").arg("--version").output().is_ok() {
        return "sudo zypper install pipewire pipewire-pulseaudio wireplumber easyeffects".into();
    }

    "Install PipeWire, WirePlumber, pipewire-pulse and EasyEffects with your distribution package manager".into()
}

#[cfg(target_os = "linux")]
fn linux_restart_command() -> String {
    "systemctl --user try-restart pipewire.service pipewire-pulse.service wireplumber.service".into()
}


#[cfg(target_os = "linux")]
fn command_available(command: &str) -> bool {
    let script = format!("command -v {command} >/dev/null 2>&1");
    Command::new("sh")
        .args(["-c", script.as_str()])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn user_service_active(unit: &str) -> bool {
    Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", unit])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn linux_detected_backend() -> (String, String) {
    let pipewire_available = command_available("pipewire")
        || command_available("pw-cli")
        || user_service_active("pipewire.service");
    let pipewire_pulse_available = command_available("pipewire-pulse")
        || command_available("pactl")
        || user_service_active("pipewire-pulse.service");
    let wireplumber_available = command_available("wireplumber")
        || user_service_active("wireplumber.service");
    let easyeffects_available = command_available("easyeffects");

    let mut installed = Vec::new();
    let mut missing = Vec::new();

    if pipewire_available { installed.push("PipeWire"); } else { missing.push("PipeWire"); }
    if pipewire_pulse_available { installed.push("pipewire-pulse/Pulse bridge"); } else { missing.push("pipewire-pulse/Pulse bridge"); }
    if wireplumber_available { installed.push("WirePlumber"); } else { missing.push("WirePlumber"); }
    if easyeffects_available { installed.push("EasyEffects"); } else { missing.push("EasyEffects optional GUI"); }

    let label = if pipewire_available && wireplumber_available {
        if easyeffects_available {
            "PipeWire + EasyEffects detected".to_string()
        } else {
            "PipeWire detected".to_string()
        }
    } else {
        "Linux EQ backend incomplete".to_string()
    };

    let detail = format!(
        "Detected: {}. Missing/optional: {}.",
        if installed.is_empty() { "none".to_string() } else { installed.join(", ") },
        if missing.is_empty() { "none".to_string() } else { missing.join(", ") },
    );

    (label, detail)
}

#[cfg(target_os = "linux")]
pub(crate) fn restart_linux_audio_services() -> Result<(), AppError> {
    let command = linux_restart_command();
    append_log_line("INFO", format!("Running Linux EQ setup command: {command}"));
    let output = Command::new("systemctl")
        .args([
            "--user",
            "try-restart",
            "pipewire.service",
            "pipewire-pulse.service",
            "wireplumber.service",
        ])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            append_log_line("INFO", "PipeWire user services restarted or were not active.");
            Ok(())
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            append_log_line(
                "WARN",
                format!(
                    "PipeWire user service restart returned status {:?}. stdout='{}' stderr='{}'",
                    output.status.code(),
                    stdout,
                    stderr
                ),
            );
            Err(AppError::Message(format!(
                "PipeWire setup file was written, but restarting user services failed. Run manually: {}",
                command
            )))
        }
        Err(error) => {
            append_log_line("WARN", format!("Failed to run systemctl for Linux EQ setup: {error}"));
            Err(AppError::Message(format!(
                "PipeWire setup file was written, but systemctl was unavailable. Run manually: {}",
                command
            )))
        }
    }
}


#[cfg(target_os = "linux")]
pub(crate) fn route_linux_audio_to_eq_sink() -> Result<(), AppError> {
    if !command_available("pactl") {
        append_log_line(
            "INFO",
            "pactl is unavailable; leaving PipeWire EQ node routing to the desktop sound settings.",
        );
        return Ok(());
    }

    append_log_line(
        "INFO",
        "Setting SmartEQPresetSwitcher EQ as the default Pulse/PipeWire sink.",
    );

    let default_output = Command::new("pactl")
        .args(["set-default-sink", "smart-eq-preset-switcher.eq"])
        .output();

    match default_output {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            return Err(AppError::Message(format!(
                "PipeWire EQ was generated, but pactl could not set it as default. stdout='{}' stderr='{}'",
                stdout, stderr
            )));
        }
        Err(error) => return Err(error.into()),
    }

    // Move already-playing streams too. Ignore per-stream failures because some
    // applications intentionally pin their output device.
    let script = r#"pactl list short sink-inputs 2>/dev/null | awk '{print $1}' | while read -r input; do
  [ -n "$input" ] || continue
  pactl move-sink-input "$input" smart-eq-preset-switcher.eq >/dev/null 2>&1 || true
done"#;
    let _ = Command::new("sh").args(["-c", script]).status();

    Ok(())
}


fn build_eq_backend_status(snapshot: PresetLibrary) -> EqBackendStatus {
    let (active_group_name, active_preset_name) = active_selection(&snapshot);
    let has_active = active_preset_name.is_some();

    #[cfg(target_os = "windows")]
    {
        let config_path = PathBuf::from(&snapshot.config_path);
        let installed_path = snapshot.installed_config_path.as_ref().map(PathBuf::from);
        let connected = installed_path
            .as_ref()
            .map(|path| paths_equivalent(path, &config_path))
            .unwrap_or(false);

        let (state, status_label, status_detail) = if !has_active {
            (
                "no_active_preset",
                "No active preset",
                "Choose a preset and press Apply. Equalizer APO can only process the active preset.",
            )
        } else if connected {
            (
                "connected",
                "Equalizer APO connected",
                "The Windows Equalizer APO ConfigPath points at the SmartEQPresetSwitcher writable backend config folder.",
            )
        } else if snapshot.installed_config_path.is_some() {
            (
                "setup_needed",
                "APO setup needed",
                "Equalizer APO is installed, but its ConfigPath is not pointing at this app's managed config folder.",
            )
        } else {
            (
                "setup_needed",
                "APO not detected",
                "Use the setup panel to install or repair Equalizer APO and open the Device Selector.",
            )
        };

        let detected_backend_label = snapshot
            .installed_config_path
            .as_ref()
            .map(|_| "Equalizer APO detected".to_string());
        let detected_backend_detail = snapshot
            .installed_config_path
            .as_ref()
            .map(|path| format!("Equalizer APO ConfigPath: {path}"));

        return EqBackendStatus {
            platform: "windows".into(),
            state: state.into(),
            backend_name: "Equalizer APO".into(),
            status_label: status_label.into(),
            status_detail: status_detail.into(),
            active_group_name,
            active_preset_name,
            config_path: Some(snapshot.config_path),
            installed_config_path: snapshot.installed_config_path,
            active_export_path: None,
            pipewire_config_path: None,
            setup_action_label: "Open APO setup".into(),
            detected_backend_label,
            detected_backend_detail,
            install_command: None,
            restart_command: None,
            setup_hint: None,
        };
    }

    #[cfg(target_os = "linux")]
    {
        let (active_export_path, pipewire_config_path) = linux_eq_paths(&snapshot);
        let export_exists = active_export_path.exists();
        let pipewire_config_exists = pipewire_config_path.exists();

        let (state, status_label, status_detail) = if !has_active {
            (
                "no_active_preset",
                "No active preset",
                "Choose a preset and press Apply. Linux exports are generated from the active preset.",
            )
        } else if export_exists && pipewire_config_exists {
            (
                "export_ready",
                "Linux EQ export ready",
                "The active preset is exported and a PipeWire filter-chain config snippet exists. Restart PipeWire or your session if you just created it, then route audio to the generated EQ node.",
            )
        } else if export_exists {
            (
                "setup_needed",
                "Preset exported only",
                "The active preset was exported for manual Linux EQ tools, but no PipeWire filter-chain config was generated.",
            )
        } else {
            (
                "setup_needed",
                "System EQ not connected",
                "The preset is active inside SmartEQPresetSwitcher, but Linux system-wide EQ export has not been generated yet.",
            )
        };

        let (detected_backend_label, detected_backend_detail) = linux_detected_backend();

        return EqBackendStatus {
            platform: "linux".into(),
            state: state.into(),
            backend_name: "PipeWire / EasyEffects".into(),
            status_label: status_label.into(),
            status_detail: status_detail.into(),
            active_group_name,
            active_preset_name,
            config_path: Some(snapshot.config_path),
            installed_config_path: None,
            active_export_path: Some(active_export_path.to_string_lossy().into_owned()),
            pipewire_config_path: Some(pipewire_config_path.to_string_lossy().into_owned()),
            setup_action_label: if pipewire_config_exists { "Restart PipeWire EQ".into() } else { "Setup Linux EQ export".into() },
            detected_backend_label: Some(detected_backend_label),
            detected_backend_detail: Some(detected_backend_detail),
            install_command: Some(linux_install_command()),
            restart_command: Some(linux_restart_command()),
            setup_hint: Some(if pipewire_config_exists {
                "PipeWire setup exists. Restart services or route audio to the SmartEQPresetSwitcher EQ node if you do not hear the EQ yet.".into()
            } else if export_exists {
                "PipeWire setup was not generated. Try re-applying the preset; GraphicEQ presets are now converted to a parametric approximation automatically.".into()
            } else {
                "Apply a preset, then run setup. For system-wide PipeWire auto-setup, prefer AutoEQ parametric presets with Filter lines.".into()
            }),
        };
    }

    #[cfg(target_os = "macos")]
    {
        EqBackendStatus {
            platform: "macos".into(),
            state: "unsupported".into(),
            backend_name: "Unsupported".into(),
            status_label: "System EQ unsupported".into(),
            status_detail: "macOS system-wide EQ setup is not implemented in this project.".into(),
            active_group_name,
            active_preset_name,
            config_path: Some(snapshot.config_path),
            installed_config_path: snapshot.installed_config_path,
            active_export_path: None,
            pipewire_config_path: None,
            setup_action_label: "View details".into(),
            detected_backend_label: None,
            detected_backend_detail: None,
            install_command: None,
            restart_command: None,
            setup_hint: None,
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        EqBackendStatus {
            platform: "unknown".into(),
            state: "unsupported".into(),
            backend_name: "Unsupported".into(),
            status_label: "System EQ unsupported".into(),
            status_detail: "This platform does not have a supported SmartEQPresetSwitcher backend.".into(),
            active_group_name,
            active_preset_name,
            config_path: Some(snapshot.config_path),
            installed_config_path: snapshot.installed_config_path,
            active_export_path: None,
            pipewire_config_path: None,
            setup_action_label: "View details".into(),
            detected_backend_label: None,
            detected_backend_detail: None,
            install_command: None,
            restart_command: None,
            setup_hint: None,
        }
    }
}

#[cfg(target_os = "windows")]
fn paths_equivalent(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }

    let left = left.canonicalize().unwrap_or_else(|_| left.to_path_buf());
    let right = right.canonicalize().unwrap_or_else(|_| right.to_path_buf());
    left == right
}



fn parse_autoeq_variant(value: &str) -> Result<autoeq::AutoEqPresetVariant, AppError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" | "auto-target" | "auto_target" => Ok(autoeq::AutoEqPresetVariant::Auto),
        "parametric" | "parametric-eq" | "parametriceq" | "filter" | "filters" => {
            Ok(autoeq::AutoEqPresetVariant::Parametric)
        }
        "graphic" | "graphic-eq" | "graphiceq" => Ok(autoeq::AutoEqPresetVariant::Graphic),
        other => Err(AppError::Message(format!(
            "Unknown AutoEQ preset variant '{other}'. Expected auto, parametric, or graphic."
        ))),
    }
}

#[tauri::command]
pub fn get_config_path(state: State<'_, AppState>) -> Result<String, AppError> {
    let guard = state.lock()?;
    Ok(guard.get_config_path())
}

#[tauri::command]
pub fn load_logs() -> Result<LogSnapshot, AppError> {
    read_log_snapshot()
}

#[tauri::command]
pub fn load_autoeq_index(
    app: AppHandle,
    force_refresh: Option<bool>,
) -> Result<Vec<autoeq::AutoEqIndexEntry>, AppError> {
    autoeq::load_index(&app, force_refresh.unwrap_or(false))
}

#[tauri::command]
pub fn get_autoeq_graphic_preset(
    app: AppHandle,
    name: String,
    source: String,
) -> Result<String, AppError> {
    autoeq::get_graphic_preset(&app, &name, &source)
}

#[tauri::command]
pub fn get_autoeq_preset_variant(
    app: AppHandle,
    name: String,
    source: String,
    variant: String,
) -> Result<String, AppError> {
    let variant = parse_autoeq_variant(&variant)?;
    autoeq::get_preset_variant(&app, &name, &source, variant)
}

#[tauri::command]
pub fn install_or_reinstall_apo(app: AppHandle) -> Result<PresetLibrary, AppError> {
    // Equalizer APO is a Windows‑only application.  When called on
    // non‑Windows platforms we report that installation is not
    // available.  This prevents attempting to run PowerShell scripts
    // on unsupported systems.
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        return Err(AppError::Message(
            "Installing or reinstalling Equalizer APO is only supported on Windows.".into(),
        ));
    }

    // The following block executes only on Windows.  It invokes the
    // installer PowerShell script and refreshes the application
    // snapshot after completion.
    #[cfg(target_os = "windows")]
    {
        append_log_line("INFO", "Starting Equalizer APO install or reinstall.");
        let script_path = write_install_script()?;
        let log_path = installer_log_path();
        let result = run_installer_script(script_path.as_path(), log_path.as_path());

        let cleanup_result = fs::remove_file(&script_path);
        if let Err(error) = cleanup_result {
            append_log_line("WARN", error.to_string());
        }

        append_installer_log_to_app(log_path.as_path());
        let installer_summary = installer_log_summary(log_path.as_path());

        let cleanup_log_result = fs::remove_file(&log_path);
        if let Err(error) = cleanup_log_result {
            append_log_line("WARN", error.to_string());
        }

        match &result {
            Ok(_) => append_log_line("INFO", "Equalizer APO install script completed."),
            Err(error) => {
                let summary = installer_summary.unwrap_or_else(|| error.to_string());
                let failure_message = format!("Equalizer APO install failed: {summary}");
                append_log_line("ERROR", failure_message.as_str());
                return Err(AppError::Message(format!(
                    "{failure_message}. Open Logs for the full installer output."
                )));
            }
        }

        let snapshot = refresh_runtime(&app)?;
        append_log_line(
            "INFO",
            format!(
                "Equalizer APO snapshot refreshed after install. Detected install path: {}",
                snapshot
                    .installed_config_path
                    .as_deref()
                    .unwrap_or("not detected")
            ),
        );
        return Ok(snapshot);
    }
}

#[tauri::command]
pub fn set_config_path(
    app: AppHandle,
    state: State<'_, AppState>,
    new_path: String,
) -> Result<PresetLibrary, AppError> {
    {
        let mut guard = state.lock()?;
        guard.set_config_path(PathBuf::from(new_path))?;
    }
    refresh_runtime(&app)
}

#[tauri::command]
pub fn load_presets(state: State<'_, AppState>) -> Result<PresetLibrary, AppError> {
    let mut guard = state.lock()?;
    guard.snapshot()
}


#[tauri::command]
pub fn get_eq_backend_status(state: State<'_, AppState>) -> Result<EqBackendStatus, AppError> {
    let mut guard = state.lock()?;
    let snapshot = guard.snapshot()?;
    Ok(build_eq_backend_status(snapshot))
}

#[tauri::command]
pub fn export_linux_eq_status(state: State<'_, AppState>) -> Result<EqBackendStatus, AppError> {
    #[cfg(target_os = "linux")]
    {
        crate::linux_eq::export_active_preset()?;
    }

    let mut guard = state.lock()?;
    let snapshot = guard.snapshot()?;
    Ok(build_eq_backend_status(snapshot))
}


#[tauri::command]
pub fn setup_linux_system_eq(state: State<'_, AppState>) -> Result<EqBackendStatus, AppError> {
    #[cfg(target_os = "linux")]
    {
        crate::linux_eq::export_active_preset()?;

        let mut guard = state.lock()?;
        let snapshot = guard.snapshot()?;
        let status = build_eq_backend_status(snapshot);

        if status.state == "export_ready" {
            restart_linux_audio_services()?;
            if let Err(error) = route_linux_audio_to_eq_sink() {
                append_log_line("WARN", format!("PipeWire EQ routing failed: {error}"));
            }
        } else {
            append_log_line(
                "INFO",
                format!(
                    "Linux EQ setup exported preset but did not restart PipeWire because state is '{}': {}",
                    status.state, status.status_detail
                ),
            );
        }

        return Ok(status);
    }

    #[cfg(not(target_os = "linux"))]
    {
        let mut guard = state.lock()?;
        let snapshot = guard.snapshot()?;
        Ok(build_eq_backend_status(snapshot))
    }
}

#[tauri::command]
pub fn apply_preset(
    app: AppHandle,
    state: State<'_, AppState>,
    group: String,
    name: String,
) -> Result<PresetLibrary, AppError> {
    {
        let mut guard = state.lock()?;
        guard.apply_preset(&group, &name)?;
    }
    #[cfg(target_os = "linux")]
    {
        match crate::linux_eq::export_active_preset() {
            Ok(()) => {
                if let Err(error) = restart_linux_audio_services() {
                    append_log_line(
                        "WARN",
                        format!("Linux EQ was exported after apply, but audio service reload failed: {error}"),
                    );
                } else if let Err(error) = route_linux_audio_to_eq_sink() {
                    append_log_line("WARN", format!("Linux EQ was exported after apply, but routing failed: {error}"));
                }
            }
            Err(error) => {
                append_log_line("WARN", format!("Linux EQ export after apply failed: {error}"));
            }
        }
    }
    refresh_runtime(&app)
}

#[tauri::command]
pub fn save_preset(
    app: AppHandle,
    state: State<'_, AppState>,
    group: String,
    name: String,
    content: String,
) -> Result<PresetLibrary, AppError> {
    {
        let mut guard = state.lock()?;
        guard.save_preset(&group, &name, &content)?;
    }
    #[cfg(target_os = "linux")]
    {
        if let Err(error) = crate::linux_eq::export_active_preset() {
            append_log_line("WARN", format!("Linux EQ export after save failed: {error}"));
        }
    }
    refresh_runtime(&app)
}

#[tauri::command]
pub fn create_group(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<PresetLibrary, AppError> {
    {
        let mut guard = state.lock()?;
        guard.create_group(&name)?;
    }
    refresh_runtime(&app)
}

#[tauri::command]
pub fn set_group_emoji(
    app: AppHandle,
    state: State<'_, AppState>,
    group: String,
    emoji: Option<String>,
) -> Result<PresetLibrary, AppError> {
    {
        let mut guard = state.lock()?;
        guard.set_group_emoji(&group, emoji)?;
    }
    refresh_runtime(&app)
}

#[tauri::command]
pub fn rename_group(
    app: AppHandle,
    state: State<'_, AppState>,
    old_name: String,
    new_name: String,
) -> Result<PresetLibrary, AppError> {
    {
        let mut guard = state.lock()?;
        guard.rename_group(&old_name, &new_name)?;
    }
    refresh_runtime(&app)
}

#[tauri::command]
pub fn delete_group(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<PresetLibrary, AppError> {
    {
        let mut guard = state.lock()?;
        guard.delete_group(&name)?;
    }
    refresh_runtime(&app)
}

#[tauri::command]
pub fn reorder_groups(
    app: AppHandle,
    state: State<'_, AppState>,
    order: Vec<String>,
) -> Result<PresetLibrary, AppError> {
    {
        let mut guard = state.lock()?;
        guard.reorder_groups(&order)?;
    }
    refresh_runtime(&app)
}

#[tauri::command]
pub fn create_preset(
    app: AppHandle,
    state: State<'_, AppState>,
    group: String,
    name: String,
    content: Option<String>,
) -> Result<PresetLibrary, AppError> {
    {
        let mut guard = state.lock()?;
        guard.create_preset(&group, &name, content)?;
    }
    refresh_runtime(&app)
}

#[tauri::command]
pub fn rename_preset(
    app: AppHandle,
    state: State<'_, AppState>,
    group: String,
    old_name: String,
    new_name: String,
) -> Result<PresetLibrary, AppError> {
    {
        let mut guard = state.lock()?;
        guard.rename_preset(&group, &old_name, &new_name)?;
    }
    refresh_runtime(&app)
}

#[tauri::command]
pub fn delete_preset(
    app: AppHandle,
    state: State<'_, AppState>,
    group: String,
    name: String,
) -> Result<PresetLibrary, AppError> {
    {
        let mut guard = state.lock()?;
        guard.delete_preset(&group, &name)?;
    }
    refresh_runtime(&app)
}

#[tauri::command]
pub fn move_preset(
    app: AppHandle,
    state: State<'_, AppState>,
    old_group: String,
    new_group: String,
    name: String,
    target_index: Option<usize>,
) -> Result<PresetLibrary, AppError> {
    {
        let mut guard = state.lock()?;
        guard.move_preset(&old_group, &new_group, &name, target_index)?;
    }
    refresh_runtime(&app)
}

#[tauri::command]
pub fn import_presets(
    app: AppHandle,
    state: State<'_, AppState>,
    group: String,
    paths: Vec<String>,
) -> Result<PresetLibrary, AppError> {
    {
        let mut guard = state.lock()?;
        guard.import_presets(&group, &paths)?;
    }
    refresh_runtime(&app)
}

#[tauri::command]
pub fn attach_convolution_wav(
    app: AppHandle,
    state: State<'_, AppState>,
    group: String,
    name: String,
    content: String,
    source_path: String,
) -> Result<PresetLibrary, AppError> {
    {
        let mut guard = state.lock()?;
        guard.attach_convolution_wav(&group, &name, &content, &PathBuf::from(source_path))?;
    }
    refresh_runtime(&app)
}

#[tauri::command]
pub fn remove_convolution_wav(
    app: AppHandle,
    state: State<'_, AppState>,
    group: String,
    name: String,
    content: String,
) -> Result<PresetLibrary, AppError> {
    {
        let mut guard = state.lock()?;
        guard.remove_convolution_wav(&group, &name, &content)?;
    }
    refresh_runtime(&app)
}

#[tauri::command]
pub fn export_preset(
    state: State<'_, AppState>,
    group: String,
    name: String,
    destination: String,
) -> Result<String, AppError> {
    let guard = state.lock()?;
    guard.export_preset(&group, &name, &PathBuf::from(destination))?;
    Ok(name)
}

#[tauri::command]
pub fn export_app_settings(
    state: State<'_, AppState>,
    destination: String,
) -> Result<(), AppError> {
    let mut guard = state.lock()?;
    guard.export_app_settings(&PathBuf::from(destination))
}

#[tauri::command]
pub fn import_app_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    source: String,
) -> Result<PresetLibrary, AppError> {
    {
        let mut guard = state.lock()?;
        guard.import_app_settings(&PathBuf::from(source))?;
    }
    refresh_runtime(&app)
}

#[tauri::command]
pub fn rebuild_tray_menu(app: AppHandle) -> Result<PresetLibrary, AppError> {
    refresh_runtime(&app)
}

#[tauri::command]
pub fn get_autorun_enabled(app: AppHandle) -> Result<bool, AppError> {
    Ok(current_runtime_settings(&app)?.autorun_enabled)
}

#[tauri::command]
pub fn set_autorun_enabled(app: AppHandle, enabled: bool) -> Result<bool, AppError> {
    set_autorun_enabled_state(&app, enabled)?;
    Ok(refresh_runtime_settings(&app)?.autorun_enabled)
}

#[tauri::command]
pub fn reveal_path_in_explorer(path: String) -> Result<(), AppError> {
    let target = PathBuf::from(path);
    if !target.exists() {
        return Err(AppError::Message(format!(
            "The file or folder does not exist: {}",
            target.display()
        )));
    }

    #[cfg(target_os = "windows")]
    {
        // On Windows use the native Explorer.  Highlight files when
        // possible using the /select syntax.
        if target.is_dir() {
            Command::new("explorer.exe").arg(&target).spawn()?;
        } else {
            Command::new("explorer.exe")
                .arg(format!("/select,{}", target.display()))
                .spawn()?;
        }
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        // On Linux fall back to xdg-open.  If the target is a file
        // attempt to open its containing directory so the file is
        // visible in a file manager.  If that fails, try opening the
        // file directly as a last resort.
        let open_result = if target.is_dir() {
            Command::new("xdg-open").arg(&target).spawn()
        } else {
            // For files, open the parent directory.  If there is no
            // parent use the file path itself.
            let parent = target.parent().unwrap_or(&target);
            Command::new("xdg-open").arg(parent).spawn()
        };
        match open_result {
            Ok(_) => return Ok(()),
            Err(e) => {
                // Try opening the file directly.
                if Command::new("xdg-open").arg(&target).spawn().is_ok() {
                    return Ok(());
                }
                return Err(AppError::Message(format!(
                    "Failed to open path '{}': {}",
                    target.display(),
                    e
                )));
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        // On macOS use the `open` command.  Use `-R` to reveal
        // files in Finder when available.
        if target.is_dir() {
            Command::new("open").arg(&target).spawn()?;
        } else {
            // Use -R to reveal the file in Finder.
            Command::new("open").args(["-R", target.to_str().unwrap_or_default()]).spawn()?;
        }
        return Ok(());
    }

    // Fallback for unsupported platforms: return an error indicating the
    // feature is unavailable.
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        Err(AppError::Message(
            "Revealing paths is not supported on this platform.".to_string(),
        ))
    }
}

#[tauri::command]
pub fn open_apo_device_selector() -> Result<(), AppError> {
    // The Equalizer APO Device Selector executable is part of the Windows
    // installation.  On non‑Windows systems report that this feature is
    // unavailable.  We do this at runtime to avoid compilation
    // differences in the Tauri command table.
    #[cfg(not(target_os = "windows"))]
    {
        return Err(AppError::Message(
            "The Equalizer APO Device Selector is only available on Windows.".into(),
        ));
    }

    #[cfg(target_os = "windows")]
    {
        append_log_line("INFO", "Opening Equalizer APO Device Selector.");
        let install_path = detect_installed_install_path().ok_or_else(|| {
            AppError::Message(
                "Equalizer APO is not installed yet. Install or reinstall it first.".to_string(),
            )
        })?;
        let selector_path = selector_path_from_install_path(&install_path);

        if !selector_path.exists() {
            append_log_line(
                "ERROR",
                format!(
                    "Device Selector was not found at '{}'.",
                    selector_path.display()
                ),
            );
            return Err(selector_path_error(&selector_path));
        }

        let result = launch_device_selector(&selector_path, &install_path);
        match &result {
            Ok(_) => append_log_line(
                "INFO",
                format!(
                    "Device Selector launched from '{}'.",
                    selector_path.display()
                ),
            ),
            Err(error) => append_log_line(
                "ERROR",
                format!("Device Selector launch failed: {error}"),
            ),
        }
        return result;
    }
}

#[tauri::command]
pub fn open_repository_url() -> Result<(), AppError> {
    append_log_line(
        "INFO",
        "Opening the project repository in the default browser.",
    );
    webbrowser::open("https://github.com/smyhlin/SmartEQPresetSwitcher")
        .map_err(|error| AppError::Message(format!("Failed to open the repository URL: {error}")))
}

#[tauri::command]
pub fn open_logs_location() -> Result<(), AppError> {
    let folder_path = log_folder_path()?;
    append_log_line(
        "INFO",
        format!("Opening the logs folder at '{}'.", folder_path.display()),
    );
    reveal_path_in_explorer(folder_path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_path_from_install_path_should_point_to_device_selector_exe() {
        let install_path = Path::new(r"C:\Program Files\EqualizerAPO");

        let selector_path = selector_path_from_install_path(install_path);

        assert_eq!(
            selector_path,
            PathBuf::from(r"C:\Program Files\EqualizerAPO\DeviceSelector.exe")
        );
    }

    #[test]
    fn should_retry_with_windows_powershell_only_when_pwsh_is_missing() {
        let error = AppError::Message(
            "The elevated PowerShell command 'pwsh.exe' exited with code 1. The system cannot find the file specified.".to_string(),
        );

        assert!(should_retry_with_windows_powershell(&error));
    }

    #[test]
    fn should_not_retry_windows_powershell_for_installer_failures() {
        let error = AppError::Message("Equalizer APO installer exited with code 1603.".to_string());

        assert!(!should_retry_with_windows_powershell(&error));
    }
}
