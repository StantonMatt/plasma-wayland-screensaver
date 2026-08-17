# Plasma Visual Screensaver

A native Qt 6 / KDE Frameworks 6 visual screensaver for KDE Plasma on Wayland.
It starts after a configurable idle interval, covers every connected output,
and disappears on the first keyboard, pointer, touch, or compositor resume
event.

## Install on Kubuntu 26.04

Download the `.deb` and its `.sha256` file from the
[latest GitHub Release](https://github.com/StantonMatt/plasma-wayland-screensaver/releases/latest).
In the download directory, verify and install it with:

```bash
sha256sum --check plasma-visual-screensaver_*.deb.sha256
sudo apt install ./plasma-visual-screensaver_*.deb
```

You can also double-click the `.deb` to install it with Discover. The package
adds the application launcher and KDE autostart integration. It starts in the
background at the next Plasma login; to start it immediately, run:

```bash
plasma-visual-screensaver --background
```

Open **Plasma Visual Screensaver** from the application menu to configure it.
Settings are retained across upgrades in
`~/.config/plasma-visual-screensaverrc`.
The **About and updates** section shows the installed application version. Its
**Check for Updates** button opens KDE Discover's system update page, where PPA
updates can be reviewed and installed. If Discover is unavailable, the button
opens the latest GitHub release instead.

> **Security notice:** this is a decorative overlay on an **unlocked** session.
> It does not authenticate, lock input, protect running applications, or replace
> Plasma's lock screen. Anyone who dismisses it can use the session.

## Architecture

- `IdleMonitor` registers a `KIdleTime` timeout and arms
  `catchNextResumeEvent()` while the visual is active. It does not read or poll
  `/dev/input`.
- `ScreensaverStateMachine` owns the waiting/activating/active transitions and
  is independently unit tested.
- `OverlayManager` creates one `QQuickView` per `QScreen`. Each view is a
  LayerShellQt overlay-layer surface anchored to all four edges of its assigned
  output. Qt screen add/remove and geometry signals handle hot-plug, resizing,
  rotation, and rearrangement. Optional exclusive zone `-1` coverage extends
  surfaces beneath Plasma panels without changing panel configuration.
- `Inhibitor` requests the XDG Desktop Portal `Inhibit` API with both idle and
  suspend flags. If the portal is unavailable it falls back to Plasma's
  `org.freedesktop.PowerManagement.Inhibit` service. Activation is refused if
  neither inhibitor can be acquired, so the application never knowingly shows
  an overlay that can be blanked or suspended underneath it.
- `ApplicationController` ties those components together and exports
  single-instance D-Bus commands for settings, preview, and quit.
- Animation and background are independent. Replaceable animation modules
  provide None, Aurora Drift, Floating Orbs, Bouncing Balls, Hyperspace,
  Digital Rain, Kaleidoscope, Fireflies, Neon Ribbons, Constellations, and
  Slithering Snakes (with adjustable predictive AI, visibly magnetic food,
  single-target pursuit of the closest turn-reachable food anywhere around
  the snake, collision-aware curved approaches,
  collision-safe spawning, exact head-path body following, outward self-tail
  escapes, swept neck/body collision detection, forward growth, persistent
  food vacuum locks, optional self-collision, deadly or wraparound edges, and
  size-proportional edible death particles). Snake length uses adaptive
  individual and arena-wide painted-area budgets rather than a small fixed cap,
  with progressively more food required for extreme late-game growth. Ambient
  food expires after a randomized 34–46 seconds and is replenished elsewhere
  to avoid persistent bright points on OLED panels;
  backgrounds provide Pure Black and several dark gradients. Every module has
  contextual controls for motion speed, population/detail, scale, palette, and
  trails or glow. Frame-rate choices range from 15 through 240 fps, with an
  automatic mode that follows each output's presentation rate independently.
  Every module also honors the static reduced-motion setting.
- `PresentationClock` gives each overlay window its own frame cadence. It uses
  the output refresh rate and sub-millisecond `QChronoTimer` precision to
  schedule `QWindow::requestUpdate()`, advancing motion from measured elapsed
  time rather than assuming an ideal interval.
  This avoids Qt Quick's approximately 60 Hz global animation timer fallback
  when multiple windows are visible, while fixed caps still reduce GPU use.
- Digital Rain builds each glyph stream once as cached Qt Quick text and only
  changes column transforms per frame. It avoids repainting thousands of glyphs
  through JavaScript Canvas on every update.
- Slithering Snakes uses a native Qt Quick scene-graph renderer instead of a
  full-screen JavaScript Canvas. It batches the ecosystem into compact GPU
  triangle geometry, runs physics at 30 Hz with smooth 60 Hz interpolation, and
  shares one simulation across all outputs in synchronized and seamless modes.
  Its scene-graph geometry retains grow-only buffer capacity as the visible
  vertex count changes, uses continuous joined body ribbons, and isolates
  malformed primitives instead of allowing one to invalidate a complete frame.
  Mature death-food populations are bounded while folding the defeated snake's
  edible value into the available particles; dense feasts also use simplified
  particle glow and less frequent cluster rescoring. The JavaScript Canvas
  fallback is unloaded whenever native rendering is available. This avoids
  recomputing identical AI, reallocating the GPU stream on routine growth, or
  redrawing multi-megapixel textures at 175–240 Hz.
- `AnimationState` advances seamless moving objects once per frame and collides
  them against the union of the actual `QScreen` geometries. Different output
  sizes, vertical offsets, and gaps therefore form real boundaries while an
  object can still cross a physically connected monitor seam. Bouncing Balls
  supports 1–20 independently sized bodies, bidirectional gravity, physics
  speed, elasticity, trails, and optional ball-to-ball collisions.
- `KConfig` stores settings in
  `~/.config/plasma-visual-screensaverrc` (or the configured XDG equivalent).

No X11-only XScreenSaver interfaces are used. The application intentionally
does not modify Plasma's lock, DPMS, suspend, or security configuration.

## Dependencies on Kubuntu 26.04 LTS

The Resolute package catalog available during development contained Qt 6.10.2,
KDE Frameworks 6.24, and LayerShellQt 6.6.4. Install the build dependencies:

```bash
sudo apt update
sudo apt install build-essential cmake ninja-build extra-cmake-modules \
  appstream desktop-file-utils lintian shellcheck \
  qt6-base-dev qt6-declarative-dev qt6-tools-dev \
  libkf6config-dev libkf6idletime-dev liblayershellqtinterface-dev
```

At runtime Plasma should provide `xdg-desktop-portal`,
`xdg-desktop-portal-kde`, PowerDevil, and the Qt Quick/Controls modules. A normal
Kubuntu Plasma installation already includes these.

## Build and run

```bash
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build build
ctest --test-dir build --output-on-failure
./build/bin/plasma-visual-screensaver --settings
```

Useful commands:

```bash
./build/bin/plasma-visual-screensaver --preview
./build/bin/plasma-visual-screensaver --background
./build/bin/plasma-visual-screensaver --quit
```

Only one instance runs per session. Subsequent commands are forwarded over the
session D-Bus. Preview saves current UI values and then uses the same overlay and
inhibition path as idle activation.

### Snake path developer preview

Stop any installed/background instance, then launch the development build with
the preview-only diagnostics enabled:

```bash
plasma-visual-screensaver --quit
./build/bin/plasma-visual-screensaver --preview --dev
```

For Slithering Snakes, the thin translucent line points to its single food
target: the closest available particle outside the head's current turning
pocket, including particles behind it. The bold colored curve is the
collision-aware trajectory the planner
currently predicts the snake will travel. The bright white arrow shows its
immediate steering direction. A curved trajectory can therefore collect the
marked particle even when the head is not pointing straight at it. Input
dismisses the preview as usual. Developer tracing is never enabled for automatic
idle activation and adds no rendering work to the normal screensaver.

## Install from source

System-wide installation (including the application launcher and KDE autostart
entry):

```bash
sudo cmake --install build
```

For a user-local install, configure an explicit prefix and ensure its `bin`
directory is on `PATH` when Plasma processes autostart entries:

```bash
cmake -S . -B build-user -G Ninja \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX="$HOME/.local" \
  -DKDE_INSTALL_AUTOSTARTDIR="$HOME/.config/autostart"
cmake --build build-user
cmake --install build-user
```

To uninstall, use the install manifest generated for the same build tree:

```bash
sudo xargs -d '\n' rm -v < build/install_manifest.txt
```

Omit `sudo` when uninstalling a user-local build. The explicit autostart
destination in the user-local configuration is important: XDG reads per-user
autostart entries from `~/.config/autostart`, not `~/.local/etc/xdg/autostart`.

The settings file is deliberately not removed. It can be deleted separately:

```bash
rm ~/.config/plasma-visual-screensaverrc
```

Log out and back in after installing to exercise the autostart entry, or launch
`plasma-visual-screensaver --background` immediately.

## Build the distributable package

The release helper performs a clean build, tests, shell and QML lint,
AppStream and desktop-file validation, Debian packaging, Debian policy lint,
package inspection, and checksum generation:

```bash
./scripts/build-deb.sh
```

Artifacts are written to `dist/`. Install the generated package with:

```bash
sudo apt install ./dist/plasma-visual-screensaver_*.deb
```

GitHub Actions runs this same process on every push and pull request. A tag
matching the CMake project version, such as `v0.5.0`, publishes the verified
`.deb` and checksum to a GitHub Release. See [PUBLISHING.md](PUBLISHING.md) for
the complete maintainer checklist.

## Updating and uninstalling the package

Download a newer release and install it with the same `apt install ./file.deb`
command. Your personal configuration is not part of the package and remains
unchanged.

For automatic updates through Discover, Software Updater, and `apt upgrade`,
use the project's
[Launchpad PPA](https://launchpad.net/~stantonmatt/+archive/ubuntu/plasma-visual-screensaver).
Once its package page reports the Resolute build as **Published**, install with:

```bash
sudo add-apt-repository ppa:stantonmatt/plasma-visual-screensaver
sudo apt update
sudo apt install plasma-visual-screensaver
```

The PPA package upgrades an existing standalone `.deb` installation without
resetting preferences. Future releases arrive through the computer's normal
update tools. See [PPA.md](PPA.md) for migration, removal, and maintainer
release instructions.

```bash
sudo apt remove plasma-visual-screensaver
```

Removal also leaves your settings available for a later reinstall. To remove
those separately:

```bash
rm ~/.config/plasma-visual-screensaverrc
```

## Long-run Slithering Snakes benchmark

Use the deterministic soak benchmark when investigating performance that
changes as snakes grow, die, and create food. It simulates eight minutes at
30 Hz using the largest monitor's 3440x1440 geometry and the maximum density,
trail, intelligence, and self-collision settings:

```bash
./scripts/benchmark-snakes.sh
```

Each simulated minute reports wall-clock cost alongside live segment, food,
trail-storage, death, and estimated renderer-vertex counts. The command saves
the complete Qt Test log, a machine-readable window CSV, a mature-state phase
profile, fixed-fixture planner and feeding CSVs, and a renderer CSV under the ignored
`benchmark-results/` directory. Compare the
per-window `ms_per_step` and `realtime_ratio` columns rather than only the total
runtime; a rising curve reveals long-session degradation that a short
throughput test can hide. The profile divides a final 30 seconds between food
analysis, AI planning, movement, food capture, and collisions/explosions.
The fixed planner fixture repeats identical plans for fourteen 120-segment
snakes among 400 food particles, so its `ms_per_plan` is the less noisy metric
for comparing AI implementation changes that alter the evolving ecosystem.
The feeding fixture measures a stable 400-particle by 14-snake capture search;
body-placement and collision-grid fixtures track length-sensitive movement and
collision work for fourteen 120-segment snakes. A near-field safety fixture
tracks the predictive guard that runs between full AI plans.
When a configured `build/` directory is available, the command also builds and
runs three-sample CPU scene-graph geometry and QML-to-C++ synchronization
benchmarks for a mature 14-snake, 400-particle frame. These renderer metrics do
not include GPU driver or compositor time.

## Manual Wayland test checklist

Automated tests cover configuration validation/persistence and controller state
transitions. The compositor-dependent behavior needs a real Plasma Wayland
session:

1. Confirm `echo "$XDG_SESSION_TYPE"` prints `wayland` and start `--preview`.
   Verify every connected monitor is covered and panels/windows are not visible.
2. Test keyboard, pointer movement/button/wheel, touchscreen, and tablet input
   separately. One event must dismiss all outputs without passing a meaningful
   action to the underlying application.
3. Preview again, then connect and disconnect a monitor. The new output should
   gain a surface and a removed output must disappear without a crash.
4. While active, change resolution, scale, rotation, and monitor arrangement in
   Plasma settings. Every output should remain completely covered.
5. Toggle panel coverage. When enabled, no taskbar or panel pixels should remain
   visible; disabling it should leave panel-reserved areas uncovered.
6. Select independent, synchronized, and seamless monitor behavior. With
   Bouncing Balls and seamless mode, verify balls cross a connected monitor
   boundary, bounce from unequal outer edges, and never disappear into a
   non-existent part of a stepped or gapped layout.
7. Combine every animation with each background and palette. Exercise each
   contextual speed, density/detail, scale, and trail/glow control. For Bouncing
   Balls, test counts 1 and 20, upward/zero/downward gravity, low/high
   elasticity, and collisions on/off. Test moving and centered clock modes,
   slow/normal/fast clock speeds, the clock toggle, all frame rates, and reduced
   motion. Test automatic refresh and several fixed caps, including 15, 60,
   144, and 240 fps. Moving items must freeze under reduced motion.
8. Confirm that Settings shows the same version as
   `plasma-visual-screensaver --version`, then select **Check for Updates** and
   verify that KDE Discover opens its system update page.
9. Let the configured idle interval expire naturally. Confirm activity dismisses
   it and that another complete idle interval activates it again.
10. Suspend and resume both while waiting and while preview is active. Confirm no
   stale overlay or inhibitor remains after resume. (The active inhibitor may
   intentionally defer an automatic suspend until dismissal.)
11. Run `plasma-visual-screensaver --quit` and verify the process exits. Start it
   again and confirm settings persisted.
12. Use `busctl --user list | grep -E 'portal|PowerManagement'` and PowerDevil's
   battery/status UI to verify an inhibition appears only while the overlay is
   active and is released after every dismissal and failed activation.

## Troubleshooting

- **Preview immediately disappears:** a real input/resume event arrived as the
  overlay appeared. Stop touching input devices and retry. KIdleTime deliberately
  treats the first activity as dismissal.
- **“Refusing to activate without a power/display inhibitor”:** ensure
  `xdg-desktop-portal`, `xdg-desktop-portal-kde`, and PowerDevil are running in
  the user session. Inspect `journalctl --user -b` for portal/PowerDevil errors.
- **A monitor is not covered:** confirm the session is Wayland, check
  `kscreen-doctor -o`, then retry after restarting the process. LayerShellQt and
  KWin must both support the layer-shell protocol.
- **No idle activation:** use `qdbus6 org.kde.KIdleTime /KIdleTime` only for
  diagnostics if available, and check process logs. Do not disable Plasma's lock
  or power settings to diagnose this application.
- **Animation looks uneven:** select “Match each monitor” and confirm Plasma is
  actually using the expected modes with `kscreen-doctor -o`. For diagnostics,
  run a preview with `QSG_RENDER_TIMING=1`; `perWindowFrameDelta` reports the
  measured render cadence. Fixed caps intentionally trade smoothness for power.
- **Autostart cannot find a user-local binary:** add `~/.local/bin` to the
  environment imported by the Plasma user session, or use a system-wide install.

## API references used

- [KIdleTime API](https://api.kde.org/kidletime.html)
- [LayerShellQt usage and CMake target](https://api.kde.org/legacy/plasma/layer-shell-qt/html/dir_f3eec1e9e98e02e34c8efeb863b66c5f.html)
- [Qt `QGuiApplication` screen lifecycle](https://doc.qt.io/qt-6/qguiapplication.html)
- [Qt `QScreen` geometry signals](https://doc.qt.io/qt-6/qscreen.html)
- [Qt Quick scene graph and render loops](https://doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph.html)
- [Qt `QSGGeometry` allocation and retained vertex counts](https://doc.qt.io/qt-6/qsggeometry.html)
- [Qt `QWindow::requestUpdate()`](https://doc.qt.io/qt-6/qwindow.html#requestUpdate)
- [XDG Desktop Portal Inhibit API](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Inhibit.html)
