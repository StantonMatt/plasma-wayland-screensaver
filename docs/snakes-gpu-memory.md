# Qt Quick Wayland memory growth

On the measured desktop (RTX 4090, NVIDIA 595.91.07, Plasma Wayland,
Qt 6.10.2), screensaver anonymous memory grew about 2 MiB/min while three
overlays rendered. Heaptrack attributed the retained allocations to
`wl_display_read_events`: one 216-byte `wl_buffer.release` closure per presented
frame per window accumulated until the EGL surface was destroyed.

The cause is NVIDIA's **egl-wayland2 1.0.1**, selected by
`09_nvidia_wayland2.json`. It creates **two** private queues: a surface queue
and a swapchain queue. The explicit-sync presentation path dispatches the
surface queue but never dispatches the swapchain queue in steady state. Its
`wl_buffer` objects inherit the swapchain queue, and have no release listener
when explicit sync is available. libwayland still queues those events until
someone dispatches them. This is independent of the Snakes simulation,
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
These measurements establish the driver workaround in a plain window.

Real-desktop A/B runs on the same RTX 4090 used three overlays for 90 seconds
per configuration. Artifacts are in `~/.local/share/pvs-design/leak/ab/`:

| Configuration | CPU (% of one core) | Frame interval p99 (ms) | Maximum interval (ms) | Leak |
| --- | ---: | ---: | ---: | --- |
| Legacy egl-wayland v1 | 17 | 31–38 | 150–265 | Fixed |
| egl-wayland2, `__NV_DISABLE_EXPLICIT_SYNC=1` | 26 | 21–24 | 35–51 | Fixed |
| egl-wayland2, no workaround | 15 | 22–25 | 50–60 | ~2 MiB/min |

Those earlier runs established `noexplicit` as a safe fallback. A later
comparison on the same RTX 4090 / egl-wayland2 1.0.1 desktop used three outputs
at 100/175/240 Hz, with **12 minutes untraced per mode** for memory/CPU and a
separate **90 seconds traced** for frame intervals. Artifacts are in
`~/.cache/agent-scratch/plasma-wayland-screensaver/leak3-pgcka1wc/`:

| Mode | Anonymous slope (MiB/min) | CPU (% of one core) | Frame interval p99, 100/175/240 Hz (ms) | Maximum (ms) |
| --- | ---: | ---: | --- | ---: |
| `noexplicit` | 0.0020 | 22.8 | 19.8 / 23.3 / 20.3 | 34.6 |
| `drain` | 0.0015 | 9.8 | 17.8 / 21.2 / 18.3 | 23.0 |
| `off` | 2.14 | 9.2 | 17.8 / 21.2 / 18.3 | — |

Drain logged activation for all three surfaces. It kept memory growth near the
flat control, reduced CPU by 13 percentage points of one core versus
`noexplicit`, and matched the leaking baseline's p99 intervals. These results
make drain the default for source-verified versions. A multi-hour soak is
running separately; no completed soak result is claimed here. Only 1.0.1 has
these desktop measurements; support for 1.0.2 and the inspected main source
is based on identical queue behavior.

## Automatic NVIDIA workaround

At the start of `main()`, before `QGuiApplication` or any EGL use, OpenGL on
Wayland gets a process-local workaround. The app checks
`/usr/share/egl/egl_external_platform.d` and `/etc/egl/egl_external_platform.d`,
reads the platform JSONs and resolves their referenced libraries without
loading EGL. SONAME resolution uses absolute `LD_LIBRARY_PATH` entries first,
then the exact native loader-cache entry, then system fallback paths. Symlinks
are canonicalized before comparing versions. Preflight checks the native ELF
header and verifies that all four Wayland exports resolve to this executable
before EGL can cache the named-queue function. Ambiguous library selection,
relative/tokenized loader paths, preloads/auditing and HWCAP selection fall back
to `noexplicit`.

| Startup condition (no user overrides) | Policy |
| --- | --- |
| Resolved egl-wayland2 1.0.1, 1.0.2 or inspected main (1.0.3), certain loader resolution | Drain swapchain queue; retain explicit sync |
| Any other/unknown egl-wayland2 version or uncertain resolution | Set `__NV_DISABLE_EXPLICIT_SYNC=1` before EGL initializes |
| No usable egl-wayland2, or an ineligible backend/platform | No change |

Legacy egl-wayland presence does not affect this choice. The startup log names
the canonical library when drain is selected. Each rendering surface must then
log `PVS EGL drain: active wl_surface=N (egl-wayland2 VERSION)`. Unexpected
queue names/callers and surfaces destroyed without activation are diagnosed.
If a rendered surface never activates, restart with `PVS_EGL_LEAK_FIX=noexplicit`:
explicit sync cannot be disabled after EGL initialization. The fallback logs:

