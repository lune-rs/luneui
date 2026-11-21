# shellcheck shell=bash
# Sourced by gen-api-docs.sh and check-compat.sh: keeps their rustdoc runs from taking the machine
# down. A rustdoc run once grew to 113 GB on a 32 GB Mac (swap and compression) and the cause was not
# reproduced, so the scripts guard against it whatever the cause (D413):
#   - one run at a time per build directory (a lock), so two runs never share a target directory;
#   - CARGO_INCREMENTAL=0, so the build directory does not pile up incremental state;
#   - a watchdog that kills any rustc or rustdoc started by this script whose memory footprint passes
#     LUNEUI_RUSTDOC_MAX_GB (default 8), so the run fails and says why instead.
# bash 3.2 safe. The caller calls guard_lock and guard_watch once each and runs guard_cleanup on exit,
# which says what the watchdog killed, if anything.

export CARGO_INCREMENTAL=0

GUARD_LOCK=""
GUARD_WATCH_PID=""
GUARD_KILLED_FILE=""

guard_cleanup() {
  if [ -n "$GUARD_WATCH_PID" ]; then
    kill "$GUARD_WATCH_PID" 2> /dev/null || true
    wait "$GUARD_WATCH_PID" 2> /dev/null || true
  fi
  if guard_killed > /dev/null; then
    echo "memory guard: $(guard_killed); the run failed because of it (LUNEUI_RUSTDOC_MAX_GB raises the cap)" >&2
  fi
  if [ -n "$GUARD_LOCK" ]; then rm -rf "$GUARD_LOCK"; fi
  if [ -n "$GUARD_KILLED_FILE" ]; then rm -f "$GUARD_KILLED_FILE"; fi
}

# guard_lock <build dir> <script name>: takes <build dir>.lock or exits 2 naming the run that holds it.
# A lock whose holder is gone is taken over.
guard_lock() {
  local dir="$1" name="$2" holder
  mkdir -p "$(dirname "$dir")"
  GUARD_LOCK="$dir.lock"
  if ! mkdir "$GUARD_LOCK" 2> /dev/null; then
    holder="$(cat "$GUARD_LOCK/pid" 2> /dev/null || echo '')"
    if [ -n "$holder" ] && kill -0 "$holder" 2> /dev/null; then
      GUARD_LOCK=""
      echo "$name: another run (pid $holder) is using $dir; wait for it to end" >&2
      exit 2
    fi
    rm -rf "$GUARD_LOCK"
    mkdir "$GUARD_LOCK"
  fi
  echo "$$" > "$GUARD_LOCK/pid"
}

# guard_descendants: the pids of every process under this script.
guard_descendants() {
  ps -A -o pid= -o ppid= | awk -v root="$$" '
    { parent[$1] = $2 }
    END { for (p in parent) { q = p; while (q in parent && q != root && q > 1) q = parent[q]; if (q == root && p != root) print p } }'
}

# guard_watch <script name>: starts the watchdog in the background. The cap is LUNEUI_RUSTDOC_MAX_GB
# (whole GB, default 8). The footprint is top's MEM column, which counts compressed memory (ps's RSS
# does not, and read near zero for the 113 GB run).
guard_watch() {
  local name="$1" cap_gb="${LUNEUI_RUSTDOC_MAX_GB:-8}"
  case "$cap_gb" in '' | *[!0-9]*) echo "$name: LUNEUI_RUSTDOC_MAX_GB must be a whole number of GB" >&2; exit 2 ;; esac
  GUARD_KILLED_FILE="$(mktemp "${TMPDIR:-/tmp}/rustdoc-guard.XXXXXX")"
  (
    cap_mb=$((cap_gb * 1024))
    while :; do
      kids=" $(guard_descendants | tr '\n' ' ') "
      top -l 1 -stats pid,mem,command -o mem -n 40 2> /dev/null | awk -v cap="$cap_mb" -v kids="$kids" '
        $3 ~ /^(rustdoc|rustc)$/ && index(kids, " " $1 " ") {
          v = $2; sub(/[+-]$/, "", v); u = substr(v, length(v)); n = v + 0
          if (u == "G") n *= 1024; else if (u == "K") n /= 1024
          if (n > cap) print $1, int(n), $3 }' |
        while read -r pid mb cmd; do
          kill -9 "$pid" 2> /dev/null && echo "$cmd (pid $pid) reached $mb MB, over the ${cap_gb} GB cap" >> "$GUARD_KILLED_FILE"
        done
      sleep 2
    done
  ) &
  GUARD_WATCH_PID=$!
}

# guard_killed: prints what the watchdog killed (nothing when it killed nothing); true when it did.
guard_killed() {
  [ -n "$GUARD_KILLED_FILE" ] && [ -s "$GUARD_KILLED_FILE" ] && cat "$GUARD_KILLED_FILE"
}
