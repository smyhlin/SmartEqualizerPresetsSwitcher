# SmartEQPresetSwitcher

SmartEQPresetSwitcher is a cross-platform EQ preset switcher for managing, editing, applying, importing, exporting, and backing up EQ presets. It is built with SvelteKit, TypeScript, Rust and Tauri 2.

On Windows it integrates with Equalizer APO, can install or repair it, and can open the official Device Selector. On Linux it provides GUI, tray, TUI, boot-sync and Linux EQ export workflows for PipeWire-oriented systems.

---
<img width="1610" height="903" alt="image" src="https://github.com/user-attachments/assets/5067d285-3f1b-4a7e-a834-e00f6468451f" />

---
<img width="1011" height="183" alt="{543C6396-6142-49DF-8CCE-04A829D97903}" src="https://github.com/user-attachments/assets/7a27512b-e538-4497-9501-e190fe38c456" />

---


## Release state

Current source version: `0.3.0`. This source tree is prepared for GitHub push and source packaging. Generated dependency/build folders are intentionally excluded from the archive.

## What It Does

- Organizes presets into groups with drag-and-drop ordering.
- Applies presets from the main window, the system tray or the TUI.
- Edits preset `.txt` files in-app and exports them back to disk.
- Imports Equalizer APO preset files and convolution `.wav` files.
- Keeps convolution file references synced and can reveal linked files with the native file manager.
- Imports and exports full app-data backups as JSON.
- Imports AutoEQ presets.
- Stores readable local-timestamped logs and can open the logs folder directly from the app.
- Supports launch-on-startup on Windows and Linux.
- Supports a Linux headless boot-sync mode.
- Exports active presets to Linux EQ files for PipeWire-oriented workflows.

## Platform Scope

### Windows x64

- Main GUI and tray mode.
- Equalizer APO install/reinstall helper.
- Equalizer APO Device Selector launcher.
- Per-user autorun through the Windows Run registry key.
- NSIS installer build.

### Linux x64

- Main GUI and tray mode when a desktop is available.
- TUI mode through `--tui`.
- Headless boot-sync mode through `--boot-sync`.
- Desktop autostart through `~/.config/autostart`.
- User systemd boot-sync service through `~/.config/systemd/user`.
- Linux EQ export files under `~/.config/SmartEQPresetSwitcher/linux-eq`.
- Native Arch package, Debian package and optional AppImage build scripts.

Equalizer APO itself is Windows-only. Linux support is implemented as compatibility/export tooling for compatible EQ preset syntax, not as a native Equalizer APO port.

## Rename and Data Migration

The project was previously named `SmartEqualizerAPOPresetsManager`. Runtime folders, executable names, package names, desktop files and docs now use `SmartEQPresetSwitcher`.

On first start, the app migrates the old per-user data folder when possible:

```text
SmartEqualizerAPO -> SmartEQPresetSwitcher
```

Existing managed config markers from the old app name are still recognized and replaced with the new `SmartEQPresetSwitcher` markers when the active config is rebuilt.



## AutoEQ target variants

AutoEQ import now supports target-aware variants instead of assuming GraphicEQ only:

- **Auto target**: prefers ParametricEQ / `Filter:` output for Linux PipeWire/EasyEffects and Windows Equalizer APO/Peace, then falls back to GraphicEQ.
- **ParametricEQ / Filter**: best for PipeWire filter-chain `param_eq`, EasyEffects-style PEQ workflows, Equalizer APO and Peace.
- **GraphicEQ**: fallback for simple graphic equalizers and manual editing.

This matters because AutoEq itself produces settings for many equalizer apps and says the user must apply them with the selected equalizer app. The app therefore should not hard-code one AutoEQ format forever.


### Tray + Linux EQ behavior

The GUI now starts with the tray enabled again. On KDE/Wayland/NVIDIA the launcher forces the GTK/WebKit process to X11/XWayland first; if a distro-specific tray problem appears, run:

```bash
SMART_EQ_DISABLE_TRAY=1 smart-eq-preset-switcher --gui
```

Linux system EQ setup exports the active preset, generates a PipeWire filter-chain config, reloads the user audio services on Apply, and uses `pactl` to set/move streams to the generated EQ sink when available. Parametric AutoEQ presets are used directly. GraphicEQ-only presets are converted into a conservative 31-band parametric approximation so PipeWire `param_eq` can still load them.

## EQ backend status

