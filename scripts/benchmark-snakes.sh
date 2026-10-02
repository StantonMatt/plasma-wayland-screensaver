#!/usr/bin/env bash
set -euo pipefail

project_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
build_dir=${SNAKE_BENCHMARK_BUILD_DIR:-"${project_root}/build-snakes-benchmark"}
results_dir=${SNAKE_BENCHMARK_RESULTS_DIR:-"${project_root}/benchmark-results"}
mkdir -p -- "${results_dir}"
timestamp=$(date -u +%Y%m%dT%H%M%SZ)
log_file="${results_dir}/snakes-native-${timestamp}.log"
renderer_csv="${results_dir}/snakes-renderer-${timestamp}.csv"

cmake -S "${project_root}" -B "${build_dir}" -G Ninja \
    -DCMAKE_BUILD_TYPE=Release -DBUILD_TESTING=ON
cmake --build "${build_dir}" --target test-snakerenderer --parallel "$(nproc)"

RUSTC=/usr/bin/rustc RUSTDOC=/usr/bin/rustdoc CARGO_HOME="${build_dir}/cargo-home" \
CARGO_TARGET_DIR="${build_dir}/cargo-target/Release" CARGO_NET_OFFLINE=true \
    /usr/bin/cargo run --frozen --offline --release \
    --manifest-path "${project_root}/rust/snakes-core/Cargo.toml" \
    --example bench_mechanics | tee "${log_file}"

# The production screensaver steers with AiController; measure it on the
# same long-run workload so AI planning costs stay visible.
RUSTC=/usr/bin/rustc RUSTDOC=/usr/bin/rustdoc CARGO_HOME="${build_dir}/cargo-home" \
CARGO_TARGET_DIR="${build_dir}/cargo-target/Release" CARGO_NET_OFFLINE=true \
    /usr/bin/cargo run --frozen --offline --release \
    --manifest-path "${project_root}/rust/snakes-core/Cargo.toml" \
    --example ai_scorecard | tee -a "${log_file}"

printf 'run,geometry_ms_per_frame,sync_ms_per_step\n' > "${renderer_csv}"
for run in 1 2 3; do
    geometry_output=$(QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
        "${build_dir}/bin/test-snakerenderer" benchmarkMatureGeometry -iterations 1000 -o -,txt)
    sync_output=$(QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
        "${build_dir}/bin/test-snakerenderer" benchmarkMatureSyncFrame -iterations 100000 -o -,txt)
    printf '%s\n%s\n' "${geometry_output}" "${sync_output}" | tee -a "${log_file}"
    geometry_ms=$(awk '/msecs per iteration/ { print $1; exit }' <<< "${geometry_output}")
    sync_ms=$(awk '/msecs per iteration/ { print $1; exit }' <<< "${sync_output}")
    printf '%s,%s,%s\n' "${run}" "${geometry_ms}" "${sync_ms}" >> "${renderer_csv}"
done
printf '\nNative benchmark log: %s\nRenderer CSV: %s\n' "${log_file}" "${renderer_csv}"
