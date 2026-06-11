# TUI, Headless Mode and Linux EQ Support

## TUI mode

The TUI is intended for systems without a desktop session, SSH usage, recovery work and quick preset changes.

Start interactive mode:

```bash
smart_eq_preset_switcher --tui
```

Run one command and exit:

```bash
smart_eq_preset_switcher --tui list
smart_eq_preset_switcher --tui apply MyGroup MyPreset
smart_eq_preset_switcher --tui autorun status
```

Available commands:

```text
help
list | ls
groups
presets <group>
apply <group> <preset>
create-group <name>
create-preset <group> <name>
import <group> <file> [files...]
config <path>
autorun status
autorun enable
autorun disable
linux-status | status
quit | exit
```

## Boot sync mode

Boot sync updates the active preset output without opening a GUI or waiting for user input:

```bash
smart_eq_preset_switcher --boot-sync
```

This is used by the Linux systemd user service created by autorun.

## Autorun commands

```bash
smart_eq_preset_switcher --autorun status
smart_eq_preset_switcher --autorun enable
smart_eq_preset_switcher --autorun disable
```

On Windows this writes a per-user Run registry entry. On Linux it writes both:

```text
~/.config/autostart/smart-eq-preset-switcher.desktop
~/.config/systemd/user/smart-eq-preset-switcher.service
```

The Linux path also attempts to enable user lingering through `loginctl enable-linger` so boot sync can run before a graphical login when the system supports it.



## AutoEQ target variants

AutoEQ import now supports target-aware variants instead of assuming GraphicEQ only:

- **Auto target**: prefers ParametricEQ / `Filter:` output for Linux PipeWire/EasyEffects and Windows Equalizer APO/Peace, then falls back to GraphicEQ.
- **ParametricEQ / Filter**: best for PipeWire filter-chain `param_eq`, EasyEffects-style PEQ workflows, Equalizer APO and Peace.
- **GraphicEQ**: fallback for simple graphic equalizers and manual editing.

This matters because AutoEq itself produces settings for many equalizer apps and says the user must apply them with the selected equalizer app. The app therefore should not hard-code one AutoEQ format forever.

## EQ backend status and Linux setup

The GUI exposes an **EQ backend** status panel. On Linux this panel is intentionally honest:

- `System EQ not connected` means a preset is active in SmartEQPresetSwitcher, but no Linux system EQ export/setup file exists yet.
- `Linux EQ export ready` means the active preset has been exported and a PipeWire filter-chain snippet exists.
- The setup action exports the active preset and writes:
  - `~/.config/SmartEQPresetSwitcher/linux-eq/active-equalizerapo.txt`
  - `~/.config/SmartEQPresetSwitcher/linux-eq/active-parametric-eq.txt`
  - `~/.config/pipewire/pipewire.conf.d/99-smart-eq-preset-switcher-parametric-eq.conf`

The generated PipeWire setup uses `libpipewire-module-filter-chain` with the builtin `param_eq` filter and points it at the active Equalizer APO / AutoEQ-style preset file. After first setup, the user may need to restart the PipeWire user services or log out/in, then select or route audio to the generated SmartEQPresetSwitcher EQ sink/source.

EasyEffects remains the recommended GUI EQ workflow when the user wants interactive Linux effects instead of a generated PipeWire filter-chain snippet. The GUI setup panel shows distro-specific install commands for Arch/pacman, Debian/apt, Fedora/dnf and openSUSE/zypper style systems. It can restart the current user PipeWire services after writing the config, but it does not run privileged package-manager commands automatically.

## Linux EQ export files

The Linux export path is:

```text
~/.config/SmartEQPresetSwitcher/linux-eq/
```

Generated files:

```text
active-equalizerapo.txt
active-parametric-eq.txt
```

The PipeWire snippet path is:

```text
~/.config/pipewire/pipewire.conf.d/99-smart-eq-preset-switcher-parametric-eq.conf
```

Restart PipeWire after changing system-level PipeWire configuration:

```bash
systemctl --user restart pipewire pipewire-pulse
```

## GUI shortcut

The GUI registers this global shortcut:

```text
Ctrl+Alt+E
```

It toggles the main window while the app is running in tray mode.

## Platform limitations

Equalizer APO install/reinstall and Device Selector are Windows-only backend actions. On Linux these GUI actions return clear errors instead of trying to launch PowerShell or Windows executables.

## Legacy migration

If a previous `SmartEqualizerAPOPresetsManager` installation exists, SmartEQPresetSwitcher migrates the old per-user config folder to the new name on startup when the new folder does not already exist.

