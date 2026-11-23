#!/usr/bin/env bash
# Compatibility check (D342): does the working tree keep every public item of a base release
# working for an app written against that release, and draw it the same? Three parts:
#   1 API      the public API of the five library crates at <rev> against the tree, from rustdoc JSON
#              (`api-docs compat`, tools/api-docs/src/compat.rs), plus the manifests: rust-version
#              and the winit and wgpu requirements.
#   2 consumers  <rev>'s gallery, integration tests (crates/*/tests) and docs/ (the data files those
#              tests read), unmodified, built against the tree's libraries in a hybrid tree and run.
#   3 pixels   <rev>'s gallery dumped against <rev>'s own libraries and against the tree's, compared
#              with dump-compare, no --allow.
#
#   scripts/check-compat.sh [--base <rev>] [--tree <dir>] [--api-only | --only-part <2|3>] [--self-test [<case>...]]
#
#   --base       revision to compare against (default v0.7.0), resolved with
#                `git rev-parse --verify <rev>^{commit}`; unknown is exit 2.
#   --tree       the candidate directory (default the repo root's working tree, uncommitted files
#                included).
#   --api-only   part 1 only (warm, a few seconds; this is the part that is in the gate).
#   --only-part  part 2 or part 3 alone, without part 1 (the deliberate-break proofs of D342 use it, as
#                a full run costs about 40 minutes; with no flag all three parts run, as always).
#   --self-test  part 1's proof on known breaks: each of 14 edits (B1-B14) must exit 1 and name the
#                item, each of 5 (A1-A5) must exit 0. Runs nothing else and needs no base. Case ids
#                after it limit the run (a debugging aid; the full run is the proof).
# Exit 0 held; 1 a break (one `break: ...` line per finding, then a count; or a failed self-test);
# 2 usage or setup (an unknown rev, a failed build of the base, a missing tool, a JSON format the
# pinned rustdoc-types does not read, a self-test anchor not found).
#
# Parts 2 and 3 build and dump the whole gallery twice; they run before a merge and in the done-when
# of a step that touches public API, never per commit. A test that a base cannot pass for a reason
# that is not API is listed in scripts/check-compat.exclude (it ships with no entries; an entry
# needs a plan revision naming the test and what it pins).
# Writes only under target/ and ${TMPDIR:-/tmp}/luneui-check-compat-<cksum of the repo root>/
# (the base source, the hybrid tree and the self-test copy); it edits no file of the tree. Never
# pushes and removes only what it made. The JSON needs the nightly flag --output-format json; it
# sets RUSTC_BOOTSTRAP=1 for the rustdoc runs only, as gen-api-docs.sh does (D287). bash 3.2 safe.
set -euo pipefail
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
REV="v0.7.0"
TREE=""
API_ONLY=0
ONLY_PART=""
SELF=0
CASES=""

die() {
  echo "check-compat: $*" >&2
  exit 2
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --base) [ "$#" -ge 2 ] || die "--base needs a value"; REV="$2"; shift 2 ;;
    --tree) [ "$#" -ge 2 ] || die "--tree needs a value"; TREE="$2"; shift 2 ;;
    --api-only) API_ONLY=1; shift ;;
    --only-part)
      [ "$#" -ge 2 ] || die "--only-part needs 2 or 3"
      case "$2" in 2 | 3) ONLY_PART="$2" ;; *) die "--only-part needs 2 or 3" ;; esac
      shift 2
      ;;
    --self-test)
      SELF=1
      shift
      while [ "$#" -gt 0 ] && case "$1" in -*) false ;; *) true ;; esac; do
        CASES="$CASES $1"
        shift
      done
      ;;
    -h | --help) sed -n '2,/^set -euo pipefail$/p' "${BASH_SOURCE[0]}" | sed '$d'; exit 0 ;;
    *) die "unknown argument: $1 (see -h)" ;;
  esac
done

