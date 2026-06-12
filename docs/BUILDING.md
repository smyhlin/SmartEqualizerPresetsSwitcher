# Building and Bootstrapping

SmartEQPresetSwitcher is a Tauri 2, SvelteKit, TypeScript and Rust desktop app. It supports Windows x64 and Linux x64 builds.

## Clean source rule

Do not commit generated dependency/build folders:

- `node_modules/`
- `.svelte-kit/`
- `build/`
- `src-tauri/target/`
- `dist/`

Use `package-lock.json` and `src-tauri/Cargo.lock` as the reproducible dependency anchors.

## Naming convention

Current product name: `SmartEQPresetSwitcher`.

Current executable and Rust crate stem: `smart_eq_preset_switcher`.

Current Linux package/binary alias: `smart-eq-preset-switcher`.

The older `SmartEqualizerAPOPresetsManager` name is only allowed in migration compatibility code.

## Linux x64 bootstrap

Supported bootstrap targets:

- Arch Linux x86_64 with `pacman`
- Debian or Ubuntu x86_64 with `apt-get`

Run:

```bash
scripts/bootstrap-linux.sh
```

Useful flags:

```bash
scripts/bootstrap-linux.sh --no-install
scripts/bootstrap-linux.sh --skip-check
scripts/bootstrap-linux.sh --skip-npm
scripts/bootstrap-linux.sh --rust
scripts/bootstrap-linux.sh --rustup
```

### Arch Rust provider note

Arch Linux has two common Rust providers:

- `rust`, the repository Rust toolchain package.
- `rustup`, the Rust toolchain manager package.

These packages conflict with each other. The bootstrap script runs in `auto` mode by default:

- If `rustc` and `cargo` already exist, it keeps the existing toolchain.
- If `rustup` already exists, it keeps rustup and ensures the stable toolchain is installed.
- If neither provider exists, it installs `rustup` by default.
- If the Arch `rust` package is installed but `rustc` or `cargo` are missing from `PATH`, it does **not** try to install `rustup`; it prints a PATH/toolchain warning instead.

Use this when you explicitly want repository Rust:

```bash
scripts/bootstrap-linux.sh --rust
```

Use this when you explicitly want rustup:

```bash
scripts/bootstrap-linux.sh --rustup
```

If pacman reports `rustup and rust are in conflict`, keep one provider only. For an existing Arch `rust` install, rerun:

```bash
scripts/bootstrap-linux.sh --rust
```

The script installs or verifies:

- Node.js and npm
- Rust stable toolchain
- WebKitGTK 4.1 development packages
- GTK3
- AppIndicator/Ayatana indicator support
- librsvg
- libxdo/xdotool support
- tar and zstd for packaging
- FUSE compatibility package when available, useful for AppImage workflows

For Windows cross-build support, add `--cross-windows`:

```bash
scripts/bootstrap-linux.sh --cross-windows
```

This additionally installs LLVM, LLD, clang, NSIS (from AUR on Arch via `yay`), the
`x86_64-pc-windows-msvc` Rust target, and `cargo-xwin`. The cross-build scripts
(`build-windows-from-linux.sh`, `build-linux.sh --windows`) pass this flag
automatically, so you do not need to specify it manually.

## Linux builds

The default Linux build now auto-selects the correct package target for the host distro:

- Arch/pacman systems build an Arch package: `dist/arch/*.pkg.tar.zst`
- Debian/Ubuntu systems build a Debian package: `dist/deb/*.deb`
- Unknown Linux systems fall back to AppImage.

Run:

```bash
scripts/build-linux.sh
```

Useful flags:

```bash
scripts/build-linux.sh --auto
scripts/build-linux.sh --arch
scripts/build-linux.sh --deb
scripts/build-linux.sh --appimage
scripts/build-linux.sh --windows
scripts/build-linux.sh --all
scripts/build-linux.sh --clean
scripts/build-linux.sh --skip-bootstrap
scripts/build-linux.sh --skip-check
```

### Arch output

On Arch, use the default auto mode or force Arch packaging:

```bash
scripts/build-linux.sh
scripts/build-linux.sh --arch
npm run build:linux
npm run build:linux:arch
```

