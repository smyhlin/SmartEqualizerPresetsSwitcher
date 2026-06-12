#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_BOOTSTRAP=1
RUN_CHECK=1
CLEAN=0
INSTALL_TOOLS=1
TARGET="x86_64-pc-windows-msvc"
BUNDLES="nsis"

for arg in "$@"; do
  case "$arg" in
    --skip-bootstrap) RUN_BOOTSTRAP=0 ;;
    --no-check|--skip-check) RUN_CHECK=0 ;;
    --clean) CLEAN=1 ;;
    --no-install|--skip-system-install) INSTALL_TOOLS=0 ;;
    --nsis) BUNDLES="nsis" ;;
    --target=*) TARGET="${arg#--target=}" ;;
    -h|--help)
      cat <<USAGE
Cross-build a Windows x64 NSIS installer from Linux.

Usage:
  scripts/build-windows-from-linux.sh [options]

Options:
  --clean                 Remove previous Windows cross-build output first.
  --skip-bootstrap        Do not run scripts/bootstrap-linux.sh.
  --skip-check            Do not run npm/Svelte checks.
  --no-install            Do not install Linux system packages, only verify tools.
  --nsis                  Build NSIS installer. Default and only Linux cross-build bundle.
  --target=<triple>       Rust Windows target. Default: x86_64-pc-windows-msvc.

Outputs:
  src-tauri/target/<target>/release/bundle/nsis/SmartEQPresetSwitcher-*-setup.exe

Notes:
  Tauri's native NSIS bundler is not available on Linux, so the script
  builds the binary with cargo-xwin and packages it with makensis directly.
  This produces a minimal installer (no WebView2 runtime bootstrapper).
  MSI/WiX installers must be built on Windows.
USAGE
      exit 0
      ;;
    *) echo "Unknown argument: $arg" >&2; exit 2 ;;
  esac
done

info() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33mWARN:\033[0m %s\n' "$*"; }
fail() { printf '\033[1;31mERROR:\033[0m %s\n' "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

target_root() {
  if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
    printf '%s\n' "$CARGO_TARGET_DIR"
    return
  fi

  local cargo_config
  cargo_config="$ROOT_DIR/.cargo/config.toml"
  if [[ -f "$cargo_config" ]]; then
    local cfg_target
    cfg_target="$(grep -E '^\s*target-dir\s*=' "$cargo_config" 2>/dev/null | sed -E 's/.*=\s*"([^"]+)".*/\1/')"
    if [[ -n "$cfg_target" ]]; then
      if [[ "$cfg_target" = /* ]]; then
        printf '%s\n' "$cfg_target"
      else
        # Resolve relative to the parent of .cargo/ (Cargo convention)
        printf '%s\n' "$(cd "$ROOT_DIR" && realpath "$cfg_target")"
      fi
      return
    fi
  fi

  for candidate in \
    "$ROOT_DIR/../.cargo-target" \
    "$ROOT_DIR/.cargo-target" \
    "$ROOT_DIR/src-tauri/target"
  do
    if [[ -d "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return
    fi
  done

  printf '%s\n' "$ROOT_DIR/src-tauri/target"
}

install_cross_packages() {
  if [[ "$INSTALL_TOOLS" -eq 0 ]]; then
    info "Skipping system package installation for Windows cross-build."
    return
  fi

  local extra=()
  if [[ "$RUN_BOOTSTRAP" -eq 0 || "$RUN_CHECK" -eq 0 ]]; then
    extra+=(--skip-check)
  fi
  if [[ "$RUN_CHECK" -eq 1 ]]; then
    extra+=(--skip-npm)
  fi
  info "Installing Windows cross-build dependencies via bootstrap-linux.sh --cross-windows."
  "$ROOT_DIR/scripts/bootstrap-linux.sh" "${extra[@]}" --cross-windows
}

[[ "$(uname -s)" == "Linux" ]] || fail "This cross-build script must be run on Linux."
case "$(uname -m)" in
  x86_64|amd64) ;;
  *) fail "Only Linux x86_64 hosts are supported. Current arch: $(uname -m)" ;;
esac

cd "$ROOT_DIR"

case "$BUNDLES" in
  nsis) ;;
  *) fail "Only NSIS is supported for Windows cross-builds from Linux." ;;
esac

if [[ "$CLEAN" -eq 1 ]]; then
  info "Cleaning Windows cross-build outputs."
  TARGET_DIR="$(target_root)"
  rm -rf build .svelte-kit "$TARGET_DIR/$TARGET/release/bundle" "$TARGET_DIR/release/bundle/nsis"
fi

if [[ "$RUN_BOOTSTRAP" -eq 1 ]]; then
  info "Running Linux bootstrap for host build dependencies."
  if [[ "$RUN_CHECK" -eq 1 ]]; then
    "$ROOT_DIR/scripts/bootstrap-linux.sh"
  else
    "$ROOT_DIR/scripts/bootstrap-linux.sh" --skip-check
  fi
else
  info "Skipping Linux bootstrap."
fi

install_cross_packages

missing=()
for cmd in node npm cargo rustc llvm-rc lld-link; do
  have "$cmd" || missing+=("$cmd")
done
if (( ${#missing[@]} > 0 )); then
  fail "Missing Windows cross-build tools: ${missing[*]}"
fi

# makensis may come from AUR — allow but warn
if ! have makensis; then
  warn "makensis (NSIS) not found. It should have been installed by bootstrap."
  warn "Cross-build will likely fail without it."
fi

if ! have rustup; then
  if [[ -x "$HOME/.cargo/bin/rustup" ]]; then
    export PATH="$HOME/.cargo/bin:$PATH"
    info "Found rustup in ~/.cargo/bin (not in PATH). Added it."
  else
    info "Installing rustup side-by-side with system Rust for MSVC cross-build target."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --no-modify-path --default-toolchain stable 2>&1
    export PATH="$HOME/.cargo/bin:$PATH"
  fi
fi

info "Ensuring Rust Windows target is installed: $TARGET"
rustup target add "$TARGET" 2>/dev/null || true

if ! have cargo-xwin; then
  info "Installing cargo-xwin."
  cargo install --locked cargo-xwin
fi

if [[ "$RUN_CHECK" -eq 1 && "$RUN_BOOTSTRAP" -eq 0 ]]; then
  info "Running project check."
  npm run check
fi

# Ensure rustup's cargo is used for cross-build (it has the MSVC target)
export PATH="$HOME/.cargo/bin:$PATH"
BUILD_DIR="$ROOT_DIR/src-tauri/target"
info "Building cross-compiled binary with cargo-xwin (NSIS bundler not available on Linux)."
CARGO_TARGET_DIR="$BUILD_DIR" npm run tauri -- build --no-bundle --runner cargo-xwin --target "$TARGET"

BINARY="$BUILD_DIR/$TARGET/release/smart_eq_preset_switcher.exe"
INSTALLER_DIR="$BUILD_DIR/$TARGET/release/bundle/nsis"

if [[ ! -f "$BINARY" ]]; then
  fail "Cross-compiled binary not found at $BINARY"
fi

info "Cross-compiled binary found: $BINARY"
mkdir -p "$INSTALLER_DIR"

info "Creating NSIS installer..."
NSIS_SCRIPT="$INSTALLER_DIR/installer.nsi"
APP_EXE="smart_eq_preset_switcher.exe"

# Copy the binary into the bundler directory so File uses a relative path.
cp "$BINARY" "$INSTALLER_DIR/$APP_EXE"

cat > "$NSIS_SCRIPT" <<'NSISEOF'
Unicode true
ManifestDPIAware true
!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "x64.nsh"

Name "SmartEQPresetSwitcher"
OutFile "SmartEQPresetSwitcher-0.3.0-x64-setup.exe"
InstallDir "$PROGRAMFILES64\SmartEQPresetSwitcher"
InstallDirRegKey HKLM "Software\SmartEQPresetSwitcher" ""

RequestExecutionLevel admin

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

Section "Install"
  SetOutPath "$INSTDIR"
  File "smart_eq_preset_switcher.exe"

  WriteRegStr HKLM "Software\SmartEQPresetSwitcher" "" "$INSTDIR"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\SmartEQPresetSwitcher" "DisplayName" "SmartEQPresetSwitcher"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\SmartEQPresetSwitcher" "UninstallString" "$INSTDIR\uninstall.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\SmartEQPresetSwitcher" "DisplayIcon" "$INSTDIR\smart_eq_preset_switcher.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\SmartEQPresetSwitcher" "DisplayVersion" "0.3.0"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\SmartEQPresetSwitcher" "Publisher" "myhlinc"

  WriteUninstaller "$INSTDIR\uninstall.exe"
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\smart_eq_preset_switcher.exe"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\SmartEQPresetSwitcher"
  DeleteRegKey HKLM "Software\SmartEQPresetSwitcher"
SectionEnd
NSISEOF

info "Running makensis..."
makensis -INPUTCHARSET UTF8 "$NSIS_SCRIPT" 2>&1
# OutFile is relative to the script dir when -INPUTCHARSET is used,
# but makensis still writes it relative to CWD. Explicit path below.
if [[ -f "$INSTALLER_DIR/SmartEQPresetSwitcher-0.3.0-x64-setup.exe" ]]; then
  info "NSIS installer created: $INSTALLER_DIR/SmartEQPresetSwitcher-0.3.0-x64-setup.exe"
else
  warn "NSIS installer may have been written to a different location. Check output above."
fi

info "Build artifacts:"
find "$INSTALLER_DIR" -maxdepth 1 -type f -name '*.exe' -print | sort || true
