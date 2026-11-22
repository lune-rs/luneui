#!/usr/bin/env bash
# Builds apps/demo as an APK, runs it on one Android emulator of its own and checks that a frame was
# presented. Usage: scripts/android-emu.sh [--probe | --smoke | --keyboard | --demo | --build-only] [--landscape]
#   [--demo-arg <arg>] [--keep-booted | --delete-device]
#   (default)       build and package target/android-emu/demo.apk, create the AVD if missing, boot it
#                   headless, install, launch, wait up to 120 s for `presented 1` in
#                   target/android-emu/stdout.log, take one screenshot (target/android-emu/shot-1.png),
#                   force-stop the app, shut the emulator down.
#   --probe         launch the demo with --probe (it prints `probe <key> <value>` lines, among them
#                   `probe content_rect <l>,<t>,<r>,<b> window <w>x<h> scale <s>`), take shot-1.png 2 s after
#                   `presented 1`, and write the `probe` and `presented` lines and the device's `wm size` and
#                   `wm density` to target/android-emu/probe.txt.
#   --smoke         --probe, and launch 1 (`--probe --probe-clipboard`) also requires, each failing with the missing
#                   line quoted: `probe safe_area_event` equal to what the content rect of the same run gives
#                   (left, top, right and bottom over the scale, clamped at 0; D7); the clipboard round trip
#                   (`probe clipboard_write` and `probe clipboard` equal); `presented 31`, which the demo
#                   schedules with `Repaint::At` and which must arrive while idle (no input); live touch through
#                   the OS (`adb shell input`, D12): a tap on the Cards `probe tab_item` gives `probe tab 1`, a
#                   swipe along `probe pager` gives `probe page 1`, a tap on the Home item gives `probe tab 0`;
#                   the life cycle: `input keyevent KEYCODE_HOME`, `probe lifecycle suspended`, no `presented`
#                   line until `probe lifecycle resumed` after `am start` again, then one. Launch 2 (`--probe
#                   --focus-field`, shot-2.png) requires `mInputShown=true` (dumpsys input_method) within 10 s,
#                   `probe keyboard_inset 0,0,0,<b>` with b above 200, the safe area's bottom still the one at
#                   rest (D8), and `adb shell input text luneui` giving `probe field "luneui"`. Two screenshots.
#   --keyboard      launch 2 alone: `--probe --focus-field`, the same keyboard checks, one screenshot.
#   --demo          launch with `--demo --touches` (plus any --demo-arg): the showcase tour plays hands-free
#                   (D415, D417). A `demo mark <name> FAIL` line fails at once (exit 1, the line quoted); shot-1.png,
#                   shot-2.png and shot-3.png are taken at `demo mark sheet-half ok`, `page-2 ok` and `settings ok`;
#                   `demo end <n>/<n>` is required within 60 s plus 120 s, and 16 marks `ok`; the `demo` lines go to
#                   target/android-emu/demo.txt. Exclusive with --probe, --smoke and --keyboard.
#   --landscape     with any mode: `settings put system accelerometer_rotation 0` and `user_rotation 1`, put in
#                   before the launch and again once the demo is on top (the system turns the display for the
#                   app on top, not while the launcher is), the display required to reach ROTATION_90; both
#                   settings are put back to the values read on every exit path. A smoke run then judges the
#                   safe area by the same formula (whatever the content rect is).
#   --build-only    build and package the APK and stop (no emulator, no AVD): the local link check.
#   --demo-arg <a>  pass one more argument to the demo (repeatable), e.g. --focus-field.
#   --keep-booted   leave the emulator booted at the end (a later run reuses it, and shuts it down
#                   unless it too has --keep-booted).
#   --delete-device shut down and delete the AVD named below, then exit (it takes no other flag: exit 2).
# Packaging (D420) uses only the installed SDK, no Gradle and no cargo-apk: `cargo rustc --crate-type
# cdylib` with the NDK's clang as linker, `aapt2 link` of apps/demo/android/AndroidManifest.xml (no
# Java), the library stripped and zipped in, `zipalign`, `apksigner` with a debug key made once under
# target/android-emu/.
# Launch arguments (D420): the flags go one per line to the app's files/launch-args through `run-as`
# before the launch; the app reads and deletes the file, so a launch from the home screen has no flags.
# Logs: android-activity forwards the app's stdout to logcat's RustStdoutStderr tag; the script follows
# that tag into target/android-emu/stdout.log (and keeps a bounded `logcat -d` in logcat.txt).
# The AVD is named luneui-demo-pixel7 (pixel_7, system-images;android-34;google_apis;arm64-v8a;
# LUNEUI_ANDROID_AVD picks another name for an AVD of its own), `hw.keyboard=no` so the soft keyboard
# shows, booted with `-no-window -no-audio -no-boot-anim -no-snapshot -port 5580 -gpu <mode>`
# (LUNEUI_ANDROID_GPU, default swiftshader_indirect). Every adb call is `-s emulator-5580`.
# Setup: ANDROID_HOME or ANDROID_SDK_ROOT (else ~/Library/Android/sdk); the newest NDK under
# $SDK/ndk or ANDROID_NDK_HOME; JDK 17 from LUNEUI_ANDROID_JAVA_HOME, JAVA_HOME, `java_home -v 17` or
# Homebrew's openjdk@17.
# Never: touches another AVD or another serial, runs `adb kill-server`, opens the emulator window,
# drives its UI, records video, takes more than three screenshots, or writes outside target/android-emu/
# (cargo's own target dir, the new AVD in ~/.android/avd and the debug key aside). A serial already on
# port 5580 that is not this AVD is exit 2.
# Exit codes: 0 passed; 1 a check failed (build, boot, launch, no frame); 2 usage or setup (a tool, the
# rustup target, the image or a foreign serial on the port is missing or in the way).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

