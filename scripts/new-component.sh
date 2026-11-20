#!/usr/bin/env bash
# Scaffolds a new component drawn by `Ui` with tools/new-component: its catalogue entry
# (status planned), the widget module, its test file and a gallery page with one scene, wired into
# the build.
#   scripts/new-component.sh <area> <slug> [--title "<Title>"] [--owner <name>] [--dry-run]
# area: widget, overlay, layout or data (any other area is added by hand: docs/components/adding.md).
# slug: lowercase letters and digits, single hyphens, starting with a letter (the id is
# <area>.<slug>). Refuses (exit 2, nothing written) when a target exists, an anchor of the edited
# files is not found exactly once, or the name is bad. --dry-run prints what it would write and
# edits and writes nothing. It prints the commands to run next and the checklist rules still to
# hold by hand. Exit 0 done, 1 a write failed, 2 usage or a refusal. bash 3.2 safe.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [ "$#" -lt 2 ]; then
  echo "usage: new-component.sh <area> <slug> [--title \"<Title>\"] [--owner <name>] [--dry-run]" >&2
  exit 2
fi
command -v cargo > /dev/null 2>&1 || { echo "new-component: 'cargo' is not installed" >&2; exit 2; }
cd "$ROOT"

status=0
cargo run -q --locked -p new-component -- --root "$ROOT" "$@" || status=$?
exit "$status"
