# Constellation and Starfield paint oracle

`oracle/` is frozen verbatim from commit `87bb0c9` (the slice 1 release).
These files remain test inputs; production QML uses native rendering.

`python3 tests/parity/primitives/record.py` records the old QML paint bodies
using a fake Canvas context. Instrumentation replaces only `getContext` and
the paint handler entry point. The drawing math is not translated into the
recorder. FrameClock and VisualUtils are the unchanged old dependencies.
`--check` reproduces the checked-in TSV byte for byte in Qt's JavaScript engine.
No desktop compositor or user bus is used by the recorder.

216 frames / 33,120 ordered records cover both visuals, all six palettes,
initial and reduced-motion phase zero, minimum/maximum/owner/default settings,
negative and maximum signed seeds, negative elapsed time, long-running phase,
and samples immediately before/after lane zero wraps. Each frame row carries the complete config,
phase and count. Each primitive row carries its index, endpoints/centre,
colour, alpha, half width/radius and kind. Rust checks all records against
these fixtures after advancing via the public C ABI.

Kinds: 0 = butt-ended line, 1 = solid disc, 2 = radial disc. The radial core is
last; Constellation links remain in `(i,j)` order followed by all point discs.

`test-visualrenderer` loads the unmodified old main QML components for real
OpenGL pixel comparisons. `tests/run-visuals-gpu.py` runs it under private
virtual KWin. Sparse edge differences are expected between QPainter's area
coverage and analytic fragment coverage; geometry, colour, motion and draw
order are checked independently by this oracle. Capture metrics record the
maximum as well as whole-image MAE/RMSE and changed-channel fraction.