AVD="${LUNEUI_ANDROID_AVD:-luneui-demo-pixel7}"
GPU="${LUNEUI_ANDROID_GPU:-swiftshader_indirect}"
IMAGE="system-images;android-34;google_apis;arm64-v8a"
DEVICE=pixel_7
PORT=5580
SERIAL="emulator-$PORT"
TARGET=aarch64-linux-android
API=26
PLATFORM=android-34
PKG=rs.lune.luneui.demo
ACTIVITY="$PKG/android.app.NativeActivity"
OUT="$ROOT/target/android-emu"
MANIFEST="$ROOT/apps/demo/android/AndroidManifest.xml"
KEYSTORE="$OUT/debug.keystore"

usage() {
  echo "usage: scripts/android-emu.sh [--probe | --smoke | --keyboard | --demo | --build-only] [--landscape] [--demo-arg <arg>] [--keep-booted | --delete-device]" >&2
  exit 2
}

keep=0
delete=0
probe=0
smoke=0
keyboard=0
demo=0
landscape=0
build_only=0
demo_args=()
nargs=$#
while [ "$#" -gt 0 ]; do
  case "$1" in
    --probe) probe=1 ;;
    --smoke) smoke=1; probe=1 ;;
    --keyboard) keyboard=1; probe=1 ;;
    --demo) demo=1 ;;
    --landscape) landscape=1 ;;
    --build-only) build_only=1 ;;
    --demo-arg)
      [ "$#" -ge 2 ] || usage
      demo_args+=("$2")
      shift
      ;;
    --keep-booted) keep=1 ;;
    --delete-device) delete=1 ;;
    *) usage ;;
  esac
  shift
done
# --delete-device stands alone: with a mode or --keep-booted it would delete and exit 0 without the mode.
if [ "$delete" -eq 1 ] && [ "$nargs" -ne 1 ]; then
  usage
fi
if [ "$build_only" -eq 1 ] && { [ "$probe" -eq 1 ] || [ "$delete" -eq 1 ] || [ "$demo" -eq 1 ]; }; then
  usage
fi
# --demo is its own mode, and --smoke and --keyboard are each their own.
if [ "$demo" -eq 1 ] && [ "$probe" -eq 1 ]; then
  usage
fi
if [ "$smoke" -eq 1 ] && [ "$keyboard" -eq 1 ]; then
  usage
fi

booted_by_us=0
rotation_changed=0
old_accel=""
old_user=""
emu_pid=""
follower_pid=""
ADB=""

adb_s() { "$ADB" -s "$SERIAL" "$@"; }

stop_follower() {
  if [ -n "$follower_pid" ]; then
    kill "$follower_pid" >/dev/null 2>&1 || true
    wait "$follower_pid" 2>/dev/null || true
    follower_pid=""
  fi
}

# Shuts this script's emulator down (the console's `emu kill`, then the process we started if it is
# still there after 30 s).
shutdown_emulator() {
  adb_s emu kill >/dev/null 2>&1 || true
  if [ -n "$emu_pid" ]; then
    for _ in $(seq 1 60); do
      kill -0 "$emu_pid" >/dev/null 2>&1 || break
      sleep 0.5
    done
    if kill -0 "$emu_pid" >/dev/null 2>&1; then
      kill "$emu_pid" >/dev/null 2>&1 || true
    fi
  fi
}

# Puts the rotation settings --landscape changed back to the values it read (a key that was unset is
# deleted).
restore_rotation() {
  if [ "$rotation_changed" -eq 1 ]; then
    rotation_changed=0
    for pair in "accelerometer_rotation:$old_accel" "user_rotation:$old_user"; do
      key="${pair%%:*}"
      val="${pair#*:}"
      if [ -z "$val" ] || [ "$val" = "null" ]; then
        adb_s shell settings delete system "$key" >/dev/null 2>&1 || true
      else
        adb_s shell settings put system "$key" "$val" >/dev/null 2>&1 || true
      fi
    done
  fi
}

cleanup() {
  stop_follower
  restore_rotation
  if [ "$booted_by_us" -eq 1 ] && [ "$keep" -eq 0 ]; then
    shutdown_emulator
  fi
}
trap cleanup EXIT

fail() {
  echo "android-emu: FAILED ($1)" >&2
  exit 1
}

setup_fail() {
  echo "android-emu: setup: $1" >&2
  exit 2
}

