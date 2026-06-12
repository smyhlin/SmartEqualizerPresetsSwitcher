mod autoeq;
mod commands;
mod logging;
mod state;

// Expose additional modules publicly so that they can be called
// directly from the binary (e.g. for boot‑sync and autorun when
// running headless).
pub mod autorun;
pub mod linux_eq;
pub mod tui;

use tauri::{
    menu::{
        CheckMenuItemBuilder, Menu, MenuBuilder, MenuItemBuilder, PredefinedMenuItem,
        SubmenuBuilder,
    },
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Runtime, State, WindowEvent,
};
#[cfg(desktop)]
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::{
    commands::{
        apply_preset, attach_convolution_wav, create_group, create_preset, delete_group,
        delete_preset, disable_eq, export_app_settings, export_preset,
        get_autoeq_graphic_preset, get_autoeq_preset_variant,
        get_autorun_enabled, get_config_path, import_app_settings, import_presets,
        export_linux_eq_status, get_eq_backend_status, install_or_reinstall_apo, load_autoeq_index, load_logs, load_presets, move_preset, setup_linux_system_eq,
        open_apo_device_selector, open_logs_location, open_repository_url, rebuild_tray_menu,
        remove_convolution_wav, rename_group, rename_preset, reorder_groups,
        reveal_path_in_explorer, save_preset, set_autorun_enabled, set_config_path,
        set_group_emoji,
    },
    logging::append_log_line,
    state::{
        AppError, AppRuntimeSettings, AppState, PresetLibrary, TraySelection,
        EVENT_OPEN_ABOUT_REQUESTED, EVENT_PRESETS_UPDATED, EVENT_SETTINGS_UPDATED,
    },
};

const TRAY_ID: &str = "smart-eq-tray";
const WINDOW_LABEL: &str = "main";
const MENU_ID_MANAGE: &str = "menu.manage";
const MENU_ID_AUTORUN: &str = "menu.autorun";
const MENU_ID_ABOUT: &str = "menu.about";
const MENU_ID_EXIT: &str = "menu.exit";
const MENU_ID_DISABLE_EQ: &str = "menu.disable_eq";
const MENU_ID_EMPTY_GROUPS: &str = "menu.empty-groups";
const MENU_ID_EMPTY_PRESETS_PREFIX: &str = "menu.empty-presets";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StartupMode {
    Gui,
    Tray,
}

fn startup_mode() -> StartupMode {
    let mut mode = StartupMode::Gui;

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--gui" => mode = StartupMode::Gui,
            "--tray" | "--background" | "--minimized" => mode = StartupMode::Tray,
            _ => {}
        }
    }

    mode
}


