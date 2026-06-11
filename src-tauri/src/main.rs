#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]


#[cfg(target_os = "linux")]
fn truthy_env(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn prepare_linux_display_env(args: &[String]) {
    let requested_wayland = truthy_env("SMART_EQ_USE_WAYLAND")
        || args
            .iter()
            .any(|arg| matches!(arg.as_str(), "--wayland" | "--native-wayland"));

    if !requested_wayland {
        // Force the GTK/WebKit process to XWayland/X11 before Tauri creates
        // the webview/tray. Respecting an inherited "wayland,x11" value was
        // enough to trigger GDK protocol errors on KDE/NVIDIA.
        std::env::set_var("GDK_BACKEND", "x11");
        std::env::set_var("WINIT_UNIX_BACKEND", "x11");
        eprintln!(
            "SmartEQPresetSwitcher: forcing GDK_BACKEND=x11 on Linux. Set SMART_EQ_USE_WAYLAND=1 or pass --wayland to opt into native Wayland."
        );
    }

    // WebKitGTK + accelerated compositing/DMABUF can be fragile on some
    // KDE/Wayland/NVIDIA setups. These are harmless when unsupported and can
    // still be overridden by the user before launch.
    if std::env::var_os("WEBKIT_DISABLE_COMPOSITING_MODE").is_none() {
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    }
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
}

#[cfg(not(target_os = "linux"))]
fn prepare_linux_display_env(_args: &[String]) {}

fn set_panic_logging_hook() {
    std::panic::set_hook(Box::new(|panic_info| {
        let message = format!("Unhandled panic: {panic_info}");
        eprintln!("SmartEQPresetSwitcher {message}");
        smart_eq_preset_switcher_lib::log_process_error("ERROR", message);
    }));
}

fn main() {
    set_panic_logging_hook();
    // Collect arguments for CLI dispatch.  The first element is the
    // executable path which we skip.  We clone into owned Strings so
    // that they can be passed to the TUI or autorun functions.
    let args: Vec<String> = std::env::args().skip(1).collect();
    prepare_linux_display_env(&args);
    if !args.is_empty() {
        match args[0].as_str() {
            "--tui" => {
                // Pass remaining arguments to the TUI and exit with its code.
                let code = smart_eq_preset_switcher_lib::tui::run(&args[1..]);
                std::process::exit(code);
            }
            "--boot-sync" => {
                // Run boot synchronization to refresh the active preset.
                match smart_eq_preset_switcher_lib::autorun::boot_sync() {
                    Ok(()) => std::process::exit(0),
                    Err(err) => {
                        eprintln!("Boot sync failed: {err}");
                        std::process::exit(1);
                    }
                }
            }
            "--gui" | "--tray" | "--background" | "--minimized" => {
                // Explicit GUI/tray launch modes are handled by the Tauri setup code.
            }
            "--autorun" => {
                // Handle autorun commands: status, enable, disable.  If
                // no subcommand is provided default to status.
                let sub = args.get(1).map(|s| s.as_str()).unwrap_or("status");
                use smart_eq_preset_switcher_lib::autorun;
                let code = match sub {
                    "enable" => match autorun::enable() {
                        Ok(()) => {
                            println!("Autorun enabled.");
                            0
                        }
                        Err(e) => {
                            eprintln!("Failed to enable autorun: {e}");
                            1
                        }
                    },
                    "disable" => match autorun::disable() {
                        Ok(()) => {
                            println!("Autorun disabled.");
                            0
                        }
                        Err(e) => {
                            eprintln!("Failed to disable autorun: {e}");
                            1
                        }
                    },
                    "status" | _ => match autorun::status() {
                        Ok(enabled) => {
                            println!("Autorun is {}", if enabled { "enabled" } else { "disabled" });
                            0
                        }
                        Err(e) => {
                            eprintln!("Failed to query autorun status: {e}");
                            1
                        }
                    },
                };
                std::process::exit(code);
            }
            _ => {}
        }
    }
    // Preserve existing elevated CLI handler for Windows registry
    // operations before starting the GUI.  This call also covers
    // Windows elevation tasks run through try_handle_cli_mode.
    if let Some(exit_code) = smart_eq_preset_switcher_lib::try_handle_cli_mode() {
        std::process::exit(exit_code);
    }
    smart_eq_preset_switcher_lib::run();
}
