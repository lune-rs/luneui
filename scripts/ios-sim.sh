#!/usr/bin/env bash
# Builds apps/demo for the iOS simulator, runs it on one simulator device of its own and checks that
# a frame was presented. Usage: scripts/ios-sim.sh [--probe] [--keep-booted | --delete-device]
#   (default)       build, assemble target/ios-sim/Demo.app (ad-hoc signed), boot the device, install,
#                   launch, wait up to 60 s for `presented 1` in target/ios-sim/stdout.log, take one
#                   screenshot (target/ios-sim/shot-1.png), terminate, shut the device down.
#   --probe         launch the demo with --probe (it prints `probe <key> <value>` lines), take shot-1.png
#                   2 s after `presented 1`, send the demo to the background by launching the Settings app
#                   (simctl launch only, never the UI), bring it back with a second launch without
#                   --terminate-running-process, take shot-2.png (two screenshots in all), and write the
#                   `probe` lines of both launches to target/ios-sim/probe.txt.
#   --keep-booted   leave the device booted at the end.
#   --delete-device shut down and delete the device named below, then exit.
# The device is named luneui-demo-iphone (type iPhone-16-Pro, runtime iOS-18-0; override with
# LUNEUI_IOS_DEVICE_TYPE and LUNEUI_IOS_RUNTIME, ids without the CoreSimulator prefix). It is found by
# exact name and created when missing.
# Never: touches any other device, uses `booted` as a device argument, opens Simulator.app, drives the
# UI, records video, or writes outside target/ios-sim/ (cargo's own target dir aside).
# Exit codes: 0 passed; 1 a check failed (build, launch, no frame); 2 usage or setup (a tool, the rustup
# target or the runtime is missing).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

NAME=luneui-demo-iphone
DEVICE_TYPE="${LUNEUI_IOS_DEVICE_TYPE:-iPhone-16-Pro}"
RUNTIME="${LUNEUI_IOS_RUNTIME:-iOS-18-0}"
TARGET=aarch64-apple-ios-sim
BUNDLE_ID=rs.lune.luneui.demo
OUT="$ROOT/target/ios-sim"
APP="$OUT/Demo.app"

keep=0
delete=0
probe=0
for arg in "$@"; do
  case "$arg" in
    --probe) probe=1 ;;
    --keep-booted) keep=1 ;;
    --delete-device) delete=1 ;;
    *) echo "usage: scripts/ios-sim.sh [--probe] [--keep-booted | --delete-device]" >&2; exit 2 ;;
  esac
done

UDID=""
booted_by_us=0

# The udid of the device named $NAME, or nothing.
find_udid() {
  xcrun simctl list devices | sed -nE "s/^[[:space:]]+${NAME} \(([0-9A-F-]{36})\) .*/\1/p" | head -n 1
}

shutdown_device() {
  if [ -n "$UDID" ]; then
    xcrun simctl shutdown "$UDID" >/dev/null 2>&1 || true
  fi
}

cleanup() {
  if [ "$booted_by_us" -eq 1 ] && [ "$keep" -eq 0 ]; then
    shutdown_device
  fi
}
trap cleanup EXIT

fail() {
  echo "ios-sim: FAILED ($1)" >&2
  exit 1
}

setup_fail() {
  echo "ios-sim: setup: $1" >&2
  exit 2
}

command -v xcrun >/dev/null 2>&1 || setup_fail "xcrun not found (install Xcode)"
xcrun simctl help >/dev/null 2>&1 || setup_fail "xcrun simctl not available"

if [ "$delete" -eq 1 ]; then
  UDID="$(find_udid)"
  if [ -z "$UDID" ]; then
    echo "ios-sim: no device named $NAME"
    exit 0
  fi
  shutdown_device
  xcrun simctl delete "$UDID"
  echo "ios-sim: deleted $NAME ($UDID)"
  exit 0
fi

rustup target list --installed 2>/dev/null | grep -qx "$TARGET" \
  || setup_fail "rustup target missing (run: rustup target add $TARGET)"
xcrun simctl list runtimes | grep -q "com.apple.CoreSimulator.SimRuntime.$RUNTIME" \
  || setup_fail "runtime $RUNTIME not installed (set LUNEUI_IOS_RUNTIME to an installed one: xcrun simctl list runtimes)"

UDID="$(find_udid)"
if [ -z "$UDID" ]; then
  UDID="$(xcrun simctl create "$NAME" "com.apple.CoreSimulator.SimDeviceType.$DEVICE_TYPE" "com.apple.CoreSimulator.SimRuntime.$RUNTIME")" \
    || setup_fail "could not create the device (type $DEVICE_TYPE, runtime $RUNTIME)"
  echo "ios-sim: created $NAME ($UDID)"