fn truthy_env_value(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn tray_enabled_for_startup(mode: StartupMode) -> bool {
    if truthy_env_value("SMART_EQ_DISABLE_TRAY") {
        return false;
    }

    if matches!(mode, StartupMode::Tray) {
        return true;
    }

    // Linux GUI mode now uses an X11/XWayland launcher by default, so the tray
    // can be on again. Users who hit a distro-specific AppIndicator bug can run
    // SMART_EQ_DISABLE_TRAY=1 smart-eq-preset-switcher --gui.
    true
}

pub fn try_handle_cli_mode() -> Option<i32> {
    state::try_handle_cli_mode()
}

pub fn log_process_error(level: &str, message: impl AsRef<str>) {
    logging::append_log_line(level, message);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _, _| {
            let _ = show_main_window(&app);
        }));
        // Removed the autostart plugin.  Autorun is now handled by
        // our own cross‑platform implementation in the `autorun` module.

        // Initialise the global shortcut plugin with a handler that
        // toggles the main window when the configured key is pressed.
        let toggle_shortcut_for_handler = Shortcut::new(
            Some(Modifiers::ALT | Modifiers::CONTROL),
            Code::KeyE,
        );
        builder = builder.plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if *shortcut == toggle_shortcut_for_handler
                        && event.state() == ShortcutState::Pressed
                    {
                        let _ = toggle_main_window(app);
                    }
                })
                .build(),
        );
    }

    let builder = builder
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let state = AppState::initialize()?;
            app.manage(state);
            append_log_line("INFO", "Application initialized.");

            let mode = startup_mode();
            append_log_line("INFO", format!("Startup mode: {mode:?}."));

            if tray_enabled_for_startup(mode) {
                append_log_line("INFO", "Initializing tray icon.");
                let menu = construct_tray_menu(app.handle())?;
                let icon = app
                    .default_window_icon()
                    .cloned()
                    .ok_or(AppError::MissingIcon)?;

                match TrayIconBuilder::with_id(TRAY_ID)
                    .icon(icon)
                    .tooltip("SmartEQPresetSwitcher")
                    .menu(&menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(handle_tray_menu_event)
                    .on_tray_icon_event(handle_tray_icon_event)
                    .build(app)
                {
                    Ok(_tray) => append_log_line("INFO", "Tray icon initialized."),
                    Err(error) => {
                        append_log_line("ERROR", format!("Tray icon initialization failed: {error}"));
                        if matches!(mode, StartupMode::Tray) {
                            return Err(error.into());
                        }
                    }
                }
            } else {
                append_log_line("INFO", "Tray disabled by SMART_EQ_DISABLE_TRAY.");
            }

            configure_main_window(app.handle())?;
            maybe_prompt_for_config_migration(app.handle())?;
            let _ = refresh_runtime(app.handle());

            match mode {
                StartupMode::Gui => {
                    if let Err(error) = show_main_window(app.handle()) {
                        append_log_line("ERROR", format!("Failed to show main window on startup: {error}"));
                    }
                }
                StartupMode::Tray => {
                    if let Err(error) = hide_main_window(app.handle()) {
                        append_log_line("WARN", format!("Failed to hide main window for tray startup: {error}"));
                    }
                }
            }

            // Register the global shortcut within the setup context.  This
            // ensures the OS knows which shortcut we want to listen for.
            #[cfg(desktop)]
            {
                let toggle_shortcut = Shortcut::new(
                    Some(Modifiers::ALT | Modifiers::CONTROL),
                    Code::KeyE,
                );
                let _ = app.global_shortcut().register(toggle_shortcut);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            disable_eq,
            get_config_path,
            get_eq_backend_status,
            export_linux_eq_status,
            setup_linux_system_eq,
            set_config_path,
            load_presets,
            load_autoeq_index,
            get_autoeq_graphic_preset,
            get_autoeq_preset_variant,
            apply_preset,
            save_preset,
            create_group,
            set_group_emoji,
            rename_group,
            delete_group,
            reorder_groups,
            create_preset,
            rename_preset,
            delete_preset,
            move_preset,
            import_presets,
            install_or_reinstall_apo,
            attach_convolution_wav,
            remove_convolution_wav,
            export_app_settings,
            import_app_settings,
            export_preset,
            get_autorun_enabled,
            set_autorun_enabled,
            rebuild_tray_menu,
            reveal_path_in_explorer,
            open_apo_device_selector,
            load_logs,
            open_repository_url,
            open_logs_location
        ]);

    if let Err(error) = builder.run(tauri::generate_context!()) {
        append_log_line("ERROR", error.to_string());
    }
}

pub(crate) fn refresh_runtime<R: Runtime>(app: &AppHandle<R>) -> Result<PresetLibrary, AppError> {
    // The GUI path intentionally runs without a native tray on Linux/KDE to
    // avoid AppIndicator/WebKitGTK runtime crashes. Runtime refresh must still
    // succeed when no tray exists; tray rebuilding is best-effort only.
    if let Err(error) = rebuild_native_tray_menu(app) {
        match error {
            AppError::MissingTray => {
                append_log_line("INFO", "Skipping tray menu rebuild because no tray icon is active.");
            }
            other => {
                append_log_line("WARN", format!("Tray menu rebuild failed: {other}"));
            }
        }
    }

    let snapshot = {
        let state: State<'_, AppState> = app.state();
        let mut guard = state.lock()?;
        guard.snapshot()?
    };

    app.emit(EVENT_PRESETS_UPDATED, snapshot.clone())?;
    let _ = emit_runtime_settings(app)?;
    Ok(snapshot)
}

