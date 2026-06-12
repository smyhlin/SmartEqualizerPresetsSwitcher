//! Simple terminal user interface (TUI).
//!
//! This module provides an interactive command‑line interface that
//! exposes most of the functionality of the SmartEQPresetSwitcher
//! Presets Manager without requiring a graphical environment.  The
//! interface prints helpful status information, accepts commands
//! separated by whitespace and gracefully handles errors.  Users
//! familiar with shell tools should find it intuitive: type `help` to
//! see available commands, `list` to show preset groups and active
//! selections, and `apply <group> <preset>` to switch presets.

use std::io::{self, Write};

use crate::autorun;
use crate::linux_eq;
use crate::state::{AppError, AppState};
use std::path::PathBuf;

/// Entry point for the TUI.  Accepts a slice of arguments after
/// `--tui`.  When arguments are provided the command is executed
/// once and the program exits.  When no additional arguments are
/// provided an interactive REPL is launched.
pub fn run(args: &[String]) -> i32 {
    // Initialize application state.  The REPL borrows the state
    // mutably for the duration of the program.  If initialization
    // fails we cannot continue.
    let mut state = match AppState::initialize() {
        Ok(s) => s,
        Err(error) => {
            eprintln!("Error initializing application state: {error}");
            return 1;
        }
    };
    if !args.is_empty() {
        // Non‑interactive invocation: treat the arguments as a single
        // command.  Parsing returns a boolean indicating whether the
        // command succeeded.  An unknown or failing command yields a
        // non‑zero exit code.
        return match handle_command(args, &mut state) {
            Ok(true) => 0,
            Ok(false) => 0,
            Err(error) => {
                eprintln!("{error}");
                1
            }
        };
    }
    // Launch REPL.
    print_welcome();
    let stdin = io::stdin();
    loop {
        print!("smart-eq> ");
        let _ = io::stdout().flush();
        let mut input = String::new();
        if stdin.read_line(&mut input).is_err() {
            break;
        }
        let trimmed = input.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parts: Vec<String> = trimmed
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
        match handle_command(&parts, &mut state) {
            Ok(continue_loop) => {
                if !continue_loop {
                    break;
                }
            }
            Err(error) => {
                eprintln!("{error}");
            }
        }
    }
    0
}

/// Prints a brief welcome message and basic usage instructions.
fn print_welcome() {
    println!("SmartEQPresetSwitcher (TUI mode)");
    println!("Type 'help' to see available commands.  Type 'quit' or 'exit' to leave.");
    println!();
}

/// Prints a help message listing available commands and their usage.
fn print_help() {
    println!("Available commands:");
    println!("  help                           Show this message");
    println!("  list | ls                      List preset groups and active presets");
    println!("  apply <group> <preset>         Activate the given preset");
    println!("  groups                         List group names");
    println!("  presets <group>                List presets in a group");
    println!("  create-group <name>            Create a new preset group");
    println!("  create-preset <group> <name>   Create a new empty preset");
    println!("  import <group> <file> [files]  Import one or more preset files into a group");
    println!("  config <path>                  Set the active backend configuration path");
    println!("  autorun status                 Show whether the app is configured to start automatically");
    println!("  autorun enable                 Enable autorun at login / boot");
    println!("  autorun disable                Disable autorun");
    println!("  linux-status                   Export the current preset to Linux EQ files");
    println!("  quit | exit                    Exit the program");
    println!();
}