# --- the SDK, the NDK and JDK 17 -------------------------------------------------------------------
SDK="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}}"
[ -d "$SDK" ] || setup_fail "no Android SDK at $SDK (set ANDROID_HOME)"
export ANDROID_HOME="$SDK" ANDROID_SDK_ROOT="$SDK"
ADB="$SDK/platform-tools/adb"
[ -x "$ADB" ] || setup_fail "no adb at $ADB"

if [ -n "${ANDROID_NDK_HOME:-}" ]; then
  NDK="$ANDROID_NDK_HOME"
else
  NDK="$(ls -d "$SDK"/ndk/*/ 2>/dev/null | sort -V | tail -n 1 | sed 's:/$::')"
fi
[ -n "$NDK" ] && [ -d "$NDK" ] || setup_fail "no NDK under $SDK/ndk (set ANDROID_NDK_HOME)"
NDK_BIN="$(ls -d "$NDK"/toolchains/llvm/prebuilt/*/bin 2>/dev/null | head -n 1)"
CLANG="$NDK_BIN/aarch64-linux-android$API-clang"
[ -x "$CLANG" ] || setup_fail "no $CLANG in the NDK"
[ -x "$NDK_BIN/llvm-strip" ] || setup_fail "no llvm-strip in the NDK"

BT="$(ls -d "$SDK"/build-tools/*/ 2>/dev/null | sort -V | tail -n 1 | sed 's:/$::')"
[ -n "$BT" ] && [ -x "$BT/aapt2" ] && [ -x "$BT/zipalign" ] && [ -x "$BT/apksigner" ] \
  || setup_fail "no build-tools with aapt2, zipalign and apksigner under $SDK/build-tools"
ANDROID_JAR="$SDK/platforms/$PLATFORM/android.jar"
[ -f "$ANDROID_JAR" ] || setup_fail "no $ANDROID_JAR (sdkmanager \"platforms;$PLATFORM\")"

is_jdk17() { [ -x "$1/bin/java" ] && "$1/bin/java" -version 2>&1 | head -n 1 | grep -q '"17\.'; }
JDK=""
for cand in "${LUNEUI_ANDROID_JAVA_HOME:-}" "${JAVA_HOME:-}" \
  "$(/usr/libexec/java_home -v 17 2>/dev/null || true)" \
  /opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home \
  /usr/local/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home; do
  if [ -n "$cand" ] && is_jdk17 "$cand"; then
    JDK="$cand"
    break
  fi
done
[ -n "$JDK" ] || setup_fail "no JDK 17 (set LUNEUI_ANDROID_JAVA_HOME; apksigner, keytool and avdmanager need it)"
export JAVA_HOME="$JDK"
export PATH="$JDK/bin:$PATH"
AVDMANAGER="$SDK/cmdline-tools/latest/bin/avdmanager"
EMULATOR="$SDK/emulator/emulator"

# The emulator serial's AVD name, or nothing when no emulator is on the port.
serial_avd() {
  "$ADB" devices 2>/dev/null | awk -v s="$SERIAL" '$1 == s { found = 1 } END { exit !found }' || return 0
  adb_s emu avd name 2>/dev/null | tr -d '\r' | head -n 1
}

# --- --delete-device -------------------------------------------------------------------------------
if [ "$delete" -eq 1 ]; then
  [ -x "$AVDMANAGER" ] || setup_fail "no avdmanager at $AVDMANAGER"
  on_port="$(serial_avd)"
  if [ "$on_port" = "$AVD" ]; then
    echo "android-emu: shutting down $AVD on $SERIAL"
    shutdown_emulator
    sleep 3
  fi
  if "$AVDMANAGER" list avd -c 2>/dev/null | grep -qx "$AVD"; then
    "$AVDMANAGER" delete avd -n "$AVD" || fail "delete avd"
    echo "android-emu: deleted $AVD"
  else
    echo "android-emu: no AVD named $AVD"
  fi
  exit 0
fi

rustup target list --installed 2>/dev/null | grep -qx "$TARGET" \
  || setup_fail "rustup target missing (run: rustup target add $TARGET)"

# --- build and package (D1) ------------------------------------------------------------------------
mkdir -p "$OUT"
echo "android-emu: building demo for $TARGET (release cdylib)"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CLANG"
cargo rustc -p demo --lib --crate-type cdylib --release --target "$TARGET" --locked || fail "build"
SO="${CARGO_TARGET_DIR:-$ROOT/target}/$TARGET/release/libdemo.so"
[ -f "$SO" ] || fail "no library at $SO"

STAGE="$OUT/stage"
rm -rf "$STAGE"
mkdir -p "$STAGE/lib/arm64-v8a"
"$NDK_BIN/llvm-strip" --strip-unneeded -o "$STAGE/lib/arm64-v8a/libdemo.so" "$SO" || fail "llvm-strip"
if ! "$NDK_BIN/llvm-readelf" --dyn-syms "$STAGE/lib/arm64-v8a/libdemo.so" | grep -q ' ANativeActivity_onCreate$'; then
  fail "libdemo.so does not export ANativeActivity_onCreate"
fi

