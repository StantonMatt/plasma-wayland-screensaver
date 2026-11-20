# Matrix parity oracle

`oracle/Matrix.qml`, `FrameClock.qml`, and `VisualUtils.js` are unchanged imports
from commit `8992f46f45099906e5d74f50f7cba77cfaf5125c`. They are frozen test inputs,
not a second implementation. `record.py` injects only a Repeater accessor and a
name on the white head to read the original properties. No animation math is
rewritten. The null presentation clock keeps supplied phases deterministic.

`fixtures.tsv` has 120 frames and 7,044 columns: all six palettes, owner settings,
minimum/maximum/default settings, negative/large seeds and time, reduced motion,
and times immediately before/after a column wrap. It records stream glyph IDs,
column positions, lengths, rates, start offsets, content heights and head offsets.
The Rust fixture test checks every field and the ABI-driven phase.

Generate with `python3 tests/parity/matrix/record.py`; verify with `--check`,
which compares numbers within 1e-9 absolute + 1e-12 relative. Font-dependent metrics were recorded with Qt 6.10.2 and DejaVu
Sans Mono (47 px -> ceil font height 55). A different resolved monospace font
can change the Qt layout fields; this is explicit, not normalized away. Qt
fallback glyph faces are independently checked by real OpenGL image captures.