DOC_TARGET=aarch64-apple-darwin
PACKAGES="lune-ui-core lune-ui-wgpu lune-ui-winit lune-ui-platform lune-ui"
LIBS="lune_ui_core lune_ui_wgpu lune_ui_winit lune_ui_platform lune_ui"
S="$ROOT/target/check-compat"
WORK="${TMPDIR:-/tmp}"
WORK="${WORK%/}/luneui-check-compat-$(printf '%s' "$ROOT" | cksum | cut -d ' ' -f 1)"
BASE_SRC="$WORK/base-src"
HYBRID="$WORK/hybrid"
BASE_TARGET="$S/base-target"
HYBRID_TARGET="$S/hybrid-target"

if [ -z "$TREE" ]; then
  TREE="$ROOT"
else
  TREE="$(cd "$TREE" 2> /dev/null && pwd -P)" || die "--tree: not a directory"
fi
[ -f "$TREE/Cargo.toml" ] || die "$TREE has no Cargo.toml"
[ "$API_ONLY" = 1 ] && [ -n "$ONLY_PART" ] && die "--api-only and --only-part exclude each other"

for tool in cargo git tar cksum; do
  command -v "$tool" > /dev/null 2>&1 || die "'$tool' is not installed"
done
if command -v rustup > /dev/null 2>&1; then
  rustup target list --installed 2> /dev/null | grep -qx "$DOC_TARGET" \
    || die "the $DOC_TARGET target is not installed; run: rustup target add $DOC_TARGET"
fi
mkdir -p "$S" "$WORK"
# shellcheck source=scripts/rustdoc-guard.sh
. "$ROOT/scripts/rustdoc-guard.sh"
trap guard_cleanup EXIT
guard_lock "$S" check-compat
guard_watch check-compat

# --- helpers --------------------------------------------------------------------------------------

# guard_cargo_config <dir>: no cargo config above <dir> except CARGO_HOME's (the pixel gate's guard).
guard_cargo_config() {
  local home d
  home="${CARGO_HOME:-$HOME/.cargo}"
  home="$(cd "$home" 2> /dev/null && pwd -P || printf '%s' "$home")"
  d="$(cd "$1/.." && pwd -P)"
  while :; do
    if [ -d "$d/.cargo" ] && [ "$(cd "$d/.cargo" && pwd -P)" != "$home" ]; then
      for f in "$d/.cargo/config.toml" "$d/.cargo/config"; do
        [ -e "$f" ] && die "cargo config above $1 would reach its build: $f"
      done
    fi
    [ "$d" = "/" ] && break
    d="$(dirname "$d")"
  done
}

# copy_tree <from> <to>: the tree without target/, .git and .claude (uncommitted files included).
# tar -m: files get the extraction time, so cargo rebuilds from exactly these sources.
copy_tree() {
  mkdir -p "$2"
  tar -C "$1" -c --exclude='./target' --exclude='./.git' --exclude='./.claude' . | tar -x -m -C "$2"
}

# doc_json <source dir> <target dir> <offline flag or ''> <package>...: rustdoc JSON of the packages
# into <target dir>/$DOC_TARGET/doc, the command of gen-api-docs.sh.
doc_json() {
  local src="$1" tgt="$2" off="$3" p log
  shift 3
  log="$S/doc-json.log"
  for p in "$@"; do
    # shellcheck disable=SC2086
    (cd "$src" && env -u RUSTDOCFLAGS -u CARGO_ENCODED_RUSTDOCFLAGS RUSTC_BOOTSTRAP=1 CARGO_TARGET_DIR="$tgt" \
      cargo rustdoc $off -q -p "$p" --lib --all-features --target "$DOC_TARGET" -- -Z unstable-options --output-format json) \
      > "$log" 2>&1 || {
      cat "$log" >&2
      die "rustdoc JSON of $p failed in $src"
    }
  done
}

# metadata <source dir> <out file> <flags>: cargo metadata of the workspace.
metadata() {
  # shellcheck disable=SC2086
  (cd "$1" && cargo metadata --format-version 1 --no-deps $3 > "$2") 2> "$S/metadata.log" || {
    cat "$S/metadata.log" >&2
    die "cargo metadata failed in $1"
  }
}

# api_compat <base json dir> <head json dir> <base meta> <head meta>: runs the comparator; prints its
# lines and leaves its exit status (0 held, 1 breaks) in COMPAT_RC. Exit 2 from it ends the run.
api_compat() {
  local out rc=0
  out="$(cd "$ROOT" && cargo run -q -p api-docs -- compat --base-json "$1" --head-json "$2" --base-meta "$3" --head-meta "$4" 2> "$S/compat.err")" || rc=$?
  if [ "$rc" -ge 2 ]; then
    cat "$S/compat.err" >&2
    die "api-docs compat failed (exit $rc)"
  fi
  COMPAT_RC="$rc"
  COMPAT_OUT="$out"
}

