#!/usr/bin/env bash
set -euo pipefail

project_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
runner=/usr/lib/qt6/bin/qmltestrunner
if [[ ! -x "${runner}" ]]; then
    echo "Qt 6 qmltestrunner was not found at ${runner}" >&2
    exit 1
fi

results_dir=${SNAKE_BENCHMARK_RESULTS_DIR:-"${project_root}/benchmark-results"}
mkdir -p -- "${results_dir}"
timestamp=$(date -u +%Y%m%dT%H%M%SZ)
log_file="${results_dir}/snakes-longrun-${timestamp}.log"
csv_file="${results_dir}/snakes-longrun-${timestamp}.csv"
profile_csv_file="${results_dir}/snakes-profile-${timestamp}.csv"
planner_csv_file="${results_dir}/snakes-planner-${timestamp}.csv"
feeding_csv_file="${results_dir}/snakes-feeding-${timestamp}.csv"
renderer_csv_file="${results_dir}/snakes-renderer-${timestamp}.csv"
renderer_log_file="${results_dir}/snakes-renderer-${timestamp}.log"
movement_csv_file="${results_dir}/snakes-movement-${timestamp}.csv"
collision_csv_file="${results_dir}/snakes-collision-${timestamp}.csv"
safety_csv_file="${results_dir}/snakes-safety-${timestamp}.csv"

QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
    "${runner}" \
    -input "${project_root}/benchmarks/qml/tst_snakes_longrun.qml" \
    -o -,txt | tee "${log_file}"

awk -F'SNAKE_BENCHMARK_CSV,' \
    '/SNAKE_BENCHMARK_CSV,/ { print $2 }' \
    "${log_file}" > "${csv_file}"
awk -F'SNAKE_PROFILE_CSV,' \
    '/SNAKE_PROFILE_CSV,/ { print $2 }' \
    "${log_file}" > "${profile_csv_file}"
awk -F'SNAKE_PLANNER_CSV,' \
    '/SNAKE_PLANNER_CSV,/ { print $2 }' \
    "${log_file}" > "${planner_csv_file}"
awk -F'SNAKE_FEEDING_CSV,' \
    '/SNAKE_FEEDING_CSV,/ { print $2 }' \
    "${log_file}" > "${feeding_csv_file}"
awk -F'SNAKE_MOVEMENT_CSV,' \
    '/SNAKE_MOVEMENT_CSV,/ { print $2 }' \
    "${log_file}" > "${movement_csv_file}"
awk -F'SNAKE_COLLISION_CSV,' \
    '/SNAKE_COLLISION_CSV,/ { print $2 }' \
    "${log_file}" > "${collision_csv_file}"
awk -F'SNAKE_SAFETY_CSV,' \
    '/SNAKE_SAFETY_CSV,/ { print $2 }' \
    "${log_file}" > "${safety_csv_file}"

renderer_benchmark="${project_root}/build/bin/test-snakerenderer"
if [[ -f "${project_root}/build/CMakeCache.txt" ]]; then
    cmake --build "${project_root}/build" --target test-snakerenderer \
        --parallel "$(nproc)" >/dev/null
fi
if [[ -x "${renderer_benchmark}" ]]; then
    printf 'run,geometry_ms_per_frame,sync_ms_per_step\n' > "${renderer_csv_file}"
    : > "${renderer_log_file}"
    for run in 1 2 3; do
        geometry_output=$(QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
            "${renderer_benchmark}" benchmarkMatureGeometry -iterations 1000 \
            -o -,txt)
        sync_output=$(QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
            "${renderer_benchmark}" benchmarkMatureSyncFrame -iterations 1000 \
            -o -,txt)
        printf '%s\n%s\n' "${geometry_output}" "${sync_output}" \
            | tee -a "${renderer_log_file}"
        geometry_milliseconds=$(awk '/msecs per iteration/ { print $1; exit }' \
            <<< "${geometry_output}")
        sync_milliseconds=$(awk '/msecs per iteration/ { print $1; exit }' \
            <<< "${sync_output}")
        printf '%s,%s,%s\n' "${run}" "${geometry_milliseconds}" \
            "${sync_milliseconds}" >> "${renderer_csv_file}"
    done
else
    printf 'Renderer benchmark unavailable; configure the build directory first.\n' \
        | tee "${renderer_log_file}"
    printf 'run,geometry_ms_per_frame,sync_ms_per_step\n' > "${renderer_csv_file}"
fi

echo
echo "Benchmark log: ${log_file}"
echo "Benchmark CSV: ${csv_file}"
echo "Profile CSV: ${profile_csv_file}"
echo "Planner CSV: ${planner_csv_file}"
echo "Feeding CSV: ${feeding_csv_file}"
echo "Movement CSV: ${movement_csv_file}"
echo "Collision CSV: ${collision_csv_file}"
echo "Safety CSV: ${safety_csv_file}"
echo "Renderer CSV: ${renderer_csv_file}"
