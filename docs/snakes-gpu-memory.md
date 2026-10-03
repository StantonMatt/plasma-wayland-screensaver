# Qt Quick Wayland memory growth

On the measured desktop (RTX 4090, NVIDIA 595.91.07, Plasma Wayland,
Qt 6.10.2), screensaver anonymous memory grew about 2 MiB/min while overlays
rendered. Heaptrack attributed the retained allocations to
`wl_display_read_events`: 216-byte Wayland events accumulated at approximately
the sum of the overlay frame rates. The evidence points to presentation/frame
events queued by the NVIDIA EGL Wayland path without being dispatched. The
precise Qt/driver defect has not been established.

A plain Qt Quick `qml` window reproduced the growth without screensaver code:
OpenGL grew 0.83 MiB/min; Vulkan measured 0.000 MiB/min and started about 30 MiB
lower. Original evidence is in `~/.local/share/pvs-design/leak/`:
`opengl-samples.txt`, `vulkan-samples.txt`, `samples.txt`, and `heap-report.txt`.
These are measurements from the investigation, not a guarantee for every driver.

## Backend selection

Before creating Qt Quick windows, the app prefers Vulkan on Wayland. A
startup-only probe creates a `QVulkanInstance` and a hidden native surface,
checks for a graphics queue capable of presenting to that surface,
`VK_KHR_swapchain`, surface formats and FIFO presentation. It creates no
logical device or swapchain. The temporary surface is destroyed immediately;
the instance is retained until shutdown. Each Qt Quick window receives that
instance and the probed physical device before rendering, including settings,
previews and overlays created on newly connected monitors. If the probe fails,
the app selects OpenGL. The probe establishes presentation capability; it
cannot guarantee that a later device/swapchain allocation will succeed.
The helper filters `SurfaceCreated` before Qt Quick handles it. Qt Wayland
creates the Vulkan surface lazily after this notification; ordinary native
surface creation therefore supplies the instance before swapchain creation.
No window-factory changes or per-frame Vulkan probes are needed.

The startup message is `PVS graphics backend: vulkan` or
`PVS graphics backend: opengl`. Fallback adds
`(Vulkan presentation unavailable)`. This is the selected backend;
`QSG_INFO=1` also reports the backend actually initialized by Qt.

Overrides, in precedence order:

1. `QT_QUICK_BACKEND` is left to Qt, including `software` and custom backends.
2. `QSG_RHI_BACKEND` is honored. `vulkan` is probed with OpenGL fallback;
   other Qt values are left to Qt.
3. `--graphics-api opengl|vulkan` overrides `PVS_GRAPHICS_API=opengl|vulkan`.
4. With no override, Wayland prefers Vulkan; other platforms use OpenGL.

`offscreen` and `minimal` select OpenGL for PVS/Qt Vulkan requests without
loading Vulkan. Other explicit Qt overrides retain their precedence. Software
tests do not load Vulkan. Invalid PVS values exit with status 2 when no Qt
override is set.
Override variables/arguments must be applied to the process that actually owns
the D-Bus service; a second invocation forwards to the already-running instance
and cannot change its graphics API.

For example, when starting a fresh process:

```bash
PVS_GRAPHICS_API=opengl plasma-visual-screensaver --background
# equivalent:
plasma-visual-screensaver --graphics-api opengl --background
```

Qt's Vulkan support is in qt6-base. Building this code also needs the Vulkan
headers (`libvulkan-dev`). Vulkan is dynamically loaded by Qt: no Vulkan link
library is added. The loader (`libvulkan1`) and a functioning Vulkan ICD are
needed at runtime; Mesa systems normally use `mesa-vulkan-drivers`, NVIDIA
systems use the matching NVIDIA Vulkan driver. OpenGL remains usable without a
Vulkan runtime.

## Rendering and timing

The existing SnakeMaterial shader packs contain SPIR-V 1.0 standard and
batchable vertex variants and a SPIR-V fragment variant. No shader, simulation
or steady-state render-loop changes are required. `qsb --dump` on both built
packs verifies those entries.

Qt RHI keeps its default swap interval of 1, which selects FIFO on Vulkan.
`PresentationClock` still consumes Qt Quick `frameSwapped`, records its
monotonic timestamp on the render thread, and schedules GUI ticks per window.
`afterRendering` measures CPU command recording, and `frameSwapped` measures
submission rather than physical scanout on either backend. Refresh-divisor
pacing and presentation prediction continue to operate independently for each
output. Do not set Qt's no-vsync overrides during pacing comparisons.

## Verify on the measured desktop

Check Vulkan window setup first (small test windows, no full-screen overlay):