# --- self-test ------------------------------------------------------------------------------------

ST=""
ORIG=""
TOUCHED=""

# edit <file under the copy> <old text> <new text>: replaces <old> (which must occur exactly once).
edit() {
  local f="$ST/$1" n
  [ -f "$f" ] || die "self-test: no file $1"
  n="$(OLD="$2" perl -0777 -ne 'my $c = () = /\Q$ENV{OLD}\E/g; print $c' "$f")"
  [ "$n" = 1 ] || die "self-test anchor not found exactly once in $1 (found ${n:-0}): $2"
  if [ ! -e "$ORIG/$1" ]; then
    mkdir -p "$ORIG/$(dirname "$1")"
    cp "$f" "$ORIG/$1"
    TOUCHED="$TOUCHED $1"
  fi
  OLD="$2" NEW="$3" perl -0777 -pi -e 's/\Q$ENV{OLD}\E/$ENV{NEW}/' "$f"
}

# fixture <file under the copy> <old text> <new text>: an edit that stays: the base side of a case
# that needs something the tree does not have (applied before the pristine build, and never restored).
fixture() {
  edit "$@"
  rm -rf "$ORIG"
  TOUCHED=""
}

restore_all() {
  local f
  for f in $TOUCHED; do cp "$ORIG/$f" "$ST/$f"; done
  rm -rf "$ORIG"
  TOUCHED=""
}

# run_case <id> <expected exit> <package to rebuild or ''> <text the output must name or ''>: the
# edits are already applied; builds the head, compares, restores.
run_case() {
  local id="$1" want="$2" pkg="$3" needle="$4" head="$S/st-head" first
  rm -rf "$head"
  cp -R "$S/st-pristine" "$head"
  if [ -n "$pkg" ]; then
    doc_json "$ST" "$STT" --offline "$pkg"
    cp "$STT/$DOC_TARGET/doc/$(printf '%s' "$pkg" | tr '-' '_').json" "$head/"
  fi
  metadata "$ST" "$head/meta.json" --offline
  api_compat "$S/st-pristine" "$head" "$S/st-pristine/meta.json" "$head/meta.json"
  restore_all
  first="$(printf '%s\n' "$COMPAT_OUT" | head -n 1)"
  if [ "$COMPAT_RC" != "$want" ]; then
    echo "check-compat: self-test $id FAILED: exit $COMPAT_RC, wanted $want" >&2
    printf '%s\n' "$COMPAT_OUT" >&2
    SELF_FAILED=$((SELF_FAILED + 1))
    return
  fi
  if [ -n "$needle" ] && ! printf '%s\n' "$COMPAT_OUT" | grep -F -q -- "$needle"; then
    echo "check-compat: self-test $id FAILED: exit $COMPAT_RC but no finding names '$needle'" >&2
    printf '%s\n' "$COMPAT_OUT" >&2
    SELF_FAILED=$((SELF_FAILED + 1))
    return
  fi
  echo "self-test $id ok (exit $COMPAT_RC): $first"
  SELF_RAN=$((SELF_RAN + 1))
}

want_case() {
  [ -z "$CASES" ] && return 0
  case " $CASES " in *" $1 "*) return 0 ;; *) return 1 ;; esac
}