The main window now has an **EQ backend** status button in the header. It tells the user whether the app is only managing presets locally or whether the OS audio backend is ready to consume the active preset.

- Windows: reports Equalizer APO detection and whether APO `ConfigPath` points at the managed SmartEQPresetSwitcher config folder.
- Linux: reports PipeWire / EasyEffects export readiness, the active export path, and the generated PipeWire filter-chain setup file.
- Linux setup exports the active preset to `~/.config/SmartEQPresetSwitcher/linux-eq/active-equalizerapo.txt` and writes a PipeWire filter-chain snippet at `~/.config/pipewire/pipewire.conf.d/99-smart-eq-preset-switcher-parametric-eq.conf`.

On Linux, system-wide EQ is not called "connected" unless the PipeWire setup file exists. The GUI opens a setup gate when the Linux backend is missing, while still allowing an explicit local-only bypass for first-run preset creation or troubleshooting. The Linux setup panel detects common package-manager families and can open a visible terminal with the matching PipeWire/WirePlumber install command. EasyEffects is optional GUI workflow guidance, not a required app-managed backend package. The setup action writes the user PipeWire filter-chain file and, when a valid parametric preset exists, runs `systemctl --user try-restart pipewire.service pipewire-pulse.service wireplumber.service`.

## Runtime Layout

### Windows

```text
%APPDATA%\SmartEQPresetSwitcher
%APPDATA%\SmartEQPresetSwitcher\presets
%APPDATA%\SmartEQPresetSwitcher\config
```

If Equalizer APO is still pointing at a protected config directory, the app prompts to move its `ConfigPath` to the writable app-managed folder. Changing `ConfigPath` or updating protected Equalizer APO files can trigger a Windows UAC prompt.

### Linux

```text
~/.config/SmartEQPresetSwitcher
~/.config/SmartEQPresetSwitcher/presets
~/.config/SmartEQPresetSwitcher/config
~/.config/SmartEQPresetSwitcher/linux-eq
~/.config/pipewire/pipewire.conf.d/99-smart-eq-preset-switcher-parametric-eq.conf
```

## TUI Quick Start

Interactive TUI:

```bash
smart_eq_preset_switcher --tui
```

Single command mode:

```bash
smart_eq_preset_switcher --tui list
smart_eq_preset_switcher --tui apply MyGroup MyPreset
smart_eq_preset_switcher --tui autorun status
```

Boot sync:

```bash
smart_eq_preset_switcher --boot-sync
```

Autorun CLI:

```bash
smart_eq_preset_switcher --autorun status
smart_eq_preset_switcher --autorun enable
smart_eq_preset_switcher --autorun disable
```

## Equalizer APO Setup

Use the `Troubleshoot` button in the main window on Windows to:

- Download and silently install Equalizer APO with the official `/S` installer.
- Re-run the same install chain if the install needs repair.
- Open the official Device Selector so playback and capture devices receive APO processing.

On Linux these Windows-only actions are hidden from the main UI and return clear unsupported-platform errors if called directly.

## Development

Detailed build instructions are in [docs/BUILDING.md](docs/BUILDING.md). Linux runtime diagnostics are in [docs/TUI_AND_LINUX.md](docs/TUI_AND_LINUX.md).

### Linux bootstrap and builds

```bash
scripts/bootstrap-linux.sh            # automatically installs build deps
scripts/bootstrap-linux.sh --cross-windows  # also install cross-build deps (NSIS AUR, LLVM, etc.)
scripts/build-linux.sh                # auto: Arch -> pacman, Debian -> deb
scripts/build-linux.sh --arch         # native Arch package
scripts/build-linux.sh --deb          # Debian package (via build-deb.sh)
scripts/build-linux.sh --appimage
scripts/build-linux.sh --windows      # cross-build Windows NSIS from Linux, best-effort
scripts/build-linux.sh --all
scripts/build-arch-package.sh         # direct Arch pacman builder
scripts/build-deb.sh                  # direct Debian .deb builder
scripts/build-windows-from-linux.sh   # direct Windows cross-build script
```

On Arch, use the default `scripts/build-linux.sh` or force `--arch`. The output
is a native pacman package and can be installed with:

```bash
sudo pacman -U dist/arch/smart-eq-preset-switcher-*.pkg.tar.zst
```

`.deb` is for Debian/Ubuntu users, not for installing on Arch. The Arch package is generated through `tauri build --no-bundle` plus `makepkg`, not by manually tarring a package root, so the binary embeds production assets and `pacman -U` can validate the metadata.