pub(crate) fn refresh_runtime_settings<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<AppRuntimeSettings, AppError> {
    if let Err(error) = rebuild_native_tray_menu(app) {
        match error {
            AppError::MissingTray => {
                append_log_line("INFO", "Skipping tray settings rebuild because no tray icon is active.");
            }
            other => {
                append_log_line("WARN", format!("Tray settings rebuild failed: {other}"));
            }
        }
    }
    emit_runtime_settings(app)
}

pub(crate) fn current_runtime_settings<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<AppRuntimeSettings, AppError> {
    Ok(AppRuntimeSettings {
        autorun_enabled: current_autorun_enabled(app)?,
    })
}

pub(crate) fn set_autorun_enabled_state<R: Runtime>(
    app: &AppHandle<R>,
    enabled: bool,
) -> Result<(), AppError> {
    // Delegate autorun control to the cross‑platform implementation in
    // the `autorun` module.  Ignore the unused `app` parameter on
    // non‑desktop targets to avoid warnings.
    let _ = app;
    if enabled {
        crate::autorun::enable()?;
    } else {
        crate::autorun::disable()?;
    }
    Ok(())
}

fn emit_runtime_settings<R: Runtime>(app: &AppHandle<R>) -> Result<AppRuntimeSettings, AppError> {
    let settings = current_runtime_settings(app)?;
    app.emit(EVENT_SETTINGS_UPDATED, settings.clone())?;
    Ok(settings)
}

fn current_autorun_enabled<R: Runtime>(app: &AppHandle<R>) -> Result<bool, AppError> {
    let _ = app;
    crate::autorun::status()
}

fn configure_main_window<R: Runtime>(app: &AppHandle<R>) -> Result<(), AppError> {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        let window_clone = window.clone();
        window.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if let Err(error) = window_clone.hide() {
                    append_log_line("ERROR", error.to_string());
                }
            }
        });
    }

    Ok(())
}

fn maybe_prompt_for_config_migration<R: Runtime>(app: &AppHandle<R>) -> Result<(), AppError> {
    let (should_prompt_migration, default_path) = {
        let state: State<'_, AppState> = app.state();
        let guard = state.lock()?;
        (
            guard.should_prompt_for_config_migration()?,
            guard.default_config_path_string(),
        )
    };

    if !should_prompt_migration {
        return Ok(());
    }

    let accepted = app
        .dialog()
        .message(format!(
            "Equalizer APO is currently configured to use a protected config folder.\n\nSmartEQPresetSwitcher works best with a writable config path:\n{default_path}\n\nLet the app switch Equalizer APO to that location now?"
        ))
        .title("Move Equalizer APO ConfigPath")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Switch Now".to_string(),
            "Keep Current Path".to_string(),
        ))
        .blocking_show();

    {
        let state: State<'_, AppState> = app.state();
        let mut guard = state.lock()?;
        guard.mark_config_path_prompted(true)?;
    }

    if accepted {
        let update_result = {
            let state: State<'_, AppState> = app.state();
            let mut guard = state.lock()?;
            guard.set_config_path(std::path::PathBuf::from(default_path))
        };

        match update_result {
            Ok(()) => {
                let _ = refresh_runtime(app);
            }
            Err(error) => {
                app.dialog()
                    .message(format!(
                        "The config path was not changed.\n\n{}\n\nYou can try again later from the main window.",
                        error
                    ))
                    .title("Config Path Update Failed")
                    .kind(MessageDialogKind::Error)
                    .blocking_show();
            }
        }
    }

    Ok(())
}