Expected Arch output:

```text
dist/arch/smart-eq-preset-switcher-<version>-1-x86_64.pkg.tar.zst
```

Install it with pacman:

```bash
sudo pacman -U dist/arch/smart-eq-preset-switcher-*.pkg.tar.zst
```

The Arch package script builds the Tauri release binary with `tauri build --no-bundle`, so production frontend assets are embedded from `build.frontendDist` without invoking AppImage/linuxdeploy. It then uses `makepkg` to produce a real pacman package and does not hand-roll package metadata.

### Debian output

Debian packages are useful for Debian/Ubuntu users, not for installing on Arch. To build a proper `.deb`:

```bash
scripts/build-linux.sh --deb
# or directly
scripts/build-deb.sh
npm run build:deb
npm run build:linux:deb
```

The Debian package builder mirrors the Arch builder: it produces the Tauri release binary with `tauri build --no-bundle`, then wraps it with `dpkg-deb` including a launcher wrapper, desktop file, icon, license, and proper `DEBIAN/control` metadata. Unlike the basic Tauri deb bundler, it handles Wayland/X11 fallback and provides the `smart-eq-preset-switcher` command.

On Arch, building `.deb` requires `dpkg-deb` from the `dpkg` package — install it only if you intentionally need to produce Debian packages on Arch.


### KDE/Wayland runtime note

The Arch package installs `/usr/bin/smart-eq-preset-switcher` as a small launcher wrapper. By default it forces conservative XWayland/X11 WebKitGTK settings before the real binary starts:

```bash
GDK_BACKEND=x11
WINIT_UNIX_BACKEND=x11
GDK_RENDERING=image
WEBKIT_DISABLE_DMABUF_RENDERER=1
WEBKIT_DISABLE_COMPOSITING_MODE=1
```

This avoids the KDE/Wayland/WebKitGTK `Gdk-Message: Error 71 (Protocol error) dispatching to Wayland display` failure seen on some NVIDIA setups. Native Wayland remains opt-in for testing:

```bash
SMART_EQ_USE_WAYLAND=1 smart-eq-preset-switcher --gui
```

### AppImage output

AppImage is optional:

```bash
scripts/build-linux.sh --appimage
npm run build:linux:appimage
```

AppImage bundling depends on Tauri/linuxdeploy and can fail on some distro/runtime combinations even after the Rust application itself compiled correctly. On Arch, prefer the native pacman package:

```bash
scripts/build-linux.sh --arch
```

## Arch package build

Run:

```bash
scripts/build-arch-package.sh
```

Useful flags:

```bash
scripts/build-arch-package.sh --clean
scripts/build-arch-package.sh --skip-bootstrap
scripts/build-arch-package.sh --skip-check
```

Expected output:

```text
dist/arch/smart-eq-preset-switcher-<version>-1-x86_64.pkg.tar.zst
```

The Arch package script builds the release binary through `tauri build --no-bundle`, prepares a temporary `makepkg` workspace, writes a generated `PKGBUILD`, installs a launcher wrapper, the desktop file, icon, license and binary through `package()`, and lets `makepkg` create valid pacman package metadata.


## Windows NSIS cross-build from Linux

Prefer building Windows installers on Windows or CI. Tauri supports cross-compiling Windows apps from Linux/macOS with caveats, and the documented cross-host path is **NSIS only**; MSI/WiX installers must be created on Windows.

Run from Linux:

```bash
scripts/build-linux.sh --windows
# or directly
scripts/build-windows-from-linux.sh
npm run build:windows:linux
```

Useful flags:

```bash
scripts/build-windows-from-linux.sh --clean
scripts/build-windows-from-linux.sh --skip-bootstrap
scripts/build-windows-from-linux.sh --skip-check
scripts/build-windows-from-linux.sh --no-install
scripts/build-windows-from-linux.sh --target=x86_64-pc-windows-msvc
```

Required Linux-side tools:

- `rustup` with `x86_64-pc-windows-msvc`
- `cargo-xwin`
- `makensis`
- `llvm-rc`
- `lld-link`

On Arch, the script can install official system tools with:

