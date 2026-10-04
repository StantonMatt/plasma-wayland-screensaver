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
  Slithering Snakes (with adjustable AI that plans paths, avoids collisions,
  and seeks open space; bigger snakes contest food and cut rivals off, while
  smaller snakes avoid losing head-on encounters; short boosts for cut-offs,
  escapes and food races cost tail segments dropped as edible pellets and
  need time to cool down; glowing hexagon power-ups grant one effect at a
  time: Surge gives free, back-to-back boosts and a faster snake for chained
  attacks, Magnet pulls in food from three times as far, and Phase lets snakes
  slip through rivals' bodies until it wears off. Snakes seek them out to hunt,
  scavenge and escape. Crackling arcs, an orbiting magnet ring and a translucent
  hologram show each effect. Turn power-ups off with the Settings checkbox;
  glossy, shaded, glowing bodies with pointed tails,
  chevrons, saddle bands and glowing spine lights on the biggest snakes;
  spade-shaped heads with eyes that follow movement, blinking and tongue
  flicks; a gold crown for the leader, boost bow waves and contrails, sparks
  on kills, bodies that dissolve into shards, and new food sprites; visibly
  magnetic food, collision-safe spawning, exact head-path body following,
  outward self-tail escapes, swept neck/body collision detection that follows
  the tapered body, forward growth, persistent food vacuum locks, optional
  self-collision, deadly or wraparound edges, and
  size-proportional edible death particles). Rendering costs less, with the
  previous look kept automatically on systems without shader support. A soft
  shadow keeps the clock readable over busy scenes. Snake length uses adaptive
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
  swap feedback to align `QWindow::requestUpdate()` with the output's refresh
  cycle, keeping an absolute pacing grid to avoid drift. If swap feedback is
  unavailable, it falls back to absolute `QChronoTimer` deadlines. Motion
  advances from measured elapsed time rather than assuming an ideal interval.
  This avoids Qt Quick's approximately 60 Hz global animation timer fallback
  when multiple windows are visible, while fixed caps still reduce GPU use.
- Digital Rain builds each glyph stream once as cached Qt Quick text and only
  changes column transforms per frame. It avoids repainting thousands of glyphs
  through JavaScript Canvas on every update.
- Slithering Snakes runs simulation and AI in the Rust core, owned by a native
  `SnakeSimulation`. Physics advances at 30 Hz from the presentation clock,
  with interpolated rendering capped at 60 Hz. Synchronized and seamless modes
  share one world that survives individual monitor removal. Each renderer reads
  the same exported ABI frame directly, with reusable frame and geometry buffers,
  continuous joined body ribbons, food halos, magnetic streaks, eyes, crowns,
  and wrap copies. `Snakes.qml` only binds the native scene-graph item; there is
  no JavaScript simulation or Canvas fallback. Reduced motion pauses the world.
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
sudo apt install build-essential cargo rustc cmake ninja-build extra-cmake-modules \
  appstream desktop-file-utils lintian shellcheck \
  qt6-base-dev qt6-base-private-dev qt6-declarative-dev qt6-shadertools-dev qt6-tools-dev \
  libkf6config-dev libkf6idletime-dev liblayershellqtinterface-dev
