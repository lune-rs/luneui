#!/usr/bin/env bash
# Cross-target checks. Usage: scripts/check-targets.sh [--skip-linux]
#   windows  cargo clippy --workspace --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings
#            (needs that rustup target installed; prints the `rustup target add` hint when it is not)
#   macos    cargo clippy --workspace --all-targets --locked --target aarch64-apple-darwin -- -D warnings
#            (compile check only, no run; needs that rustup target installed, same hint when it is not)
#   linux    fmt --check, clippy --locked -D warnings, test --locked --no-fail-fast, in a rust:1.92
#            container with the repo mounted read-only (D414: the test
#            the_real_decisions_file_has_the_known_changing_rows is skipped, as CI skips it, D412:
#            it reads docs/DECISIONS.md, a link into the agent-docs store outside the mount).
#            Target, cargo and rustup state live in the named volumes
#            luneui-linux-{target,cargo,rustup}: created if missing, never removed. The container
#            is --rm. Skipped (not failed) when docker is missing, the daemon is not running, or
#            --skip-linux is given. This script never starts Docker.
#   ios, ios-sim, android
#            cargo clippy -p lune-ui-core -p lune-ui-wgpu -p lune-ui-winit -p lune-ui-platform -p lune-ui
#            -p demo --lib --bins --locked --target <t> -- -D warnings, for aarch64-apple-ios,
#            aarch64-apple-ios-sim and aarch64-linux-android (compile checks only, no run; the five library
#            crates and the demo, never a library crate alone, never the host tools or test targets; the
#            same rustup hint when a target is not installed)
# Prints `windows: passed|FAILED`, `macos: passed|FAILED`, `ios: passed|FAILED`, `ios-sim: passed|FAILED`,
# `android: passed|FAILED` and `linux: passed|FAILED|skipped (<reason>)`.
# Exit codes: 0 every half that ran passed; 1 a check failed; 2 usage error.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

WIN_TARGET=x86_64-pc-windows-msvc
MAC_TARGET=aarch64-apple-darwin
skip_linux=0
for arg in "$@"; do
  case "$arg" in
    --skip-linux) skip_linux=1 ;;
    *) echo "usage: scripts/check-targets.sh [--skip-linux]" >&2; exit 2 ;;
  esac
done

failed=0

# Windows half.
installed="$(rustup target list --installed 2>/dev/null || true)"
if grep -qx "$WIN_TARGET" <<< "$installed"; then
  if cargo clippy --workspace --all-targets --locked --target "$WIN_TARGET" -- -D warnings; then
    echo "windows: passed"
  else
    echo "windows: FAILED"
    failed=1
  fi
else
  echo "windows: FAILED (run: rustup target add $WIN_TARGET)"
  failed=1
fi

# macOS half.
if grep -qx "$MAC_TARGET" <<< "$installed"; then
  if cargo clippy --workspace --all-targets --locked --target "$MAC_TARGET" -- -D warnings; then
    echo "macos: passed"
  else
    echo "macos: FAILED"
    failed=1
  fi
else
  echo "macos: FAILED (run: rustup target add $MAC_TARGET)"
  failed=1
fi

# Mobile halves: the library crates and the demo for the phone targets.
mobile_half() {
  local name="$1" target="$2"
  if grep -qx "$target" <<< "$installed"; then
    if cargo clippy -p lune-ui-core -p lune-ui-wgpu -p lune-ui-winit -p lune-ui-platform -p lune-ui \
      -p demo --lib --bins --locked --target "$target" -- -D warnings; then
      echo "$name: passed"
    else
      echo "$name: FAILED"
      failed=1
    fi
  else
    echo "$name: FAILED (run: rustup target add $target)"
    failed=1
  fi
}
mobile_half ios aarch64-apple-ios
mobile_half ios-sim aarch64-apple-ios-sim
mobile_half android aarch64-linux-android

# Linux half.
ensure_volumes() {
  local v
  for v in target cargo rustup; do
    docker volume inspect "luneui-linux-$v" >/dev/null 2>&1 \
      || docker volume create "luneui-linux-$v" >/dev/null || return 1
  done
}

if [ "$skip_linux" -eq 1 ]; then
  echo "linux: skipped (--skip-linux)"
elif ! command -v docker >/dev/null 2>&1; then
  echo "linux: skipped (docker not installed)"
elif ! docker info >/dev/null 2>&1; then
  echo "linux: skipped (docker daemon not running)"
elif ! ensure_volumes; then
  echo "linux: FAILED (could not create luneui-linux-* volumes)"
  failed=1
else
  if docker run --rm \
    -v "$ROOT:/work:ro" \
    -v luneui-linux-target:/target \
    -v luneui-linux-cargo:/usr/local/cargo \
    -v luneui-linux-rustup:/usr/local/rustup \
    -e CARGO_TARGET_DIR=/target \
    -w /work \
    rust:1.92 \
    bash -c '
      set -euo pipefail
      run() { echo "+ $*"; "$@" || { echo "FAILED: $*" >&2; exit 1; }; }
      run cargo fmt --all -- --check
      run cargo clippy --workspace --all-targets --locked -- -D warnings
      run cargo test --workspace --locked --no-fail-fast -- --skip the_real_decisions_file_has_the_known_changing_rows
      echo "check-targets linux: all passed"
    '; then
    echo "linux: passed"
  else
    echo "linux: FAILED"
    failed=1
  fi
fi

exit "$failed"
