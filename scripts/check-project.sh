#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_NPM=1
RUN_CARGO=1
STRICT_SOURCE=0
FAIL_LOCAL_GENERATED=0

for arg in "$@"; do
  case "$arg" in
    --skip-npm) RUN_NPM=0 ;;
    --skip-cargo) RUN_CARGO=0 ;;
    --strict-source|--archive|--packaging) STRICT_SOURCE=1 ;;
    --fail-local-generated|--clean-worktree) FAIL_LOCAL_GENERATED=1 ;;
    -h|--help)
      cat <<USAGE
Run project sanity checks.

Usage:
  scripts/check-project.sh [--skip-npm] [--skip-cargo] [--strict-source] [--fail-local-generated]

Options:
  --skip-npm       Do not run npm run check.
  --skip-cargo     Do not run cargo check.
  --strict-source  Validate source/archive hygiene. In a git checkout this fails
                   if generated directories are tracked. In an extracted dev tree
                   it reports generated directories without failing.
  --fail-local-generated
                   With --strict-source, also fail if generated directories exist
                   locally. Use only in a clean temporary packaging tree, not after
                   npm ci or a build.
USAGE
      exit 0
      ;;
    *) echo "Unknown argument: $arg" >&2; exit 2 ;;
  esac
done

info() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
fail() { printf '\033[1;31mERROR:\033[0m %s\n' "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

cd "$ROOT_DIR"

info "Checking shell scripts."
for script in scripts/*.sh; do
  bash -n "$script"
done

info "Checking batch scripts are present."
for script in scripts/*.bat; do
  [[ -f "$script" ]] || fail "Missing Windows script: $script"
done

generated_paths=(node_modules .svelte-kit build src-tauri/target src-tauri/gen dist)

if [[ "$STRICT_SOURCE" -eq 1 ]]; then
  info "Checking source/archive hygiene."

  if [[ -d .git ]] && have git; then
    tracked_generated="$(git ls-files "${generated_paths[@]}" 2>/dev/null || true)"
    if [[ -n "$tracked_generated" ]]; then
      printf '%s\n' "$tracked_generated" >&2
      fail "Generated paths are tracked by git. Remove them before packaging."
    fi
  else
    info "No git metadata detected; checking local tree only."
  fi

  local_generated=()
  for path in "${generated_paths[@]}"; do
    if [[ -e "$path" ]]; then
      local_generated+=("$path")
    fi
  done

  if (( ${#local_generated[@]} > 0 )); then
    if [[ "$FAIL_LOCAL_GENERATED" -eq 1 ]]; then
      printf '%s\n' "${local_generated[@]}" >&2
      fail "Generated paths exist in the local tree. Remove them or package from a clean tree."
    fi
    info "Generated directories exist locally after bootstrap/build and are ignored in source-hygiene mode: ${local_generated[*]}"
    info "Use --fail-local-generated only when checking a clean temporary packaging tree."
  fi
elif [[ -d .git ]] && have git; then
  info "Checking for accidentally tracked generated directories."
  tracked_generated="$(git ls-files "${generated_paths[@]}" 2>/dev/null || true)"
  if [[ -n "$tracked_generated" ]]; then
    printf '%s\n' "$tracked_generated" >&2
    fail "Generated paths are tracked by git. Remove them from the repository."
  fi
else
  info "Skipping generated-directory archive check in developer mode. Use --strict-source before packaging."
fi

info "Checking for placeholder markers."
if grep -RIn --exclude-dir=.git --exclude-dir=node_modules --exclude-dir=src-tauri/target --exclude-dir=build --exclude-dir=.svelte-kit \
  --exclude='check-project.sh' \
  -E 'TODO|FIXME|todo!\(|unimplemented!\(' .; then
  fail "Placeholder markers found. Resolve them or add a justified allowlist."
fi

info "Checking project rename consistency."
if grep -RIn --exclude-dir=.git --exclude-dir=node_modules --exclude-dir=src-tauri/target --exclude-dir=build --exclude-dir=.svelte-kit --exclude-dir=dist \
  --exclude='check-project.sh' --exclude='state.rs' --exclude='autorun.rs' --exclude='README.md' --exclude='BUILDING.md' --exclude='TUI_AND_LINUX.md' \
  -E 'SmartEqualizer|smartequalizer|smart_equalizer|smart-equalizer-apo|Smart Equalizer' .; then
  fail "Legacy SmartEqualizer project name found outside the migration allowlist."
fi

if [[ "$RUN_NPM" -eq 1 ]]; then
  have npm || fail "npm is required for npm checks."
  info "Running npm check."
  npm run check
fi

if [[ "$RUN_CARGO" -eq 1 ]]; then
  have cargo || fail "cargo is required for Rust checks."
  info "Running cargo check."
  (cd src-tauri && cargo check)
fi

info "Project checks complete."