fn main_window<R: Runtime>(app: &AppHandle<R>) -> Result<tauri::WebviewWindow<R>, AppError> {
    app.get_webview_window(WINDOW_LABEL).ok_or_else(|| {
        let message = "The main window is not available yet.".to_string();
        append_log_line("ERROR", &message);
        AppError::Message(message)
    })
}

fn hide_main_window<R: Runtime>(app: &AppHandle<R>) -> Result<(), AppError> {
    let window = main_window(app)?;
    window.hide()?;
    append_log_line("INFO", "Main window hidden.");
    Ok(())
}

fn show_main_window<R: Runtime>(app: &AppHandle<R>) -> Result<(), AppError> {
    let window = main_window(app)?;
    append_log_line("INFO", "Showing main window.");

    match window.is_minimized() {
        Ok(true) => {
            if let Err(error) = window.unminimize() {
                append_log_line("WARN", format!("Failed to unminimize main window: {error}"));
            }
        }
        Ok(false) => {}
        Err(error) => append_log_line("WARN", format!("Failed to query main window minimized state: {error}")),
    }

    if let Err(error) = window.show() {
        append_log_line("ERROR", format!("Failed to call window.show(): {error}"));
        return Err(AppError::from(error));
    }

    if let Err(error) = window.set_focus() {
        // Wayland/KDE can deny focus stealing from tray callbacks. Showing the
        // window is what matters; focus failure must not close the app or show
        // a scary error dialog.
        append_log_line("WARN", format!("Main window shown but focus request was denied: {error}"));
    }

    match window.is_visible() {
        Ok(true) => append_log_line("INFO", "Main window is visible."),
        Ok(false) => append_log_line("ERROR", "Main window show call returned, but window is still not visible."),
        Err(error) => append_log_line("WARN", format!("Failed to query main window visible state: {error}")),
    }

    Ok(())
}

/// Toggles the visibility of the main window.  If the window is
/// currently visible it will be hidden; otherwise it will be shown
/// and focused.  Errors are logged but not returned to the caller.
fn toggle_main_window<R: Runtime>(app: &AppHandle<R>) -> Result<(), AppError> {
    let window = main_window(app)?;
    let visible = window.is_visible().unwrap_or(false);
    if visible {
        hide_main_window(app)?;
    } else {
        show_main_window(app)?;
    }
    Ok(())
}

fn show_about_dialog<R: Runtime>(app: &AppHandle<R>) -> Result<(), AppError> {
    append_log_line("INFO", "Opening About dialog.");

    // Emit the app-modal event for an already loaded GUI, but also show a
    // native dialog so About still works from the tray even if the hidden
    // webview has not mounted/listened yet.
    let _ = app.emit(EVENT_OPEN_ABOUT_REQUESTED, ());

    app.dialog()
        .message(
            "SmartEQPresetSwitcher\n\nCross-platform EQ preset switcher for Windows and Linux.\n\nOn Windows it integrates with Equalizer APO. On Linux it provides GUI, tray, TUI, boot-sync and EQ export workflows."
        )
        .title("About SmartEQPresetSwitcher")
        .kind(MessageDialogKind::Info)
        .blocking_show();

    Ok(())
}

fn handle_tray_menu_event<R: Runtime>(app: &AppHandle<R>, event: tauri::menu::MenuEvent) {
    append_log_line("INFO", format!("Tray menu action: {}", event.id().as_ref()));
    let result = match event.id().as_ref() {
        MENU_ID_MANAGE => show_main_window(app),
        MENU_ID_AUTORUN => toggle_autorun_from_tray(app),
        MENU_ID_ABOUT => show_about_dialog(app),
        MENU_ID_EXIT => {
            app.exit(0);
            Ok(())
        }
        MENU_ID_DISABLE_EQ => {
            let set_result = (|| -> Result<(), AppError> {
                let state: State<'_, AppState> = app.state();
                let mut guard = state.lock()?;
                guard.set_eq_disabled(true)
            })();
            if let Err(e) = set_result {
                append_log_line("ERROR", format!("Failed to disable EQ via tray: {e}"));
            }
            #[cfg(target_os = "linux")]
            commands::disable_linux_eq();
            let _ = refresh_runtime(app);
            Ok(())
        }
        item_id => apply_from_tray(app, item_id),
    };

    if let Err(error) = result {
        append_log_line("ERROR", format!("Tray menu action failed: {error}"));
    }
}

