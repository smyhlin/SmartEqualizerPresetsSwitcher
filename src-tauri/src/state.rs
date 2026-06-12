use std::{
    env,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use tauri::Error as TauriError;
use thiserror::Error;
#[cfg(target_os = "windows")]
use std::process::Command;
// The Windows registry is only available on the Windows platform.
#[cfg(target_os = "windows")]
use winreg::{
    enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, KEY_WOW64_64KEY},
    RegKey,
};

use crate::logging::append_log_line;

pub const APP_FOLDER_NAME: &str = "SmartEQPresetSwitcher";
const LEGACY_APP_FOLDER_NAME: &str = "SmartEqualizerAPO";
pub const EVENT_PRESETS_UPDATED: &str = "smart-eq://presets-updated";
pub const EVENT_SETTINGS_UPDATED: &str = "smart-eq://settings-updated";
pub const EVENT_OPEN_ABOUT_REQUESTED: &str = "smart-eq://open-about";
#[cfg(target_os = "windows")]
pub const REGISTRY_KEY_PATH: &str = r"SOFTWARE\EqualizerAPO";
#[cfg(target_os = "windows")]
pub const REGISTRY_VALUE_NAME: &str = "ConfigPath";
#[cfg(target_os = "windows")]
pub const REGISTRY_INSTALL_PATH_VALUE_NAME: &str = "InstallPath";
const MANAGED_CONFIG_DIR_NAME: &str = "SmartEQPresetSwitcher";
const MANAGED_ACTIVE_PRESET_FILE_NAME: &str = "active-preset.txt";
const MANAGED_BLOCK_START: &str = "# >>> SmartEQPresetSwitcher >>>";
const MANAGED_BLOCK_END: &str = "# <<< SmartEQPresetSwitcher <<<";
const LEGACY_MANAGED_BLOCK_START: &str = "# >>> SmartEqualizerAPOPresetsManager >>>";
const LEGACY_MANAGED_BLOCK_END: &str = "# <<< SmartEqualizerAPOPresetsManager <<<";