if [ ! -f "$KEYSTORE" ]; then
  # A throwaway debug key for the local APK, never a release key.
  keytool -genkeypair -keystore "$KEYSTORE" -storepass android -keypass android -alias luneui-debug \
    -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=luneui debug" >/dev/null 2>&1 || fail "keytool"
fi

rm -f "$OUT/base.apk" "$OUT/unaligned.apk" "$OUT/demo.apk" "$OUT/demo.apk.idsig"
"$BT/aapt2" link --debug-mode -I "$ANDROID_JAR" --manifest "$MANIFEST" --version-code 1 --version-name 1.0 \
  -o "$OUT/base.apk" || fail "aapt2 link"
cp "$OUT/base.apk" "$OUT/unaligned.apk"
(cd "$STAGE" && zip -X -q "$OUT/unaligned.apk" lib/arm64-v8a/libdemo.so) || fail "zip"
"$BT/zipalign" -f -p 4 "$OUT/unaligned.apk" "$OUT/demo.apk" || fail "zipalign"
"$BT/apksigner" sign --ks "$KEYSTORE" --ks-pass pass:android --key-pass pass:android "$OUT/demo.apk" \
  || fail "apksigner sign"
"$BT/apksigner" verify "$OUT/demo.apk" || fail "apksigner verify"
echo "android-emu: libdemo.so $(wc -c < "$STAGE/lib/arm64-v8a/libdemo.so" | tr -d ' ') bytes stripped, demo.apk $(wc -c < "$OUT/demo.apk" | tr -d ' ') bytes"

if [ "$build_only" -eq 1 ]; then
  echo "android-emu: passed (build only)"
  exit 0
fi

# --- the AVD and the emulator (D11) ----------------------------------------------------------------
[ -x "$AVDMANAGER" ] || setup_fail "no avdmanager at $AVDMANAGER"
[ -x "$EMULATOR" ] || setup_fail "no emulator at $EMULATOR"
[ -d "$SDK/system-images/android-34/google_apis/arm64-v8a" ] \
  || setup_fail "system image missing (sdkmanager \"$IMAGE\")"

"$ADB" start-server >/dev/null 2>&1 || setup_fail "adb start-server"
on_port="$(serial_avd)"
if [ -n "$on_port" ] && [ "$on_port" != "$AVD" ]; then
  setup_fail "$SERIAL is the emulator '$on_port', not $AVD: stop it or free port $PORT"
fi

if ! "$AVDMANAGER" list avd -c 2>/dev/null | grep -qx "$AVD"; then
  echo "android-emu: creating $AVD ($DEVICE, $IMAGE)"
  echo no | "$AVDMANAGER" create avd -n "$AVD" -k "$IMAGE" -d "$DEVICE" >"$OUT/avdmanager.log" 2>&1 \
    || { tail -n 5 "$OUT/avdmanager.log" >&2; setup_fail "could not create the AVD $AVD"; }
fi
AVD_CONFIG="$HOME/.android/avd/$AVD.avd/config.ini"
if [ -f "$AVD_CONFIG" ]; then
  # The soft keyboard shows only with no hardware keyboard on the AVD.
  if grep -q '^hw.keyboard=' "$AVD_CONFIG"; then
    sed -i.bak 's/^hw.keyboard=.*/hw.keyboard=no/' "$AVD_CONFIG" && rm -f "$AVD_CONFIG.bak"
  else
    echo "hw.keyboard=no" >> "$AVD_CONFIG"
  fi
fi