fn handle_tray_icon_event<R: Runtime>(tray: &tauri::tray::TrayIcon<R>, event: TrayIconEvent) {
    if let TrayIconEvent::DoubleClick {
        button: MouseButton::Left,
        ..
    } = event
    {
        if let Err(error) = show_main_window(tray.app_handle()) {
            append_log_line("ERROR", error.to_string());
        }
    }
}

fn apply_from_tray<R: Runtime>(app: &AppHandle<R>, item_id: &str) -> Result<(), AppError> {
    let selection = {
        let state: State<'_, AppState> = app.state();
        let guard = state.lock()?;
        guard.resolve_tray_selection(item_id)?
    };

    {
        let state: State<'_, AppState> = app.state();
        let mut guard = state.lock()?;
        guard.apply_preset(&selection.group, &selection.preset)?;
    }

    #[cfg(target_os = "linux")]
    {
        match crate::linux_eq::export_active_preset() {
            Ok(()) => {
                if let Err(error) = crate::commands::restart_linux_audio_services() {
                    crate::logging::append_log_line(
                        "WARN",
                        format!("Linux EQ was exported after tray apply, but audio service reload failed: {error}"),
                    );
                } else if let Err(error) = crate::commands::route_linux_audio_to_eq_sink() {
                    crate::logging::append_log_line("WARN", format!("Linux EQ was exported after tray apply, but routing failed: {error}"));
                }
            }
            Err(error) => {
                crate::logging::append_log_line("WARN", format!("Linux EQ export after tray apply failed: {error}"));
            }
        }
    }

    let _ = refresh_runtime(app)?;
    Ok(())
}

fn toggle_autorun_from_tray<R: Runtime>(app: &AppHandle<R>) -> Result<(), AppError> {
    let next_enabled = !current_autorun_enabled(app)?;
    set_autorun_enabled_state(app, next_enabled)?;
    let _ = refresh_runtime_settings(app)?;
    Ok(())
}

fn rebuild_native_tray_menu<R: Runtime>(app: &AppHandle<R>) -> Result<(), AppError> {
    let menu = construct_tray_menu(app)?;
    let tray = app.tray_by_id(TRAY_ID).ok_or(AppError::MissingTray)?;
    tray.set_menu(Some(menu))?;
    Ok(())
}

fn construct_tray_menu<R: Runtime>(app: &AppHandle<R>) -> Result<Menu<R>, AppError> {
    let (snapshot, targets) = {
        let state: State<'_, AppState> = app.state();
        let mut guard = state.lock()?;
        let snapshot = guard.snapshot()?;
        let targets = build_tray_targets(&snapshot);
        guard.replace_tray_targets(targets.clone());
        (snapshot, targets)
    };
    let autorun_enabled = current_autorun_enabled(app)?;

    let presets_submenu = build_presets_submenu(app, &snapshot, &targets)?;
    let manage_item = MenuItemBuilder::with_id(MENU_ID_MANAGE, "Manage Presets...").build(app)?;
    let autorun_item = CheckMenuItemBuilder::with_id(MENU_ID_AUTORUN, "Start with login")
        .checked(autorun_enabled)
        .build(app)?;

    let eq_disabled = {
        let state: State<'_, AppState> = app.state();
        let guard = state.lock()?;
        guard.is_eq_disabled()
    };
    let disable_eq_item = if eq_disabled {
        MenuItemBuilder::with_id(MENU_ID_DISABLE_EQ, "EQ: Bypassed")
            .enabled(false)
            .build(app)?
    } else {
        MenuItemBuilder::with_id(MENU_ID_DISABLE_EQ, "Bypass EQ").build(app)?
    };

    let about_item = MenuItemBuilder::with_id(MENU_ID_ABOUT, "About...").build(app)?;
    let exit_item = MenuItemBuilder::with_id(MENU_ID_EXIT, "Exit").build(app)?;
    let separator = PredefinedMenuItem::separator(app)?;

    MenuBuilder::new(app)
        .items(&[
            &presets_submenu,
            &manage_item,
            &autorun_item,
            &disable_eq_item,
            &separator,
            &about_item,
            &exit_item,
        ])
        .build()
        .map_err(AppError::from)
}