self_test() {
  command -v perl > /dev/null 2>&1 || die "'perl' is not installed (the self-test edits with it)"
  ST="$WORK/selftest-src"
  ORIG="$WORK/selftest-orig"
  STT="$S/selftest-target"
  SELF_FAILED=0
  SELF_RAN=0
  rm -rf "$ST" "$ORIG" "$S/st-pristine"
  copy_tree "$TREE" "$ST"
  guard_cargo_config "$ST"
  # Two public types the tree has no use for: one with two inherent blocks of different bounds (B13)
  # and one whose trait impl has an associated type (B12). Both sides of every case carry them.
  fixture crates/lune-ui-core/src/color.rs '/// `c` with alpha 0:' '/// Self-test fixture: a generic type with two inherent blocks.
pub struct SelftestPair<T>(pub T);

impl<T> SelftestPair<T> {
    /// Self-test fixture.
    pub fn plain(&self) {}
}

impl<T: Clone> SelftestPair<T> {
    /// Self-test fixture.
    pub fn cloned(&self) {}
}

/// Self-test fixture: a type whose trait impl has an associated type.
pub struct SelftestIter;

impl Iterator for SelftestIter {
    type Item = u8;
    fn next(&mut self) -> Option<u8> {
        None
    }
}

/// `c` with alpha 0:'
  echo "check-compat: self-test: building the pristine copy"
  doc_json "$ST" "$STT" --offline $PACKAGES
  mkdir -p "$S/st-pristine"
  for lib in $LIBS; do cp "$STT/$DOC_TARGET/doc/$lib.json" "$S/st-pristine/"; done
  metadata "$ST" "$S/st-pristine/meta.json" --offline

  local core=lune-ui-core
  local resp=crates/lune-ui-core/src/ui/response.rs
  local input=crates/lune-ui-core/src/input.rs
  local color=crates/lune-ui-core/src/color.rs
  local text=crates/lune-ui-core/src/text.rs
  local fake=crates/lune-ui-core/src/testing/text.rs
  local layout=crates/lune-ui-core/src/layout.rs

  if want_case B1; then
    edit $resp '    pub skip_tab: bool,' '    pub skip_tab: bool,
    /// Self-test field.
    pub selftest_extra: bool,'
    run_case B1 1 $core 'Sense'
  fi
  if want_case B2; then
    edit crates/lune-ui-core/src/widgets/checkbox.rs 'pub fn box_rect(r: Rect) -> Rect {' 'pub fn box_rect(r: Rect, _selftest_pad: f32) -> Rect {'
    edit crates/lune-ui-core/src/widgets/checkbox.rs 'let b = box_rect(r);' 'let b = box_rect(r, 0.0);'
    run_case B2 1 $core 'box_rect'
  fi
  if want_case B3; then
    edit crates/lune-ui-core/src/widgets/tab_strip.rs 'pub fn busy_treatment(' 'pub(crate) fn busy_treatment('
    run_case B3 1 $core 'busy_treatment'
  fi
  if want_case B4; then
    edit crates/lune-ui-core/src/motion.rs 'pub const SPIN_PERIOD: Duration = Duration::from_millis(1200);' 'pub mod selftest_moved {
    use std::time::Duration;
    /// Moved with no re-export.
    pub const SPIN_PERIOD: Duration = Duration::from_millis(1200);
}
use self::selftest_moved::SPIN_PERIOD;'
    run_case B4 1 $core 'SPIN_PERIOD'
  fi
  if want_case B5; then
    edit $input '    Forward,
    Other(u16),' '    Forward,
    SelftestAdded,
    Other(u16),'
    run_case B5 1 $core 'PointerButton'
  fi
  if want_case B6; then
    edit $input 'pub struct Modifiers {' '#[non_exhaustive]