if [ -z "$on_port" ]; then
  echo "android-emu: booting $AVD on $SERIAL (-gpu $GPU)"
  boot_start=$SECONDS
  nohup "$EMULATOR" -avd "$AVD" -no-window -no-audio -no-boot-anim -no-snapshot -port "$PORT" -gpu "$GPU" \
    >"$OUT/emulator.log" 2>&1 < /dev/null &
  emu_pid=$!
  booted_by_us=1
  for _ in $(seq 1 600); do
    if ! kill -0 "$emu_pid" >/dev/null 2>&1; then
      tail -n 10 "$OUT/emulator.log" >&2 || true
      fail "the emulator exited while booting (log $OUT/emulator.log)"
    fi
    [ "$(adb_s shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ] && break
    sleep 0.5
  done
  [ "$(adb_s shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ] \
    || fail "no sys.boot_completed within 300 s (log $OUT/emulator.log)"
  echo "android-emu: booted in $((SECONDS - boot_start)) s"
else
  echo "android-emu: reusing $AVD on $SERIAL"
  # An emulator of this script's own AVD left by --keep-booted is this script's to shut down too.
  booted_by_us=1
fi
adb_s shell input keyevent KEYCODE_WAKEUP >/dev/null 2>&1 || true
adb_s shell wm dismiss-keyguard >/dev/null 2>&1 || true

# --- install, launch arguments, launch (D4, D5) ----------------------------------------------------
adb_s install -r "$OUT/demo.apk" >"$OUT/install.log" 2>&1 || { cat "$OUT/install.log" >&2; fail "install"; }

rm -f "$OUT/stdout.log" "$OUT/stdout-2.log" "$OUT/logcat.txt" "$OUT/shot-1.png" "$OUT/shot-2.png" \
  "$OUT/shot-3.png" "$OUT/probe.txt" "$OUT/demo.txt"

dump_logcat() {
  adb_s logcat -d -v threadtime -t 3000 > "$OUT/logcat.txt" 2>/dev/null || true
}

# Waits up to $3 seconds for a line matching the grep pattern $2 in file $1.
wait_line() {
  local file="$1" pat="$2" secs="$3"
  for _ in $(seq 1 $((secs * 2))); do
    grep -q -- "$pat" "$file" 2>/dev/null && return 0
    sleep 0.5
  done
  return 1
}

# Waits up to $4 seconds for at least $3 lines matching the grep pattern $2 in file $1.
wait_count() {
  local file="$1" pat="$2" n="$3" secs="$4"
  for _ in $(seq 1 $((secs * 2))); do
    [ "$(grep -c -- "$pat" "$file" 2>/dev/null || true)" -ge "$n" ] && return 0
    sleep 0.5
  done
  return 1
}

# The display's rotation (ROTATION_0 to ROTATION_270).
display_rotation() {
  adb_s shell dumpsys window displays 2>/dev/null | grep -oE 'mDisplayRotation=ROTATION_[0-9]+' | head -n 1 | sed 's/.*=//'
}

# --landscape: puts the rotation settings in (accelerometer_rotation 0, user_rotation 1) and waits up to
# $1 seconds for the display to turn (ROTATION_90). The system turns the display for the app on top and
# not while the launcher is (it holds portrait), so this runs again with the demo on top.
rotate_display() {
  adb_s shell settings put system accelerometer_rotation 0 || fail "settings put accelerometer_rotation"
  adb_s shell settings put system user_rotation 1 || fail "settings put user_rotation"
  for _ in $(seq 1 $(($1 * 2))); do
    [ "$(display_rotation)" = "ROTATION_90" ] && return 0
    sleep 0.5
  done
  return 1
}

# Launches the demo with the arguments $2... (written one per line to files/launch-args with run-as),
# its stdout following into the file $1, and waits up to 120 s for `presented 1`. The app is
# force-stopped first: a singleTask activity that is still alive would not read the arguments.
launch() {
  local log="$1"
  shift
  stop_follower
  adb_s shell am force-stop "$PKG" >/dev/null 2>&1 || true
  if [ "$#" -gt 0 ]; then
    printf '%s\n' "$@" \
      | adb_s shell "run-as $PKG sh -c 'mkdir -p files && cat > files/launch-args'" \
      || fail "could not write the launch arguments with run-as"
  else
    adb_s shell "run-as $PKG rm -f files/launch-args" >/dev/null 2>&1 || true
  fi
  adb_s logcat -c >/dev/null 2>&1 || true
  : > "$log"
  adb_s logcat -v raw -s RustStdoutStderr:I >> "$log" 2>/dev/null &
  follower_pid=$!
  adb_s shell am start -W -n "$ACTIVITY" >"$OUT/am-start.log" 2>&1 || { cat "$OUT/am-start.log" >&2; fail "am start"; }
  # `am start -W` gives up waiting after about 10 s with `Status: timeout` when the emulator is busy right
  # after a boot; the app may still come up, so only an error answer fails here and `presented 1` decides.
  if grep -q '^Status: timeout' "$OUT/am-start.log"; then
    echo "android-emu: am start -W timed out; waiting for the first frame anyway"
  else
    grep -q '^Status: ok' "$OUT/am-start.log" || { cat "$OUT/am-start.log" >&2; fail "am start did not report Status: ok"; }
  fi
  if ! wait_line "$log" "^presented 1 " 120; then
    dump_logcat
    echo "--- stdout (tail) ---" >&2
    tail -n 20 "$log" >&2 || true
    echo "--- logcat (errors, tail) ---" >&2
    grep -E ' (E|F) |FATAL|Abort|SIGSEGV|panicked' "$OUT/logcat.txt" | tail -n 20 >&2 || true
    fail "no 'presented 1' within 120 s (full logcat in $OUT/logcat.txt)"
  fi
  grep "^presented " "$log" | head -n 3 || true
  if [ "$landscape" -eq 1 ] && [ "$(display_rotation)" != "ROTATION_90" ]; then
    rotate_display 15 || fail "--landscape: the display did not turn (rotation $(display_rotation), want ROTATION_90)"
    sleep 3
  fi
}

# Whether the demo's window has input focus.
focused() {
  adb_s shell dumpsys window 2>/dev/null | grep mCurrentFocus | grep -q "$PKG"
}

# Input goes to the focused window, and a system dialog (Play services puts one up on a fresh emulator,
# a few seconds after a boot) can take focus from the demo. Until the demo is focused, three tries:
# Back (which the demo ignores, so it only dismisses a dialog) and `am start` on the demo's singleTask
# activity, which brings its task forward.
ensure_focus() {
  for _ in 1 2 3; do
    if focused; then
      sleep 0.5
      focused && return 0
    fi
    echo "android-emu: the demo is not the focused window ($(adb_s shell dumpsys window 2>/dev/null | grep mCurrentFocus | head -n 1 | sed 's/^ *//' | tr -d '\r')): Back, then bringing it forward"
    adb_s shell input keyevent KEYCODE_BACK >/dev/null 2>&1 || true
    sleep 1
    adb_s shell am start -W -n "$ACTIVITY" >/dev/null 2>&1 || true
    sleep 2
  done
  focused || fail "the demo never became the focused window (is a system dialog on top?)"
}

# Sends the input command $2... to the demo and waits for one more line matching the grep pattern $1
# in $log (a global) than there was: three tries, each after `ensure_focus`, 5 s each.
input_until() {
  local pat="$1" before try
  shift
  before="$(grep -c -- "$pat" "$log" || true)"
  for try in 1 2 3; do
    ensure_focus
    "$@" || return 1
    for _ in $(seq 1 10); do
      [ "$(grep -c -- "$pat" "$log" || true)" -gt "$before" ] && return 0
      sleep 0.5
    done
  done
  return 1
}

shot() {
  adb_s exec-out screencap -p > "$OUT/$1" 2>/dev/null || fail "screenshot $1"
  [ -s "$OUT/$1" ] || fail "empty screenshot $1"
  echo "android-emu: screenshot $OUT/$1 ($(wc -c < "$OUT/$1" | tr -d ' ') bytes)"
}

# --- --landscape: rotate the emulator's screen through the system settings ----------------------------
if [ "$landscape" -eq 1 ]; then
  old_accel="$(adb_s shell settings get system accelerometer_rotation | tr -d '\r')"
  old_user="$(adb_s shell settings get system user_rotation | tr -d '\r')"
  rotation_changed=1
  rotate_display 3 || true
  echo "android-emu: rotation settings put (user_rotation 1); were accelerometer_rotation '$old_accel', user_rotation '$old_user'; display rotation now $(display_rotation)"
fi

app_args=()
if [ "$probe" -eq 1 ]; then
  app_args=(--probe)
fi
if [ "$smoke" -eq 1 ]; then
  app_args+=(--probe-clipboard)
fi
if [ "$keyboard" -eq 1 ]; then
  app_args+=(--focus-field)
fi
if [ "$demo" -eq 1 ]; then
  app_args=(--demo --touches)
fi
app_args+=(${demo_args[@]+"${demo_args[@]}"})

# --- checks over a log ------------------------------------------------------------------------------

# Every `probe safe_area_event l,t,r,b` line of $1 must equal what the `probe content_rect` line
# before it gives (D7: l/s, t/s, (w-r)/s, (h-b)/s, each at least 0). With $2 = rest, the bottom must be
# the one the first content rect gives instead (D8: the keyboard does not move the safe bottom).
check_safe_area() {
  local log="$1" bottom="$2" verdict
  verdict="$(awk -v bottom="$bottom" '
    function mx(v) { return v < 0 ? 0 : v }
    function near(a, b) { d = a - b; return (d < 0 ? -d : d) < 0.01 }
    /^probe content_rect / {
      split($3, r, ","); split($5, wh, "x"); s = $7
      exp_l = mx(r[1] / s); exp_t = mx(r[2] / s); exp_r = mx((wh[1] - r[3]) / s); exp_b = mx((wh[2] - r[4]) / s)
      if (!seen) { rest_b = exp_b; seen = 1 }
      next
    }
    /^probe safe_area_event / {
      split($3, v, ","); n++
      want_b = (bottom == "rest") ? rest_b : exp_b
      if (!seen || !near(v[1], exp_l) || !near(v[2], exp_t) || !near(v[3], exp_r) || !near(v[4], want_b)) {
        bad = bad " [" $3 " != " exp_l "," exp_t "," exp_r "," want_b "]"
      }
    }
    END { printf "checked=%d bad=%s", n, (bad == "" ? "none" : bad) }
  ' "$log")"
  echo "android-emu: safe area: $verdict"
  case "$verdict" in
    "checked=0 "*) fail "no 'probe safe_area_event' line" ;;
    *"bad=none") ;;
    *) fail "the safe area differs from the content rect's: $verdict" ;;
  esac
}

