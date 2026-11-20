#!/usr/bin/env bash
# Generates docs/DECISIONS-INDEX.md (one line per row of docs/DECISIONS.md, with its status) with
# tools/decisions-index.
#   scripts/gen-decisions-index.sh          write the index
#   scripts/gen-decisions-index.sh --check  fail (exit 1, with a diff) when the index is stale
# Exit 2 on a usage or setup error or a validation error of DECISIONS.md (printed as
# `DECISIONS.md:<line>: <reason>`). Output is byte-stable: no dates, sorted by id. bash 3.2 safe.
set -euo pipefail
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# DECISIONS_FILE and DECISIONS_INDEX let tests run against temp copies; defaults are the real files.
SRC="${DECISIONS_FILE:-$ROOT/docs/DECISIONS.md}"
OUT="${DECISIONS_INDEX:-$ROOT/docs/DECISIONS-INDEX.md}"

flag=
case "${1:-}" in
  '') ;;
  --check) flag=--check ;;
  *) echo "usage: gen-decisions-index.sh [--check]" >&2; exit 2 ;;
esac

command -v cargo > /dev/null 2>&1 || { echo "gen-decisions-index: 'cargo' is not installed" >&2; exit 2; }
cd "$ROOT"

status=0
if [ -n "$flag" ]; then
  cargo run -q --locked -p decisions-index -- "$flag" "$SRC" "$OUT" || status=$?
else
  cargo run -q --locked -p decisions-index -- "$SRC" "$OUT" || status=$?
fi
if [ "$status" -eq 1 ]; then
  echo "gen-decisions-index: DECISIONS-INDEX.md is stale; run scripts/gen-decisions-index.sh" >&2
fi
exit "$status"