#[derive(Debug, Error)]
pub enum AppError {
    #[error("The per-user configuration directory is unavailable on this system.")]
    AppDataUnavailable,
    #[error("The tray icon was not initialized.")]
    MissingTray,
    #[error("The bundled application icon is missing.")]
    MissingIcon,
    #[error("A concurrent operation failed because the app state lock was poisoned.")]
    StatePoisoned,
    #[error("Group '{0}' was not found.")]
    GroupNotFound(String),
    #[error("Preset '{name}' was not found in group '{group}'.")]
    PresetNotFound { group: String, name: String },
    #[error("'{0}' already exists.")]
    AlreadyExists(String),
    #[error("'{0}' is not a valid Windows file name.")]
    InvalidName(String),
    #[error("The tray menu item '{0}' is no longer valid.")]
    UnknownMenuItem(String),
    #[error("The Equalizer APO config path registry entry is missing.")]
    RegistryValueMissing,
    #[error("Administrator privileges were declined. Please accept the UAC prompt, or run the application as administrator.")]
    ElevationDeclined,
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Tauri(#[from] TauriError),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PresetLibrary {
    pub app_data_dir: String,
    pub config_path: String,
    pub default_config_path: String,
    #[serde(default)]
    pub installed_config_path: Option<String>,
    pub groups: Vec<PresetGroup>,
    pub needs_config_migration: bool,
    pub config_path_prompted: bool,
    pub eq_disabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppRuntimeSettings {
    pub autorun_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PresetGroup {
    pub name: String,
    pub order: usize,
    pub emoji: Option<String>,
    pub active_preset: Option<String>,
    pub presets: Vec<PresetItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PresetItem {
    pub name: String,
    pub order: usize,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub convolution: Option<PresetConvolution>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PresetConvolution {
    pub wav_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wav_base64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PresetsMetadata {
    #[serde(default)]
    pub config_path_prompted: bool,
    #[serde(default)]
    pub eq_disabled: bool,
    #[serde(default)]
    pub groups: Vec<GroupMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GroupMetadata {
    pub name: String,
    pub order: usize,
    #[serde(default)]
    pub emoji: Option<String>,
    pub active_preset: Option<String>,
    #[serde(default)]
    pub presets: Vec<PresetMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PresetMetadata {
    pub name: String,
    pub order: usize,
}

#[derive(Debug, Clone)]
pub struct TraySelection {
    pub group: String,
    pub preset: String,
}

#[derive(Debug)]
pub struct AppState {
    inner: Mutex<AppStateInner>,
}

#[derive(Debug)]
pub struct AppStateInner {
    app_data_dir: PathBuf,
    presets_dir: PathBuf,
    metadata_path: PathBuf,
    default_config_path: PathBuf,
    current_config_path: PathBuf,
    detected_install_config_path: Option<PathBuf>,
    metadata: PresetsMetadata,
    tray_menu_targets: Vec<(String, TraySelection)>,
}

impl AppState {
    pub fn initialize() -> Result<Self, AppError> {
        let config_root = dirs::config_dir().ok_or(AppError::AppDataUnavailable)?;
        let app_data_dir = config_root.join(APP_FOLDER_NAME);
        migrate_legacy_app_data_dir(&config_root, &app_data_dir)?;
        let presets_dir = app_data_dir.join("presets");
        let metadata_path = app_data_dir.join("presets.json");
        let default_config_path = app_data_dir.join("config");
        let detected_install_config_path = detect_installed_config_path();

        fs::create_dir_all(&presets_dir)?;
        fs::create_dir_all(&default_config_path)?;

        let metadata = load_metadata(&metadata_path)?;
        let current_config_path = read_registry_config_path()
            .ok()
            .or_else(|| detected_install_config_path.clone())
            .unwrap_or_else(|| default_config_path.clone());

        let mut inner = AppStateInner {
            app_data_dir,
            presets_dir,
            metadata_path,
            default_config_path,
            current_config_path,
            detected_install_config_path,
            metadata,
            tray_menu_targets: Vec::new(),
        };

        inner.sync_metadata_with_disk()?;
        inner.persist_metadata()?;

        if inner.is_config_path_writable()? {
            inner.write_active_config()?;
        }

        Ok(Self {
            inner: Mutex::new(inner),
        })
    }

    pub fn lock(&self) -> Result<MutexGuard<'_, AppStateInner>, AppError> {
        self.inner.lock().map_err(|_| AppError::StatePoisoned)
    }
}

impl AppStateInner {
    pub fn snapshot(&mut self) -> Result<PresetLibrary, AppError> {
        self.refresh_runtime_paths();
        self.sync_metadata_with_disk()?;
        self.normalize_single_active_selection();

        let groups = self
            .metadata
            .groups
            .iter()
            .enumerate()
            .map(|(group_index, group)| {
                let presets = group
                    .presets
                    .iter()
                    .enumerate()
                    .filter_map(|(preset_index, preset)| {
                        let preset_path = self.preset_path(&group.name, &preset.name);
                        if !preset_path.exists() {
                            return None;
                        }

                        let content = fs::read_to_string(preset_path).unwrap_or_default();
                        let convolution = self.build_convolution_state(&content);
                        Some(PresetItem {
                            name: preset.name.clone(),
                            order: preset_index,
                            content,
                            convolution,
                        })
                    })
                    .collect::<Vec<_>>();

                PresetGroup {
                    name: group.name.clone(),
                    order: group_index,
                    emoji: group.emoji.clone(),
                    active_preset: group.active_preset.clone(),
                    presets,
                }
            })
            .collect::<Vec<_>>();

        Ok(PresetLibrary {
            app_data_dir: path_to_string(&self.app_data_dir),
            config_path: path_to_string(&self.current_config_path),
            default_config_path: path_to_string(&self.default_config_path),
            installed_config_path: self
                .detected_install_config_path
                .as_ref()
                .map(|path| path_to_string(path)),
            groups,
            needs_config_migration: !self.is_config_path_writable()?,
            config_path_prompted: self.metadata.config_path_prompted,
            eq_disabled: self.metadata.eq_disabled,
        })
    }

    fn refresh_runtime_paths(&mut self) {
        self.detected_install_config_path = detect_installed_config_path();
        self.current_config_path = read_registry_config_path()
            .ok()
            .or_else(|| self.detected_install_config_path.clone())
            .unwrap_or_else(|| self.default_config_path.clone());
    }

    fn build_convolution_state(&self, content: &str) -> Option<PresetConvolution> {
        let wav_path = extract_convolution_path(content)?;
        let resolved_path = self.resolve_convolution_reference(Path::new(wav_path.as_str()));
        let mut convolution = PresetConvolution {
            wav_path,
            wav_base64: None,
            error: None,
        };

        if !resolved_path.exists() || !resolved_path.is_file() {
            convolution.error = Some(format!(
                "The file or folder does not exist: {}",
                resolved_path.display()
            ));
            return Some(convolution);
        }

        match fs::read(&resolved_path) {
            Ok(bytes) => {
                convolution.wav_base64 = Some(STANDARD.encode(bytes));
            }
            Err(error) => {
                convolution.error = Some(error.to_string());
            }
        }

        Some(convolution)
    }

    fn resolve_convolution_reference(&self, wav_path: &Path) -> PathBuf {
        if wav_path.is_absolute() {
            wav_path.to_path_buf()
        } else {
            self.current_config_path.join(wav_path)
        }
    }

    fn preset_convolution_path(&self, group_name: &str, preset_name: &str) -> PathBuf {
        self.group_path(group_name)
            .join(format!("{preset_name}.wav"))
    }

    pub fn get_config_path(&self) -> String {
        path_to_string(&self.current_config_path)
    }

    pub fn default_config_path_string(&self) -> String {
        path_to_string(&self.default_config_path)
    }

    pub fn mark_config_path_prompted(&mut self, prompted: bool) -> Result<(), AppError> {
        self.metadata.config_path_prompted = prompted;
        self.persist_metadata()
    }

    pub fn should_prompt_for_config_migration(&self) -> Result<bool, AppError> {
        Ok(!self.metadata.config_path_prompted
            && self.current_config_path != self.default_config_path
            && !self.is_config_path_writable()?)
    }

    pub fn resolve_tray_selection(&self, menu_id: &str) -> Result<TraySelection, AppError> {
        self.tray_menu_targets
            .iter()
            .find(|(id, _)| id == menu_id)
            .map(|(_, selection)| selection.clone())
            .ok_or_else(|| AppError::UnknownMenuItem(menu_id.to_string()))
    }

    pub fn replace_tray_targets(&mut self, targets: Vec<(String, TraySelection)>) {
        self.tray_menu_targets = targets;
    }

    pub fn set_config_path(&mut self, new_path: PathBuf) -> Result<(), AppError> {
        // Validate and normalize the path before proceeding.  This will
        // ensure that Windows reserved characters are stripped and the
        // resulting path is canonical.
        let normalized_path = validate_and_normalize_path(&new_path)?;

        // Ensure the directory exists before attempting to set it.
        ensure_directory(&normalized_path)?;

        // On Windows we need to update the registry.  On other
        // platforms this call is a no‑op because the helper is
        // stubbed out.  The `cfg` block below contains the Windows
        // implementation.  After this block executes the Windows
        // registry should reflect our desired path.
        #[cfg(target_os = "windows")]
        {
            match write_registry_config_path(&normalized_path) {
                Ok(()) => {}
                Err(error)
                    if error.kind() == std::io::ErrorKind::PermissionDenied
                        || error.kind() == std::io::ErrorKind::Other =>
                {
                    // The registry write failed due to insufficient
                    // privileges.  Attempt to run the helper with
                    // elevation.  If that fails the error will be
                    // propagated.
                    run_elevated_registry_helper(&normalized_path)?;
                }
                Err(error) => return Err(error.into()),
            }

            // Re-read the registry and normalize both paths for comparison.
            let actual_config_path = normalize_path(&read_registry_config_path()?);
            let expected_path = normalize_path(&normalized_path);

            if actual_config_path != expected_path {
                // The registry value differs from the requested path.
                // Update our local state to match and surface an
                // informative error.  This prevents silent
                // inconsistencies when another process changes the
                // registry between our write and read.
                self.current_config_path = actual_config_path.clone();
                return Err(AppError::Message(format!(
                    "Equalizer APO ConfigPath was updated to '{}', but the requested path was '{}'. This may indicate another process modified the registry.",
                    path_to_string(&actual_config_path),
                    path_to_string(&normalized_path),
                )));
            }

            // If the paths match, update our local state and write
            // out the active configuration to disk.
            self.current_config_path = actual_config_path;
            return self.write_active_config();
        }

        // On non‑Windows platforms there is no registry to update.  We
        // simply update the current configuration path and write the
        // active configuration.  The write operation handles the
        // managed preset logic.
        #[cfg(not(target_os = "windows"))]
        {
            self.current_config_path = normalized_path;
            return self.write_active_config();
        }
    }

    pub fn create_group(&mut self, name: &str) -> Result<(), AppError> {
        let valid_name = validate_name(name)?;
        if self.group_index(&valid_name).is_some() {
            return Err(AppError::AlreadyExists(valid_name));
        }

        ensure_directory(&self.group_path(&valid_name))?;
        self.metadata.groups.push(GroupMetadata {
            name: valid_name,
            order: self.metadata.groups.len(),
            emoji: None,
            active_preset: None,
            presets: Vec::new(),
        });
        self.reindex_orders();
        self.persist_metadata()
    }

    pub fn rename_group(&mut self, old_name: &str, new_name: &str) -> Result<(), AppError> {
        let new_name = validate_name(new_name)?;
        let group_index = self
            .group_index(old_name)
            .ok_or_else(|| AppError::GroupNotFound(old_name.to_string()))?;
        if self.group_index(&new_name).is_some() {
            return Err(AppError::AlreadyExists(new_name));
        }

        fs::rename(self.group_path(old_name), self.group_path(&new_name))?;
        self.metadata.groups[group_index].name = new_name;
        self.persist_metadata()?;
        self.write_active_config()
    }

    pub fn set_group_emoji(
        &mut self,
        group_name: &str,
        emoji: Option<String>,
    ) -> Result<(), AppError> {
        let group_index = self
            .group_index(group_name)
            .ok_or_else(|| AppError::GroupNotFound(group_name.to_string()))?;

        let normalized_emoji = emoji.and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });

        self.metadata.groups[group_index].emoji = normalized_emoji;
        self.persist_metadata()
    }

    pub fn delete_group(&mut self, name: &str) -> Result<(), AppError> {
        let group_index = self
            .group_index(name)
            .ok_or_else(|| AppError::GroupNotFound(name.to_string()))?;
        let group_path = self.group_path(name);
        if group_path.exists() {
            fs::remove_dir_all(group_path)?;
        }

        self.metadata.groups.remove(group_index);
        self.reindex_orders();
        self.persist_metadata()?;
        self.write_active_config()
    }

    pub fn reorder_groups(&mut self, ordered_names: &[String]) -> Result<(), AppError> {
        let existing_names = self
            .metadata
            .groups
            .iter()
            .map(|group| group.name.clone())
            .collect::<Vec<_>>();
        if ordered_names.len() != existing_names.len()
            || ordered_names
                .iter()
                .any(|name| !existing_names.iter().any(|existing| existing == name))
        {
            return Err(AppError::Message(
                "The supplied group order does not match the existing groups.".to_string(),
            ));
        }

        self.metadata.groups.sort_by_key(|group| {
            ordered_names
                .iter()
                .position(|name| name == &group.name)
                .unwrap_or(usize::MAX)
        });
        self.reindex_orders();
        self.persist_metadata()?;
        self.write_active_config()
    }

    pub fn create_preset(
        &mut self,
        group_name: &str,
        preset_name: &str,
        content: Option<String>,
    ) -> Result<(), AppError> {
        let preset_name = validate_name(preset_name)?;
        let group_index = self
            .group_index(group_name)
            .ok_or_else(|| AppError::GroupNotFound(group_name.to_string()))?;

        if self.preset_index(group_index, &preset_name).is_some() {
            return Err(AppError::AlreadyExists(format!(
                "{group_name}/{preset_name}"
            )));
        }

        let order = self.metadata.groups[group_index].presets.len();
        write_text_file_atomically(
            &self.preset_path(group_name, &preset_name),
            content.unwrap_or_default().as_str(),
        )?;

        self.metadata.groups[group_index]
            .presets
            .push(PresetMetadata {
                name: preset_name,
                order,
            });
        self.reindex_orders();
        self.persist_metadata()
    }

    pub fn save_preset(
        &mut self,
        group_name: &str,
        preset_name: &str,
        content: &str,
    ) -> Result<(), AppError> {
        let preset_name = validate_name(preset_name)?;
        let group_index = self
            .group_index(group_name)
            .ok_or_else(|| AppError::GroupNotFound(group_name.to_string()))?;

        let active_before_change = self.metadata.groups[group_index].active_preset.as_deref()
            == Some(preset_name.as_str());
        let order = self.metadata.groups[group_index].presets.len();
        write_text_file_atomically(&self.preset_path(group_name, &preset_name), content)?;
        if self.preset_index(group_index, &preset_name).is_none() {
            self.metadata.groups[group_index]
                .presets
                .push(PresetMetadata {
                    name: preset_name,
                    order,
                });
        }

        self.reindex_orders();
        self.persist_metadata()?;
        if active_before_change {
            self.write_active_config()?;
        }
        Ok(())
    }

    pub fn rename_preset(
        &mut self,
        group_name: &str,
        old_name: &str,
        new_name: &str,
    ) -> Result<(), AppError> {
        let new_name = validate_name(new_name)?;
        let group_index = self
            .group_index(group_name)
            .ok_or_else(|| AppError::GroupNotFound(group_name.to_string()))?;
        let preset_index =
            self.preset_index(group_index, old_name)
                .ok_or_else(|| AppError::PresetNotFound {
                    group: group_name.to_string(),
                    name: old_name.to_string(),
                })?;

        if self.preset_index(group_index, &new_name).is_some() {
            return Err(AppError::AlreadyExists(format!("{group_name}/{new_name}")));
        }

        let old_preset_path = self.preset_path(group_name, old_name);
        let new_preset_path = self.preset_path(group_name, &new_name);
        let old_convolution_path = self.preset_convolution_path(group_name, old_name);
        let new_convolution_path = self.preset_convolution_path(group_name, &new_name);
        let existing_content = fs::read_to_string(&old_preset_path).unwrap_or_default();
        let had_convolution_copy = old_convolution_path.exists();

        fs::rename(&old_preset_path, &new_preset_path)?;
        if had_convolution_copy {
            fs::rename(&old_convolution_path, &new_convolution_path)?;
            let updated_content =
                replace_convolution_path(&existing_content, &new_convolution_path);
            write_text_file_atomically(&new_preset_path, updated_content.as_str())?;
        }

        self.metadata.groups[group_index].presets[preset_index].name = new_name.clone();
        if self.metadata.groups[group_index].active_preset.as_deref() == Some(old_name) {
            self.metadata.groups[group_index].active_preset = Some(new_name);
        }

        self.persist_metadata()?;
        self.write_active_config()
    }

    pub fn delete_preset(&mut self, group_name: &str, preset_name: &str) -> Result<(), AppError> {
        let group_index = self
            .group_index(group_name)
            .ok_or_else(|| AppError::GroupNotFound(group_name.to_string()))?;
        let preset_index = self.preset_index(group_index, preset_name).ok_or_else(|| {
            AppError::PresetNotFound {
                group: group_name.to_string(),
                name: preset_name.to_string(),
            }
        })?;

        let preset_path = self.preset_path(group_name, preset_name);
        let convolution_path = self.preset_convolution_path(group_name, preset_name);
        if preset_path.exists() {
            fs::remove_file(preset_path)?;
        }
        if convolution_path.exists() {
            fs::remove_file(convolution_path)?;
        }

        self.metadata.groups[group_index]
            .presets
            .remove(preset_index);
        if self.metadata.groups[group_index].active_preset.as_deref() == Some(preset_name) {
            self.metadata.groups[group_index].active_preset = None;
        }

        self.reindex_orders();
        self.persist_metadata()?;
        self.write_active_config()
    }

    pub fn move_preset(
        &mut self,
        old_group_name: &str,
        new_group_name: &str,
        preset_name: &str,
        target_index: Option<usize>,
    ) -> Result<(), AppError> {
        let old_group_index = self
            .group_index(old_group_name)
            .ok_or_else(|| AppError::GroupNotFound(old_group_name.to_string()))?;
        let new_group_index = self
            .group_index(new_group_name)
            .ok_or_else(|| AppError::GroupNotFound(new_group_name.to_string()))?;
        let preset_index = self
            .preset_index(old_group_index, preset_name)
            .ok_or_else(|| AppError::PresetNotFound {
                group: old_group_name.to_string(),
                name: preset_name.to_string(),
            })?;

        let preset_metadata = self.metadata.groups[old_group_index]
            .presets
            .remove(preset_index);
        let old_preset_path = self.preset_path(old_group_name, preset_name);
        let new_preset_path = self.preset_path(new_group_name, preset_name);
        let old_convolution_path = self.preset_convolution_path(old_group_name, preset_name);
        let new_convolution_path = self.preset_convolution_path(new_group_name, preset_name);
        let existing_content = fs::read_to_string(&old_preset_path).unwrap_or_default();
        let had_convolution_copy = old_convolution_path.exists();
        let was_active = self.metadata.groups[old_group_index]
            .active_preset
            .as_deref()
            == Some(preset_name);

        if old_group_name != new_group_name {
            if new_preset_path.exists() {
                return Err(AppError::AlreadyExists(format!(
                    "{new_group_name}/{preset_name}"
                )));
            }

            fs::rename(&old_preset_path, &new_preset_path)?;
            if had_convolution_copy {
                fs::rename(&old_convolution_path, &new_convolution_path)?;
                let updated_content =
                    replace_convolution_path(&existing_content, &new_convolution_path);
                write_text_file_atomically(&new_preset_path, updated_content.as_str())?;
            }
            if was_active {
                self.metadata.groups[old_group_index].active_preset = None;
                self.metadata.groups[new_group_index].active_preset = Some(preset_name.to_string());
            }
        }

        let mut target_slot =
            target_index.unwrap_or(self.metadata.groups[new_group_index].presets.len());
        if old_group_name == new_group_name && preset_index < target_slot {
            target_slot = target_slot.saturating_sub(1);
        }
        target_slot = target_slot.min(self.metadata.groups[new_group_index].presets.len());
        self.metadata.groups[new_group_index]
            .presets
            .insert(target_slot, preset_metadata);

        self.reindex_orders();
        self.persist_metadata()?;
        self.write_active_config()
    }

    pub fn export_app_settings(&mut self, destination: &Path) -> Result<(), AppError> {
        let mut snapshot = self.snapshot()?;
        populate_backup_convolution_bytes(&mut snapshot, &self.current_config_path);
        let payload = serde_json::to_string_pretty(&snapshot)?;
        write_text_file_atomically(destination, payload.as_str())
    }

    pub fn import_app_settings(&mut self, source: &Path) -> Result<(), AppError> {
        let payload = fs::read_to_string(source)?;
        let imported: PresetLibrary = serde_json::from_str(&payload)?;
        let imported_config_path = imported.config_path.trim().to_string();
        if imported_config_path.is_empty() {
            return Err(AppError::Message(
                "The imported backup is missing a config path.".to_string(),
            ));
        }

        let mut rebuilt_groups = Vec::with_capacity(imported.groups.len());
        for (group_order, group) in imported.groups.iter().enumerate() {
            let group_name = validate_name(&group.name)?;
            let normalized_emoji = group.emoji.as_ref().and_then(|value| {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            });

            let mut rebuilt_presets = Vec::with_capacity(group.presets.len());
            for (preset_order, preset) in group.presets.iter().enumerate() {
                let preset_name = validate_name(&preset.name)?;
                rebuilt_presets.push(PresetMetadata {
                    name: preset_name,
                    order: preset_order,
                });
            }

            let active_preset = group.active_preset.as_ref().and_then(|value| {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    return None;
                }

                if rebuilt_presets.iter().any(|preset| preset.name == trimmed) {
                    Some(trimmed.to_string())
                } else {
                    None
                }
            });

            rebuilt_groups.push(GroupMetadata {
                name: group_name,
                order: group_order,
                emoji: normalized_emoji,
                active_preset,
                presets: rebuilt_presets,
            });
        }

        let staging_dir = self.app_data_dir.join("presets.importing");
        let backup_dir = self.app_data_dir.join("presets.backup");
        if staging_dir.exists() {
            fs::remove_dir_all(&staging_dir)?;
        }
        if backup_dir.exists() {
            fs::remove_dir_all(&backup_dir)?;
        }
        fs::create_dir_all(&staging_dir)?;

        for group in &imported.groups {
            let group_name = validate_name(&group.name)?;
            let group_dir = staging_dir.join(&group_name);
            fs::create_dir_all(&group_dir)?;

            for preset in &group.presets {
                let preset_name = validate_name(&preset.name)?;
                let preset_path = group_dir.join(format!("{preset_name}.txt"));
                let convolution_path = self.preset_convolution_path(&group_name, &preset_name);
                let mut content_to_write = preset.content.clone();

                if let Some(convolution) = preset.convolution.as_ref() {
                    let mut restored_bytes: Option<Vec<u8>> = None;
                    if let Some(wav_base64) = convolution.wav_base64.as_ref() {
                        restored_bytes = STANDARD.decode(wav_base64.as_bytes()).ok();
                    }

                    if restored_bytes.is_none() && !convolution.wav_path.trim().is_empty() {
                        let imported_reference = resolve_import_convolution_reference(
                            &imported.config_path,
                            Path::new(convolution.wav_path.as_str()),
                        );
                        if imported_reference.exists() && imported_reference.is_file() {
                            restored_bytes = fs::read(imported_reference).ok();
                        }
                    }

                    if let Some(bytes) = restored_bytes {
                        let staged_convolution_path = group_dir.join(format!("{preset_name}.wav"));
                        write_binary_file_atomically(&staged_convolution_path, bytes.as_slice())?;
                        content_to_write =
                            replace_convolution_path(&content_to_write, &convolution_path);
                    }
                }

                write_text_file_atomically(&preset_path, content_to_write.as_str())?;
            }
        }

        if self.presets_dir.exists() {
            if let Err(error) = fs::rename(&self.presets_dir, &backup_dir) {
                let _ = fs::remove_dir_all(&staging_dir);
                return Err(error.into());
            }
        }

        if let Err(error) = fs::rename(&staging_dir, &self.presets_dir) {
            if backup_dir.exists() {
                let _ = fs::rename(&backup_dir, &self.presets_dir);
            }
            let _ = fs::remove_dir_all(&staging_dir);
            return Err(error.into());
        }

        if backup_dir.exists() {
            let _ = fs::remove_dir_all(&backup_dir);
        }

        self.metadata.groups = rebuilt_groups;
        self.metadata.config_path_prompted = imported.config_path_prompted;
        self.reindex_orders();
        self.persist_metadata()?;
        self.set_config_path(PathBuf::from(imported_config_path))
    }

    pub fn apply_preset(&mut self, group_name: &str, preset_name: &str) -> Result<(), AppError> {
        let group_index = self
            .group_index(group_name)
            .ok_or_else(|| AppError::GroupNotFound(group_name.to_string()))?;
        if self.preset_index(group_index, preset_name).is_none() {
            return Err(AppError::PresetNotFound {
                group: group_name.to_string(),
                name: preset_name.to_string(),
            });
        }

        self.clear_active_selection();
        self.metadata.groups[group_index].active_preset = Some(preset_name.to_string());
        self.metadata.eq_disabled = false;
        self.persist_metadata()?;
        self.write_active_config()
    }

    pub fn import_presets(&mut self, group_name: &str, paths: &[String]) -> Result<(), AppError> {
        let group_index = self
            .group_index(group_name)
            .ok_or_else(|| AppError::GroupNotFound(group_name.to_string()))?;

        for raw_path in paths {
            let file_path = PathBuf::from(raw_path);
            let base_name = file_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(sanitize_import_name)
                .unwrap_or_else(|| "Imported Preset".to_string());
            let unique_name = self.unique_preset_name(group_index, &base_name);
            let content = match file_path
                .extension()
                .and_then(|extension| extension.to_str())
                .map(|value| value.to_ascii_lowercase())
                .as_deref()
            {
                Some("wav") => {
                    let convolution_path = self.preset_convolution_path(group_name, &unique_name);
                    let bytes = fs::read(&file_path)?;
                    write_binary_file_atomically(&convolution_path, bytes.as_slice())?;
                    build_convolution_preset_content(&convolution_path)
                }
                _ => fs::read_to_string(&file_path)?,
            };
            write_text_file_atomically(
                &self.preset_path(group_name, &unique_name),
                content.as_str(),
            )?;

            let order = self.metadata.groups[group_index].presets.len();
            self.metadata.groups[group_index]
                .presets
                .push(PresetMetadata {
                    name: unique_name,
                    order,
                });
        }

        self.reindex_orders();
        self.persist_metadata()
    }

    pub fn attach_convolution_wav(
        &mut self,
        group_name: &str,
        preset_name: &str,
        content: &str,
        source_path: &Path,
    ) -> Result<(), AppError> {
        let group_index = self
            .group_index(group_name)
            .ok_or_else(|| AppError::GroupNotFound(group_name.to_string()))?;
        if self.preset_index(group_index, preset_name).is_none() {
            return Err(AppError::PresetNotFound {
                group: group_name.to_string(),
                name: preset_name.to_string(),
            });
        }

        let normalized_source = normalize_path(source_path);
        if !normalized_source.exists() || !normalized_source.is_file() {
            return Err(AppError::Message(format!(
                "The file or folder does not exist: {}",
                normalized_source.display()
            )));
        }

        let convolution_path = self.preset_convolution_path(group_name, preset_name);
        let source_matches_target = normalize_path(&convolution_path) == normalized_source;
        if !source_matches_target {
            let bytes = fs::read(&normalized_source)?;
            write_binary_file_atomically(&convolution_path, bytes.as_slice())?;
        }

        let updated_content = replace_convolution_path(content, &convolution_path);
        write_text_file_atomically(
            &self.preset_path(group_name, preset_name),
            updated_content.as_str(),
        )?;
        self.persist_metadata()?;
        self.write_active_config()
    }

    pub fn remove_convolution_wav(
        &mut self,
        group_name: &str,
        preset_name: &str,
        content: &str,
    ) -> Result<(), AppError> {
        let group_index = self
            .group_index(group_name)
            .ok_or_else(|| AppError::GroupNotFound(group_name.to_string()))?;
        if self.preset_index(group_index, preset_name).is_none() {
            return Err(AppError::PresetNotFound {
                group: group_name.to_string(),
                name: preset_name.to_string(),
            });
        }

        let convolution_path = self.preset_convolution_path(group_name, preset_name);
        let updated_content = remove_convolution_path(content);
        if convolution_path.exists() {
            fs::remove_file(convolution_path)?;
        }
        write_text_file_atomically(
            &self.preset_path(group_name, preset_name),
            updated_content.as_str(),
        )?;
        self.persist_metadata()?;
        self.write_active_config()
    }

    pub fn export_preset(
        &self,
        group_name: &str,
        preset_name: &str,
        destination: &Path,
    ) -> Result<(), AppError> {
        let group_index = self
            .group_index(group_name)
            .ok_or_else(|| AppError::GroupNotFound(group_name.to_string()))?;
        if self.preset_index(group_index, preset_name).is_none() {
            return Err(AppError::PresetNotFound {
                group: group_name.to_string(),
                name: preset_name.to_string(),
            });
        }

        let content = fs::read_to_string(self.preset_path(group_name, preset_name))?;
        write_text_file_atomically(destination, content.as_str())
    }

    fn sync_metadata_with_disk(&mut self) -> Result<(), AppError> {
        let mut ordered_groups = Vec::new();
        let disk_group_names = list_group_names(&self.presets_dir)?;

        for group in &self.metadata.groups {
            if disk_group_names.contains(&group.name) {
                ordered_groups.push(group.name.clone());
            }
        }
        for group_name in disk_group_names {
            if !ordered_groups.contains(&group_name) {
                ordered_groups.push(group_name);
            }
        }

        let mut rebuilt_groups = Vec::new();
        for (group_order, group_name) in ordered_groups.iter().enumerate() {
            let old_group = self
                .metadata
                .groups
                .iter()
                .find(|group| group.name == *group_name);
            let disk_presets = list_preset_names(&self.group_path(group_name))?;
            let mut ordered_presets = Vec::new();

            if let Some(old_group) = old_group {
                for preset in &old_group.presets {
                    if disk_presets.contains(&preset.name) {
                        ordered_presets.push(preset.name.clone());
                    }
                }
            }
            for preset_name in disk_presets {
                if !ordered_presets.contains(&preset_name) {
                    ordered_presets.push(preset_name);
                }
            }

            let active_preset = old_group
                .and_then(|group| group.active_preset.clone())
                .filter(|active| ordered_presets.contains(active));
            let emoji = old_group.and_then(|group| group.emoji.clone());

            rebuilt_groups.push(GroupMetadata {
                name: group_name.clone(),
                order: group_order,
                emoji,
                active_preset,
                presets: ordered_presets
                    .into_iter()
                    .enumerate()
                    .map(|(preset_order, preset_name)| PresetMetadata {
                        name: preset_name,
                        order: preset_order,
                    })
                    .collect(),
            });
        }

        self.metadata.groups = rebuilt_groups;
        self.normalize_single_active_selection();
        Ok(())
    }

    fn persist_metadata(&mut self) -> Result<(), AppError> {
        self.normalize_single_active_selection();
        self.reindex_orders();
        let payload = serde_json::to_string_pretty(&self.metadata)?;
        write_text_file_atomically(&self.metadata_path, payload.as_str())
    }

    fn reindex_orders(&mut self) {
        for (group_order, group) in self.metadata.groups.iter_mut().enumerate() {
            group.order = group_order;
            for (preset_order, preset) in group.presets.iter_mut().enumerate() {
                preset.order = preset_order;
            }
        }
    }

    pub(crate) fn write_active_config(&mut self) -> Result<(), AppError> {
        let config_txt_path = self.current_config_path.join("config.txt");
        let managed_preset_path = self.managed_live_preset_path();
        let managed_preset_payload = self.build_managed_preset_payload()?;
        let existing_config = fs::read_to_string(&config_txt_path).unwrap_or_default();
        let updated_config = build_config_with_managed_include(
            existing_config.as_str(),
            self.managed_include_path().as_str(),
        );

        self.write_live_config_files(
            &config_txt_path,
            updated_config.as_str(),
            &managed_preset_path,
            managed_preset_payload.as_str(),
        )
    }

    fn is_config_path_writable(&self) -> Result<bool, AppError> {
        is_directory_writable(&self.current_config_path)
    }

    fn group_index(&self, name: &str) -> Option<usize> {
        self.metadata
            .groups
            .iter()
            .position(|group| group.name == name)
    }

    fn preset_index(&self, group_index: usize, name: &str) -> Option<usize> {
        self.metadata.groups[group_index]
            .presets
            .iter()
            .position(|preset| preset.name == name)
    }

    fn group_path(&self, group_name: &str) -> PathBuf {
        self.presets_dir.join(group_name)
    }

    fn preset_path(&self, group_name: &str, preset_name: &str) -> PathBuf {
        self.group_path(group_name)
            .join(format!("{preset_name}.txt"))
    }

    fn unique_preset_name(&self, group_index: usize, base_name: &str) -> String {
        if self.preset_index(group_index, base_name).is_none() {
            return base_name.to_string();
        }

        let mut suffix = 2usize;
        loop {
            let candidate = format!("{base_name} {suffix}");
            if self.preset_index(group_index, &candidate).is_none() {
                return candidate;
            }
            suffix += 1;
        }
    }

    fn clear_active_selection(&mut self) {
        for group in &mut self.metadata.groups {
            group.active_preset = None;
        }
    }

    fn normalize_single_active_selection(&mut self) {
        let mut active_seen = false;
        for group in &mut self.metadata.groups {
            if group.active_preset.is_some() {
                if active_seen {
                    group.active_preset = None;
                } else {
                    active_seen = true;
                }
            }
        }
    }

    pub fn is_eq_disabled(&self) -> bool {
        self.metadata.eq_disabled
    }

    pub fn set_eq_disabled(&mut self, disabled: bool) -> Result<(), AppError> {
        self.metadata.eq_disabled = disabled;
        if disabled {
            self.clear_active_selection();
        }
        self.persist_metadata()
    }

    fn active_selection(&self) -> Option<(String, String)> {
        self.metadata.groups.iter().find_map(|group| {
            group
                .active_preset
                .as_ref()
                .map(|preset| (group.name.clone(), preset.clone()))
        })
    }

    fn include_path_for_preset(&self, preset_path: &Path) -> String {
        if let Some(relative) = relative_path(&self.current_config_path, preset_path) {
            return path_to_string(&relative);
        }

        path_to_string(preset_path)
    }

    fn managed_live_directory(&self) -> PathBuf {
        self.current_config_path.join(MANAGED_CONFIG_DIR_NAME)
    }

    fn managed_live_preset_path(&self) -> PathBuf {
        self.managed_live_directory()
            .join(MANAGED_ACTIVE_PRESET_FILE_NAME)
    }

    fn managed_include_path(&self) -> String {
        let managed_path = self.managed_live_preset_path();
        self.include_path_for_preset(&managed_path)
    }

    fn build_managed_preset_payload(&self) -> Result<String, AppError> {
        let payload = if let Some((group_name, preset_name)) = self.active_selection() {
            let active_path = self.preset_path(group_name.as_str(), preset_name.as_str());
            if active_path.exists() {
                let preset_content = fs::read_to_string(active_path)?;
                format!(
                    "# Generated by SmartEQPresetSwitcher\r\n# Active preset: {} / {}\r\n\r\n{}",
                    group_name,
                    preset_name,
                    normalize_windows_newlines(preset_content.as_str()),
                )
            } else {
                "# Generated by SmartEQPresetSwitcher\r\n# No active preset selected.\r\n"
                    .to_string()
            }
        } else {
            "# Generated by SmartEQPresetSwitcher\r\n# No active preset selected.\r\n"
                .to_string()
        };

        Ok(ensure_trailing_newline(payload.as_str()))
    }

    fn write_live_config_files(
        &self,
        config_txt_path: &Path,
        config_txt_content: &str,
        managed_preset_path: &Path,
        managed_preset_content: &str,
    ) -> Result<(), AppError> {
        match write_live_config_files_direct(
            config_txt_path,
            config_txt_content,
            managed_preset_path,
            managed_preset_content,
        ) {
            Ok(()) => Ok(()),
            Err(AppError::Io(error)) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                #[cfg(target_os = "windows")]
                {
                    return run_elevated_live_config_helper(
                        &self.app_data_dir,
                        config_txt_path,
                        config_txt_content,
                        managed_preset_path,
                        managed_preset_content,
                    );
                }

                #[cfg(not(target_os = "windows"))]
                {
                    Err(AppError::Io(error))
                }
            }
            Err(error) => Err(error),
        }
    }
}

pub fn try_handle_cli_mode() -> Option<i32> {
    let mut args = env::args().skip(1);
    let command = args.next()?;
    match command.as_str() {
        "--elevated-set-config-path" => {
            let Some(path) = args.next() else {
                return Some(1);
            };

            let exit_code = match write_registry_config_path(Path::new(&path)) {
                Ok(()) => 0,
                Err(error) => {
                    append_log_line("ERROR", error.to_string());
                    1
                }
            };
            Some(exit_code)
        }
        "--elevated-write-live-config" => {
            let Some(staged_config_path) = args.next() else {
                return Some(1);
            };
            let Some(config_txt_path) = args.next() else {
                return Some(1);
            };
            let Some(staged_preset_path) = args.next() else {
                return Some(1);
            };
            let Some(managed_preset_path) = args.next() else {
                return Some(1);
            };

            let exit_code = match write_elevated_live_config(
                Path::new(&staged_config_path),
                Path::new(&config_txt_path),
                Path::new(&staged_preset_path),
                Path::new(&managed_preset_path),
            ) {
                Ok(()) => 0,
                Err(error) => {
                    append_log_line("ERROR", error.to_string());
                    1
                }
            };
            Some(exit_code)
        }
        _ => None,
    }
}

fn migrate_legacy_app_data_dir(config_root: &Path, app_data_dir: &Path) -> Result<(), AppError> {
    let legacy_dir = config_root.join(LEGACY_APP_FOLDER_NAME);
    if app_data_dir.exists() || !legacy_dir.exists() {
        return Ok(());
    }

    match fs::rename(&legacy_dir, app_data_dir) {
        Ok(()) => {
            append_log_line(
                "INFO",
                format!(
                    "Migrated legacy app data from '{}' to '{}'.",
                    legacy_dir.display(),
                    app_data_dir.display()
                ),
            );
            Ok(())
        }
        Err(rename_error) => {
            append_log_line(
                "WARN",
                format!(
                    "Unable to rename legacy app data directory, trying recursive copy instead: {rename_error}"
                ),
            );
            copy_dir_recursive(&legacy_dir, app_data_dir)?;
            Ok(())
        }
    }
}

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<(), AppError> {
    ensure_directory(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else {
            if let Some(parent) = target_path.parent() {
                ensure_directory(parent)?;
            }
            fs::copy(&source_path, &target_path)?;
        }
    }
    Ok(())
}

fn load_metadata(metadata_path: &Path) -> Result<PresetsMetadata, AppError> {
    if !metadata_path.exists() {
        return Ok(PresetsMetadata::default());
    }

    let payload = fs::read_to_string(metadata_path)?;
    Ok(serde_json::from_str(&payload)?)
}

#[cfg(target_os = "windows")]
fn read_registry_config_path() -> Result<PathBuf, AppError> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = hklm.open_subkey_with_flags(REGISTRY_KEY_PATH, KEY_READ | KEY_WOW64_64KEY)?;
    let value: String = key
        .get_value(REGISTRY_VALUE_NAME)
        .map_err(|_| AppError::RegistryValueMissing)?;
    Ok(PathBuf::from(value))
}

fn detect_installed_config_path() -> Option<PathBuf> {
    let install_path = detect_installed_install_path()?;
    let config_path = install_path.join("config");
    config_path.exists().then_some(config_path)
}

#[cfg(target_os = "windows")]
pub(crate) fn detect_installed_install_path() -> Option<PathBuf> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = hklm
        .open_subkey_with_flags(REGISTRY_KEY_PATH, KEY_READ | KEY_WOW64_64KEY)
        .ok()?;
    let install_path: String = key.get_value(REGISTRY_INSTALL_PATH_VALUE_NAME).ok()?;
    Some(PathBuf::from(install_path))
}

#[cfg(target_os = "windows")]
fn write_registry_config_path(path: &Path) -> Result<(), std::io::Error> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = hklm.open_subkey_with_flags(REGISTRY_KEY_PATH, KEY_SET_VALUE | KEY_WOW64_64KEY)?;
    key.set_value(REGISTRY_VALUE_NAME, &path_to_string(path))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn run_elevated_registry_helper(path: &Path) -> Result<(), AppError> {
    run_elevated_cli(&[
        "--elevated-set-config-path".to_string(),
        path_to_string(path),
    ])
}

#[cfg(target_os = "windows")]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(target_os = "windows")]
fn run_elevated_live_config_helper(
    app_data_dir: &Path,
    config_txt_path: &Path,
    config_txt_content: &str,
    managed_preset_path: &Path,
    managed_preset_content: &str,
) -> Result<(), AppError> {
    let token = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let staged_config_path = app_data_dir.join(format!("live-config-{token}.txt"));
    let staged_preset_path = app_data_dir.join(format!("live-preset-{token}.txt"));

    write_text_file_atomically(&staged_config_path, config_txt_content)?;
    write_text_file_atomically(&staged_preset_path, managed_preset_content)?;

    let result = run_elevated_cli(&[
        "--elevated-write-live-config".to_string(),
        path_to_string(&staged_config_path),
        path_to_string(config_txt_path),
        path_to_string(&staged_preset_path),
        path_to_string(managed_preset_path),
    ]);

    let _ = fs::remove_file(staged_config_path);
    let _ = fs::remove_file(staged_preset_path);
    result
}

// The following stub implementations provide non‑Windows platforms with
// fallbacks for registry and elevation helpers.  They either
// silently no‑op or return sensible errors.  See the Windows
// versions above for the actual implementations.

#[cfg(not(target_os = "windows"))]
fn read_registry_config_path() -> Result<PathBuf, AppError> {
    Err(AppError::RegistryValueMissing)
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn detect_installed_install_path() -> Option<PathBuf> {
    None
}

#[cfg(not(target_os = "windows"))]
fn write_registry_config_path(_path: &Path) -> Result<(), std::io::Error> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "Windows registry config path is not supported on this platform.",
    ))
}

