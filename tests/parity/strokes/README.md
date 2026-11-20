# Frozen Ribbons and Kaleidoscope oracle

`oracle/` is copied unchanged from 87bb0c9 (the paint implementation predates
slice 1). The recorder substitutes only the paint context and handler name at
runtime. It executes the original formulas and records transformed path commands,
colors, draw order, widths and opacity. `record.py --check` reproduces 144 frames
across all palettes, setting extremes, negative seed, long phase, reduced motion
and the strict glow > 0.25 boundary. Rust compares all 11,328 drawing commands.

GPU captures load these unchanged components in the same real OpenGL window as
the production native renderer at a deterministic elapsed time. Their FrameClock
has no presentation clock, so manually set phases remain stable.