fi

echo "ios-sim: building demo for $TARGET"
IPHONEOS_DEPLOYMENT_TARGET=17.0 cargo build -p demo --target "$TARGET" || fail "build"
BIN="${CARGO_TARGET_DIR:-$ROOT/target}/$TARGET/debug/demo"
[ -f "$BIN" ] || fail "no binary at $BIN"

mkdir -p "$OUT"
rm -rf "$APP"
mkdir -p "$APP"
cp "$BIN" "$APP/demo"
cp "$ROOT/apps/demo/ios/Info.plist" "$APP/Info.plist"
codesign --force --sign - "$APP" >/dev/null 2>&1 || fail "codesign (ad-hoc)"

state="$(xcrun simctl list devices | grep -F "($UDID)" | head -n 1)"
if ! grep -q "(Booted)" <<< "$state"; then
  booted_by_us=1
  xcrun simctl boot "$UDID" || fail "boot"
fi
booted_by_us=1
xcrun simctl bootstatus "$UDID" -b >/dev/null 2>&1 || fail "bootstatus"

xcrun simctl install "$UDID" "$APP" || fail "install"
rm -f "$OUT/stdout.log" "$OUT/stderr.log" "$OUT/stdout-2.log" "$OUT/stderr-2.log" \
  "$OUT/shot-1.png" "$OUT/shot-2.png" "$OUT/probe.txt"
: > "$OUT/stdout.log"
: > "$OUT/stderr.log"
app_args=()
if [ "$probe" -eq 1 ]; then
  app_args=(--probe)
fi
xcrun simctl launch --terminate-running-process \
  --stdout="$OUT/stdout.log" --stderr="$OUT/stderr.log" "$UDID" "$BUNDLE_ID" ${app_args[@]+"${app_args[@]}"} || fail "launch"

seen=0
for _ in $(seq 1 120); do
  if grep -q "presented 1 " "$OUT/stdout.log" 2>/dev/null; then
    seen=1
    break
  fi
  sleep 0.5
done
if [ "$seen" -ne 1 ]; then
  echo "--- stderr (tail) ---" >&2
  tail -n 20 "$OUT/stderr.log" >&2 || true
  fail "no 'presented 1' within 60 s"
fi
grep "^presented " "$OUT/stdout.log" | head -n 3 || true

if [ "$probe" -eq 1 ]; then
  grep -q "^probe inner_size " "$OUT/stdout.log" || fail "no 'probe inner_size' line"
  sleep 2
else
  sleep 1
fi
xcrun simctl io "$UDID" screenshot --type=png "$OUT/shot-1.png" >/dev/null 2>&1 || fail "screenshot"
[ -s "$OUT/shot-1.png" ] || fail "empty screenshot"
sips -g pixelWidth -g pixelHeight "$OUT/shot-1.png" | tail -n 2

if [ "$probe" -eq 1 ]; then
  # Background the demo by launching the Settings app, then foreground it with a plain launch.
  xcrun simctl launch "$UDID" com.apple.Preferences >/dev/null || fail "launch Settings"
  sleep 3
  : > "$OUT/stdout-2.log"
  : > "$OUT/stderr-2.log"
  xcrun simctl launch --stdout="$OUT/stdout-2.log" --stderr="$OUT/stderr-2.log" \
    "$UDID" "$BUNDLE_ID" --probe || fail "second launch"
  sleep 3
  xcrun simctl io "$UDID" screenshot --type=png "$OUT/shot-2.png" >/dev/null 2>&1 || fail "screenshot 2"
  [ -s "$OUT/shot-2.png" ] || fail "empty screenshot 2"
  sips -g pixelWidth -g pixelHeight "$OUT/shot-2.png" | tail -n 2
  {
    echo "# launch 1 (stdout.log, to the end of the run)"
    grep "^probe \|^presented " "$OUT/stdout.log" || true
    echo "# launch 2 (stdout-2.log)"
    grep "^probe \|^presented " "$OUT/stdout-2.log" || true
  } > "$OUT/probe.txt"
  echo "ios-sim: probe lines in $OUT/probe.txt ($(grep -c '^probe ' "$OUT/probe.txt") lines)"
fi

xcrun simctl terminate "$UDID" "$BUNDLE_ID" >/dev/null 2>&1 || true
echo "ios-sim: passed"
