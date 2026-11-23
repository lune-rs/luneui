#!/usr/bin/env bash
# Builds a web app and checks it in a browser: the web counterpart of ios-sim.sh (D427). Usage:
#   scripts/web-run.sh <package> [--demo] [--gl] [--smoke] [--no-build] [--keep-serving]
#   scripts/web-run.sh --self-test
#   (default)       scripts/web-build.sh <package>, serve target/web/<package>/ on 127.0.0.1 (an ephemeral port,
#                   loopback only, only files under that directory), open it in the separate Chromium (headless,
#                   a throwaway profile under target/web-run/, never the user's browsers), wait up to 30 s for
#                   `presented 1` in the page's console, require `probe adapter_backend BrowserWebGpu` and take
#                   one screenshot (target/web-run/<package>/shot-1.png).
#   --demo          the page's query is `?demo&touches`: the showcase tour plays hands-free. A `demo mark <name>
#                   FAIL` line fails at once (the line quoted); shot-1.png, shot-2.png and shot-3.png are taken at
#                   `sheet-half`, `page-2` and `settings`; it requires `demo end 16/16` and 16 marks `ok`.
#   --gl            the query gains `backend=gl` (WebGL2 only) and the run requires `probe adapter_backend Gl`;
#                   its files are prefixed `gl-`.
#   --smoke         the query gains `probe` (the demo prints its probe lines): the run also requires the frame
#                   the demo schedules a second after its frame 30 with `Repaint::At` (`presented 31`).
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
  echo "usage: scripts/web-run.sh <package> [--demo] [--gl] [--smoke] [--no-build] [--keep-serving] | --self-test" >&2
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
for arg in "$@"; do
  case "$arg" in
    --demo | --gl | --smoke | --keep-serving) driver_flags+=("$arg") ;;
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
