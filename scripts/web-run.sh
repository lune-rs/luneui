#!/usr/bin/env bash
# Builds a web app and checks it in a browser: the web counterpart of ios-sim.sh (D427). Usage:
#   scripts/web-run.sh <package> [--demo] [--gl] [--smoke] [--type-field] [--phone] [--query <q>] [--no-build] [--keep-serving]
#   scripts/web-run.sh --self-test
#   (default)       scripts/web-build.sh <package>, serve target/web/<package>/ on 127.0.0.1 (an ephemeral port,
#                   loopback only, only files under that directory), open it in the separate Chromium (headless,
#                   a throwaway profile under target/web-run/, never the user's browsers), wait up to 30 s for
#                   `presented 1` in the page's console, require `probe adapter_backend BrowserWebGpu` and take
#                   one screenshot (target/web-run/<package>/shot-1.png).
#   --demo          the page's query is `?demo&touches`: the showcase tour plays hands-free. A `demo mark <name>
#                   FAIL` line fails at once (the line quoted); shot-1.png, shot-2.png and shot-3.png are taken at
#                   `sheet-half`, `page-2` and `settings`; it requires `demo end 16/16` and 16 marks `ok`. For
#                   `web-demo` it is the attract tour, played once through its six scenes (13 marks, `demo end
#                   13/13`); the shots are at `glass-live`, `data-live` and `theme-live` (one, at `motion-live`,
#                   with `--phone`), and the run prints the app's `web-demo frames` readings summarised (report
#                   only, no check waits on them). Every other `web-demo` run asks for no tour (`?interactive`), except `--keep-serving`, whose URL is a visitor's.
#   --gl            the query gains `backend=gl` (WebGL2 only) and the run requires `probe adapter_backend Gl`;
#                   its files are prefixed `gl-`.
#   --smoke         the query gains `probe` (the demo prints its probe lines): the run also requires the frame
#                   the demo schedules a second after its frame 30 with `Repaint::At` (`presented 31`).
#                   It also runs the clipboard round trip (`?probe-clipboard`, the browser's clipboard-read and
#                   clipboard-write granted for the server's origin): `probe clipboard` must equal `probe
#                   clipboard_write`, and `navigator.clipboard.readText()` in the page must return that text.
#                   A third page makes `navigator.clipboard.writeText` reject and requires that the toolkit's write raises no
#                   unhandled promise rejection (a failed write is silent).
#                   It also drives the web text bridge (D429; the query gains `probe-text-input`): the page's search
#                   field is clicked twice (at most one `probe focused` line for both), `hello` is typed
#                   (exactly `hello`), Backspace gives `hell`, a CDP composition then commit gives `hell日本`,
#                   Ctrl+A and Backspace empty it, then typing after Tab and after a click outside the canvas
#                   changes nothing; a second bridge attached and dropped leaves one element (`1 2 1`).
#   --type-field    for the web demo on its Text scene (`--query scene=text`): the query gains `probe-text`; the page's
#                   field is clicked, all of it is selected with the platform's chord (Cmd or Ctrl and A) and
#                   `keys` is typed with real key events; the app's `probe field` line must then say exactly
#                   `keys` (the text bridge, the keymap and the field's engine in one), and `text-typed.png` is shot.
#   --phone         for an app that fills the window (not `demo`): a phone viewport, 390x844 at device scale 3 with
#                   touch, instead of the desktop's 1440x900 at 2; the run requires `probe layout narrow` (the
#                   desktop run requires `probe layout wide`); files are prefixed `phone-`. Such an app also
#                   gets shot-2.png four seconds after shot-1.png, to show its motion.
#   --query <q>     appended to the page's query (`scene=data`, `still`, `motion=reduced`).
#   --no-build      serve what target/web/<package>/ holds.
#   --keep-serving  print the URL and wait for Ctrl-C, for a person; no browser is launched.
#   --self-test     the server's path rules (`..`, encoded and absolute paths answer 404) and the blank-shot check
#                   on known inputs; no browser, no build.
#   Every run fails on a `pageerror` and on a console error or warning that holds `wgpu`, `Validation` or
#   `panicked`; every shot needs more than 32 distinct colours. The page's console lines go to
#   target/web-run/<package>/demo.txt (gl-demo.txt). At most 5 screenshots per run, page.screenshot() only.
# The browser is LUNEUI_CHROMIUM (default ~/chromium/Chromium.app/Contents/MacOS/Chromium); a path inside
# /Applications/Google Chrome.app or /Applications/Firefox.app is refused. It is killed on every exit path.
# Needs node and pnpm (the driver's one package, an exact-pinned playwright-core, scripts/web/pnpm-lock.yaml).
# Not in the gate. Exit codes: 0 passed; 1 a check failed; 2 usage or setup (a tool, the browser, a refused path
# or a build failure).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

usage() {
  echo "usage: scripts/web-run.sh <package> [--demo] [--gl] [--smoke] [--type-field] [--phone] [--query <q>] [--no-build] [--keep-serving] | --self-test" >&2
  exit 2
}
fail() {
  echo "web-run: $*" >&2
  exit 2
}

PKG=""
build=1
self_test=0
driver_flags=()
while [ "$#" -gt 0 ]; do
  arg="$1"
  shift
  case "$arg" in
    --demo | --gl | --smoke | --type-field | --phone | --keep-serving) driver_flags+=("$arg") ;;
    --query)
      [ "$#" -gt 0 ] || usage
      case "$1" in *[!A-Za-z0-9_=\&.-]*) fail "bad --query \`$1\`" ;; esac
      driver_flags+=("$arg" "$1")
      shift
      ;;
    --no-build) build=0 ;;
    --self-test) self_test=1 ;;
    -*) usage ;;
    *)
      [ -z "$PKG" ] || usage
      PKG="$arg"
      ;;
  esac
done
if [ "$self_test" -eq 0 ] && [ -z "$PKG" ]; then usage; fi
if [ "$self_test" -eq 1 ] && { [ -n "$PKG" ] || [ "${#driver_flags[@]}" -ne 0 ] || [ "$build" -eq 0 ]; }; then usage; fi
case "$PKG" in
  *[!A-Za-z0-9_-]*) fail "bad package name \`$PKG\`" ;;
esac

command -v node >/dev/null 2>&1 || fail "node is not installed"
command -v pnpm >/dev/null 2>&1 || fail "pnpm is not installed"
pnpm install --frozen-lockfile --dir scripts/web >/dev/null 2>&1 \
  || fail "pnpm install --frozen-lockfile failed in scripts/web (network?)"

if [ "$self_test" -eq 1 ]; then
  exec node scripts/web/run.mjs --self-test --out "$ROOT/target/web-run/self-test"
fi

if [ "$build" -eq 1 ]; then
  scripts/web-build.sh "$PKG" || fail "the build failed"
fi

CHROMIUM="${LUNEUI_CHROMIUM:-$HOME/chromium/Chromium.app/Contents/MacOS/Chromium}"
exec node scripts/web/run.mjs --package "$PKG" --build-dir "$ROOT/target/web/$PKG" \
  --out "$ROOT/target/web-run/$PKG" --chromium "$CHROMIUM" ${driver_flags[@]+"${driver_flags[@]}"}