pub struct Modifiers {'
    run_case B6 1 $core 'Modifiers'
  fi
  if want_case B7; then
    edit $text 'pub trait TextSystem {' 'pub trait TextSystem {
    /// Self-test: a required method.
    fn selftest_required(&self);
'
    edit $fake 'impl TextSystem for FakeText {' 'impl TextSystem for FakeText {
    fn selftest_required(&self) {}
'
    run_case B7 1 $core 'TextSystem'
  fi
  if want_case B8; then
    # Copy needs Clone, so both go; the library still builds without them.
    edit $layout '#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Insets {' '#[derive(Debug, PartialEq, Default)]
pub struct Insets {'
    run_case B8 1 $core 'Clone'
  fi
  if want_case B9; then
    edit crates/lune-ui-core/src/desktop.rs '    pub units_per_logical: f32,' '    pub units_per_logical: f64,'
    run_case B9 1 $core 'units_per_logical'
  fi
  if want_case B10; then
    edit Cargo.toml 'rust-version = "1.85"' 'rust-version = "1.86"'
    run_case B10 1 '' 'rust-version'
  fi
  if want_case B11; then
    edit crates/lune-ui-winit/Cargo.toml 'winit = "0.30"' 'winit = "0.31"'
    run_case B11 1 '' 'winit'
  fi
  if want_case B12; then
    edit $color 'type Item = u8;' 'type Item = u16;'
    edit $color 'fn next(&mut self) -> Option<u8> {' 'fn next(&mut self) -> Option<u16> {'
    run_case B12 1 $core 'SelftestIter::Item'
  fi
  if want_case B13; then
    edit $color 'impl<T: Clone> SelftestPair<T> {' 'impl<T: Clone + Copy> SelftestPair<T> {'
    run_case B13 1 $core 'SelftestPair'
  fi
  if want_case B14; then
    # A target-specific entry whose requirement differs from the untargeted one is not additive.
    edit crates/lune-ui-wgpu/Cargo.toml 'wgpu = { version = "30", features = ["webgl"] }' 'wgpu = { version = "30.0", features = ["webgl"] }'
    run_case B14 1 '' 'wgpu'
  fi
  if want_case A1; then
    edit $color '/// `c` with alpha 0:' '/// Self-test: a new public function.
pub fn selftest_added() {}

/// `c` with alpha 0:'
    run_case A1 0 $core ''
  fi
  if want_case A2; then
    edit $input 'pub enum Event {' 'pub enum Event {
    /// Self-test: a new variant.
    SelftestAdded,'
    run_case A2 0 $core ''
  fi
  if want_case A3; then
    edit $text 'pub trait TextSystem {' 'pub trait TextSystem {
    /// Self-test: a new provided method.
    fn selftest_provided(&self) {}
'
    run_case A3 0 $core ''
  fi
  if want_case A4; then
    run_case A4 0 $core ''
  fi
  if want_case A5; then
    # An added target-specific entry with the untargeted entry's requirement (extra features on one
    # target, as lune-ui-wgpu's webgl on wasm) is not a break (D425).
    edit crates/lune-ui-winit/Cargo.toml 'web-time = "1"' 'web-time = "1"
winit = { version = "0.30", features = ["rwh_06"] }'
    run_case A5 0 '' ''
  fi

