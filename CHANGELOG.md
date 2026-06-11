# Changelog

## 0.3.0 - 2026-06-11

### Added

- Rebranded runtime/package naming to `SmartEQPresetSwitcher` with legacy folder migration.
- Linux GUI, tray, TUI, boot-sync and native Arch package workflows.
- EQ backend status panel for Windows Equalizer APO and Linux PipeWire/EasyEffects detection.
- AutoEQ target selector with Auto, ParametricEQ / Filter and GraphicEQ import modes.
- Linux PipeWire filter-chain export, PipeWire restart/routing helper, and GraphicEQ-to-parametric fallback conversion.
- Bootstrap/build scripts for Linux, Windows, Arch packages, Debian packages and optional AppImage builds.

### Changed

- Arch build now uses `tauri build --no-bundle` plus `makepkg`, producing a real pacman-installable `.pkg.tar.zst`.
- Linux packaged launcher forces conservative X11/XWayland WebKitGTK environment by default for KDE/Wayland/NVIDIA reliability.
- Linux GUI starts with tray enabled again; `SMART_EQ_DISABLE_TRAY=1` disables it for troubleshooting.
- Windows-only Equalizer APO repair controls are hidden on Linux.
- Project hygiene checks distinguish developer mode from clean source packaging checks.

### Fixed

- Rust compile issues around mutable snapshots, private state calls and Tauri command registration.
- AppImage/linuxdeploy no longer blocks the default Arch build path.
- Packaged Arch binary no longer tries to load `localhost`; it embeds production frontend assets.
- AutoEQ first-preview cache/download decode failures and stale variant handling.
- Linux preset mutations no longer fail when tray refresh is unavailable.

## 0.2.0 - 2026-04-21

### Added

- New footer utility controls: `Logs`, `Troubleshoot`, `About`, and the `Launch on Windows startup` toggle.
- In-app logs viewer with readable local timestamps and direct opening of the logs folder.
- About modal with the project description and a repository link that opens in the default browser.
- Troubleshoot modal for detecting Equalizer APO state, reinstalling it, and reopening the official Device Selector.

### Changed

- Equalizer APO install and reinstall now resolve the real SourceForge mirror URL before downloading the installer.
- Installer and launcher diagnostics now append detailed step-by-step output into the app log for easier support.
- Device Selector launching now uses the resolved Equalizer APO install path and working directory.

### Backend Functions

- Added `load_logs` and `open_logs_location` for log viewing and log-folder access.
- Added `open_repository_url` for browser launching from the About panel.
- Added `install_or_reinstall_apo` and `open_apo_device_selector` for the troubleshooting workflow.
