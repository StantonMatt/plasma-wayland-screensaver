# Contributing

Bug reports and focused pull requests are welcome.

## Development setup

Use Kubuntu 26.04 with a Plasma Wayland session and install the dependencies
listed in the README, including the distribution's `rustc` and `cargo`
(1.93 or newer). The Rust crate has no third-party dependencies and is built
with `--frozen`; CTest runs its Rust tests and a C++ ABI smoke test. Build and
test with:

```bash
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Debug
cmake --build build
ctest --test-dir build --output-on-failure
/usr/lib/qt6/bin/qmllint qml/Screensaver.qml qml/Settings.qml qml/visuals/*.qml
```

Changes involving overlays, input dismissal, inhibition, frame scheduling, or
multi-monitor behavior should also exercise the README's manual Wayland test
checklist. Do not weaken the security notice: this application is decorative
and does not lock an unlocked session.

Before publishing code, run the repository's required review-fix loop until a
fresh review reports no actionable findings.

## Visual cost benchmarks

Run the installed binary against all 11 visuals, including the None floor,
without displaying anything on the desktop:

```bash
heavy scripts/benchmark-visuals.sh --target virtual --duration 20 --warmup 5
heavy scripts/benchmark-visuals.sh --target virtual \
  --binary /usr/bin/plasma-visual-screensaver \
  --candidate build/bin/plasma-visual-screensaver --visuals snakes aurora \
  --repeats 3 --max-cpu-regression .10 --max-frame-loss .05
```

Defaults are 10 seconds warmup followed by 60 measured seconds, density 55,
scale 200, speed 300, trails 100, ocean palette, 60 FPS and seamless monitors.
The harness copies the user rc into private config directories, preserves its
snake/ball/clock settings, and overrides both General and animation profiles.
Use repeated `--set Key=value` options to change the workload. A/B repeats use
ABBA blocks per visual. Choose gate tolerances using an A/A run on the same
target; the reported CV is observed variation, not a confidence interval.
Depth worlds use seed 7 by default. Repeat `--seed`, e.g. `--seed 7 --seed 23`,
to compare each world in its own ABBA block; JSON reports per-seed and pooled
ratios. Seeds are decimal u64 values, normalized with `| 1`. Both binaries
must support the explicit `PVS_DEPTH_SEED` environment override (acknowledged
in the app log); empty/invalid values keep normal random activation seeding.
Use `--random-seed` for random worlds, including when benchmarking older
binaries. This hook controls depth worlds, not legacy or snake randomness.
Ranked Markdown/JSON, per-output FPS and interval
percentiles, CPU/RSS samples, optional system-wide NVIDIA telemetry, logs and
binary/config hashes go to a new directory under
`~/.cache/agent-scratch/plasma-wayland-screensaver/visual-bench/`.

Virtual KWin uses three side-by-side outputs (2560×1440, 3440×1440, 1920×1080)
and software GL by default. This measures CPU raster cost and Qt submissions;
it does not establish hardware GPU cost or physical presentation cadence.
Use `--virtual-renderer hardware` or compare on the same real desktop for GPU
optimisations. For an unattended real-session run, use `--target real
--wait-idle 5`: a no-window Wayland input-idle guard waits a fresh quiet
interval, pauses the installed daemon's verified systemd autostart unit, and
aborts on input. Cleanup stops previews and restores that unit on completion,
Ctrl-C or error; desktop lock/display-power inhibitors live only for the run.
The guard requires ext-idle-notify v2, a C++ compiler and Qt6Core/Qt6DBus/
wayland-client development files. `--help` documents gates and caveats;
`python3 scripts/test-benchmark-visuals.py` tests cleanup with mocked services.