AppImage remains optional, but it uses Tauri/linuxdeploy and can fail on some distro/runtime
combinations. On Arch the native `.pkg.tar.zst` path is preferred.

On Arch, `rust` and `rustup` conflict. The bootstrap script keeps an existing
Rust toolchain by default. Use `scripts/bootstrap-linux.sh --rust` if you use
the repository `rust` package, or `scripts/bootstrap-linux.sh --rustup` if you
want rustup.


On KDE/Wayland, the installed Arch launcher forces conservative XWayland/WebKitGTK environment variables by default. Native Wayland can be tested with:

```bash
SMART_EQ_USE_WAYLAND=1 smart-eq-preset-switcher --gui
```

Expected Linux outputs:

```text
dist/arch/*.pkg.tar.zst                                # Arch pacman package
dist/deb/*.deb                                         # Debian/Ubuntu package
src-tauri/target/release/bundle/appimage/*.AppImage    # optional
src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/*.exe  # optional Linux -> Windows NSIS
```

### Linux runtime diagnostics

The app writes its own log file here:

```bash
cat ~/.config/SmartEQPresetSwitcher/logs/application.log
```

For desktop-launch or crash diagnostics on systemd desktops:

```bash
journalctl --user -b --grep='SmartEQPresetSwitcher|smart-eq-preset-switcher|smart_eq_preset_switcher' --no-pager
journalctl -b _COMM=smart-eq-preset-switcher --no-pager
coredumpctl list smart-eq-preset-switcher smart_eq_preset_switcher
coredumpctl info smart-eq-preset-switcher
```

To run it in foreground and capture stderr/stdout:

```bash
smart-eq-preset-switcher --gui 2>&1 | tee /tmp/smarteq-run.log
```

A GTK/WebKit tray app can appear as multiple processes in KDE process dialogs. That is normal unless memory usage keeps climbing over time.


### KDE/Wayland runtime note

On Linux the app defaults to the X11/XWayland GTK backend for GUI launches because WebKitGTK/AppIndicator combinations can hit `Gdk-Message: Error 71 (Protocol error) dispatching to Wayland display` on KDE/Wayland/NVIDIA setups. To explicitly test native Wayland:

```bash
SMART_EQ_USE_WAYLAND=1 smart-eq-preset-switcher --gui
# or
smart-eq-preset-switcher --wayland --gui
```

The normal GUI path initializes the tray on Linux. Start background/tray-only mode explicitly with:

```bash
smart-eq-preset-switcher --tray
```

If a distro-specific AppIndicator issue appears, disable the tray for one launch with:

```bash
SMART_EQ_DISABLE_TRAY=1 smart-eq-preset-switcher --gui
```


### Windows installer from Linux

Windows installers are best built on Windows, but the Linux build flow can now cross-build an **NSIS** installer as a best-effort path:

```bash
scripts/build-linux.sh --windows
# or directly
scripts/build-windows-from-linux.sh
```

This path uses `cargo-xwin` and the MSVC Rust target. It needs `rustup`, `cargo-xwin`, NSIS, LLVM/LLD and `llvm-rc`. On Arch, `nsis` is in the AUR (`paru -S nsis`); official packages (`llvm lld clang`) come from `pacman`. MSI/WiX installers still require Windows; Linux cross-build is NSIS-only.

Expected output:

```text
src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/*.exe
```

### Windows bootstrap and build

```bat
scripts\bootstrap-windows.bat
scripts\build-windows.bat
```

Expected Windows output:

```text
src-tauri\target\release\bundle\nsis\*.exe
```

### Direct commands

```bash
npm ci
npm run check
npm run check:project
npm run tauri -- dev
npm run tauri -- build
```

## Important Docs

- [Building and bootstrapping](docs/BUILDING.md)
- [TUI, headless mode and Linux EQ support](docs/TUI_AND_LINUX.md)

## Repository Hygiene

Committed on purpose:

- Application source code
- `package-lock.json`
- `src-tauri/Cargo.lock`
- Tauri icon assets in `src-tauri/icons/`
- Bootstrap/build scripts
- Documentation

Ignored on purpose:

- `node_modules/`
- `.svelte-kit/`
- `build/`
- `src-tauri/gen/`
- `src-tauri/target/`
- `.cargo-target/`
- `dist/`

This repository is prepared for source upload, not binary distribution.

## License

MIT. See [LICENSE](LICENSE).
