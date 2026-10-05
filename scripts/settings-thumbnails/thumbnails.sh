#!/bin/bash
# Regenerates the gallery thumbnails from the real screensaver.
# usage: thumbnails.sh <plasma-visual-screensaver binary> <output dir>
# Each visual runs in its own headless KWin (640x360) on a private D-Bus
# without service activation, captured on Pure Black after it settles, then
# converted to 320x180 luminance-alpha PNGs by make_thumbs.py.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
binary="${1:?binary}"; out="${2:?output dir}"
scratch="${XDG_CACHE_HOME:-$HOME/.cache}/agent-scratch/plasma-wayland-screensaver/settings-thumbnails"
mkdir -p "$scratch"
work="$(mktemp -d "$scratch/capture.XXXXXX")"
trap 'rm -rf "$work"' EXIT
export W=640 H=360
grab() {  # visual [delay-s duration-ms] [Key=Value...]
    local v="$1" delay="$2" dur="$3"; shift 3
    GRAB_DELAY="$delay" DUR="$dur" timeout 150 dbus-run-session --config-file="$here/session-bus.conf" -- \
        python3 "$here/session.py" "$binary" "$work" "$v" "$work/raw/$v.png" BackgroundStyle=black "$@" \
        </dev/null
    echo "captured $v"
}
mkdir -p "$work/raw"
for v in aurora orbs starfield matrix kaleidoscope fireflies ribbons; do grab "$v" 6 9000; done
grab bounce 5 9000 AnimationScale=170 TrailAmount=75 BallCount=7
grab constellation 6 9000 AnimationDensity=90 TrailAmount=85 AnimationScale=150
grab snakes 70 75000 AnimationDensity=65
python3 "$here/make_thumbs.py" "$work/raw" "$out"
