# AGENTS.md

Repository instructions for coding agents working on SmartEQPresetSwitcher.

Keep this file short and practical. Do not expand it with generic advice. Agents should read the code first, reproduce issues where possible, and prefer small, targeted patches over broad rewrites.

## Project summary

SmartEQPresetSwitcher is a Tauri 2 + SvelteKit + Rust desktop app for EQ preset management.

Core stack:

- Frontend: SvelteKit, TypeScript, Tailwind-style utility classes.
- Backend: Rust, Tauri 2 commands, app state persisted in per-user config.
- Windows backend: Equalizer APO integration.
- Linux backend: PipeWire-oriented EQ export and filter-chain setup.
- Packaging: Arch `makepkg`, Debian bundle, optional AppImage, Windows Tauri bundle.

Current source version: `0.3.0`.

## Most important current design decision

Linux “Disable EQ” is intentionally a **soft bypass**, not a PipeWire graph teardown.

Do not reintroduce hard routing teardown for normal disable.

Current bypass behavior:

- Set `eq_disabled=true`.
- Clear active preset selection.
- Write a flat shadow parametric preset:

```text
Preamp: -0.1 dB
```

- Keep the PipeWire EQ pipeline/config alive.
- Re-enable by selecting/applying a preset again.

Never use normal Disable EQ to:

- delete PipeWire links,
- remove PipeWire config,
- remove active export files,
- force-move every stream back to hardware,
- tear down the filter-chain graph.

Those hard actions can mute system audio on PipeWire/WirePlumber and belong only in a future explicit reset/teardown action.

## Linux EQ architecture

Linux is not Equalizer APO. Linux support is compatibility/export tooling.

Important files:

```text
src-tauri/src/linux_eq.rs       # Linux export, GraphicEQ/parametric conversion, PipeWire config generation
src-tauri/src/commands.rs       # Tauri commands, backend status, Linux setup/bypass commands
src-tauri/src/state.rs          # persisted app metadata, groups, presets, eq_disabled
src-tauri/src/tui.rs            # TUI commands including disable
src-tauri/src/lib.rs            # app setup, tray, command registration
src/lib/types.ts                # frontend shared types
src/lib/tauri.ts                # invoke wrappers
src/routes/+page.svelte         # main GUI flow
src/lib/components/*            # UI panels/modals
```

Linux runtime paths:

```text
~/.config/SmartEQPresetSwitcher/linux-eq/active-equalizerapo.txt
~/.config/SmartEQPresetSwitcher/linux-eq/active-parametric-eq.txt
~/.config/pipewire/pipewire.conf.d/99-smart-eq-preset-switcher-parametric-eq.conf
```

Use PipeWire/WirePlumber tooling for diagnostics:

```bash
pactl get-default-sink
pactl list short sinks
pactl list short sink-inputs
wpctl status --name
pw-link -lI
journalctl --user -u pipewire.service -u wireplumber.service -b -n 120 --no-pager
```

## Windows EQ architecture

Windows-specific Equalizer APO behavior must stay behind Windows cfg gates where applicable.

Windows-only flows:

- APO install/reinstall helper.
- APO Device Selector.
- Registry `ConfigPath` detection/update.
- Windows Run-key autorun.

Linux UI must not show Windows APO troubleshooting controls.

## Build scripts and packaging

Scripts live in `scripts/`; do not add root-level bootstrap scripts.

Common commands:

```bash
npm ci
npm run check
bash -n scripts/*.sh
scripts/check-project.sh --strict-source
```

Linux builds:

```bash
scripts/bootstrap-linux.sh
scripts/build-linux.sh
scripts/build-linux.sh --arch
scripts/build-arch-package.sh
```

Arch package rules:

- Use `tauri build --no-bundle` for production asset embedding.
- Use `makepkg` for the final `.pkg.tar.zst`.
- Do not hand-roll pacman package metadata.
- `/usr/bin/smart-eq-preset-switcher` is a launcher wrapper.
- The real binary lives under `/usr/lib/smart-eq-preset-switcher/`.

Generated folders must not be committed:

```text
node_modules/
.svelte-kit/
build/
dist/
src-tauri/target/
src-tauri/gen/
```

## Validation expectations

Before saying a patch is done, run the strongest available checks:

```bash
npm run check
bash -n scripts/*.sh
scripts/check-project.sh --strict-source
```

If Rust is available:

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

If packaging was touched on Arch:

```bash
scripts/build-arch-package.sh
sudo pacman -U dist/arch/smart-eq-preset-switcher-*.pkg.tar.zst
```

If Linux audio behavior was touched, include manual runtime checks in the response:

```bash
smart-eq-preset-switcher --gui 2>&1 | tee /tmp/smarteq-run.log
cat ~/.config/SmartEQPresetSwitcher/logs/application.log | tail -120
pactl get-default-sink
pactl list short sinks
wpctl status --name
pw-link -lI | grep -i 'smart\|eq\|alsa_output'
```

## Documentation rules

When behavior changes, update docs in the same patch:

- `README.md` for user-facing behavior.
- `docs/BUILDING.md` for build/bootstrap/package changes.
- `docs/TUI_AND_LINUX.md` for Linux runtime, TUI and PipeWire behavior.
- `CHANGELOG.md` for release-level changes.

Use honest wording. Do not claim Linux system EQ is “connected” unless the app can actually verify the relevant state.

## Coding style rules

- Prefer small, direct functions over large rewrites.
- Do not swallow errors in new code; log stdout/stderr/status for external commands.
- Keep UI status messages clear and user-facing.
- Keep platform-specific code behind `#[cfg(target_os = "...")]` where possible.
- Keep command names stable unless the frontend and docs are updated together.
- Do not add new dependencies without a clear reason.
- Do not regenerate `src-tauri/gen/` into commits.

## Common traps

- Do not confuse `smart_eq_preset_switcher` with installed wrapper `smart-eq-preset-switcher`.
- Do not run `.deb` as the preferred package on Arch; use the pacman package.
- Do not assume `pactl` names, `wpctl` IDs and PipeWire node names are interchangeable.
- Do not make AppImage the default Linux output on Arch.
- Do not reintroduce `rustup` as mandatory on Arch when system `rust` exists.
- Do not bring back old branding except for explicit migration compatibility.

## Current source of truth for bypass

The simplified Linux bypass strategy is deliberate:

```text
Disable EQ = flat shadow preset, Preamp: -0.1 dB
```

If a future task asks for “proper hard disable”, first reproduce the bug and add it as a separate explicit action such as “Reset PipeWire EQ graph”. Do not change the normal Disable EQ path back into graph teardown.