  if [ "$SELF_FAILED" -gt 0 ]; then
    echo "check-compat: self-test FAILED: $SELF_FAILED case(s) failed, $SELF_RAN passed" >&2
    exit 1
  fi
  echo "check-compat: self-test: $SELF_RAN cases passed"
  exit 0
}

if [ "$SELF" = 1 ]; then
  self_test
fi

# --- base revision --------------------------------------------------------------------------------
SHA="$(git -C "$ROOT" rev-parse --verify "$REV^{commit}" 2> /dev/null)" || die "not a commit: $REV"
SHORT="$(printf '%s' "$SHA" | cut -c 1-7)"
RUSTC_KEY="$(rustc -V 2> /dev/null | cksum | cut -d ' ' -f 1)"
BASE_JSON="$S/json-$SHA-$RUSTC_KEY"

# prepare_base: the base's source from git archive, fresh each run (tar -m: see copy_tree).
BASE_READY=0
prepare_base() {
  [ "$BASE_READY" = 1 ] && return 0
  rm -rf "$BASE_SRC"
  mkdir -p "$BASE_SRC"
  git -C "$ROOT" archive "$SHA" | tar -x -m -C "$BASE_SRC" || die "git archive of $REV failed"
  [ -f "$BASE_SRC/apps/gallery/Cargo.toml" ] || die "base $REV has no apps/gallery (nothing to compare)"
  guard_cargo_config "$BASE_SRC"
  BASE_READY=1
}

COMPAT_RC=0
COMPAT_OUT=""
BREAKS=0

# --- part 1: API ----------------------------------------------------------------------------------
if [ -z "$ONLY_PART" ]; then
  echo "check-compat: part 1 (API) against $REV ($SHORT)"
  if [ ! -f "$BASE_JSON/meta.json" ]; then
    prepare_base
    echo "check-compat: building the base's rustdoc JSON (cached in ${BASE_JSON#"$ROOT"/})"
    doc_json "$BASE_SRC" "$BASE_TARGET" --offline $PACKAGES
    rm -rf "$BASE_JSON.tmp"
    mkdir -p "$BASE_JSON.tmp"
    for lib in $LIBS; do cp "$BASE_TARGET/$DOC_TARGET/doc/$lib.json" "$BASE_JSON.tmp/"; done
    metadata "$BASE_SRC" "$BASE_JSON.tmp/meta.json" "--offline --locked"
    rm -rf "$BASE_JSON"
    mv "$BASE_JSON.tmp" "$BASE_JSON"
  fi
  if [ "$TREE" = "$ROOT" ]; then
    HEAD_TARGET="$ROOT/target/api-docs"
    # gen-api-docs.sh builds, clears and reads this directory under its own lock (D413): take it
    # too, so the two never run in it together.
    guard_lock "$HEAD_TARGET" check-compat
  else
    HEAD_TARGET="$S/head-target"
  fi
  doc_json "$TREE" "$HEAD_TARGET" "" $PACKAGES
  metadata "$TREE" "$S/head-meta.json" "--offline --locked"
  api_compat "$BASE_JSON" "$HEAD_TARGET/$DOC_TARGET/doc" "$BASE_JSON/meta.json" "$S/head-meta.json"
  printf '%s\n' "$COMPAT_OUT"
  if [ "$COMPAT_RC" != 0 ]; then
    BREAKS=$((BREAKS + 1))
  fi
fi

if [ "$API_ONLY" = 1 ]; then
  [ "$BREAKS" = 0 ] || { echo "check-compat: FAILED: API break" >&2; exit 1; }
  echo "check-compat: ok (API only)"
  exit 0
fi

# --- part 2: the base's consumers against the tree's libraries --------------------------------------
prepare_base
echo "check-compat: the hybrid tree: the base's gallery and tests on the tree's libraries"
rm -rf "$HYBRID"
copy_tree "$TREE" "$HYBRID"
rm -rf "$HYBRID/apps/gallery"
cp -R "$BASE_SRC/apps/gallery" "$HYBRID/apps/gallery"
for d in "$HYBRID"/crates/*/; do rm -rf "${d}tests"; done
for d in "$BASE_SRC"/crates/*/tests; do
  [ -d "$d" ] || continue
  name="$(basename "$(dirname "$d")")"
  [ -d "$HYBRID/crates/$name" ] || continue
  cp -R "$d" "$HYBRID/crates/$name/tests"
done
# The base's tests read the workspace's data files (docs/requirements.md, the component entries): a
# consumer ships its own, so the hybrid's docs/ is the base's too (D342). A base with no docs/ gives
# a hybrid with none.
rm -rf "$HYBRID/docs"
if [ -d "$BASE_SRC/docs" ]; then
  cp -R "$BASE_SRC/docs" "$HYBRID/docs"
  cmp -s "$BASE_SRC/docs/requirements.md" "$HYBRID/docs/requirements.md" \
    || die "the hybrid's docs/requirements.md is not the base's after the copy"
elif [ -e "$HYBRID/docs" ]; then
  die "the hybrid holds a docs/ the base does not have"
fi
cp "$TREE/Cargo.lock" "$HYBRID/Cargo.lock"

# Packages the base does not have are left out of the workspace run.
metadata "$BASE_SRC" "$S/base-meta-members.json" "--offline"
metadata "$HYBRID" "$S/hybrid-meta-members.json" "--offline"
(cd "$ROOT" && cargo run -q -p api-docs -- compat --list-members "$S/base-meta-members.json") | sort > "$S/members-base.txt"
(cd "$ROOT" && cargo run -q -p api-docs -- compat --list-members "$S/hybrid-meta-members.json") | sort > "$S/members-hybrid.txt"
EXCLUDE=()
for p in $(comm -13 "$S/members-base.txt" "$S/members-hybrid.txt"); do
  EXCLUDE+=(--exclude "$p")
done

# scripts/check-compat.exclude: `path | test name | reason | D-row`; the test name is skipped by name.
SKIPS=()
if [ -f "$ROOT/scripts/check-compat.exclude" ]; then
  while IFS= read -r line; do
    case "$line" in '' | '#'*) continue ;; esac
    name="$(printf '%s' "$line" | awk -F'|' '{gsub(/^ +| +$/, "", $2); print $2}')"
    [ -n "$name" ] || die "scripts/check-compat.exclude: a line without a test name: $line"
    SKIPS+=(--skip "$name")
  done < "$ROOT/scripts/check-compat.exclude"