```

Rust 1.93 or newer is required. CMake prefers `/usr/bin/cargo` and
`/usr/bin/rustc`; Cargo builds use the committed lockfile offline, with no
third-party crates. Debug uses the Rust dev profile; all other build types,
including Debian's `None`, use the optimized release profile.

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

For Slithering Snakes, the bright white arrow shows each snake's immediate
steering direction. The current Rust ABI does not export food routes or planned
trajectories. Input dismisses the preview as usual. Developer tracing is never
enabled for automatic idle activation and adds no rendering work to the normal
screensaver.

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
matching the CMake project version, such as `v0.10.0`, publishes the verified
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

## Presentation frame timing

Build Release, then run:

```bash
./scripts/frame-timing.sh --duration 300 --output-dir ~/.cache/agent-scratch/plasma-wayland-screensaver/frame-baseline
```

The script runs maximum
Snakes density, trails and intelligence at 30 FPS and auto on one and two
3440×1440 virtual KWin outputs. It uses a private D-Bus session with service
autoactivation disabled, fresh config/home/runtime directories, and no inherited
desktop display or bus. Failure stops the run; there is no real-desktop fallback.
Use `--outputs 1` or `--rates 30` to select a subset, and `--binary PATH` for a
binary outside `build-frametiming/bin/`. Each run saves raw CSV, logs and a JSON
summary with per-window interval/tick/sync/render percentiles, interval histograms,
mean intervals, observed submission rates, deadline counts, RSS and process CPU
usage. Initial five seconds are excluded from frame metrics. Reanalyze saved CSV
and logs with `--summarize-only --output-dir PATH` (use `--warmup` to override the
exclusion period).

CI checks three 640×360 outputs for 45 seconds on both forced Vulkan (lavapipe)
and OpenGL (llvmpipe), using the freshly built package. Reproduce it with
`--graphics-api vulkan --render-loop basic --outputs 3 --rates 60 --width 640 --height 360
--duration 45 --check`. A backend fallback, fewer than 80% of expected frames
on any window, or a median interval more than 25% from the target fails the run.
The Vulkan check also verifies each initialized scene graph uses a render thread
and swap interval zero, even when the harness supplies the `basic` override.
The expected cadence uses each output's advertised refresh and the clock's
whole-refresh divisor. KWin's virtual CLI exposes one fixed refresh rate;
this check does not reproduce mixed-refresh hardware outputs.

`PVS_FRAME_TRACE=/path/frames.csv` enables buffered tracing; unset, no timing
hooks or resource timer are installed. `PVS_FRAME_TRACE_DURATION_MS` additionally
makes `--preview` a timed measurement without desktop inhibition services.
Frames record monotonic nanoseconds at Qt `frameSwapped`, synchronous GUI tick
work and physics steps consumed by scene-graph synchronization. Sync includes
`updatePaintNode`; render measures CPU command recording. These are swap/submission
measurements on virtual outputs, not physical scanout or GPU execution times.
Raw deadline counts include sub-millisecond jitter, so summaries also report a
1 ms tolerance. A late interval does not establish a skipped frame; the slot
estimate rounds to the nearest nominal interval. Compare the mean and histogram
with the median: periodic short corrections can preserve average FPS while
most intervals are late. Advertised refresh comes from `QScreen`; swap cadence
alone cannot establish the virtual compositor's actual refresh.

Auto retains the Snakes 60 FPS cap. The clock keeps an absolute refresh grid
anchored at its first swap and starts work one refresh before each selected
presentation slot. Later submissions do not move that grid, since submission
latency is not display phase. Missing swap feedback uses absolute timer pacing;
feedback recovery or an advertised refresh change establishes a fresh grid.
Traces additionally record the wake deadline, tick and update-request timestamps,
predicted presentation, queued swap-callback timestamp and cumulative timer
fallback count. Summaries show wake lateness, request-to-swap time and prediction
error; pacing timestamps describe the last tick consumed by a sync. These fields
distinguish scheduling delay from backend latency without physical presentation
feedback.
The buffer is written at graceful exit and contributes to sampled RSS.

## Long-run Slithering Snakes benchmark

Run the Rust simulation benchmark and native renderer benchmarks:

```bash
./scripts/benchmark-snakes.sh
```

The script configures a Release build under `build-snakes-benchmark`, runs the
Rust `bench_mechanics` (mechanics only) and `ai_scorecard` (production AI)
examples with the distribution toolchain in frozen/offline mode, and measures native frame synchronization and scene-graph geometry for a
mature 14-snake, 400-particle fixture. It saves a log and three-sample renderer
CSV under `benchmark-results/`. Set `SNAKE_BENCHMARK_BUILD_DIR` to reuse another
Release build, or `SNAKE_BENCHMARK_RESULTS_DIR` to select the output directory.
Geometry measurements include interpolation and CPU tessellation; they exclude
GPU execution and compositor time. Frame synchronization reads caller-owned ABI
records without QML conversion or per-viewport simulation copies.

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

- **Graphics problems or growing memory on NVIDIA Wayland:** startup logs show
  `PVS graphics backend: opengl` by default. The app works around the
  egl-wayland2 1.0.1 release-event leak by setting
  `__NV_DISABLE_EXPLICIT_SYNC=1`, even when legacy egl-wayland is installed.
  Desktop A/B runs showed fewer long hitches than legacy, at higher CPU cost.
  Look for `PVS NVIDIA EGL Wayland workaround:` in startup logs. Other EGL
  platforms and user-set EGL/explicit-sync overrides are preserved. Start a
  fresh process with `PVS_KEEP_EGL_WAYLAND2=1` or `--keep-egl-wayland2` to opt out.
  Vulkan is experimental:
  it is known to stall without presenting frames on NVIDIA with layer-shell
  overlays across multiple monitors. To opt in, start a fresh process with
  `--graphics-api vulkan`, `PVS_GRAPHICS_API=vulkan` or `QSG_RHI_BACKEND=vulkan`;
  Qt's `QSG_RHI_BACKEND` and `QT_QUICK_BACKEND` overrides take precedence.
  Vulkan needs `libvulkan1` and a working driver
  (Mesa: `mesa-vulkan-drivers`; NVIDIA: its matching Vulkan ICD). See
  [memory investigation and desktop verification](docs/snakes-gpu-memory.md).
  Vulkan uses the threaded render loop and swap interval zero to avoid
  per-output FIFO/Wayland callback waits blocking the GUI thread; the
  presentation clock still limits updates. A driver without MAILBOX or
  IMMEDIATE presentation support falls back to OpenGL. The Vulkan path
  overrides `QSG_RENDER_LOOP`; OpenGL keeps its existing basic render loop
  on Wayland and unchanged pacing, honoring `QSG_RENDER_LOOP` overrides.
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