/// Dispatches a command.  Returns Ok(true) if the REPL should
/// continue, Ok(false) to exit, or an error.
fn handle_command(args: &[String], state: &mut AppState) -> Result<bool, AppError> {
    if args.is_empty() {
        return Ok(true);
    }
    let cmd = args[0].as_str();
    match cmd {
        "help" | "?" => {
            print_help();
            Ok(true)
        }
        "quit" | "exit" => Ok(false),
        "list" | "ls" => {
            let snapshot = {
                let mut guard = state.lock()?;
                guard.snapshot()?
            };
            if snapshot.groups.is_empty() {
                println!("No preset groups found.");
            } else {
                {
                    let guard = state.lock()?;
                    if guard.is_eq_disabled() {
                        println!("  (EQ bypassed)");
                    }
                }
                for group in &snapshot.groups {
                    let active = group.active_preset.as_deref().unwrap_or("(none)");
                    println!("{} (active: {})", group.name, active);
                    for preset in &group.presets {
                        let marker = if Some(&preset.name) == group.active_preset.as_ref() {
                            "*"
                        } else {
                            " "
                        };
                        println!("  {} {}", marker, preset.name);
                    }
                }
            }
            Ok(true)
        }
        "groups" => {
            let snapshot = {
                let mut guard = state.lock()?;
                guard.snapshot()?
            };
            for group in &snapshot.groups {
                println!("{}", group.name);
            }
            Ok(true)
        }
        "presets" => {
            if args.len() < 2 {
                println!("Usage: presets <group>");
            } else {
                let snapshot = {
                    let mut guard = state.lock()?;
                    guard.snapshot()?
                };
                let group_name = &args[1];
                if let Some(group) = snapshot.groups.iter().find(|g| &g.name == group_name) {
                    for preset in &group.presets {
                        println!("{}", preset.name);
                    }
                } else {
                    println!("Group not found: {}", group_name);
                }
            }
            Ok(true)
        }
        "apply" => {
            if args.len() < 3 {
                println!("Usage: apply <group> <preset>");
            } else {
                let group = &args[1];
                let preset = &args[2];
                {
                    let mut guard = state.lock()?;
                    // `apply_preset` updates the active preset and writes
                    // the updated configuration.  There is no need to
                    // call `write_active_config` again here.
                    guard.apply_preset(group, preset)?;
                }
                // On Linux also export the preset for PipeWire/EasyEffects.
                #[cfg(target_os = "linux")]
                {
                    let _ = linux_eq::export_active_preset();
                }
                println!("Applied preset '{} / {}'", group, preset);
            }
            Ok(true)
        }
        "create-group" => {
            if args.len() < 2 {
                println!("Usage: create-group <name>");
            } else {
                let name = &args[1];
                let mut guard = state.lock()?;
                match guard.create_group(name) {
                    Ok(()) => println!("Group '{}' created.", name),
                    Err(e) => println!("Failed to create group: {e}"),
                }
            }
            Ok(true)
        }
        "create-preset" => {
            if args.len() < 3 {
                println!("Usage: create-preset <group> <name>");
            } else {
                let group = &args[1];
                let name = &args[2];
                let mut guard = state.lock()?;
                match guard.create_preset(group, name, None) {
                    Ok(()) => println!("Preset '{}' created in group '{}'.", name, group),
                    Err(e) => println!("Failed to create preset: {e}"),
                }
            }
            Ok(true)
        }
        "import" => {
            if args.len() < 3 {
                println!("Usage: import <group> <file> [files...]");
            } else {
                let group = &args[1];
                let files: Vec<String> = args[2..].to_vec();
                let mut guard = state.lock()?;
                match guard.import_presets(group, &files) {
                    Ok(()) => println!("Imported {} file(s) into group '{}'.", files.len(), group),
                    Err(e) => println!("Failed to import presets: {e}"),
                }
            }
            Ok(true)
        }
        "config" => {
            if args.len() < 2 {
                println!("Usage: config <path>");
            } else {
                let path = PathBuf::from(&args[1]);
                let mut guard = state.lock()?;
                match guard.set_config_path(path) {
                    Ok(()) => println!("Configuration path updated."),
                    Err(e) => println!("Failed to set configuration path: {e}"),
                }
            }
            Ok(true)
        }
        "autorun" => {
            if args.len() < 2 {
                println!("Usage: autorun <status|enable|disable>");
            } else {
                match args[1].as_str() {
                    "status" => match autorun::status() {
                        Ok(enabled) => {
                            println!("Autorun is {}", if enabled { "enabled" } else { "disabled" });
                        }
                        Err(e) => println!("Failed to query autorun status: {e}"),
                    },
                    "enable" => match autorun::enable() {
                        Ok(()) => println!("Autorun enabled."),
                        Err(e) => println!("Failed to enable autorun: {e}"),
                    },
                    "disable" => match autorun::disable() {
                        Ok(()) => println!("Autorun disabled."),
                        Err(e) => println!("Failed to disable autorun: {e}"),
                    },
                    other => println!("Unknown autorun action: {}", other),
                }
            }
            Ok(true)
        }
        "disable" => {
            {
                let mut guard = state.lock()?;
                guard.set_eq_disabled(true)?;
            }
            #[cfg(target_os = "linux")]
            crate::commands::disable_linux_eq();
            println!("EQ bypassed. Select a preset and Apply to re-enable.");
            Ok(true)
        }
        "linux-status" | "status" => {
            #[cfg(target_os = "linux")]
            {
                match linux_eq::export_active_preset() {
                    Ok(()) => println!("Linux EQ configuration exported."),
                    Err(e) => println!("Failed to export Linux EQ configuration: {e}"),
                }
            }
            #[cfg(not(target_os = "linux"))]
            {
                println!("Linux EQ export is only available on Linux.");
            }
            Ok(true)
        }
        _ => {
            println!("Unknown command: {}.  Type 'help' to see available commands.", cmd);
            Ok(true)
        }
    }
}