#[cfg(any(target_os = "windows", test))]
pub(crate) fn classify_elevation_failure(
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
    fallback_message: String,
) -> AppError {
    if is_user_declined_elevation(exit_code, stdout, stderr) {
        return AppError::ElevationDeclined;
    }

    if let Some(message) = combine_shell_output(stdout, stderr) {
        return AppError::Message(message);
    }

    AppError::Message(fallback_message)
}

#[cfg(target_os = "windows")]
pub(crate) fn run_elevated_process(file_path: &Path, arguments: &[String]) -> Result<(), AppError> {
    let escaped_arguments = arguments
        .iter()
        .map(|value| format!("'\"{}\"'", escape_for_powershell(value)))
        .collect::<Vec<_>>()
        .join(", ");
    let command = format!(
        "$process = Start-Process -FilePath '{}' -ArgumentList @({}) -Verb RunAs -Wait -PassThru -ErrorAction Stop; exit $process.ExitCode",
        escape_for_powershell(file_path.as_os_str().to_string_lossy().as_ref()),
        escaped_arguments,
    );

    // Try PowerShell 7+ first (pwsh.exe), then fall back to Windows PowerShell (powershell.exe)
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
                    "The elevated PowerShell command '{}' exited with code {}.",
                    file_path.display(),
                    output
                        .status
                        .code()
                        .map_or_else(|| "unknown".to_string(), |code| code.to_string())
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
                // Continue to next shell
            }
        }
    }

    // If we get here, both shells failed
    Err(last_error.map_or_else(
        || AppError::Message("Unable to start an elevated PowerShell shell.".to_string()),
        |error| error.into(),
    ))
}