# The soft keyboard in the log $1 of a `--focus-field` launch: `mInputShown=true` within 10 s,
# `probe keyboard_inset 0,0,0,<b>` with b above 200 within 20 s, and the field typed into through the OS
# (`input text luneui` gives `probe field "luneui"`).
check_keyboard() {
  local log="$1" line="" shown=0
  for _ in $(seq 1 20); do
    if adb_s shell dumpsys input_method 2>/dev/null | grep -q 'mInputShown=true'; then
      shown=1
      break
    fi
    sleep 0.5
  done
  echo "android-emu: mInputShown=true: $([ "$shown" -eq 1 ] && echo yes || echo no)"
  [ "$shown" -eq 1 ] || fail "dumpsys input_method never showed mInputShown=true within 10 s (the soft keyboard did not rise)"
  for _ in $(seq 1 40); do
    line="$(grep '^probe keyboard_inset ' "$log" | tail -n 1 || true)"
    [ -n "$line" ] && break
    sleep 0.5
  done
  grep '^probe keyboard_inset ' "$log" || true
  [ -n "$line" ] || fail "no 'probe keyboard_inset' line with the keyboard up"
  awk -v l="$line" 'BEGIN { split(l, p, " "); split(p[3], v, ","); exit !(v[1] == 0 && v[2] == 0 && v[3] == 0 && v[4] > 200) }' \
    || fail "keyboard inset is not 0,0,0,<b> with <b> above 200: $line"
}

