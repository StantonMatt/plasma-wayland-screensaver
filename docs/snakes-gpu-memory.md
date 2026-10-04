# Qt Quick Wayland memory growth

On the measured desktop (RTX 4090, NVIDIA 595.91.07, Plasma Wayland,
Qt 6.10.2), screensaver anonymous memory grew about 2 MiB/min while three
overlays rendered. Heaptrack attributed the retained allocations to
`wl_display_read_events`: one 216-byte `wl_buffer.release` closure per presented
frame per window accumulated until the EGL surface was destroyed.

The cause is NVIDIA's **egl-wayland2 1.0.1**, selected by
`09_nvidia_wayland2.json`. It creates a private `wl_event_queue` per EGL surface,
but its explicit-sync presentation path never dispatches that queue. The
release events remain queued. This is independent of the Snakes simulation,
geometry, shaders and Qt Quick scene complexity.

A plain Qt Quick OpenGL window reproduced the leak without screensaver code.
Anonymous-memory slopes from the same desktop were:

| Configuration | MiB/min |
| --- | ---: |
| Default egl-wayland2 1.0.1 | 0.829 |
| `__NV_DISABLE_EXPLICIT_SYNC=1` | 0.001 |
| Legacy egl-wayland 1.1.21 | 0.001 |

The legacy run set
`__EGL_EXTERNAL_PLATFORM_CONFIG_FILENAMES=/usr/share/egl/egl_external_platform.d/10_nvidia_wayland.json`;
only `libnvidia-egl-wayland.so.1.1.21` was mapped. Evidence is in
`~/.local/share/pvs-design/leak/`: `control-samples.txt`,
`noexplicit-samples.txt`, `oldegl-samples.txt` and `heap-report.txt`.
These measurements establish the driver workaround in a plain window; repeat
the real-overlay check below to verify the complete screensaver on a desktop.

## Automatic NVIDIA workaround

At the start of `main()`, before `QGuiApplication` or any EGL use, OpenGL on
Wayland gets a process-local workaround. The app checks
`/usr/share/egl/egl_external_platform.d` and `/etc/egl/egl_external_platform.d`,
reads the NVIDIA JSONs and verifies their referenced libraries exist (including
SONAMEs in the native loader search paths/cache).

- If both egl-wayland2 and legacy egl-wayland are usable, it sets
  `__EGL_EXTERNAL_PLATFORM_CONFIG_FILENAMES` to the platform JSON list excluding
  `09_nvidia_wayland2.json`. The list includes `10_nvidia_wayland.json` and all
  other JSONs from both directories, preserving other EGL platforms.
- If only egl-wayland2 is usable, it sets `__NV_DISABLE_EXPLICIT_SYNC=1`.
- Without usable egl-wayland2, it changes nothing.

The legacy path is preferred because it retains explicit sync. The fallback
turns explicit sync off only for this process. There is no per-frame work,
allocation, simulation change or pacing change.

The startup log is one of:

```text
PVS NVIDIA EGL Wayland workaround: __EGL_EXTERNAL_PLATFORM_CONFIG_FILENAMES=<colon-separated JSON paths>
PVS NVIDIA EGL Wayland workaround: __NV_DISABLE_EXPLICIT_SYNC=1
```

The check requires `WAYLAND_DISPLAY` and an unset/empty or `wayland*`
`QT_QPA_PLATFORM` (a Qt `-platform` argument takes precedence). It uses the same
backend-selection precedence described below. Effective Vulkan, software,
custom Qt backends and non-Wayland platforms do not apply the workaround.
Explicit Vulkan requests also skip it if the later Vulkan probe falls back to
OpenGL: that probe happens after Qt platform initialization.

Any user-set `__EGL_EXTERNAL_PLATFORM_CONFIG_FILENAMES`,
`__EGL_EXTERNAL_PLATFORM_CONFIG_DIRS` or `__NV_DISABLE_EXPLICIT_SYNC` is respected,
including an empty value or `__NV_DISABLE_EXPLICIT_SYNC=0`. To disable the
automatic workaround, start a fresh process with either:

