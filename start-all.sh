#!/usr/bin/env bash
# Starts all Vibestar services and streams their logs into one terminal with
# per-service colored prefixes. Press Ctrl+C to stop everything.
#
# Services:
#   xr50  XVisio XR50 6DOF bridge      ws://localhost:8080
#   leap  Ultraleap v6 hand bridge     ws://localhost:6437/v6.json
#   web   Vite dev server (browser UI) http://localhost:5173
#
# Usage: ./start-all.sh [--no-xr50] [--no-leap] [--no-web]

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOGDIR="$ROOT/logs"
mkdir -p "$LOGDIR"

no_xr50=0; no_leap=0; no_web=0
for arg in "$@"; do
    case "$arg" in
        --no-xr50) no_xr50=1 ;;
        --no-leap) no_leap=1 ;;
        --no-web)  no_web=1 ;;
        -h|--help) echo "Usage: $0 [--no-xr50] [--no-leap] [--no-web]"; exit 0 ;;
        *) echo "Unknown option: $arg" >&2; exit 1 ;;
    esac
done

c_sys='\033[90m'; c_xr50='\033[36m'; c_leap='\033[32m'; c_web='\033[33m'; c_off='\033[0m'
say() { printf '%b[sys]%b %s\n' "$c_sys" "$c_off" "$*"; }

trap 'exit 130' INT TERM
trap 'echo; say "stopping all services..."; kill 0 2>/dev/null' EXIT

# run <name> <color> <workdir> <command...>
# Runs the command in its own subshell (so kill 0 reaps it), prefixing every
# line and mirroring it to logs/<name>.log.
run() {
    local name="$1" color="$2" dir="$3"; shift 3
    say "starting $name: $*"
    local prefix
    prefix="$(printf '%b[%s]%b ' "$color" "$name" "$c_off")"
    ( cd "$dir" && exec "$@" ) > >(sed -u "s/^/${prefix}/" | tee "$LOGDIR/$name.log") 2>&1 &
}

if [ "$no_xr50" -eq 0 ]; then
    run xr50 "$c_xr50" "$ROOT/xvisio-rs" cargo run --release --example server
fi

if [ "$no_leap" -eq 0 ]; then
    leap_bin="$ROOT/ultraleap-websocket/build/Ultraleap-Tracking-WS"
    if [ -x "$leap_bin" ]; then
        run leap "$c_leap" "$ROOT/ultraleap-websocket/build" "$leap_bin"
    else
        say "WARN: Ultraleap bridge not built at $leap_bin"
        say "      build: cd ultraleap-websocket && mkdir -p build && cd build && cmake .. && cmake --build ."
        say "      also ensure the Ultraleap tracking service (ultraleap-hand-tracking-service) is running."
    fi
fi

if [ "$no_web" -eq 0 ]; then
    run web "$c_web" "$ROOT/visual-test" npm run dev
fi

say "logs in $LOGDIR | web http://localhost:5173  xr50 ws://localhost:8080  leap ws://localhost:6437/v6.json"
say "press Ctrl+C to stop all"
wait
