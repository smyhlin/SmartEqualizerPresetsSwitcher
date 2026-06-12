#!/usr/bin/env bash
set -Eeuo pipefail

APP_NAME="SmartEQPresetSwitcher"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NO_INSTALL=0
SKIP_NPM=0
SKIP_CHECK=0
RUST_PROVIDER="auto"

CROSS_WINDOWS=0

for arg in "$@"; do
  case "$arg" in
    --no-install) NO_INSTALL=1 ;;
    --skip-npm) SKIP_NPM=1 ;;
    --skip-check) SKIP_CHECK=1 ;;
    --rustup) RUST_PROVIDER="rustup" ;;
    --rust) RUST_PROVIDER="rust" ;;
    --cross-windows|--xwin) CROSS_WINDOWS=1 ;;
    -h|--help)
      cat <<USAGE
Bootstrap Linux dependencies for $APP_NAME.

Usage:
  scripts/bootstrap-linux.sh [options]

Options:
  --no-install      Do not install system packages, only verify tools.
  --skip-npm        Do not run npm ci / npm install.
  --skip-check      Do not run npm run check after npm install.
  --rustup          Force installing Arch rustup package when Rust is missing.
  --rust            Force installing Arch rust package when Rust is missing.
  --cross-windows   Also install Windows cross-build tools (NSIS, LLVM, clang, lld).

Arch note:
  Arch packages 'rust' and 'rustup' conflict. In auto mode this script keeps
  whichever Rust provider is already installed and only installs rustup if no
  Rust toolchain is detected. Use --rust if you prefer the repository Rust
  package instead of rustup.
  When --cross-windows is set, this script uses yay to install NSIS from AUR.
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

if [[ "$(uname -s)" != "Linux" ]]; then
  fail "This bootstrap script is Linux-only. Use scripts/bootstrap-windows.bat on Windows."
fi

ARCH="$(uname -m)"
case "$ARCH" in
  x86_64|amd64) ;;
  *) fail "Only Linux x86_64 is supported by this project bootstrap. Current arch: $ARCH" ;;
esac

cd "$ROOT_DIR"