fn build_presets_submenu<R: Runtime>(
    app: &AppHandle<R>,
    snapshot: &PresetLibrary,
    targets: &[(String, TraySelection)],
) -> Result<tauri::menu::Submenu<R>, AppError> {
    if snapshot.groups.is_empty() {
        let empty_item = MenuItemBuilder::with_id(MENU_ID_EMPTY_GROUPS, "No presets available")
            .enabled(false)
            .build(app)?;
        return SubmenuBuilder::new(app, "Presets")
            .item(&empty_item)
            .build()
            .map_err(AppError::from);
    }

    let mut builder = SubmenuBuilder::new(app, "Presets");

    let active_label = active_preset_label(snapshot);
    let active_item = MenuItemBuilder::with_id("menu.active.current", active_label.as_str())
        .enabled(false)
        .build(app)?;
    builder = builder.item(&active_item);

    for group in &snapshot.groups {
        let group_label = menu_group_label(group);
        let mut group_builder = SubmenuBuilder::new(app, group_label.as_str());

        if group.presets.is_empty() {
            let empty_id = format!("{MENU_ID_EMPTY_PRESETS_PREFIX}.{}", group.name);
            let empty = MenuItemBuilder::with_id(empty_id, "No presets yet")
                .enabled(false)
                .build(app)?;
            group_builder = group_builder.item(&empty);
        } else {
            for preset in &group.presets {
                let menu_id = targets
                    .iter()
                    .find(|(_, selection)| {
                        selection.group == group.name && selection.preset == preset.name
                    })
                    .map(|(id, _)| id.as_str())
                    .ok_or_else(|| {
                        AppError::UnknownMenuItem(format!("{}/{}", group.name, preset.name))
                    })?;
                let item = CheckMenuItemBuilder::with_id(menu_id, &preset.name)
                    .checked(group.active_preset.as_deref() == Some(preset.name.as_str()))
                    .build(app)?;
                group_builder = group_builder.item(&item);
            }
        }

        let submenu = group_builder.build()?;
        builder = builder.item(&submenu);
    }

    builder.build().map_err(AppError::from)
}

fn build_tray_targets(snapshot: &PresetLibrary) -> Vec<(String, TraySelection)> {
    let mut targets = Vec::new();
    let mut index = 0usize;

    for group in &snapshot.groups {
        for preset in &group.presets {
            targets.push((
                format!("preset.{index}"),
                TraySelection {
                    group: group.name.clone(),
                    preset: preset.name.clone(),
                },
            ));
            index += 1;
        }
    }

    targets
}

fn menu_group_label(group: &crate::state::PresetGroup) -> String {
    match group
        .emoji
        .as_deref()
        .map(str::trim)
        .filter(|emoji| !emoji.is_empty())
    {
        Some(emoji) => format!("{emoji} {}", group.name),
        None => group.name.clone(),
    }
}

fn active_preset_label(snapshot: &PresetLibrary) -> String {
    snapshot
        .groups
        .iter()
        .find_map(|group| {
            group
                .active_preset
                .as_deref()
                .map(|preset| format!("Active: {preset}"))
        })
        .unwrap_or_else(|| "Active: None".to_string())
}