check_typing() {
  local log="$1"
  ensure_focus
  adb_s shell input text luneui || fail "input text"
  wait_line "$log" '^probe field "luneui"$' 10 \
    || { grep '^probe field ' "$log" | tail -n 3 >&2 || true; fail "'input text luneui' did not give 'probe field \"luneui\"'"; }
  echo "android-emu: typed through the OS: $(grep '^probe field ' "$log" | tail -n 1)"
}

# --- --demo: the showcase tour (D417) ---------------------------------------------------------------
if [ "$demo" -eq 1 ]; then
  launch "$OUT/stdout.log" "${app_args[@]}"
  shot_marks=(sheet-half page-2 settings)
  shot_taken=(0 0 0)
  demo_start=$SECONDS
  deadline=$((SECONDS + 60 + 120))
  ended=0
  while :; do
    failed="$(grep '^demo mark .* FAIL' "$OUT/stdout.log" | head -n 1 || true)"
    if [ -n "$failed" ]; then
      grep '^demo ' "$OUT/stdout.log" > "$OUT/demo.txt" || true
      dump_logcat
      fail "$failed"
    fi
    for i in 0 1 2; do
      if [ "${shot_taken[$i]}" -eq 0 ] && grep -q "^demo mark ${shot_marks[$i]} ok" "$OUT/stdout.log"; then
        shot_taken[$i]=1
        shot "shot-$((i + 1)).png"
      fi
    done
    if grep -q '^demo end ' "$OUT/stdout.log"; then
      ended=1
      break
    fi
    [ "$SECONDS" -lt "$deadline" ] || break
    sleep 0.25
  done
  grep '^demo ' "$OUT/stdout.log" > "$OUT/demo.txt" || true
  cat "$OUT/demo.txt"
  if [ "$ended" -ne 1 ]; then
    dump_logcat
    echo "--- stdout (tail) ---" >&2
    tail -n 20 "$OUT/stdout.log" >&2 || true
    fail "no 'demo end' within the tour's length plus 120 s"
  fi
  end_line="$(grep '^demo end ' "$OUT/stdout.log" | head -n 1)"
  awk -v l="$end_line" 'BEGIN { split(l, p, " "); split(p[3], v, "/"); exit !(v[1] == v[2] && v[2] > 0) }' \
    || fail "not every mark passed: $end_line"
  marks_ok="$(grep -c '^demo mark .* ok$' "$OUT/stdout.log" || true)"
  [ "$marks_ok" -eq 16 ] || fail "$marks_ok marks ok, not 16 ($end_line)"
  for i in 0 1 2; do
    [ "${shot_taken[$i]}" -eq 1 ] || fail "no screenshot: mark ${shot_marks[$i]} never printed ok"
  done
  echo "android-emu: $end_line after $((SECONDS - demo_start)) s, $(grep -c '^presented ' "$OUT/stdout.log" || true) presented lines, demo lines in $OUT/demo.txt"
  dump_logcat
  adb_s shell am force-stop "$PKG" >/dev/null 2>&1 || true
  stop_follower
  echo "android-emu: passed"
  exit 0
fi

# --- --keyboard: launch 2 alone ----------------------------------------------------------------------
if [ "$keyboard" -eq 1 ]; then
  launch "$OUT/stdout.log" "${app_args[@]}"
  check_keyboard "$OUT/stdout.log"
  sleep 2
  check_safe_area "$OUT/stdout.log" rest
  shot shot-1.png
  check_typing "$OUT/stdout.log"
  dump_logcat
  adb_s shell am force-stop "$PKG" >/dev/null 2>&1 || true
  stop_follower
  echo "android-emu: passed"
  exit 0
fi

# --- default and --probe, then --smoke ---------------------------------------------------------------
launch "$OUT/stdout.log" ${app_args[@]+"${app_args[@]}"}

if [ "$probe" -eq 1 ]; then
  grep -q "^probe inner_size " "$OUT/stdout.log" || fail "no 'probe inner_size' line"
  sleep 2
else
  sleep 1
fi
shot shot-1.png