fi
TAIL=()
[ "${#SKIPS[@]}" -gt 0 ] && TAIL=(-- "${SKIPS[@]}")

# hybrid_run <log name> <cargo args...>: a failure is a break (the base's code or tests no longer
# build or pass against the tree's libraries).
hybrid_run() {
  local name="$1" rc=0
  shift
  echo "check-compat: part 2: cargo $*"
  (cd "$HYBRID" && env -u LUNEUI_GALLERY_TEXT CARGO_TARGET_DIR="$HYBRID_TARGET" cargo "$@") > "$S/part2-$name.log" 2>&1 || rc=$?
  if [ "$rc" != 0 ]; then
    tail -n 40 "$S/part2-$name.log" >&2
    echo "break: part 2: 'cargo $*' failed in the hybrid tree (exit $rc; full log ${S#"$ROOT"/}/part2-$name.log)"
    BREAKS=$((BREAKS + 1))
  fi
}
if [ -z "$ONLY_PART" ] || [ "$ONLY_PART" = 2 ]; then
  echo "check-compat: part 2 (consumers)"
  hybrid_run workspace test --offline --workspace ${EXCLUDE[@]+"${EXCLUDE[@]}"} --test '*' --no-fail-fast ${TAIL[@]+"${TAIL[@]}"}
  hybrid_run winit test --offline -p lune-ui-winit --test '*' --no-fail-fast ${TAIL[@]+"${TAIL[@]}"}
  hybrid_run gallery test --offline -p gallery --no-fail-fast ${TAIL[@]+"${TAIL[@]}"}
fi

# --- part 3: pixels -----------------------------------------------------------------------------------
if [ -z "$ONLY_PART" ] || [ "$ONLY_PART" = 3 ]; then
  echo "check-compat: part 3 (pixels): the base's gallery on the base's and on the tree's libraries"
  DUMP_BASE="$S/dump-base"
  DUMP_HYBRID="$S/dump-hybrid"
  rm -rf "$DUMP_BASE" "$DUMP_HYBRID"
  mkdir -p "$DUMP_BASE" "$DUMP_HYBRID"
  (cd "$BASE_SRC" && env -u LUNEUI_GALLERY_TEXT cargo run -q --offline --locked --target-dir "$BASE_TARGET" -p gallery -- --dump "$DUMP_BASE") \
    || die "the base's gallery failed to build or dump"
  dumped="$(find "$DUMP_BASE" -maxdepth 1 -type f -name '*.png' | wc -l | tr -d ' ')"
  [ "$dumped" -gt 0 ] || die "the base's dump holds no PNGs"
  rc=0
  (cd "$HYBRID" && env -u LUNEUI_GALLERY_TEXT CARGO_TARGET_DIR="$HYBRID_TARGET" cargo run -q --offline -p gallery -- --dump "$DUMP_HYBRID") \
    > "$S/part3-dump.log" 2>&1 || rc=$?
  if [ "$rc" != 0 ]; then
    tail -n 40 "$S/part3-dump.log" >&2
    echo "break: part 3: the base's gallery failed to dump against the tree's libraries (exit $rc)"
    BREAKS=$((BREAKS + 1))
  else
    rc=0
    out="$(cd "$ROOT" && cargo run -q --locked -p dump-compare -- "$DUMP_BASE" "$DUMP_HYBRID" 2>&1)" || rc=$?
    printf '%s\n' "$out" | tail -n 3
    printf '%s\n' "$out" | tail -n 1 | grep -q -E '^[0-9]+ files, ' || die "dump-compare printed no summary (exit $rc)"
    if [ "$rc" != 0 ]; then
      printf '%s\n' "$out" | grep -v ': identical' | head -n 20 | sed 's/^/break: part 3: /'
      BREAKS=$((BREAKS + 1))
    fi
  fi
fi

if [ "$BREAKS" != 0 ]; then
  echo "check-compat: FAILED: $BREAKS part(s) broke" >&2
  exit 1
fi
echo "check-compat: ok (API, consumers and pixels held against $REV)"