Old managed config markers are also recognized, so rebuilding the active config upgrades them to the new SmartEQPresetSwitcher markers.

## Linux GUI and tray behavior

Normal desktop launches open the main GUI. Explicit tray/background launches use:

```bash
smart_eq_preset_switcher --tray
smart-eq-preset-switcher --tray
```

The tray menu uses the app's own About modal instead of a native blocking tray dialog on Linux. This avoids AppIndicator/KDE cases where a native dialog can fail to appear behind the tray menu.

KDE may show a WebKit/Tauri tray app as multiple processes. That is expected for GTK/WebKit applications and is not by itself a memory leak. Treat it as a leak only if RSS keeps growing while idle.

## Linux diagnostics

App log:

```bash
cat ~/.config/SmartEQPresetSwitcher/logs/application.log
```

Run foreground:

```bash
smart-eq-preset-switcher --gui 2>&1 | tee /tmp/smarteq-run.log
```

Systemd/journal/coredump checks:

```bash
journalctl --user -b --grep='SmartEQPresetSwitcher|smart-eq-preset-switcher|smart_eq_preset_switcher' --no-pager
journalctl -b _COMM=smart-eq-preset-switcher --no-pager
coredumpctl list smart-eq-preset-switcher smart_eq_preset_switcher
coredumpctl info smart-eq-preset-switcher
```

## Runtime diagnostics on Linux

If the tray appears but the GUI does not open, run the app from a terminal and inspect the app log plus systemd journal:

```bash
smart-eq-preset-switcher --gui 2>&1 | tee /tmp/smarteq-run.log
cat ~/.config/SmartEQPresetSwitcher/logs/application.log
journalctl --user -b --grep='SmartEQPresetSwitcher|smart-eq-preset-switcher|smart_eq_preset_switcher' --no-pager
coredumpctl list smart-eq-preset-switcher smart_eq_preset_switcher
```

A normal Tauri/WebKitGTK desktop run may show multiple processes: the main app, a WebKit web process and a WebKit network process. That is not by itself a memory leak; a leak means the RSS keeps growing while idle.

## KDE/Wayland Runtime Notes

The default GUI launch path is optimized for reliability on KDE/Wayland/NVIDIA systems:

- `GDK_BACKEND=x11` is forced by default unless `SMART_EQ_USE_WAYLAND=1` or `--wayland` is used.
- `WINIT_UNIX_BACKEND=x11` is forced for the same reason.
- WebKit accelerated compositing/DMABUF renderer are disabled by the packaged launcher for conservative KDE/NVIDIA behavior.
- The Linux `--gui` path initializes the tray by default again; use `SMART_EQ_DISABLE_TRAY=1` for troubleshooting.

This avoids the common runtime failure:

```text
Gdk-Message: Error 71 (Protocol error) dispatching to Wayland display.
```

Explicit native Wayland test:

```bash
SMART_EQ_USE_WAYLAND=1 smart-eq-preset-switcher --gui
smart-eq-preset-switcher --wayland --gui
```

Explicit tray/background mode:

```bash
smart-eq-preset-switcher --tray
```

Disable the GUI tray for troubleshooting:

```bash
SMART_EQ_DISABLE_TRAY=1 smart-eq-preset-switcher --gui
```


## Linux GUI notes

Equalizer APO repair controls are Windows-only and are hidden on Linux. Linux workflows use preset management, TUI/headless boot-sync, and exported EQ files instead. GUI preset mutations keep the tray refresh best-effort so a tray failure does not break preset operations.

The Linux backend status panel also detects whether PipeWire, pipewire-pulse/Pulse bridge, WirePlumber and EasyEffects are present or active, so the UI can separate “packages are installed” from “this preset has a PipeWire-routable parametric config”.


## Tray and Apply behavior

The Linux GUI starts with the tray enabled again. The packaged launcher forces X11/XWayland before GTK/WebKit/Tauri startup to avoid the KDE/Wayland protocol crash seen on some NVIDIA systems. To disable tray for troubleshooting:

```bash
SMART_EQ_DISABLE_TRAY=1 smart-eq-preset-switcher --gui
```

Apply on Linux now exports the active preset, attempts a user audio service reload, and then uses `pactl` to set/move streams to the generated EQ sink when available:

```bash
systemctl --user try-restart pipewire.service pipewire-pulse.service wireplumber.service
```

Parametric AutoEQ presets are written directly for PipeWire `param_eq`. GraphicEQ-only presets are converted to a 31-band parametric approximation and then loaded through the same PipeWire filter-chain path.