if [ "$smoke" -eq 1 ]; then
  log="$OUT/stdout.log"
  # The first frame's safe area is the content rect's (D7).
  check_safe_area "$log" strict
  # The clipboard (D9): the demo writes `luneui-probe-<pid>` on frame 2 and reads it on frame 4.
  wait_line "$log" '^probe clipboard ' 20 || true
  written="$(grep '^probe clipboard_write ' "$log" | head -n 1 | sed 's/^probe clipboard_write //')"
  readback="$(grep '^probe clipboard ' "$log" | head -n 1 | sed 's/^probe clipboard //')"
  echo "android-emu: clipboard written '$written', read back '$readback'"
  [ -n "$written" ] || fail "no 'probe clipboard_write' line"
  [ "$readback" = "$written" ] || fail "the clipboard read back '$readback', not '$written'"
  # The frame the demo schedules 1 s after its frame 30 (`Repaint::At`) must arrive while idle: nothing
  # else asks for frame 31 (no input has been sent yet).
  wait_line "$log" '^presented 31 ' 20 \
    || fail "the scheduled frame (Repaint::At) did not arrive while idle (no 'presented 31')"
  echo "android-emu: presented 31 arrived while idle"
  # Live touch through the OS (D12): the demo prints where its controls are, in physical pixels.
  item() { grep "^probe tab_item $1 " "$log" | tail -n 1 | sed 's/^probe tab_item [0-9]* //' | tr ',' ' '; }
  cards="$(item 1)"
  home="$(item 0)"
  [ -n "$cards" ] && [ -n "$home" ] || fail "no 'probe tab_item' lines"
  pager="$(grep '^probe pager ' "$log" | tail -n 1 | sed 's/^probe pager //')"
  [ -n "$pager" ] || fail "no 'probe pager' line"
  IFS=, read -r px0 py px1 <<< "$pager"
  # shellcheck disable=SC2086
  input_until '^probe tab 1$' adb_s shell input tap $cards \
    || fail "a tap on the Cards item ($cards) did not give 'probe tab 1'"
  echo "android-emu: tap on Cards ($cards): probe tab 1"
  sleep 1
  input_until '^probe page 1$' adb_s shell input swipe "$px0" "$py" "$px1" "$py" 300 \
    || fail "a swipe along the pager ($px0,$py to $px1) did not give 'probe page 1'"
  echo "android-emu: swipe on the pager ($px0,$py to $px1): probe page 1"
  sleep 1
  # shellcheck disable=SC2086
  input_until '^probe tab 0$' adb_s shell input tap $home \
    || fail "a tap on the Home item ($home) did not give 'probe tab 0'"
  echo "android-emu: tap on Home ($home): probe tab 0"
  # The life cycle: HOME, then the activity again. The surface is dropped on `suspended` (D6): no frame
  # is presented between `suspended` and `resumed`, and one follows.
  adb_s shell input keyevent KEYCODE_HOME || fail "input keyevent HOME"
  wait_line "$log" '^probe lifecycle suspended$' 10 || fail "no 'probe lifecycle suspended' after HOME"
  sleep 2
  adb_s shell am start -W -n "$ACTIVITY" >/dev/null 2>&1 || fail "am start after HOME"
  wait_count "$log" '^probe lifecycle resumed$' 2 10 || fail "no second 'probe lifecycle resumed'"
  sleep 3
  cycle="$(grep '^probe lifecycle ' "$log" | sed 's/^probe lifecycle //' | tr '\n' ' ')"
  echo "android-emu: life cycle: $cycle"
  grep -qE '^resumed .*suspended .*resumed' <<< "$cycle" \
    || fail "life cycle not 'resumed' at launch, then 'suspended' and 'resumed'"
  verdict="$(awk '
    /^probe lifecycle suspended/ { state = "suspended"; next }
    /^probe lifecycle resumed/   { if (state == "suspended") { state = "resumed"; trips++ } next }
    /^presented /                { if (state == "suspended") during++; else if (state == "resumed") after++ }
    END { printf "during=%d after=%d trips=%d", during, after, trips }
  ' "$log")"
  echo "android-emu: frames around the suspend: $verdict"
  [[ "$verdict" == "during=0 after="[1-9]* ]] \
    || fail "a frame was presented while suspended, or none after 'resumed' ($verdict)"
  grep '^probe tab_item \|^probe pager \|^probe tab \|^probe page ' "$log" | head -n 12 || true

  # Launch 2: the soft keyboard (D8). It is this run's second screenshot.
  launch "$OUT/stdout-2.log" --probe --focus-field ${demo_args[@]+"${demo_args[@]}"}
  check_keyboard "$OUT/stdout-2.log"
  sleep 2
  check_safe_area "$OUT/stdout-2.log" rest
  shot shot-2.png
  check_typing "$OUT/stdout-2.log"
fi

if [ "$probe" -eq 1 ]; then
  {
    echo "# device"
    adb_s shell wm size | tr -d '\r'
    adb_s shell wm density | tr -d '\r'
    echo "# stdout.log"
    grep "^probe \|^presented " "$OUT/stdout.log" || true
    if [ -s "$OUT/stdout-2.log" ]; then
      echo "# stdout-2.log (launch 2)"
      grep "^probe \|^presented " "$OUT/stdout-2.log" || true
    fi
  } > "$OUT/probe.txt"
  echo "android-emu: probe lines in $OUT/probe.txt ($(grep -c '^probe ' "$OUT/probe.txt" || true) lines)"
fi

dump_logcat
adb_s shell am force-stop "$PKG" >/dev/null 2>&1 || true
stop_follower
echo "android-emu: passed"
