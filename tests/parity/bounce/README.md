# Bounce compatibility oracle

The six files in `oracle/` are unchanged imports from revision `8992f46`:
`qml/visuals/{Bounce.qml,VisualCanvas.qml,FrameClock.qml,VisualUtils.js}` and
`src/animationstate.{h,cpp}`. They are test inputs, not the new implementation.
No old physics or rendering math is rewritten in the recorder.

Run `python3 tests/parity/bounce/record.py --check` to reproduce `fixtures.tsv`
byte for byte. The QML recorder directly calls the frozen `initializeBalls`
and `stepPhysics` methods and exports every position, velocity, size and trail
entry. It also instruments the unchanged paint handler to record every ordered
trail disc and gradient body, including gradient centres/radii/stops. Eight
scenarios, eight time steps (0 through 600), 64 frames, 1128 balls and 5306
ordered primitives, with ocean and ember palettes.
Scenarios cover owner settings, collisions on/off, signed gravity extremes,
elasticity extremes, count/scale/speed/trail limits and seeded initialization.
Rust compares f64 state and all retained trail positions within 1e-6 logical px.
Rust also checks record count/order, geometry, palette/opacity, radius/kind
and the two-circle gradient parameters. The fixture TSV is generated data; the Rust implementation is `visuals/bounce.rs`.

`test-bouncerenderer` compiles the frozen C++ under a substituted class name,
then checks the new typed projection against its old `ballsAt` output across
contiguous and gapped/negative-origin layouts, gravity/elasticity/collision
combinations, 180 simulation ticks and four presentation offsets (including
999 ms skew). The original C++ physics stays in production. This separates
physics preservation from the new Rust record generation and GPU coverage.

The same integration test loads frozen Bounce.qml for old/new OpenGL captures
and checks texture/plain-geometry equivalence, pure software fallback,
configuration changes, resizing and graphics-resource recreation. Invoke it
through the private KWin harness:

```
~/.local/bin/heavy python3 tests/run-visuals-gpu.py --build <build> \
  --test-binary test-bouncerenderer --output <new-output-directory>
```

Independent/synchronized modes retain two-centre radial-gradient bodies and
reverse-ordered 0.42-size trail discs. Seamless retains flat bodies with an
inside white border and forward-ordered, offset, shrinking velocity tails.
Shared-world interpolation and screen-region collision behavior are deliberately
left in AnimationState; native render snapshots avoid QVariant/QML churn.