```bash
PVS_KEEP_EGL_WAYLAND2=1 plasma-visual-screensaver --background
plasma-visual-screensaver --keep-egl-wayland2 --background
```

This is an upstream [egl-wayland2](https://github.com/NVIDIA/egl-wayland2) defect; the app works around it rather than
patching or replacing the system library. No upstream fix or fixed release is
assumed. Once NVIDIA fixes the queue dispatch, use the opt-out to test the new
library before removing the workaround. Plugin detection is conservative and
is not limited to a driver version string.

Vulkan remains experimental: on this desktop with three layer-shell overlays,
it presented zero frames in two runs (basic loop/FIFO, then threaded loop/swap
interval 0). The latter logged Vulkan, a render thread and swap interval 0 for
all three windows, then stalled at about 40% CPU and did not quit promptly.
Lavapipe CI and a small non-layer-shell NVIDIA window render successfully;
those results do not establish that real desktop overlays work.

## Backend selection

Before creating Qt Quick windows, the app selects OpenGL by default on all
platforms. Vulkan is used only when explicitly requested. For that path, a
startup-only probe creates a `QVulkanInstance` and a hidden native surface,
checks for a graphics queue capable of presenting to that surface,
`VK_KHR_swapchain`, surface formats and MAILBOX or IMMEDIATE presentation. It creates no
logical device or swapchain. The temporary surface is destroyed immediately;
the instance is retained until shutdown. Each Qt Quick window receives that
instance and the probed physical device before rendering, including settings,
previews and overlays created on newly connected monitors. If the probe fails,
the app selects OpenGL. The probe establishes presentation capability; it
cannot guarantee that a later device/swapchain allocation will succeed or
that layer-shell overlays will present frames.
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
4. With no override, all platforms use OpenGL without probing Vulkan.

`offscreen` and `minimal` select OpenGL for PVS/Qt Vulkan requests without
loading Vulkan. Other explicit Qt overrides retain their precedence. Software
tests do not load Vulkan. Invalid PVS values exit with status 2 when no Qt
override is set.
Override variables/arguments must be applied to the process that actually owns
the D-Bus service; a second invocation forwards to the already-running instance
and cannot change its graphics API.

For example, when starting a fresh process:

```bash
PVS_GRAPHICS_API=vulkan plasma-visual-screensaver --background
# equivalent:
plasma-visual-screensaver --graphics-api vulkan --background
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

OpenGL retains its existing basic render loop on Wayland (including explicit
`QSG_RENDER_LOOP` overrides), default swap interval of 1 and unchanged pacing.
Explicit Vulkan requests force the threaded render loop and swap interval 0,
using MAILBOX or IMMEDIATE. This avoids GUI-thread FIFO/Wayland frame-callback
waits but has not resolved the NVIDIA layer-shell stall.
`PresentationClock` still consumes Qt Quick `frameSwapped`, records its
monotonic timestamp on the render thread, and schedules GUI ticks per window.
`afterRendering` measures CPU command recording, and `frameSwapped` measures
submission rather than physical scanout on either backend. Refresh-divisor
pacing and presentation prediction continue to operate independently for each
output. Keep each backend's startup configuration during pacing comparisons.

## Verify OpenGL on the measured desktop

First ensure no other saver owns `org.kde.PlasmaVisualScreensaver`; a second
invocation otherwise forwards to that instance. Use the normal desktop D-Bus
session so portal/PowerDevil inhibition works. Do not change the installed
configuration: an isolated config below selects Snakes. This displays real
full-screen overlays. Keep input idle while sampling; dismissing an overlay
invalidates the run. After 5 minutes the script stops only its own process.

Run from the worktree in Bash:

```bash
mkdir -p "$HOME/.cache/agent-scratch/plasma-wayland-screensaver"
run_dir=$(mktemp -d "$HOME/.cache/agent-scratch/plasma-wayland-screensaver/opengl-desktop.XXXXXX")
mkdir "$run_dir/config"
cat > "$run_dir/config/plasma-visual-screensaverrc" <<'CONFIG'
[General]
VisualModule=snakes
FrameRate=0
CONFIG
env -u QSG_RHI_BACKEND -u QT_QUICK_BACKEND -u PVS_GRAPHICS_API \
  -u PVS_FRAME_TRACE -u PVS_FRAME_TRACE_DURATION_MS \
  -u __EGL_EXTERNAL_PLATFORM_CONFIG_FILENAMES -u __EGL_EXTERNAL_PLATFORM_CONFIG_DIRS \
  -u __NV_DISABLE_EXPLICIT_SYNC -u PVS_KEEP_EGL_WAYLAND2 \
  XDG_CONFIG_HOME="$run_dir/config" QSG_INFO=1 \
  ./build-vkfix/bin/plasma-visual-screensaver --graphics-api opengl --preview \
  > "$run_dir/app.log" 2>&1 &
app_pid=$!
stop_app() {
  kill "$app_pid" 2>/dev/null || true
  for attempt in {1..50}; do
    kill -0 "$app_pid" 2>/dev/null || break
    sleep 0.1
  done
  # Bound shutdown time for this measurement process.
  kill -KILL "$app_pid" 2>/dev/null || true
  wait "$app_pid" 2>/dev/null || true
}
trap stop_app EXIT
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
stop_app
trap - EXIT
rg 'PVS graphics backend|NVIDIA EGL Wayland workaround|shader|inhibit' "$run_dir/app.log"
printf 'Artifacts: %s\n' "$run_dir"
```

Check `PVS graphics backend: opengl`, the NVIDIA workaround log, and whether
Snakes visibly renders on every output. Check `/proc/<pid>/maps` during the
legacy run: it should contain `libnvidia-egl-wayland.so`, not
`libnvidia-egl-wayland2.so`. Compare another fresh process with
`--keep-egl-wayland2` added, on the same desktop and settings, to reproduce the
unmodified-driver baseline. Snakes' own steady-state pools can warm up, so
repeat for longer if a rendering run does not settle. Zero memory growth
without rendering does not demonstrate a fix.

Measure pacing in a **separate** run: tracing retains rows in memory and would
contaminate the leak measurement. For a timed real-desktop preview without
interfering with an installed D-Bus service, run:

```bash
env -u QSG_RHI_BACKEND -u QT_QUICK_BACKEND -u PVS_GRAPHICS_API \
  -u __EGL_EXTERNAL_PLATFORM_CONFIG_FILENAMES -u __EGL_EXTERNAL_PLATFORM_CONFIG_DIRS \
  -u __NV_DISABLE_EXPLICIT_SYNC -u PVS_KEEP_EGL_WAYLAND2 \
  XDG_CONFIG_HOME="$run_dir/config" QSG_INFO=1 \
  PVS_FRAME_TRACE="$run_dir/frames.csv" PVS_FRAME_TRACE_DURATION_MS=300000 \
  dbus-run-session -- ./build-vkfix/bin/plasma-visual-screensaver --graphics-api opengl --preview \
  > "$run_dir/pacing.log" 2>&1
```

This timed path bypasses desktop inhibition services; arrange display power
settings appropriately for the measurement. Check `frames.csv` per-window
submission intervals and tick/sync/render times, and compare CPU and deadline
misses with an otherwise identical `--keep-egl-wayland2` run. Visually check
smooth movement on all outputs. A virtual-output test cannot confirm the NVIDIA desktop leak
is resolved; the untraced desktop memory comparison is still required.

For an optional experimental Vulkan window check (small test windows, no
full-screen overlay):

```bash
env -u QSG_RHI_BACKEND -u QT_QUICK_BACKEND -u PVS_GRAPHICS_API \
  QT_QPA_PLATFORM=wayland PVS_GRAPHICS_API=vulkan PVS_TEST_VULKAN=1 QSG_INFO=1 \
  ./build-vkfix/bin/test-graphicsbackend-window
```

This checks shared-instance attachment to a QQuickView, a visible QML settings
window, a recreated surface and a later output, and waits for a Vulkan frame.
It does not verify NVIDIA layer-shell overlays.
