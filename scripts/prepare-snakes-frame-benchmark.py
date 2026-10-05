#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Generate a headless frame benchmark CMake driver outside the source tree.

Example (run build and benchmark jobs through heavy):
  scripts/prepare-snakes-frame-benchmark.py --source . --driver CACHE/driver
  heavy cmake -S CACHE/driver -B CACHE/build -GNinja -DCMAKE_BUILD_TYPE=Release
  heavy cmake --build CACHE/build --target appperf -j2
  heavy taskset -c 28 env QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \\
      CACHE/build/appperf --paced

The copied simulation source differs only by an exportFrame scope timer. Linker
wrappers time step/build_shader; production code gets no profiling overhead.
--paced wall-paces the measured 0-3 and 10-15 minute windows and accelerates the
unmeasured middle while retaining every tick/frame. Omit it for an accelerated
run with identical presentation timestamps. --hash checks every geometry byte
and must be run separately from acceptance timings. No GPU/Wayland draw occurs.
A source exported from v0.13.0 can use the same harness and instrumentation.
"""
import argparse
from pathlib import Path

repo = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--source', type=Path, default=repo)
parser.add_argument('--driver', type=Path, required=True)
args = parser.parse_args()
source, driver = args.source.resolve(), args.driver.resolve()
text = (source / 'src/snakesimulation.cpp').read_text()
marker = 'bool SnakeSimulation::exportFrame()\n{'
if text.count(marker) != 1:
    raise SystemExit('Cannot uniquely instrument SnakeSimulation::exportFrame')
preamble = '''#include <chrono>
#include <cstdint>
extern uint64_t perfExportNs;
namespace { struct PerfExportScope {
    std::chrono::steady_clock::time_point start=std::chrono::steady_clock::now();
    ~PerfExportScope() { perfExportNs+=std::chrono::duration_cast<std::chrono::nanoseconds>(std::chrono::steady_clock::now()-start).count(); }
}; }
'''
driver.mkdir(parents=True, exist_ok=True)
(driver / 'simulation.cpp').write_text(preamble + text.replace(marker, marker + '\n    PerfExportScope measure;'))
# Resolve the harness from this checkout, even when measuring an exported tag.
harness = repo / 'tests/perf_snakesframe.cpp'
(driver / 'CMakeLists.txt').write_text(f'''cmake_minimum_required(VERSION 3.22)
project(appperf LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_AUTOMOC ON)
set(BUILD_TESTING OFF CACHE BOOL "" FORCE)
add_subdirectory("{source}" repo)
find_package(Qt6 REQUIRED COMPONENTS Quick Qml)
find_package(KF6Config REQUIRED)
add_executable(appperf "{harness}" simulation.cpp "{source}/src/snakesimulation.h"
 "{source}/src/snakerenderer.cpp" "{source}/src/snakerenderer.h" "{source}/src/configuration.cpp" "{source}/src/configuration.h")
target_include_directories(appperf PRIVATE "{source}/src")
target_link_libraries(appperf PRIVATE SnakeMaterial SnakesCore::snakes_core Qt6::Quick Qt6::Qml KF6::ConfigCore)
target_link_options(appperf PRIVATE "LINKER:--wrap=snakes_core_step" "LINKER:--wrap=snakes_core_render_build_shader")
''')