#[cfg(target_os = "windows")]
fn run_elevated_cli(arguments: &[String]) -> Result<(), AppError> {
    let current_exe = env::current_exe()?;
    run_elevated_process(current_exe.as_path(), arguments)
}

#[cfg(target_os = "windows")]
fn write_elevated_live_config(
    staged_config_path: &Path,
    config_txt_path: &Path,
    staged_preset_path: &Path,
    managed_preset_path: &Path,
) -> Result<(), AppError> {
    let config_content = fs::read_to_string(staged_config_path)?;
    let preset_content = fs::read_to_string(staged_preset_path)?;
    write_live_config_files_direct(
        config_txt_path,
        config_content.as_str(),
        managed_preset_path,
        preset_content.as_str(),
    )
}

#[cfg(not(target_os = "windows"))]
fn write_elevated_live_config(
    _staged_config_path: &Path,
    _config_txt_path: &Path,
    _staged_preset_path: &Path,
    _managed_preset_path: &Path,
) -> Result<(), AppError> {
    Err(AppError::Message(
        "Elevated live config writes are only supported on Windows.".to_string(),
    ))
}

fn list_group_names(presets_dir: &Path) -> Result<Vec<String>, AppError> {
    let mut names = fs::read_dir(presets_dir)?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect::<Vec<_>>();
    names.sort_unstable();
    Ok(names)
}