```text
PVS NVIDIA EGL Wayland workaround: __NV_DISABLE_EXPLICIT_SYNC=1
```

The version policy runs once before EGL initializes. Drain adds no per-frame
heap allocation and does not change simulation or render-loop pacing code.

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

Users who prefer legacy can select it explicitly in a fresh process with
`__EGL_EXTERNAL_PLATFORM_CONFIG_FILENAMES` set to their legacy JSON path, as in
the probe above. The app respects that override and never forces legacy by
default.

This is an upstream [egl-wayland2](https://github.com/NVIDIA/egl-wayland2) defect; the app works around it rather than
patching or replacing the system library. No upstream fix or fixed release is
assumed. Once NVIDIA fixes the queue dispatch, use the opt-out to test the new
library before removing the workaround. Plugin detection is conservative and
is not limited to a driver version string.

### Queue drain with explicit sync retained

`PVS_EGL_LEAK_FIX=drain|noexplicit|off` selects the mitigation in a fresh
process. Unset selects drain on the verified versions and `noexplicit` otherwise.
An explicit `drain` request uses the same preflight and falls back to
`noexplicit` on an unsupported version or uncertain resolution. Explicit
`noexplicit` always disables explicit sync on usable egl-wayland2.
`off` disables both mitigations;
unknown/empty values also disable them. The existing keep flag/environment
and all explicitly set driver/platform variables retain their opt-out behavior.
In particular, unset `__NV_DISABLE_EXPLICIT_SYNC` before requesting drain;
even a user-set `0` is an existing driver-policy override.

The drain interposes named-queue creation, pending dispatch, queue destruction
and display disconnect in the executable. At creation, `dladdr` on the caller
and `realpath` restrict registration to the **exact canonical library selected
before EGL initialized**, using the same version allowlist as startup policy.
Other libraries/versions are forwarded without registration. The two upstream
names (`EGLSurface(id)` and `EGLSurface(id/swapchain-pointer)`) pair queues by
display and surface id. No EGL internals are accessed and no Qt window hooks,
timers, socket reads, flushes, fence waits or per-frame allocation are added.

When egl-wayland2 dispatches its surface queue at the start of
`eplWlSwapBuffers`, the interposer first dispatches pending events on the
matching swapchain queue. It forwards to the original Wayland function using
`dlsym(RTLD_NEXT)`. Source guarantees exclusivity of that surface's `current`
data on the EGL thread, including the swapchain. The base library holds a
surface-list read lock throughout the swap hook; destruction takes the write
lock, so teardown cannot free a copied queue pointer during this drain.
Creation roundtrips have
finished at this point. The explicit-sync buffers have no release listeners:
libwayland discards/frees the closures, without changing syncobj state. If the
compositor lacks explicit sync, existing release listeners execute normally
before buffer selection, on the same thread as upstream's implicit dispatch.
Creation-thread tracking would be unsafe because Qt moves surfaces to render
threads; this approach follows EGL's existing dispatch call instead.

Queue destruction removes registration; display disconnect also removes it
because upstream can skip queue destruction once the native display is invalid.
Registry locking protects only registration/lookup: it is released before any
Wayland dispatch/listener. A swap starts with one live swapchain before
`SwapChainRealloc`; ambiguous matches are ignored. Steady state adds a compact
lookup under one mutex and one nonblocking dispatch. Symbol resolution, caller
checks and registry growth occur during initialization/queue creation only.
An activation line is emitted once per swapchain:

```text
PVS EGL drain: active wl_surface=... (egl-wayland2 1.0.1)
```

The source investigation used upstream v1.0.1,
[`c757f0fce6f88f36a9a8c03d897ae43796b6989f`](https://github.com/NVIDIA/egl-wayland2/tree/c757f0fce6f88f36a9a8c03d897ae43796b6989f).
Relevant references:

- [`wayland-swapchain.c:127–170`](https://github.com/NVIDIA/egl-wayland2/blob/c757f0fce6f88f36a9a8c03d897ae43796b6989f/src/wayland/wayland-swapchain.c#L127-L170): dma-buf wrapper queue, params listener/roundtrip; buffers inherit that queue.
- [`wayland-swapchain.c:214–232`](https://github.com/NVIDIA/egl-wayland2/blob/c757f0fce6f88f36a9a8c03d897ae43796b6989f/src/wayland/wayland-swapchain.c#L214-L232): explicit sync skips `wl_buffer_add_listener`.
- [`wayland-swapchain.c:339–347`](https://github.com/NVIDIA/egl-wayland2/blob/c757f0fce6f88f36a9a8c03d897ae43796b6989f/src/wayland/wayland-swapchain.c#L339-L347): swapchain queue name.
- [`wayland-swapchain.c:501–612,637,729–741`](https://github.com/NVIDIA/egl-wayland2/blob/c757f0fce6f88f36a9a8c03d897ae43796b6989f/src/wayland/wayland-swapchain.c#L501-L741): explicit path uses syncobj timelines; only implicit path dispatches its queue.
- [`wayland-surface.c:119–144,891–899,1328–1334`](https://github.com/NVIDIA/egl-wayland2/blob/c757f0fce6f88f36a9a8c03d897ae43796b6989f/src/wayland/wayland-surface.c#L119-L144): ownership contract; distinct surface queue; pending dispatch before swapchain reallocation.
- [`wayland-surface.c:1057–1085`](https://github.com/NVIDIA/egl-wayland2/blob/c757f0fce6f88f36a9a8c03d897ae43796b6989f/src/wayland/wayland-surface.c#L1057-L1085): driver quiesces surface callbacks before teardown.
- [`platform-base.c:515–524,547–586,1054–1077,1082–1122`](https://github.com/NVIDIA/egl-wayland2/blob/c757f0fce6f88f36a9a8c03d897ae43796b6989f/src/base/platform-base.c#L547-L586): swap holds the surface-list read lock and validates the current draw surface; destruction takes the write lock.
- [Wayland 1.24 `wayland-client.c:1551–1654,1671–1714,2183–2194`](https://github.com/wayland-mirror/wayland/blob/1.24.0/src/wayland-client.c#L1551-L1714): queueing even without a listener; dispatch frees the closure under Wayland's normal read-thread synchronization.

v1.0.2 (`5a2c1cbe0e737cfc3fd89441ab9e40d72c81126e`) and current main
(`ce0eb711fbfb48de7b42bb615847587d15d4bab5`, meson version 1.0.3, rechecked
2026-10-04) have the same queue creation, names, surface ownership and dispatch
behavior as 1.0.1. Both still lack the explicit-path queue dispatch. Main's frame-callback
limit fix concerns compositor objects with swap interval zero, not this client
release queue. No environment variable, EGL attribute or swap-interval option
in the inspected source drains this queue while retaining explicit sync.

Do not drain all driver queues from a timer or creation thread: the queues have
listeners and EGL-owned data with different lifetimes. A Qt after-swap hook also
needs a reliable surface/queue association and teardown protection; matching
at upstream's existing dispatch provides both. Surface recreation adds stalls
and allocation, and legacy/Vulkan have already failed desktop pacing checks.

Focused tests use an isolated test-only caller DSO and actual libwayland over a
socketpair, with no compositor/GPU. They check listener-free releases, correct
queue selection, malformed/untrusted creation, render-thread migration,
recreation/disconnect, errors, listeners, and zero C++ allocations in 1000
steady dispatches. They do not establish real NVIDIA binding, memory slope,
frame pacing or desktop CPU cost.

Desktop handoff (build/test scheduling belongs to the orchestrator):

```bash
cmake -S . -B build-leak3 -G Ninja -DCMAKE_BUILD_TYPE=Release -DBUILD_TESTING=ON
heavy cmake --build build-leak3 -j 2
heavy bash "$HOME/.local/share/pvs-design/leak/verify-leak3.sh" \
  "$PWD/build-leak3/bin/plasma-visual-screensaver" 12
```

The script builds nothing and refuses an existing D-Bus owner. It uses the
current visual/configuration and normal desktop bus, runs control-noexplicit,
drain and off, samples Anonymous/process CPU every 10 seconds, excludes 60
seconds of warmup, and prints least-squares slope and CPU as a percentage of
one core. Each mode then runs a separate 90-second timed `PVS_FRAME_TRACE`
preview to report frame-interval p50/p99/max by named output (first 5 seconds
excluded). Keeping tracing separate avoids counting retained trace rows as
the leak. It saves CSVs, logs and maps on disk and stops only its own PIDs.
Keep all overlays visible and do not provide input; arrange desktop power
settings for timed previews, which bypass inhibition services. Reject runs
without actual frames or drain activation. Establish memory near the control's
flat slope and CPU/pacing near off. The measured comparison above passed these
checks and selected drain as the default for the verified versions.

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
  -u __NV_DISABLE_EXPLICIT_SYNC -u PVS_KEEP_EGL_WAYLAND2 -u PVS_EGL_LEAK_FIX \
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
default workaround run: it should contain `libnvidia-egl-wayland2.so`. An
explicit legacy comparison should instead map `libnvidia-egl-wayland.so`.
Compare another fresh process with
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
  -u __NV_DISABLE_EXPLICIT_SYNC -u PVS_KEEP_EGL_WAYLAND2 -u PVS_EGL_LEAK_FIX \
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