```bash
env -u QSG_RHI_BACKEND -u QT_QUICK_BACKEND -u PVS_GRAPHICS_API \
  QT_QPA_PLATFORM=wayland PVS_TEST_VULKAN=1 QSG_INFO=1 \
  ./build-vulkan/bin/test-graphicsbackend-window
```

This checks shared-instance attachment to a QQuickView, a visible QML settings
window, a recreated surface, and a later output, and waits for a Vulkan frame.

First ensure no other saver owns `org.kde.PlasmaVisualScreensaver`; a second
invocation otherwise forwards to that instance. Use the normal desktop D-Bus
session so portal/PowerDevil inhibition works. Do not change the installed
configuration: an isolated config below selects Snakes. This displays real
full-screen overlays. Keep input idle while sampling; dismissing an overlay
invalidates the run. After 5 minutes the script stops only its own process.

Run from the worktree in Bash:

```bash
run_dir=$(mktemp -d /tmp/pvs-vulkan-desktop.XXXXXX)
mkdir "$run_dir/config"
cat > "$run_dir/config/plasma-visual-screensaverrc" <<'CONFIG'
[General]
VisualModule=snakes
FrameRate=0
CONFIG
env -u QSG_RHI_BACKEND -u QT_QUICK_BACKEND -u PVS_GRAPHICS_API \
  -u PVS_FRAME_TRACE -u PVS_FRAME_TRACE_DURATION_MS \
  XDG_CONFIG_HOME="$run_dir/config" QSG_INFO=1 \
  ./build-vulkan/bin/plasma-visual-screensaver --preview \
  > "$run_dir/app.log" 2>&1 &
app_pid=$!
trap 'kill "$app_pid" 2>/dev/null || true; wait "$app_pid" 2>/dev/null || true' EXIT
python3 - "$app_pid" "$run_dir/anonymous.csv" <<'PYTHON'
import csv, pathlib, sys, time
pid, destination = sys.argv[1:]
start = time.monotonic()
samples = []
with open(destination, 'w') as out:
    writer = csv.writer(out)
    writer.writerow(['seconds', 'anonymous_kib'])
    while time.monotonic() - start < 300:
        try:
            lines = pathlib.Path(f'/proc/{pid}/smaps_rollup').read_text().splitlines()
        except (FileNotFoundError, ProcessLookupError):
            raise SystemExit('App exited: check for an existing instance or startup error')
        t = time.monotonic() - start
        anonymous = next(int(line.split()[1]) for line in lines if line.startswith('Anonymous:'))
        writer.writerow([t, anonymous])
        out.flush()
        if t >= 60:
            samples.append((t, anonymous))
        time.sleep(5)
xbar = sum(x for x, y in samples) / len(samples)
ybar = sum(y for x, y in samples) / len(samples)
slope = sum((x-xbar)*(y-ybar) for x, y in samples) / sum((x-xbar)**2 for x, y in samples)
print(f'Anonymous slope after 60s warmup: {slope*60/1024:.3f} MiB/min')
PYTHON
kill "$app_pid" 2>/dev/null || true
wait "$app_pid" || true
trap - EXIT
rg 'PVS graphics backend|Vulkan|vulkan|swapchain|shader|inhibit' "$run_dir/app.log"
printf 'Artifacts: %s\n' "$run_dir"
```

Expect `PVS graphics backend: vulkan`, Qt's Vulkan initialization, no shader or
swapchain errors, visible Snakes on every output, and anonymous-memory slope
near zero after warmup. Compare a fresh run adding `--graphics-api opengl` on
the same desktop and settings. Snakes' own steady-state pools can warm up, so
repeat for longer if the first run does not settle.

Measure pacing in a **separate** run: tracing retains rows in memory and would
contaminate the leak measurement. For a timed real-desktop preview without
interfering with an installed D-Bus service, run:

```bash
env -u QSG_RHI_BACKEND -u QT_QUICK_BACKEND -u PVS_GRAPHICS_API \
  XDG_CONFIG_HOME="$run_dir/config" QSG_INFO=1 \
  PVS_FRAME_TRACE="$run_dir/frames.csv" PVS_FRAME_TRACE_DURATION_MS=300000 \
  dbus-run-session -- ./build-vulkan/bin/plasma-visual-screensaver --preview \
  > "$run_dir/pacing.log" 2>&1
```

This timed path bypasses desktop inhibition services; arrange display power
settings appropriately for the measurement. Check `frames.csv` per-window
submission intervals and tick/sync/render times, and compare CPU and deadline
misses with an otherwise identical OpenGL run. Visually check smooth movement
on all outputs. A virtual-output test cannot confirm the NVIDIA desktop leak
is resolved; the untraced desktop memory comparison is still required.
