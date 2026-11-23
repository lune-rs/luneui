#!/usr/bin/env bash
# Builds one web app for the browser: no bundler (D426). Usage: scripts/web-build.sh <package>
#   <package>       an app under apps/ with a web/ directory (apps/<package>/web/index.html and what it
#                   loads) and a binary of the same name, such as `demo`.
#   1. cargo build --release --locked --target wasm32-unknown-unknown -p <package> --bin <package>
#   2. the `wasm-bindgen` CLI of exactly the version Cargo.lock holds for the crate `wasm-bindgen`
#      (`--target web --no-typescript`): the one on PATH when its version matches, else
#      target/web-tools/bin/wasm-bindgen, installed there with `cargo install --locked
#      wasm-bindgen-cli --version <v> --root target/web-tools` when missing or of another version. A CLI
#      of another version is never used: the glue and the module must match.
#   3. apps/<package>/web/* copied next to the glue, into target/web/<package>/ (which `web-run.sh` serves).
#   No wasm-opt (not installed): the sizes printed are the raw and gzipped `.wasm` as wasm-bindgen wrote it.
# Writes only under target/. Exit codes: 0 built; 2 usage or setup (a tool, the rustup target, the app, a
# CLI of the lock's version, or the build, failed; the line says which).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

fail() {
  echo "web-build: $*" >&2
  exit 2
}

if [ "$#" -ne 1 ] || [ -z "$1" ] || [[ "$1" == -* ]]; then
  echo "usage: scripts/web-build.sh <package>" >&2
  exit 2
fi
PKG="$1"
case "$PKG" in
  *[!A-Za-z0-9_-]*) fail "bad package name \`$PKG\`" ;;
esac
[ -f "apps/$PKG/web/index.html" ] || fail "apps/$PKG/web/index.html is missing (not a web app)"

TARGET=wasm32-unknown-unknown
rustup target list --installed 2>/dev/null | grep -qx "$TARGET" \
  || fail "the rustup target $TARGET is missing (run: rustup target add $TARGET)"

# The version of the crate `wasm-bindgen` in the lock: the `version` line right after `name = "wasm-bindgen"`.
WB_VERSION="$(awk '
  $0 == "name = \"wasm-bindgen\"" { want = 1; next }
  want && /^version = / { gsub(/version = |"/, ""); print; exit }
  want { want = 0 }
' Cargo.lock)"
[ -n "$WB_VERSION" ] || fail "Cargo.lock holds no crate wasm-bindgen"

TOOLS="$ROOT/target/web-tools"
cli_version() {
  # `wasm-bindgen 0.2.129` (and maybe a hash): the second word.
  "$1" --version 2>/dev/null | awk '{ print $2; exit }'
}
CLI=""
if command -v wasm-bindgen >/dev/null 2>&1 && [ "$(cli_version "$(command -v wasm-bindgen)")" = "$WB_VERSION" ]; then
  CLI="$(command -v wasm-bindgen)"
elif [ -x "$TOOLS/bin/wasm-bindgen" ] && [ "$(cli_version "$TOOLS/bin/wasm-bindgen")" = "$WB_VERSION" ]; then
  CLI="$TOOLS/bin/wasm-bindgen"
else
  echo "web-build: installing wasm-bindgen-cli $WB_VERSION into target/web-tools" >&2
  mkdir -p "$TOOLS"
  cargo install --locked wasm-bindgen-cli --version "$WB_VERSION" --root "$TOOLS" >&2 \
    || fail "no wasm-bindgen CLI of version $WB_VERSION (Cargo.lock's) and cargo install failed (network?)"
  [ "$(cli_version "$TOOLS/bin/wasm-bindgen")" = "$WB_VERSION" ] \
    || fail "the installed wasm-bindgen CLI is not version $WB_VERSION"
  CLI="$TOOLS/bin/wasm-bindgen"
fi

cargo build --release --locked --target "$TARGET" -p "$PKG" --bin "$PKG" >&2 \
  || fail "cargo build of $PKG for $TARGET failed"
WASM="target/$TARGET/release/$PKG.wasm"
[ -f "$WASM" ] || fail "the build left no $WASM"

OUT="target/web/$PKG"
mkdir -p "$OUT"
"$CLI" --target web --no-typescript --out-dir "$OUT" --out-name "$PKG" "$WASM" >&2 \
  || fail "wasm-bindgen $WB_VERSION failed on $WASM"
cp -R "apps/$PKG/web/." "$OUT/"

RAW="$(wc -c < "$OUT/${PKG}_bg.wasm" | tr -d ' ')"
GZ="$(gzip -9 -c "$OUT/${PKG}_bg.wasm" | wc -c | tr -d ' ')"
echo "web-build: $OUT ready (wasm-bindgen $WB_VERSION; ${PKG}_bg.wasm $RAW bytes, $GZ gzipped)"
