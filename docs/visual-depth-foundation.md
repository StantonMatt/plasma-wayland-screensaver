# Living visual foundation

All nine `Descriptor::available` switches are false. Production uses the released
renderers and defaults. Snakes and its ABI are independent of this foundation.

A builder owns `rust/snakes-core/src/visuals/depth/<id>/`, the matching include
header, `src/visuals/<id>/`, `src/shaders/depth/<id>/`, the local settings form and
`tests/depth/<id>/`. Each owned CMake fragment is already included. Add sources,
shaders and tests there; no central lists or dispatch arms need editing.

The local Rust descriptor owns the stable numeric visual ID, rate, availability,
Mirrored ownership and uniform fit, parameter table and creation function. Return
`Box<dyn LivingWorld>` from that factory. Mutable worlds are `Send + 'static` but
have one exclusive owner; published `Snapshot` data is `Send + Sync`. The GUI
coordinator is the only runtime owner. An unpublished pre-roll world may be
created on a worker and transferred before publishing. There is no simulation
thread and no live settings/layout configuration.

Spanning shares one union-layout world. Mirrored shares the largest canonical
rectangle except for Matrix's local grids. Independent derives stable per-output
seeds from connector identity. Output timestamps use CLOCK_MONOTONIC. The
coordinator keeps the seed when it recreates worlds, starts a recreated world at
the current time and keeps showing the previous frame until it publishes, waits
for all layer-shell views to configure, advances at most
`min(32, ceil(120 Hz × presentation lead) + 4)` ticks per `present` (publishing
the latest clamped sample when still behind), and updates only the requesting item. Samples clamp on history misses;
retained leases survive world destruction. History capacity is lead plus two
endpoints (the Qt bridge supplies the largest configured presentation period; supported maximum 1 second).

In the local C++ `adapter.cpp`, register an item factory and initialize owned
shader resources. Derive from `LivingVisualItem` and override `buildDepthNode`.
The base binds immutable endpoints, view, exact timeline and generations once
per sample and acknowledges them after rendering. The adapter must not access
the world, coordinator or configuration from a render callback. GPU resources
belong to each window's render thread. `vd_endpoint.payload` is an immutable,
visual-owned typed payload; texture pages carry immutable revision identities.

The environment passes background colours and actual activation-time QML clock
bounds. In Mirrored only the reference output's rectangle is mapped into the
canonical arena. Moving clock obstacle sampling is not implemented by this
scaffold and must be added before a visual that consumes moving obstacles is
released. No current visual consumes this bridge.

Parameters have stable IDs and case-sensitive disk keys. Enum strings are disk
identities; declaration order is not a keyed value array's identity. Keep IDs and
strings stable when extending a local table. QML forms use `writeVisualParameter`
so the existing 400 ms save debounce and close flush apply.

`DepthSettingsVersion` lives inside each `[Animations][<id>]` group, independently
of the released `AnimationSettingsVersion`. Loading never writes. The five shared
values live only in the released profile keys, so edits made by a downgraded
binary remain authoritative. An exact old-default shared tuple is untouched;
when available, Kaleidoscope then gets Spectrum and Points 8. If Points has no
stored value, customized Kaleidoscope profiles seed it with
`round(5 + density/9)` (80 → 14). These defaults are applied in memory and persist
only on an explicit save. There is no separate shared profile or activation marker.

Unknown/newer own parameter values are retained on disk. Missing own parameter
defaults are not written while a slot is disabled; explicit edits still persist.
Bounce's four released ball values remain only in General. When available, the
exact released default tuple (5, 35, 92, true) receives count 12 and elasticity 90;
any customized tuple is kept. Its count may persist as 60; the released adapter
still receives at most 20. Appearance Defaults preserves selection and other
profiles. Undo snapshots include normalized parameters, raw stored values and
pending edits, so Defaults followed by Undo also restores newer raw values after
an intervening save. The UI compares normalized values when enabling Defaults.

## Private fixture captures

Configure a developer build with `-DPVS_ENABLE_VISUAL_CAPTURE=ON`. The matching
Cargo `visual-capture` feature contains replay/seed/step/event/hash entry points
and the test-only fixture ID 65535. It is never a catalog entry. Normal builds
omit those entry points and never read capture scripts.

```
heavy python3 tests/run-depth-capture.py --build /absolute/build --round 1
```

The runner scrubs HOME, config, runtime, display and D-Bus variables; launches a
private virtual KWin; verifies exact L3 sizes, offsets and DPR 1 through KScreen;
and runs the actual OverlayManager in all three modes. There is no real-display
fallback. `--seed` is a decimal u64, `--times` contains integer nanoseconds, and
`--event-script` accepts a JSON array of fixture events. The runner writes one
validated request per scene and passes its absolute path only to the developer
capture executable. Each capture waits for all views to acknowledge layout,
data and sample generation, then grabs without advancing time.

Artifacts default to
`~/.cache/agent-scratch/plasma-wayland-screensaver/3d-renders/fixture/round-N/`.
Every round is new: invocation and hashes, requested/observed layouts, compositor
log, scene requests/manifests, per-output PNGs, a 7920×1440 panorama with black
voids, and tick/export/activation measurements. The fixture has moving circles,
entity generations and timed births/deaths, an injected event and a revisioned
texture. This is correctness proof, not a visual quality or GPU-time benchmark.
Mixed-refresh custom modes, fractional DPR and video are deferred. The Rust
clock test covers interleaved 60/100/175/240 Hz timestamps independently.
