#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail

if [[ $# != 4 ]]; then
    echo "Usage: $0 CANDIDATE_BINARY BASELINE_BINARY_OR_EMPTY GRAPHICS_API OUTPUT_DIR" >&2
    exit 2
fi
candidate=$1
baseline=$2
graphics_api=$3
output_dir=$4
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
mkdir -p "$output_dir"

# Both builds get identical isolated compositor/configuration and software drivers.
args=(--graphics-api "$graphics_api" --outputs 3 --rates 60 --width 640 --height 360
      --duration 30 --warmup 5 --min-frame-ratio 0.7 --check)
if [[ $graphics_api == vulkan ]]; then
    # Explicit Vulkan startup must replace an inherited GUI-thread render loop.
    args+=(--render-loop basic)
fi
baseline_ok=true
candidate_status=0
if [[ -n $baseline ]]; then
    if ! "$script_dir/frame-timing.sh" --binary "$baseline" "${args[@]}" --output-dir "$output_dir/baseline-1"; then
        baseline_ok=false
        echo "::warning::Release baseline measurement failed; using absolute candidate checks."
    fi
else
    baseline_ok=false
    echo "::warning::No release baseline available; using absolute candidate checks."
fi
for run in 1 2; do
    if ! "$script_dir/frame-timing.sh" --binary "$candidate" "${args[@]}" --output-dir "$output_dir/candidate-$run"; then
        candidate_status=1
    fi
done
if [[ $baseline_ok == true ]]; then
    if ! "$script_dir/frame-timing.sh" --binary "$baseline" "${args[@]}" --output-dir "$output_dir/baseline-2"; then
        baseline_ok=false
        echo "::warning::Release baseline measurement failed; using absolute candidate checks."
    fi
fi
comparison_args=(--candidate "$output_dir/candidate-1/3output-60fps/summary.json"
                 "$output_dir/candidate-2/3output-60fps/summary.json"
                 --max-regression 0.10 --min-frame-ratio 0.7 --output "$output_dir/comparison.json")
if [[ $baseline_ok == true ]]; then
    comparison_args+=(--baseline "$output_dir/baseline-1/3output-60fps/summary.json"
                     "$output_dir/baseline-2/3output-60fps/summary.json")
fi
if ! python3 "$script_dir/compare-frame-timing.py" "${comparison_args[@]}"; then
    candidate_status=1
fi
exit "$candidate_status"
