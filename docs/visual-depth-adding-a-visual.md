# Add a living visual

Start in the existing slot for your visual. The nine gallery visuals already have Rust descriptors, adapter entry points, settings resources and test directories. The overlay reads availability from Rust. CMake discovers the owned fragments, and the capture runner discovers each visual's scene catalog.

1. Implement `LivingWorld` in `rust/snakes-core/src/visuals/depth/<id>/`. Keep the descriptor's numeric ID, parameter IDs, enum strings and disk keys stable. Set its factory, tick rate and Mirrored policy. Read the seed, visible bounds, background and validated parameters from `WorldInput`. The coordinator supplies a shared union for Spanning, the descriptor's policy for Mirrored and a connector-derived seed per Independent output.
2. Prepare a mature first frame inside the activation budget, including a composed reduced-motion still. Publish immutable endpoints and share static data across samples. Export the visual's sample writer in `rust/snakes-core/include/visuals_depth_<id>.h`. Bind both endpoints and the viewport from one lease. Include glow, tails and parallax when culling primitives at a seam.
3. Register the owned `LivingVisualItem` factory in `src/visuals/<id>/adapter.cpp` using the existing `registerDepth<Name>Adapter()` entry point. Add sources in its local CMake file and shaders under `src/shaders/depth/<id>/`. Upload static data on revision changes and dynamic data per presentation on the window's render thread. Emit `gpuUnavailable()` on a failed probe so the overlay restores the selected released renderer on every output.
4. Bind `qml/settings/visuals/<id>/Settings.qml` to `config.visualParameters`. Save user edits through `settings.writeVisualParameter(key, value)`. Put the four shared control labels in `sharedLabels`. The shared Settings test checks form loading, label selection, close-time save and reload. Put visual-specific label and behavior assertions in `tests/depth/<id>/`. Test determinism, mixed-refresh sampling, event invariants and the first frame in the Rust module or `rust/snakes-core/tests/depth_<id>.rs`.

## Declare capture scenes

Create `tests/depth/scenes/<id>/round.json`. This example defines a single-output Independent capture with a forced event at two seconds. Use an empty `eventIds` array for a visual without forced events. The IDs must match the visual's Rust event handler.

```json
{
  "eventIds": [3, 42],
  "defaults": {"density": 50, "background": "midnight"},
  "scenes": {
    "A": {
      "layout": "single",
      "monitorMode": "independent",
      "seed": "1",
      "captureTimesNs": ["0", "3000000000"],
      "events": [{"id": 42, "timelineNs": "2000000000", "anchor": [1280, 720], "strength": 1}]
    }
  }
}
```

Use `single`, `wide`, `pair` or the three-output `l3` layout. The monitor modes are `independent`, `synchronized` and `seamless`. Each scene can override catalog defaults through `settings`. Shared keys are `speed`, `density`, `scale`, `trails`, `palette`, `background` and `reducedMotion`. Owned settings use their schema keys. Omitted fields use the configuration defaults. Use decimal strings for seeds and nanosecond timestamps.

- A catalog-level `prewarmSeconds` records the duration implemented in the world.
- A `video` object specifies `start` in seconds, `frames` and `fps`. Its optional `name` sets the movie or PNG sequence basename (default `world`). For a seam detail, set `seam` to true and give `crop` as `[x, y, width, height]` in the stitched panorama. An optional `exposureStrip` filename produces eight exposures of the same bright detail.
- A scene with `softwareProbe: true` checks restoration of the released renderer. Optionally supply `fallbackPixels` with a grayscale `threshold` (0–255) and a `minimum` bright-pixel count per output.
- For paired activation timing, supply `legacy` and `activation` scenes on the same layout. Set `legacy: true` in the former. The runner alternates them in one private compositor.

## Verify and enable

Use a Release build with `-DPVS_ENABLE_VISUAL_CAPTURE=ON`. Set `visual` to the slot ID, `build` to the build directory and `scratch` to its evidence directory. Each invocation needs a fresh output path.

```sh
heavy python3 tests/run-depth-capture.py --build "$build" --visual "$visual" --scene A --output "$scratch/smoke"
heavy python3 tests/run-depth-capture.py --build "$build" --visual fixture --output "$scratch/fixture"
heavy python3 tests/run-depth-capture.py --build "$build" --visual "$visual" --scene activation-study --output "$scratch/activation"
heavy python3 tests/run-depth-capture.py --build "$build" --visual "$visual" --scene benchmark --output "$scratch/cpu"
heavy python3 tests/run-depth-capture.py --build "$build" --visual "$visual" --round 1
```

The runner uses private virtual KWin and can exercise the candidate before release. It checks the luminance of each recording and saves the results with the manifests. Measure activation, tick/export time and CPU, verify reduced motion and fallback, then enable `Descriptor::available` in the owned Rust module. Send the captured round and measurements to the art director.