```bash
sudo pacman -S --needed llvm lld clang
```

**Note:** `makensis` (NSIS) is in the AUR. Install it with an AUR helper:

```bash
paru -S nsis   # or yay -S nsis
```

On Debian/Ubuntu, the script can install:

```bash
sudo apt install nsis llvm lld clang
```

The script installs `cargo-xwin` with Cargo if it is missing, then runs:

```bash
npm run tauri -- build --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis
```

Expected output:

```text
src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/*.exe
```

## Windows x64 bootstrap

The script runs from a regular Command Prompt or Windows Terminal. It
automatically installs everything through `winget` when available:

- Node.js LTS
- Rustup
- Microsoft Edge WebView2 Runtime
- Visual Studio 2022 Build Tools (with C++ workload)

If `cl.exe` is still not on `PATH` after installation, the script attempts to add the
C++ workload via the Visual Studio Installer automatically. After all automated steps,
open a **Developer Command Prompt for VS 2022** from the Start Menu and rerun the script
if MSVC is still missing.

Run:

```bat
scripts\bootstrap-windows.bat
```

Useful flags:

```bat
scripts\bootstrap-windows.bat --no-install
scripts\bootstrap-windows.bat --skip-check
scripts\bootstrap-windows.bat --skip-npm
```

The script also ensures the `x86_64-pc-windows-msvc` Rust target is installed.

After installing Node.js or Rustup, reopen the terminal if commands are still missing from `PATH`.

## Windows NSIS installer build

Run:

```bat
scripts\build-windows.bat
```

Useful flags:

```bat
scripts\build-windows.bat --clean
scripts\build-windows.bat --skip-bootstrap
scripts\build-windows.bat --skip-check
```

Before building, the script verifies:

- Node.js, cargo, rustc are available
- `cl.exe` (MSVC) is on PATH — fails with a clear error if not, to prevent cryptic build failures
- `x86_64-pc-windows-msvc` Rust target is installed — installs it automatically if missing
- makensis availability (warns only; Tauri bundles NSIS on Windows)

Expected output:

```text
src-tauri\target\x86_64-pc-windows-msvc\release\bundle\nsis\*.exe
```

If MSVC is not on `PATH`, run from a **Developer Command Prompt for VS 2022**.

## Project checks

Run the full project sanity check script:

```bash
scripts/check-project.sh
```

Useful flags:

```bash
scripts/check-project.sh --skip-npm
scripts/check-project.sh --skip-cargo
scripts/check-project.sh --strict-source --skip-npm --skip-cargo
scripts/check-project.sh --strict-source --fail-local-generated --skip-npm --skip-cargo
```

It validates shell script syntax, verifies Windows batch scripts are present, scans for placeholder markers and runs npm/Rust checks when available.

Generated folders such as `node_modules`, `.svelte-kit`, `build`, `dist` and `src-tauri/target` are normal after bootstrap/build commands.

Use `--strict-source` during normal development and after `npm ci`. It validates source/archive hygiene, fails if generated folders are tracked by git, and only reports local generated folders without failing.

Use `--strict-source --fail-local-generated` only in a clean temporary packaging tree, right before producing a source archive. That mode intentionally fails if local generated folders exist.

## npm aliases

The same workflows are also exposed through `package.json`:

```bash
npm run bootstrap:linux
npm run build:linux
npm run build:linux:deb
npm run build:linux:appimage
npm run build:linux:all
npm run build:linux:arch
npm run build:linux:windows
npm run build:windows:linux
npm run build:arch
npm run build:deb
npm run check:project
npm run check:source
npm run check:clean-source
```

`check:source` is safe after `npm ci`. `check:clean-source` is intentionally strict and should be used only before packaging from a clean tree.

On Windows:

```bat
npm run bootstrap:windows
npm run build:windows
```

## Direct Tauri commands

For manual work:

```bash
npm ci
npm run check
npm run tauri -- dev
npm run tauri -- build --bundles deb
npm run tauri -- build --bundles appimage
npm run tauri -- build --bundles nsis
```

Use platform-native builds. Build Linux bundles on Linux and Windows installers on Windows.
