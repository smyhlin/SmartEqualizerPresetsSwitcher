#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_BOOTSTRAP=1
RUN_CHECK=1
CLEAN=0
MODE="auto"

for arg in "$@"; do
  case "$arg" in
    --skip-bootstrap) RUN_BOOTSTRAP=0 ;;
    --no-check|--skip-check) RUN_CHECK=0 ;;
    --clean) CLEAN=1 ;;
    --auto) MODE="auto" ;;
    --arch|--pacman|--pkgbuild|--pkg.tar.zst) MODE="arch" ;;
    --deb) MODE="deb" ;;
    --appimage) MODE="appimage" ;;
    --all|--deb-appimage|--appimage-deb) MODE="deb,appimage" ;;
    -h|--help)
      cat <<USAGE
Build Linux x64 artifacts for SmartEQPresetSwitcher.

Usage:
  scripts/build-linux.sh [options]

Default:
  auto                  Arch/pacman -> Arch .pkg.tar.zst; Debian/Ubuntu -> .deb.

Build target options:
  --auto                Auto-select package target for this distro. Default.
  --arch                Build Arch pacman package (.pkg.tar.zst).
  --deb                 Build Debian package (.deb).
  --appimage            Build AppImage only. Uses Tauri/linuxdeploy; may be distro-sensitive.
  --all                 Build Debian package and AppImage.

Other options:
  --skip-bootstrap      Do not run scripts/bootstrap-linux.sh before building.
  --skip-check          Do not run Svelte/project checks during bootstrap/build.
  --clean               Remove previous build outputs first.

Outputs:
  dist/arch/*.pkg.tar.zst
  src-tauri/target/release/bundle/deb/*.deb
  src-tauri/target/release/bundle/appimage/*.AppImage
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

is_arch_like() {
  [[ -f /etc/arch-release ]] || have pacman
}

is_debian_like() {
  [[ -f /etc/debian_version ]] || have apt-get
}

[[ "$(uname -s)" == "Linux" ]] || fail "This build script must be run on Linux."
case "$(uname -m)" in
  x86_64|amd64) ;;
  *) fail "Only Linux x86_64 builds are supported." ;;
esac

cd "$ROOT_DIR"

if [[ "$MODE" == "auto" ]]; then
  if is_arch_like; then
    MODE="arch"
  elif is_debian_like; then
    MODE="deb"
  else
    MODE="appimage"
    warn "Unknown Linux distro. Falling back to AppImage."
  fi
fi

if [[ "$CLEAN" -eq 1 ]]; then
  info "Cleaning build outputs."
  rm -rf build .svelte-kit dist/arch src-tauri/target/release/bundle
fi

if [[ "$RUN_BOOTSTRAP" -eq 1 ]]; then
  info "Running Linux bootstrap."
  if [[ "$RUN_CHECK" -eq 1 ]]; then
    scripts/bootstrap-linux.sh
  else
    scripts/bootstrap-linux.sh --skip-check
  fi
else
  info "Skipping bootstrap."
fi

if [[ "$RUN_CHECK" -eq 1 && "$RUN_BOOTSTRAP" -eq 0 ]]; then
  info "Running project check."
  npm run check
fi

case "$MODE" in
  arch)
    info "Building Arch pacman package."
    # build-linux.sh has already handled bootstrap/checks above, so do not
    # repeat the same Svelte/project check inside the delegated Arch builder.
    args=(--skip-bootstrap --skip-check)
    [[ "$CLEAN" -eq 1 ]] && args+=(--clean)
    scripts/build-arch-package.sh "${args[@]}"
    info "Build artifacts:"
    find dist/arch -type f -name '*.pkg.tar.zst' -print | sort || true
    ;;
  deb)
    if ! have dpkg-deb; then
      fail "dpkg-deb is required for .deb bundling. On Arch, install 'dpkg' manually only if you really need .deb output, or run './scripts/build-linux.sh --arch'."
    fi
    info "Building Debian package."
    npm run tauri -- build --bundles deb
    info "Build artifacts:"
    find src-tauri/target/release/bundle/deb -type f -name '*.deb' -print | sort || true
    ;;
  appimage)
    warn "AppImage bundling uses Tauri/linuxdeploy and can fail on some distro/runtime combinations."
    warn "On Arch, prefer './scripts/build-linux.sh --arch'."
    info "Building AppImage."
    npm run tauri -- build --bundles appimage
    info "Build artifacts:"
    find src-tauri/target/release/bundle/appimage -type f -name '*.AppImage' -print | sort || true
    ;;
  deb,appimage|appimage,deb)
    if ! have dpkg-deb; then
      fail "dpkg-deb is required for .deb bundling. Install 'dpkg' manually or choose --arch/--appimage."
    fi
    warn "AppImage bundling uses Tauri/linuxdeploy and can fail on some distro/runtime combinations."
    info "Building Debian package and AppImage."
    npm run tauri -- build --bundles deb,appimage
    info "Build artifacts:"
    find src-tauri/target/release/bundle -type f \( -name '*.AppImage' -o -name '*.deb' \) -print | sort || true
    ;;
  *)
    fail "Unsupported build mode: $MODE"
    ;;
esac
