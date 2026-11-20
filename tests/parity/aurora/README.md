# Aurora oracle

`oracle/` freezes the production Canvas implementation at
`b0f8dfe8d0e01dc1b4813948a5eb6a33a3cb2d2e`. It is test input, not the runtime
fallback. `record.py` replaces only the paint entry point and Canvas context in
memory to record the original expressions, gradient stop positions, colours and
inherited shade alpha. Fixtures were recorded using Qt 6.10.2 on x86_64 Linux.

Run `python3 tests/parity/aurora/record.py --check` to verify the 448 fixture rows.
The Rust unit tests consume the TSV directly without adding dependencies.

For real OpenGL RHI pixel parity, vertex-stage RGBA32F upload and scene graph
recreation, run the focused integration fixture:

```
python3 tests/run-visuals-gpu.py --build /absolute/build --output /new/scratch/output
```

This starts a private virtual KWin with a private bus and scrubbed environment.
The fixture requires the GPU material path, saves Canvas/native/difference and
side-by-side PNGs, and writes per-image RGB error metrics. `--scale 1.25` covers
fractional device pixel ratio. QPainter uses a quantized gradient lookup table
and 8-bit intermediate source-over blending; the shader evaluates gradients
analytically. The tests allow mean absolute RGB error below 2/255 and maximum
error below 20/255. The measured errors belong in the implementation report.

The ordinary software CTest run validates the snapshot/ABI, shader packs and
explicit Rust raster fallback; it skips GPU-only slots. A software pass alone is
not evidence for the GPU path. Runtime minimum-version coverage needs Qt 6.8;
the installed 6.10.2 runtime does not establish 6.8 driver support.
