#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_BOOTSTRAP=1
RUN_CHECK=1
CLEAN=0

for arg in "$@"; do
  case "$arg" in
    --skip-bootstrap) RUN_BOOTSTRAP=0 ;;
    --no-check|--skip-check) RUN_CHECK=0 ;;
    --clean) CLEAN=1 ;;
    -h|--help)
      cat <<USAGE
Build a Debian/Ubuntu x86_64 .deb package for SmartEQPresetSwitcher.

Usage:
  scripts/build-deb.sh [--skip-bootstrap] [--skip-check] [--clean]

Output:
  dist/deb/smart-eq-preset-switcher_<version>_amd64.deb

Notes:
  This script builds the binary through \`tauri build --no-bundle\`, then uses
  dpkg-deb for final package assembly. It produces a proper .deb with desktop
  file, icon, license, and post-install configuration for PipeWire/Linux EQ.
USAGE
      exit 0
      ;;
    *) echo "Unknown argument: $arg" >&2; exit 2 ;;
  esac
done

info() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
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

[[ "$(uname -s)" == "Linux" ]] || fail "This script must be run on Linux."
case "$(uname -m)" in
  x86_64|amd64) ;;
  *) fail "Only x86_64 .deb packages are supported." ;;
esac

cd "$ROOT_DIR"

if [[ "$CLEAN" -eq 1 ]]; then
  info "Cleaning .deb package output."
  TARGET_DIR="$(target_root)"
  rm -rf dist/deb "$TARGET_DIR/release/bundle"
fi

if [[ "$RUN_BOOTSTRAP" -eq 1 ]]; then
  info "Running Linux bootstrap."
  if [[ "$RUN_CHECK" -eq 1 ]]; then
    "$ROOT_DIR/scripts/bootstrap-linux.sh"
  else
    "$ROOT_DIR/scripts/bootstrap-linux.sh" --skip-check
  fi
else
  info "Skipping bootstrap."
fi

if [[ "$RUN_CHECK" -eq 1 && "$RUN_BOOTSTRAP" -eq 0 ]]; then
  info "Running project check."
  npm run check
fi

for cmd in node npm cargo dpkg-deb fakeroot; do
  have "$cmd" || fail "Required command not found: $cmd"
done

PKG_NAME="smart-eq-preset-switcher"
PKG_VERSION="$(node -p "require('./package.json').version")"
PKG_ARCH="amd64"
OUT_DIR="$ROOT_DIR/dist/deb"
DEB_DIR="$OUT_DIR/debian"
BIN_NAME="smart_eq_preset_switcher"
ICON_NAME="com.myhli.smarteqpresetswitcher"
DEB_BIN_NAME="smart-eq-preset-switcher"
DEB="$OUT_DIR/${PKG_NAME}_${PKG_VERSION}_${PKG_ARCH}.deb"

info "Building release binary with Tauri production asset embedding, without Tauri bundlers."
npm run tauri -- build --no-bundle

TARGET_DIR="$(target_root)"
BIN="$TARGET_DIR/release/$BIN_NAME"
[[ -x "$BIN" ]] || fail "Release binary not found: $BIN"

info "Preparing dpkg-deb workspace."
rm -rf "$DEB_DIR"
mkdir -p "$DEB_DIR/DEBIAN"
mkdir -p "$DEB_DIR/usr/lib/$PKG_NAME"
mkdir -p "$DEB_DIR/usr/bin"
mkdir -p "$DEB_DIR/usr/share/applications"
mkdir -p "$DEB_DIR/usr/share/icons/hicolor/256x256/apps"
mkdir -p "$DEB_DIR/usr/share/doc/$PKG_NAME"
mkdir -p "$DEB_DIR/usr/share/licenses/$PKG_NAME"

install -Dm755 "$BIN" "$DEB_DIR/usr/lib/$PKG_NAME/$BIN_NAME"
install -Dm644 "$ROOT_DIR/LICENSE" "$DEB_DIR/usr/share/licenses/$PKG_NAME/LICENSE"
install -Dm644 "$ROOT_DIR/src-tauri/icons/icon.png" \
  "$DEB_DIR/usr/share/icons/hicolor/256x256/apps/$ICON_NAME.png"

cat > "$DEB_DIR/usr/bin/$DEB_BIN_NAME" <<'WRAPPER'
#!/usr/bin/env bash
set -euo pipefail

# KDE Plasma + NVIDIA + WebKitGTK can crash on native Wayland with:
#   Gdk-Message: Error 71 (Protocol error) dispatching to Wayland display
# Force XWayland/X11 by default for the GUI process. Users can opt into native
# Wayland for testing with SMART_EQ_USE_WAYLAND=1.
if [[ "${SMART_EQ_USE_WAYLAND:-0}" != "1" ]]; then
  export GDK_BACKEND=x11
  export WINIT_UNIX_BACKEND=x11
  export GDK_RENDERING=image
  export WEBKIT_DISABLE_DMABUF_RENDERER=1
  export WEBKIT_DISABLE_COMPOSITING_MODE=1
fi

exec /usr/lib/smart-eq-preset-switcher/smart_eq_preset_switcher "$@"
WRAPPER
chmod +x "$DEB_DIR/usr/bin/$DEB_BIN_NAME"

cat > "$DEB_DIR/usr/share/applications/$ICON_NAME.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=SmartEQPresetSwitcher
Comment=Cross-platform EQ preset switcher with Linux EQ exports
Exec=$DEB_BIN_NAME --gui
Icon=$ICON_NAME
Terminal=false
Categories=Audio;AudioVideo;Utility;
StartupNotify=true
DESKTOP

DEB_SIZE="$(du -sk "$DEB_DIR/usr" | cut -f1)"

cat > "$DEB_DIR/DEBIAN/control" <<CONTROL
Package: $PKG_NAME
Version: $PKG_VERSION
Architecture: $PKG_ARCH
Maintainer: myhlinc <myhlinc@users.noreply.github.com>
Description: Cross-platform EQ preset switcher with Linux EQ exports
 Organizes, edits, applies, imports, exports, and backs up EQ presets.
 On Linux provides GUI, tray, boot-sync, TUI and PipeWire-oriented EQ
 export workflows. On Windows integrates with Equalizer APO.
Homepage: https://github.com/smyhlin/SmartEqualizerPresetsSwitcher
License: MIT
Section: sound
Priority: optional
Installed-Size: $DEB_SIZE
Depends: libgtk-3-0 (>= 3.24), libwebkit2gtk-4.1-0 (>= 2.42),
 libayatana-appindicator3-1 (>= 0.5), librsvg2-common (>= 2.52)
Recommends: pipewire (>= 1.0)
CONTROL

if [[ -f "$ROOT_DIR/scripts/resources/smart-eq-postinst.sh" ]]; then
  cp "$ROOT_DIR/scripts/resources/smart-eq-postinst.sh" "$DEB_DIR/DEBIAN/postinst"
  chmod 755 "$DEB_DIR/DEBIAN/postinst"
fi
if [[ -f "$ROOT_DIR/scripts/resources/smart-eq-prerm.sh" ]]; then
  cp "$ROOT_DIR/scripts/resources/smart-eq-prerm.sh" "$DEB_DIR/DEBIAN/prerm"
  chmod 755 "$DEB_DIR/DEBIAN/prerm"
fi

info "Building .deb package with fakeroot dpkg-deb."
mkdir -p "$OUT_DIR"
fakeroot dpkg-deb --build "$DEB_DIR" "$DEB"

[[ -f "$DEB" ]] || fail "dpkg-deb did not produce expected package: $DEB"

info "Debian package artifact: $DEB"
info "Install with: sudo dpkg -i '$DEB'"
info "Then resolve missing deps: sudo apt-get install -f"