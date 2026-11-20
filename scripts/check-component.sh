#!/usr/bin/env bash
# Checks components against the new-widget checklist (docs/components/README.md, "New widget
# checklist"): the entry's id and path, a gallery scene, a cited decision row, a cited test, the
# done evidence. For a component still being built, run it with its id until it exits 0.
#   scripts/check-component.sh <id>...   check the named entries, whatever their status (a status
#                                        that is not `done` fails and the rules still to hold by
#                                        hand are printed)
#   scripts/check-component.sh --all     check every `done` entry (what `cargo test -p gallery` runs)
# First checks that docs/components/INDEX.md and docs/DECISIONS-INDEX.md are current (a stale one
# is exit 1 with the generator's message). Exit 0 all pass, 1 a failure or a stale index, 2 usage,
# an unknown id or a table the check cannot read. The check reads the source tree the gallery is
# built from. bash 3.2 safe.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [ "$#" -eq 0 ]; then
  echo "usage: check-component.sh <id>... | --all" >&2
  exit 2
fi
ids=()
all=0
for arg in "$@"; do
  case "$arg" in
    --all) all=1 ;;
    -*) echo "usage: check-component.sh <id>... | --all" >&2; exit 2 ;;
    *) ids+=("$arg") ;;
  esac
done
if [ "$all" -eq 1 ] && [ "${#ids[@]}" -gt 0 ]; then
  echo "usage: check-component.sh <id>... | --all (not both)" >&2
  exit 2
fi

command -v cargo > /dev/null 2>&1 || { echo "check-component: 'cargo' is not installed" >&2; exit 2; }
cd "$ROOT"

scripts/gen-component-index.sh --check
scripts/gen-decisions-index.sh --check

status=0
if [ "$all" -eq 1 ]; then
  cargo run -q -p gallery -- --check-components || status=$?
else
  cargo run -q -p gallery -- --check-components "${ids[@]}" || status=$?
fi
exit "$status"
