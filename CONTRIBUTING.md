# Contributing

Bug reports and focused pull requests are welcome.

## Development setup

Use Kubuntu 26.04 with a Plasma Wayland session and install the dependencies
listed in the README. Build and test with:

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