if [[ "$NO_INSTALL" -eq 0 ]]; then
  if have pacman; then
    info "Detected Arch/pacman. Installing Tauri build dependencies."
    packages=(
      base-devel curl wget file openssl pkgconf
      nodejs npm
      webkit2gtk-4.1 gtk3 libayatana-appindicator librsvg
      appmenu-gtk-module xdotool fuse2 zstd tar
    )

    # Arch packages `rust` and `rustup` conflict.  Do not blindly install
    # rustup when system Rust is already installed.  This was breaking
    # bootstrap on machines that already have the official `rust` package.
    case "$RUST_PROVIDER" in
      auto)
        if have rustc && have cargo; then
          info "Existing Rust toolchain detected. Keeping it."
        elif have rustup; then
          info "Existing rustup detected. Keeping it."
        elif pacman -Qi rust >/dev/null 2>&1; then
          warn "Arch 'rust' package is installed but rustc/cargo are not in PATH. Not installing rustup because it conflicts with rust."
          warn "Open a new shell or fix PATH, then rerun this script."
        else
          packages+=(rustup)
        fi
        ;;
      rustup)
        if pacman -Qi rust >/dev/null 2>&1 && ! pacman -Qi rustup >/dev/null 2>&1; then
          fail "Cannot install rustup while Arch 'rust' package is installed. Remove rust first or rerun with --rust."
        fi
        packages+=(rustup)
        ;;
      rust)
        if pacman -Qi rustup >/dev/null 2>&1 && ! pacman -Qi rust >/dev/null 2>&1; then
          fail "Cannot install rust while Arch 'rustup' package is installed. Remove rustup first or rerun with --rustup."
        fi
        packages+=(rust)
        ;;
      *) fail "Unsupported Rust provider: $RUST_PROVIDER" ;;
    esac

    if [[ "${EUID}" -eq 0 ]]; then
      pacman -S --needed --noconfirm "${packages[@]}"
    elif have sudo; then
      sudo pacman -S --needed --noconfirm "${packages[@]}"
    else
      warn "sudo not found. Install manually: pacman -S --needed ${packages[*]}"
    fi

    if [[ "$CROSS_WINDOWS" -eq 1 ]]; then
      info "Installing Windows cross-build tools (llvm, lld, clang)."
      for pkg in llvm lld clang; do
        if ! pacman -Qi "$pkg" >/dev/null 2>&1; then
          if [[ "${EUID}" -eq 0 ]]; then
            pacman -S --needed --noconfirm "$pkg"
          elif have sudo; then
            sudo pacman -S --needed --noconfirm "$pkg"
          fi
        fi
      done
      if ! have makensis; then
        if have yay; then
          info "Installing nsis from AUR via yay."
          yay -S --noconfirm nsis
        elif have paru; then
          info "Installing nsis from AUR via paru."
          paru -S --noconfirm nsis
        else
          warn "makensis (NSIS) is required for Windows cross-build but is in AUR."
          warn "Install it manually: yay -S nsis  (or paru -S nsis)"
        fi
      fi
    fi
  elif have apt-get; then
    info "Detected Debian/Ubuntu apt. Installing Tauri build dependencies."
    packages=(
      build-essential curl wget file pkg-config libssl-dev
      nodejs npm rustc cargo
      libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev
      libwebkit2gtk-4.1-dev libxdo-dev zstd tar
    )
    fuse_pkg=""
    if apt-cache show libfuse2t64 >/dev/null 2>&1; then
      fuse_pkg="libfuse2t64"
    elif apt-cache show libfuse2 >/dev/null 2>&1; then
      fuse_pkg="libfuse2"
    fi
    [[ -n "$fuse_pkg" ]] && packages+=("$fuse_pkg")
    if [[ "${EUID}" -eq 0 ]]; then
      apt-get update
      DEBIAN_FRONTEND=noninteractive apt-get install -y "${packages[@]}"
    elif have sudo; then
      sudo apt-get update
      sudo DEBIAN_FRONTEND=noninteractive apt-get install -y "${packages[@]}"
    else
      warn "sudo not found. Install manually: apt-get install ${packages[*]}"
    fi

    if [[ "$CROSS_WINDOWS" -eq 1 ]]; then
      info "Installing Windows cross-build tools for Debian/Ubuntu."
      local win_packages=(nsis llvm lld clang)
      if [[ "${EUID}" -eq 0 ]]; then
        DEBIAN_FRONTEND=noninteractive apt-get install -y "${win_packages[@]}" \
          2>/dev/null || warn "Some cross-build packages may not be available; install them manually."
      elif have sudo; then
        sudo DEBIAN_FRONTEND=noninteractive apt-get install -y "${win_packages[@]}" \
          2>/dev/null || warn "Some cross-build packages may not be available; install them manually."
      fi
    fi
  else
    warn "Unsupported package manager. Install Node.js, npm, Rust, WebKitGTK 4.1, GTK3, AppIndicator, librsvg, libxdo, zstd, tar manually."
  fi
else
  info "Skipping system package installation."
fi

if have rustup; then
  info "Ensuring stable Rust toolchain is available."
  rustup toolchain install stable >/dev/null
  rustup default stable >/dev/null
else
  info "Using system Rust toolchain."
fi

missing=()
for cmd in node npm cargo rustc; do
  have "$cmd" || missing+=("$cmd")
done
if (( ${#missing[@]} > 0 )); then
  fail "Missing required tools: ${missing[*]}"
fi

info "Tool versions"
node --version
npm --version
rustc --version
cargo --version

if [[ "$SKIP_NPM" -eq 0 ]]; then
  if [[ -f package-lock.json ]]; then
    info "Installing frontend dependencies with npm ci."
    npm ci
  else
    info "package-lock.json not found. Installing frontend dependencies with npm install."
    npm install
  fi
else
  info "Skipping npm dependency installation."
fi

if [[ "$SKIP_CHECK" -eq 0 ]]; then
  info "Running project check."
  npm run check
fi

if [[ "$CROSS_WINDOWS" -eq 1 ]]; then
  if ! have rustup; then
    if [[ -x "$HOME/.cargo/bin/rustup" ]]; then
      export PATH="$HOME/.cargo/bin:$PATH"
      info "Found rustup in ~/.cargo/bin (not in PATH). Added it."
    else
      info "Installing rustup side-by-side with system Rust (via official script, not pacman)."
      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --no-modify-path --default-toolchain stable 2>&1
      export PATH="$HOME/.cargo/bin:$PATH"
    fi
  fi
  info "Installing Rust Windows MSVC target for cross-build."
  rustup target add x86_64-pc-windows-msvc
  if ! have cargo-xwin; then
    info "Installing cargo-xwin for Windows cross-compilation."
    cargo install --locked cargo-xwin
  fi
fi

info "Linux bootstrap complete."