fn list_preset_names(group_dir: &Path) -> Result<Vec<String>, AppError> {
    if !group_dir.exists() {
        return Ok(Vec::new());
    }

    let mut names = fs::read_dir(group_dir)?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("txt"))
        .filter_map(|entry| {
            entry
                .path()
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(|value| value.to_string())
        })
        .collect::<Vec<_>>();
    names.sort_unstable();
    Ok(names)
}

fn validate_name(name: &str) -> Result<String, AppError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidName(name.to_string()));
    }

    let invalid = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    if trimmed
        .chars()
        .any(|character| invalid.contains(&character))
    {
        return Err(AppError::InvalidName(trimmed.to_string()));
    }

    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if reserved
        .iter()
        .any(|reserved_name| reserved_name.eq_ignore_ascii_case(trimmed))
    {
        return Err(AppError::InvalidName(trimmed.to_string()));
    }

    Ok(trimmed.to_string())
}

fn sanitize_import_name(name: &str) -> String {
    let cleaned = name
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => ' ',
            other => other,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    if cleaned.trim().is_empty() {
        "Imported Preset".to_string()
    } else {
        cleaned
    }
}

fn format_convolution_path(path: &Path) -> String {
    path_to_string(path).replace('"', "\\\"")
}

fn is_convolution_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    let Some((key, _)) = trimmed.split_once(':') else {
        return false;
    };

    key.trim().eq_ignore_ascii_case("Convolution")
}

fn extract_convolution_path(content: &str) -> Option<String> {
    content.lines().find_map(|line| {
        if !is_convolution_line(line) {
            return None;
        }

        let (_, value) = line.trim_start().split_once(':')?;
        let trimmed = strip_wrapping_quotes(value);
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn replace_convolution_path(content: &str, wav_path: &Path) -> String {
    let normalized = normalize_windows_newlines(content);
    let replacement = format!("Convolution: \"{}\"", format_convolution_path(wav_path));
    let mut lines = Vec::new();
    let mut replaced = false;

    for line in normalized.split("\r\n") {
        if !replaced && is_convolution_line(line) {
            let indent_len = line.len().saturating_sub(line.trim_start().len());
            let indent = &line[..indent_len];
            lines.push(format!("{indent}{replacement}"));
            replaced = true;
        } else {
            lines.push(line.to_string());
        }
    }

    if !replaced {
        let mut appended = normalized;
        if !appended.is_empty() && !appended.ends_with("\r\n") {
            appended.push_str("\r\n");
        }
        appended.push_str(&replacement);
        appended.push_str("\r\n");
        return appended;
    }

    let joined = lines.join("\r\n");
    if joined.is_empty() {
        joined
    } else {
        ensure_trailing_newline(joined.as_str())
    }
}

fn remove_convolution_path(content: &str) -> String {
    let normalized = normalize_windows_newlines(content);
    let lines = normalized
        .split("\r\n")
        .filter(|line| !is_convolution_line(line))
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    let joined = lines.join("\r\n");
    if joined.is_empty() {
        joined
    } else {
        ensure_trailing_newline(joined.as_str())
    }
}

fn strip_wrapping_quotes(value: &str) -> String {
    let trimmed = value.trim();

    if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
    {
        trimmed[1..trimmed.len().saturating_sub(1)]
            .trim()
            .to_string()
    } else {
        trimmed.to_string()
    }
}

fn build_convolution_preset_content(wav_path: &Path) -> String {
    format!("Convolution: \"{}\"\r\n", format_convolution_path(wav_path))
}

fn populate_backup_convolution_bytes(snapshot: &mut PresetLibrary, current_config_path: &Path) {
    for group in &mut snapshot.groups {
        for preset in &mut group.presets {
            let Some(convolution) = preset.convolution.as_mut() else {
                continue;
            };

            if convolution.error.is_some() || convolution.wav_base64.is_some() {
                continue;
            }

            if convolution.wav_path.trim().is_empty() {
                continue;
            }

            let referenced_path = Path::new(convolution.wav_path.as_str());
            let resolved_path = if referenced_path.is_absolute() {
                referenced_path.to_path_buf()
            } else {
                current_config_path.join(referenced_path)
            };

            match fs::read(&resolved_path) {
                Ok(bytes) => {
                    convolution.wav_base64 = Some(STANDARD.encode(bytes));
                }
                Err(error) => {
                    convolution.error = Some(error.to_string());
                }
            }
        }
    }
}

fn resolve_import_convolution_reference(config_path: &str, wav_path: &Path) -> PathBuf {
    if wav_path.is_absolute() {
        wav_path.to_path_buf()
    } else {
        PathBuf::from(config_path).join(wav_path)
    }
}

fn write_binary_file_atomically(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        ensure_directory(parent)?;
    }

    let temporary_path = path.with_extension("tmp");
    {
        let mut file = File::create(&temporary_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }

    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(temporary_path, path)?;
    Ok(())
}

fn ensure_directory(path: &Path) -> Result<(), AppError> {
    fs::create_dir_all(path)?;
    Ok(())
}

/// Validates and normalizes a config path to prevent issues
fn validate_and_normalize_path(path: &Path) -> Result<PathBuf, AppError> {
    // Must be an absolute path
    if !path.is_absolute() {
        return Err(AppError::Message(
            "Config path must be an absolute path (e.g., C:\\folder\\config)".to_string(),
        ));
    }

    // Must have a valid extension or be a directory path
    let normalized = normalize_path(path);

    // Check that path doesn't contain invalid characters for Windows
    let path_str = path_to_string(&normalized);
    if path_str.contains('<')
        || path_str.contains('>')
        || path_str.contains('|')
        || path_str.contains('?')
        || path_str.contains('*')
    {
        return Err(AppError::Message(
            "Config path contains invalid characters".to_string(),
        ));
    }

    Ok(normalized)
}

/// Normalizes a path by removing trailing separators and standardizing format
fn normalize_path(path: &Path) -> PathBuf {
    // Convert to string and clean up
    let mut path_str = path.to_string_lossy().to_string();

    // Remove trailing slashes/backslashes
    while path_str.ends_with('\\') || path_str.ends_with('/') {
        path_str.pop();
    }

    PathBuf::from(path_str)
}

fn write_live_config_files_direct(
    config_txt_path: &Path,
    config_txt_content: &str,
    managed_preset_path: &Path,
    managed_preset_content: &str,
) -> Result<(), AppError> {
    if let Some(parent) = config_txt_path.parent() {
        ensure_directory(parent)?;
    }
    if let Some(parent) = managed_preset_path.parent() {
        ensure_directory(parent)?;
    }

    write_text_file_atomically(config_txt_path, config_txt_content)?;
    write_text_file_atomically(managed_preset_path, managed_preset_content)
}

fn is_directory_writable(path: &Path) -> Result<bool, AppError> {
    if let Err(error) = fs::create_dir_all(path) {
        return if error.kind() == std::io::ErrorKind::PermissionDenied {
            Ok(false)
        } else {
            Err(error.into())
        };
    }

    let probe = path.join(".write-test.tmp");
    match File::create(&probe) {
        Ok(mut file) => {
            file.write_all(b"probe")?;
            file.sync_all()?;
            fs::remove_file(probe)?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn write_text_file_atomically(path: &Path, content: &str) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        ensure_directory(parent)?;
    }

    let temporary_path = path.with_extension("tmp");
    {
        let mut file = File::create(&temporary_path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
    }

    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(temporary_path, path)?;
    Ok(())
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn build_config_with_managed_include(existing_config: &str, include_path: &str) -> String {
    let managed_block = format!(
        "{MANAGED_BLOCK_START}\r\nInclude: {}\r\n{MANAGED_BLOCK_END}",
        include_path
    );
    let normalized_existing = normalize_windows_newlines(existing_config);
    let normalized_block = normalize_windows_newlines(managed_block.as_str());

    if let Some(updated) =
        replace_existing_managed_block(normalized_existing.as_str(), normalized_block.as_str())
    {
        return ensure_trailing_newline(updated.as_str());
    }

    if is_legacy_managed_config(normalized_existing.as_str()) {
        return ensure_trailing_newline(normalized_block.as_str());
    }

    // Replace the entire config.txt with just the managed block.
    // This prevents default APO EQ settings from stacking with the preset.
    ensure_trailing_newline(normalized_block.as_str())
}

fn replace_existing_managed_block(existing_config: &str, managed_block: &str) -> Option<String> {
    // If the managed block markers exist, replace the entire managed config
    // block with the current brand marker.  Old SmartEqualizerAPOPresetsManager
    // markers are intentionally supported so existing user configs migrate
    // cleanly.
    if existing_config.find(MANAGED_BLOCK_START).is_some()
        || existing_config.find(MANAGED_BLOCK_END).is_some()
        || existing_config.find(LEGACY_MANAGED_BLOCK_START).is_some()
        || existing_config.find(LEGACY_MANAGED_BLOCK_END).is_some()
    {
        Some(managed_block.to_string())
    } else {
        None
    }
}

fn is_legacy_managed_config(existing_config: &str) -> bool {
    let trimmed = existing_config.trim();
    (trimmed.starts_with("# SmartEQPresetSwitcher")
        || trimmed.starts_with("# SmartEqualizerAPOPresetsManager"))
        && (trimmed.contains("# Active preset:")
            || trimmed.contains("# No active preset selected."))
}

fn normalize_windows_newlines(value: &str) -> String {
    value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "\r\n")
}

fn ensure_trailing_newline(value: &str) -> String {
    let normalized = normalize_windows_newlines(value);
    if normalized.ends_with("\r\n") {
        normalized
    } else {
        format!("{normalized}\r\n")
    }
}

fn relative_path(from_dir: &Path, target: &Path) -> Option<PathBuf> {
    use std::path::Component;

    let from_components = from_dir.components().collect::<Vec<_>>();
    let target_components = target.components().collect::<Vec<_>>();
    if from_components.is_empty() || target_components.is_empty() {
        return None;
    }

    let mut common_len = 0usize;
    while common_len < from_components.len()
        && common_len < target_components.len()
        && from_components[common_len] == target_components[common_len]
    {
        common_len += 1;
    }

    let same_root = matches!(
        (from_components.first(), target_components.first()),
        (Some(Component::Prefix(a)), Some(Component::Prefix(b))) if a == b
    ) || matches!(
        (from_components.first(), target_components.first()),
        (Some(Component::RootDir), Some(Component::RootDir))
    );

    if !same_root {
        return None;
    }

    let mut relative = PathBuf::new();
    for component in &from_components[common_len..] {
        match component {
            Component::Normal(_) | Component::CurDir | Component::ParentDir => {
                relative.push("..");
            }
            Component::Prefix(_) | Component::RootDir => {}
        }
    }

    for component in &target_components[common_len..] {
        relative.push(component.as_os_str());
    }

    Some(relative)
}

#[cfg(target_os = "windows")]
fn escape_for_powershell(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(any(target_os = "windows", test))]
fn combine_shell_output(stdout: &str, stderr: &str) -> Option<String> {
    let mut parts = Vec::new();
    let stderr = stderr.trim();
    if !stderr.is_empty() {
        parts.push(stderr.to_string());
    }

    let stdout = stdout.trim();
    if !stdout.is_empty() {
        parts.push(stdout.to_string());
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

#[cfg(any(target_os = "windows", test))]
fn is_user_declined_elevation(exit_code: Option<i32>, stdout: &str, stderr: &str) -> bool {
    if exit_code == Some(1223) {
        return true;
    }

    let combined = format!("{stderr}\n{stdout}").to_ascii_lowercase();
    combined.contains("canceled by the user")
        || combined.contains("cancelled by the user")
        || combined.contains("operation was canceled")
        || combined.contains("operation was cancelled")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_elevation_failure_should_preserve_stderr() {
        let error = classify_elevation_failure(
            Some(1),
            "",
            "Start-Process failed with an unexpected error.",
            "fallback".to_string(),
        );

        match error {
            AppError::Message(message) => {
                assert_eq!(message, "Start-Process failed with an unexpected error.");
            }
            other => panic!("expected message error, got {other:?}"),
        }
    }

    #[test]
    fn classify_elevation_failure_should_treat_cancelled_prompt_as_declined() {
        let error = classify_elevation_failure(
            Some(1),
            "",
            "The operation was canceled by the user.",
            "fallback".to_string(),
        );

        assert!(matches!(error, AppError::ElevationDeclined));
    }
}
