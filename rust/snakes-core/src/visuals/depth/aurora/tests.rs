// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::visuals::depth::{
    common::layout::{Layout, OutputRect, Rect},
    ffi::View,
    world::{Environment, MonitorMode},
};
use render::{Frame, Quad};
use std::time::Instant;

fn input(stress: bool, span: bool) -> WorldInput {
    let rects = if span {
        vec![
            (0., 0., 2560., 1440.),
            (2560., 0., 3440., 1440.),
            (6000., 195., 1920., 1080.),
        ]
    } else {
        vec![(0., 0., 2560., 1440.)]
    };
    let outputs = rects
        .into_iter()
        .enumerate()
        .map(|(i, (x, y, width, height))| OutputRect {
            id: i as u64,
            rect: Rect {
                x,
                y,
                width,
                height,
            },
            primary: i == 0,
        })
        .collect();
    WorldInput {
        visual_id: 1,
        seed: 73,
        mode: if span {
            MonitorMode::Spanning
        } else {
            MonitorMode::Independent
        },
        layout: Layout::new(outputs, 1).unwrap(),
        speed: if stress { 3. } else { 1. },
        density: if stress { 100. } else { 50. },
        scale: if stress { 2. } else { 1. },
        trails: if stress { 1. } else { 0.35 },
        palette: 0,
        reduced_motion: false,
        parameters: vec![],
        environment: Environment::default(),
    }
}
fn view(v: Rect) -> View {
    View {
        output_id: 1,
        layout_generation: 1,
        timeline_ns: 0,
        viewport: v,
        uniform_scale: 1.,
        offset_x: 0.,
        offset_y: 0.,
        dpr: 1.,
    }
}
fn frame(s: &Snapshot) -> &Frame {
    s.typed.as_ref().unwrap().downcast_ref().unwrap()
}
fn records(a: &Snapshot, b: &Snapshot, v: Rect, t: f32, layer: u32) -> Vec<Quad> {
    let mut out = vec![];
    render::sample(frame(a), frame(b), &view(v), t, layer, |q| out.push(q));
    out
}
fn event(id: u32, sec: f64) -> Event {
    Event {
        id,
        at_ns: (sec * 1e9) as i64,
        anchor: [6010., 490.],
        strength: 1.,
    }
}
#[test]
fn deterministic_storm_seams_and_mixed_clocks() {
    let i = input(false, true);
    let rect = i.layout.bounds;
    let mut a = Aurora::new(i.clone());
    let mut b = Aurora::new(i.clone());
    for w in [&mut a, &mut b] {
        w.inject(event(SUBSTORM, 0.1)).unwrap();
        w.inject(event(FIREBALL, 1.)).unwrap();
    }
    let mut last = a.snapshot();
    for tick in 1..=80 {
        a.step(tick);
        b.step(tick);
        let now = a.snapshot();
        assert_eq!(a.state_hash(), b.state_hash());
        assert_eq!(
            records(&last, &now, rect, 1., 1),
            records(&last, &b.snapshot(), rect, 1., 1)
        );
        // Each output interpolates at its own refresh. At coincident times the
        // records on either side of a seam have identical world coordinates.
        for (o, hz) in i.layout.outputs.iter().zip([100, 175, 240]) {
            let sample = records(&last, &now, o.rect, 20. / hz as f32, 1);
            assert!(!sample.is_empty());
            assert!(sample.len() < records(&last, &now, rect, 20. / hz as f32, 1).len());
        }
        let left = records(&last, &now, i.layout.outputs[1].rect, 0.37, 1);
        let right = records(&last, &now, i.layout.outputs[2].rect, 0.37, 1);
        assert!(
            left.iter()
                .any(|q| q.material[0] == 2. && right.contains(q))
        );
        last = now;
    }
    assert!(a.weather.storm.is_some());
    let base = a.sky.baseline(6000.);
    assert!((a.sky.baseline(5999.) - base).abs() < 2.);
    assert!((a.sky.baseline(6500.) - (195. + 1080. * 0.91)).abs() < 0.001);
}
#[test]
fn hiding_stars_preserves_the_seeded_curtains_through_meteors() {
    let spec = input(true, false);
    let rect = spec.layout.bounds;
    let mut hidden = spec.clone();
    hidden.parameters.push(super::super::schema::Value {
        id: 3,
        kind: PARAMETERS[2].kind,
        payload: 0,
    });
    let mut on = Aurora::new(spec);
    let mut off = Aurora::new(hidden);
    for sky in [&mut on, &mut off] {
        sky.inject(event(FIREBALL, 1.)).unwrap();
        sky.inject(event(SUBSTORM, 16.)).unwrap();
    }
    assert!(!on.sky.stars.is_empty() && off.sky.stars.is_empty());
    // Include the forced fireball and naturally scheduled meteors.
    for tick in 1..=2400 {
        on.step(tick);
        off.step(tick);
        if tick % 120 == 0 {
            let sheets = |sky: &Aurora| {
                let sample = sky.snapshot();
                records(&sample, &sample, rect, 1., 1)
                    .into_iter()
                    .filter(|q| matches!(q.material[0], render::CURTAIN | render::GHOST))
                    .collect::<Vec<_>>()
            };
            assert_eq!(sheets(&on), sheets(&off));
        }
    }
}
#[test]
fn quiet_and_accessibility_switches() {
    let mut i = input(false, false);
    i.parameters = vec![(1, 0), (3, 0), (4, 0)]
        .into_iter()
        .map(|(id, payload)| super::super::schema::Value {
            id,
            kind: PARAMETERS[id as usize - 1].kind,
            payload,
        })
        .collect();
    let mut sky = Aurora::new(i.clone());
    assert_eq!(
        sky.sky.horizon,
        Aurora::new(input(false, false)).sky.horizon
    );
    for id in 1..=4 {
        sky.inject(event(id, 0.5)).unwrap();
    }
    for t in 1..=60 {
        sky.step(t);
    }
    assert!(
        sky.weather.meteor.is_none() && sky.weather.crown.is_none() && sky.weather.storm.is_none()
    );
    assert!(!sky.sky.stars.iter().any(|q| q.material[0] == render::STAR));
    i.reduced_motion = true;
    let mut still = Aurora::new(i.clone());
    let a = still.snapshot();
    still.inject(event(SUBSTORM, 1.)).unwrap();
    still.step(1); // The upper bracket duplicates the first photograph.

    let b = still.snapshot();
    for layer in 0..2 {
        assert_eq!(
            records(&a, &a, i.layout.bounds, 0., layer),
            records(&b, &b, i.layout.bounds, 1., layer)
        );
    }
    assert_eq!(frame(&b).time, 0.);
    let start = still.next_wake_ns(0).unwrap();
    assert!((600_000_000_000..605_000_000_000).contains(&start));
    assert_eq!(still.next_wake_ns(start), Some(start + 500_000_000));
    assert_eq!(still_transition(i.seed, start + 10_000_000_000).0, 0.5);
    assert_eq!(still_transition(i.seed, start + 10_499_999_999).0, 0.5);
    still.step(2);
    let next = still.snapshot();
    assert_ne!(
        records(&b, &b, i.layout.bounds, 1., 1),
        records(&next, &next, i.layout.bounds, 1., 1)
    );
}
#[test]
fn a_crowded_storm_keeps_the_existing_sheets() {
    let mut sky = Aurora::new(input(true, false));
    let n = sky.arcs.len();
    assert!(frame(&sky.snapshot()).lit_share() + 0.13 > 0.65);
    sky.enact(SUBSTORM, Some([650., 500.]), 1.);
    assert!(sky.weather.storm.is_some());
    assert_eq!(sky.arcs.len(), n);
}

#[test]
fn splits_merges_recovery_and_train() {
    let mut sky = Aurora::new(input(true, false));
    // Keep room in the illuminated sky for the expansion this test follows.
    for arc in sky.arcs.iter_mut().skip(2) {
        arc.light = 0.;
    }
    let n = sky.arcs.len();
    sky.enact(SUBSTORM, Some([650., 500.]), 1.);
    assert_eq!(sky.arcs.len(), n + 1);
    assert_eq!(sky.arcs.last().unwrap().light, 0.);
    sky.arcs[1].mean = sky.arcs[0].mean + 10.;
    sky.encounters();
    assert!(sky.arcs[1].retiring());
    sky.enact(FIREBALL, Some([500., 400.]), 1.);
    for t in 1..=50 {
        sky.step(t);
    }
    let b = sky.snapshot();
    let quads = records(&b, &b, sky.sky.bounds, 0., 1);
    assert!(
        quads
            .iter()
            .any(|q| q.material[0] == 5. && q.material[1] >= 1.)
    );
    assert!(!quads.iter().any(|q| q.material[..2] == [5., 0.]));
    for t in 51..=1100 {
        sky.step(t);
    }
    let b = sky.snapshot();
    assert!(
        records(&b, &b, sky.sky.bounds, 0., 1)
            .iter()
            .any(|q| q.material[0] == 4.)
    );
}
#[test]
fn streaks_fade_over_real_time_at_maximum_speed() {
    for event in [METEOR, FIREBALL] {
        let mut sky = Aurora::new(input(true, false));
        sky.enact(event, Some([1280., 500.]), 1.);
        let travel = sky.weather.meteor.unwrap().duration / sky.input.speed as f32;
        let mut peak = 0.0f32;
        for tick in 1..=(travel * 20.).ceil() as u64 {
            sky.step(tick);
            let photo = sky.snapshot();
            let light = records(&photo, &photo, sky.sky.bounds, 0., 1)
                .iter()
                .find(|q| q.material[..2] == [5., 0.])
                .map_or(0., |q| q.field[2]);
            let seconds = tick as f32 / 20.;
            if seconds <= 0.25 || travel - seconds <= 0.25 {
                assert!(light <= 0.501, "streak {event} at {seconds}s: {light}");
            }
            peak = peak.max(light);
        }
        assert!(peak > 0.99, "streak {event} peak: {peak}");
    }
}

// Clip each adaptively subdivided triangle to the real output, including folds
// where the strip crosses itself. Count both in the overlapping area.
fn raster_area(q: Quad, rect: Rect) -> f64 {
    let column = |u: f64| {
        let c = q.column(u as f32).map(f64::from);
        ([c[0], c[1]], [c[2], c[3]])
    };
    let triangle = |vertices: [[f64; 2]; 3]| {
        let mut poly = vertices.to_vec();
        for (axis, sign, edge) in [
            (0, 1., rect.x),
            (0, -1., rect.right()),
            (1, 1., rect.y),
            (1, -1., rect.bottom()),
        ] {
            let Some(&last) = poly.last() else {
                return 0.;
            };
            let mut prev = last;
            let mut clipped = Vec::with_capacity(7);
            for curr in poly {
                let before = sign * (prev[axis] - edge) >= 0.;
                let after = sign * (curr[axis] - edge) >= 0.;
                if before != after {
                    let t = (edge - prev[axis]) / (curr[axis] - prev[axis]);
                    clipped.push([
                        prev[0] + t * (curr[0] - prev[0]),
                        prev[1] + t * (curr[1] - prev[1]),
                    ]);
                }
                if after {
                    clipped.push(curr);
                }
                prev = curr;
            }
            poly = clipped;
        }
        poly.iter()
            .zip(poly.iter().cycle().skip(1))
            .map(|(a, b)| a[0] * b[1] - a[1] * b[0])
            .sum::<f64>()
            .abs()
            * 0.5
    };
    let steps = if matches!(q.material[0], 2. | 3.) {
        super::mesh::subdivisions(&q, 1.)
    } else {
        1
    };
    (0..steps)
        .map(|i| {
            let (a, c) = column(i as f64 / steps as f64);
            let (b, d) = column((i + 1) as f64 / steps as f64);
            triangle([a, b, c]) + triangle([c, b, d])
        })
        .sum()
}
#[test]
fn raster_budget() {
    for stress in [false, true] {
        let mut i = input(stress, false);
        let mut max = 0.0f64;
        for seed in (0..16).chain([
            super::super::common::rng::output_seed(1, 1, "Virtual-0"),
            super::super::common::rng::output_seed(1, 1, "HDMI-A-5"),
        ]) {
            i.seed = seed;
            let mut w = Aurora::new(i.clone());
            for t in 1..=3600 {
                if t == 600 {
                    w.enact(SUBSTORM, Some([1280., 450.]), 1.);
                }
                if t == 1300 {
                    w.enact(CORONA, Some([1220., 150.]), 1.);
                }
                w.step(t);
                if t % 300 != 0 {
                    continue;
                }
                let f = w.snapshot();
                let mut footprint = i.layout.bounds;
                let guard = f64::from(w.sky.optical_margin());
                footprint.x -= guard;
                footprint.width += 2. * guard;
                let full: f64 = (0..2)
                    .flat_map(|layer| records(&f, &f, i.layout.bounds, 1., layer))
                    .map(|q| raster_area(q, footprint))
                    .sum();
                // Includes the final native composite, which includes both opaque ridges, plus
                // every intersecting triangle at its actual clipped area.
                max = max.max(
                    1. + f64::from(w.sky.target_scale()).powi(2) * full / i.layout.bounds.area(),
                );
            }
        }
        println!("maximum full-screen equivalents (stress={stress}) {max:.3}");
        assert!(max <= if stress { 4.5 } else { 2.5 });
    }
}
#[test]
#[ignore = "release timing study; run explicitly outside the normal test lane"]
fn tick_export_benchmark() {
    let i = input(true, true);
    let mut w = Aurora::new(i.clone());
    w.enact(SUBSTORM, Some([7200., 450.]), 1.);
    let mut history = vec![w.snapshot(); 6];
    let mut records = vec![Vec::with_capacity(2048); i.layout.outputs.len()];
    let mut step = Vec::with_capacity(6000);
    let mut export = Vec::with_capacity(6000);
    let mut combined = Vec::with_capacity(6000);
    let mut writes = vec![Vec::with_capacity(6000); i.layout.outputs.len()];
    for t in 1..=6000 {
        let start = Instant::now();
        w.step(t);
        let stepped = start.elapsed();
        history[t as usize % 6] = w.snapshot();
        let published = start.elapsed();
        let a = frame(&history[(t as usize + 5) % 6]);
        let b = frame(&history[t as usize % 6]);
        for (index, output) in i.layout.outputs.iter().enumerate() {
            let before = Instant::now();
            let quads = &mut records[index];
            quads.clear();
            render::sample(a, b, &view(output.rect), 0.42, 1, |q| quads.push(q));
            std::hint::black_box(quads.as_slice());
            writes[index].push(before.elapsed().as_nanos());
        }
        combined.push(start.elapsed().as_nanos());
        step.push(stepped.as_nanos());
        export.push(published.as_nanos());
    }
    let report = |label: &str, data: &mut [u128]| {
        data.sort_unstable();
        println!(
            "{label}: p50={:.2} us p99={:.2} us max={:.2} us",
            data[2999] as f64 / 1000.,
            data[5939] as f64 / 1000.,
            data[5999] as f64 / 1000.
        );
    };
    report("world step", &mut step);
    report("tick+immutable export", &mut export);
    for (idx, times) in writes.iter_mut().enumerate() {
        report(&format!("viewport {} write", idx + 1), times);
    }
    report("tick+export+three viewport writes", &mut combined);
    assert!(records.iter().all(|quads| !quads.is_empty()));
}

#[test]
fn published_material_tracks_resampling_birth_and_merge() {
    let mut i = input(false, true);
    i.seed = 7;
    let mut sky = Aurora::new(i);
    let mut previous = sky.snapshot();
    let mut max = 0.0f32;
    for tick in 1..=1800 {
        if tick == 400 {
            let old_ids: Vec<_> = sky.arcs.iter().map(|a| a.id).collect();
            sky.storm(Some(7400.), 1.);
            let child = sky.arcs.last().unwrap();
            assert!(!old_ids.contains(&child.id) && child.light == 0.);
        }
        if tick == 800 {
            sky.arcs[1].mean = sky.arcs[0].mean + 10.;
            sky.arcs[0].age = 100.;
            sky.arcs[1].age = 100.;
            sky.encounters();
            assert!(sky.arcs[1].retiring() && sky.arcs[1].light > 0.9);
        }
        sky.step(tick);
        let now = sky.snapshot();
        if tick == 400 {
            let daughter = sky.arcs.last().unwrap().id % 8;
            let newborn: Vec<_> = records(&previous, &now, sky.sky.bounds, 0., 1)
                .into_iter()
                .filter(|q| q.material[0] == 2. && (q.material[1] as u32) / 8 == daughter)
                .collect();
            assert!(!newborn.is_empty());
            assert!(newborn.iter().all(|q| q.field[2] == 0. && q.field[3] == 0.));
        }
        let d = frame(&now).max_material_step(frame(&previous));
        max = max.max(d);
        assert!(d < 12., "material moved {d:.1}px in step {tick}");
        previous = now;
    }
    println!("largest presented material step across resampling, split and merge {max:.3}px");
}

#[test]
fn onset_selects_an_unclipped_presented_hem() {
    for (span, seed, seconds, preferred) in [(false, 1, 20, 1830.), (true, 7, 120, 7560.)] {
        let mut i = input(false, span);
        i.seed = if span {
            seed
        } else {
            super::super::common::rng::output_seed(seed, 1, "Virtual-0")
        };
        if span {
            i.parameters.push(super::super::schema::Value {
                id: 1,
                kind: PARAMETERS[0].kind,
                payload: 70,
            });
        }
        let mut sky = Aurora::new(i);
        let mut e = event(SUBSTORM, seconds as f64);
        e.anchor = [preferred, 0.];
        sky.inject(e).unwrap();
        for t in 1..seconds * 20 {
            sky.step(t);
        }
        let snap = sky.snapshot();
        let (arc, x, y) = frame(&snap)
            .flare_site(&sky.input.layout.outputs, preferred as f32)
            .unwrap();
        let rect = sky
            .input
            .layout
            .outputs
            .iter()
            .map(|o| o.rect)
            .find(|r| f64::from(x) >= r.x && f64::from(x) < r.right())
            .unwrap();
        assert!(f64::from(y) - rect.y > 0.28 * rect.height);
        assert!(rect.bottom() - f64::from(y) > 0.13 * rect.height);
        sky.step(seconds * 20);
        let storm = sky.weather.storm.unwrap();
        assert_eq!(storm.arc, arc);
        println!(
            "onset span={span} arc={arc} centre=({x:.1},{y:.1}) speed={:.1}px/s",
            storm.speed
        );
    }
}

#[test]
fn default_hems_have_no_long_cliffs() {
    let mut i = input(false, false);
    i.seed = super::super::common::rng::output_seed(1, 1, "Virtual-0");
    let mut world = Aurora::new(i);
    for t in 1..=18000 {
        world.step(t);
        if ![600, 1600, 4800, 18000].contains(&t) {
            continue;
        }
        let f = world.snapshot();
        let quads = records(&f, &f, world.sky.bounds, 1., 1);
        let mut prior = None;
        let mut run = 0.0f32;
        let mut longest = 0.0f32;
        for q in quads.iter().filter(|q| q.material[0] == 2.) {
            let hem = |u: f32| {
                let p = q.column(u);
                [mix(p[0], p[2], 0.12), mix(p[1], p[3], 0.12)]
            };
            let mut a = hem(0.);
            if Some(a) != prior {
                run = 0.;
            }
            let count = super::mesh::subdivisions(&q, 1.);
            for k in 1..=count {
                let b = hem(k as f32 / count as f32);
                let dx = (b[0] - a[0]).abs();
                let dy = (b[1] - a[1]).abs();
                if (0. ..=2560.).contains(&a[0]) && dy > dx * 70f32.to_radians().tan() {
                    run += dx.hypot(dy);
                    longest = longest.max(run);
                } else {
                    run = 0.;
                }
                a = b;
            }
            prior = Some(a);
        }
        println!(
            "default {}s: longest projected hem run steeper than 70 degrees = {longest:.2}px",
            t / 20
        );
        assert!(longest < 60.);
    }
}

#[test]
fn high_curtains_reach_the_upper_sky() {
    let mut i = input(true, false);
    i.density = 80.;
    i.palette = 2;
    i.seed = super::super::common::rng::output_seed(7, 1, "Virtual-0");
    let mut sky = Aurora::new(i);
    let mut far = 0;
    for t in 1..=12000 {
        sky.step(t);
        far = far.max(
            sky.arcs
                .iter()
                .filter(|a| a.mean > 600. && a.light > 0.1)
                .count(),
        );
    }
    let snap = sky.snapshot();
    let mut top = 1440.0f32;
    for q in records(&snap, &snap, sky.sky.bounds, 1., 1) {
        if q.material[0] != 2. || q.field[2] < 0.15 {
            continue;
        }
        for col in [q.left, q.right] {
            if (0. ..2560.).contains(&col[2]) {
                top = top.min(col[3]);
            }
        }
    }
    println!(
        "G at 600s: highest illuminated sheet tip {top:.1}px, maximum simultaneous distant arcs {far}"
    );
    assert!(top <= 216. && far <= 2);
}

#[test]
fn merge_transfers_flux_then_relaxes_without_screen_time_jumps_at_every_speed() {
    for speed in [0.1, 1., 3.] {
        let mut spec = input(false, false);
        spec.speed = speed;
        let mut world = Aurora::new(spec);
        world.arcs.truncate(2);
        world.normal = 2;
        world.arcs[0].age = 100.;
        world.arcs[1].age = 100.;
        world.arcs[1].mean = world.arcs[0].mean + 5.;
        world.encounters();
        let energy = |world: &Aurora| {
            world
                .arcs
                .iter()
                .take(2)
                .map(|c| c.gain * c.light)
                .sum::<f32>()
        };
        let before = energy(&world);
        let baseline = world.arcs[0].gain_baseline;
        let mut history = std::collections::VecDeque::from([before]);
        let duration_ticks = (8. / speed * 20.).ceil() as u64;
        for tick in 1..duration_ticks {
            world.step(tick);
            let now = energy(&world);
            // The transferred excess now decays, including during the slow
            // 80-second handoff at speed 10. Preserve its continuous arrival
            // and the survivor's baseline rather than permanent total flux.
            assert!(now >= baseline * 0.95 && now <= before * 1.05);
            if history.len() == 20 {
                let prior = history.pop_front().unwrap();
                assert!(
                    (now / prior - 1.).abs() <= 0.06,
                    "speed={speed} tick={tick}: one-second flux {prior} -> {now}"
                );
            }
            history.push_back(now);
            if tick == 49 {
                assert!(
                    world
                        .arcs
                        .iter()
                        .any(|c| c.merge_into.is_some() && c.light > 0.)
                );
            }
        }
    }
}

#[test]
fn corona_begins_on_the_existing_material_field() {
    let mut world = Aurora::new(input(false, true));
    let initial = world.snapshot();
    world.enact(CORONA, Some([600., 150.]), 1.);
    let start = world.snapshot();
    assert_eq!(
        records(&initial, &initial, world.sky.bounds, 0., 1),
        records(&start, &start, world.sky.bounds, 0., 1)
    );
}

#[test]
fn spectrum_hue_is_continuous_across_shader_clock_wraps() {
    // Presentation steps at both reported speed settings, before and after
    // several coarse-clock wraps, plus the hue's own periodic boundary.
    let mut config = input(false, false);
    config.palette = 1;
    let mut world = Aurora::new(config);
    let mut exported_phase = |time: f64| {
        // Exercise interpolation and the exported block consumed by the shader.
        world.weather.time = time - 0.025;
        let lower = Arc::new(world.snapshot());
        world.weather.time = time + 0.025;
        let upper = Arc::new(world.snapshot());
        let lease = super::super::ffi::Lease {
            sample: super::super::common::clock::Sample {
                lower,
                upper,
                alpha: 0.5,
                missed: false,
            },
            view: view(world.sky.bounds),
            bounds: world.sky.bounds,
        };
        let mut out = render::Atmosphere::default();
        assert!(unsafe { render::visuals_aurora_atmosphere(&lease, -1, &mut out) } > 0.);
        f64::from(out.phases[0])
    };
    for speed in [1., 3.] {
        let dt = speed / 60.;
        for centre in [32768., 65536., 98304., std::f64::consts::TAU / 0.026] {
            for offset in [0., -2., -1., 1.] {
                let before = centre + offset * dt - dt * 0.5;
                let after = before + dt;
                let (before_phase, after_phase) = (exported_phase(before), exported_phase(after));
                let delta = (after_phase - before_phase + std::f64::consts::PI)
                    .rem_euclid(std::f64::consts::TAU)
                    - std::f64::consts::PI;
                assert!(
                    (delta - dt * 0.026).abs() < 1e-6,
                    "speed={speed} time={before}: hue step={delta}"
                );
                for spatial_phase in [0., 1., 3., 5.] {
                    let change = ((after_phase + spatial_phase).sin()
                        - (before_phase + spatial_phase).sin())
                    .abs();
                    assert!(change < dt * 0.026 + 1e-6);
                }
            }
        }
    }
}

#[test]
fn ordinary_rays_stay_upright_and_coronas_keep_their_fan() {
    for span in [false, true] {
        let mut world = Aurora::new(input(true, span));
        let mut max = 0.0f32;
        for tick in 1..=1200 {
            world.step(tick);
            if tick % 100 != 0 {
                continue;
            }
            let snap = world.snapshot();
            for q in records(&snap, &snap, world.sky.bounds, 0., 1) {
                if q.material[0] != render::CURTAIN {
                    continue;
                }
                for p in [q.left, q.right] {
                    let rise = p[1] - p[3];
                    if rise > 20. {
                        max = max.max(((p[2] - p[0]) / rise).abs());
                    }
                }
            }
        }
        println!(
            "ordinary rays span={span}: max {:.3} degrees",
            max.atan().to_degrees()
        );
        assert!(
            max <= 15.0f32.to_radians().tan(),
            "span={span}: {} degrees",
            max.atan().to_degrees()
        );
        world.enact(CORONA, Some([world.sky.zenith as f64, 110.]), 1.);
        for tick in 1201..=1320 {
            world.step(tick);
        }
        let snap = world.snapshot();
        let fan = records(&snap, &snap, world.sky.bounds, 0., 1)
            .iter()
            .filter(|q| q.material[0] == render::CURTAIN)
            .flat_map(|q| [q.left, q.right])
            .filter(|p| p[1] - p[3] > 20.)
            .map(|p| ((p[2] - p[0]) / (p[1] - p[3])).abs())
            .fold(0.0f32, f32::max);
        println!(
            "corona rays span={span}: max {:.3} degrees",
            fan.atan().to_degrees()
        );
        assert!(fan > 0.35, "span={span}: corona fan {fan}");
    }
}

#[test]
fn expansion_sheets_brighten_over_a_long_exhale() {
    let mut world = Aurora::new(input(false, true));
    world.arcs.truncate(1); // Leave room for all three staggered births.
    world.enact(SUBSTORM, Some([650., 500.]), 1.);
    assert_eq!(
        world.arcs.iter().filter(|a| a.expansion.is_some()).count(),
        3
    );
    let mut previous_delay = None;
    for curtain in world.arcs.iter_mut().filter(|a| a.expansion.is_some()) {
        let (delay, _, _) = curtain.expansion.unwrap();
        if let Some(previous) = previous_delay {
            assert!((15. ..=30.).contains(&(delay - previous)));
        }
        previous_delay = Some(delay);
        // Hold the recovery edge away from the growth edge to isolate pacing.
        curtain.expansion = Some((delay, 1000., 100.));
        curtain.age = delay + 10. - 0.05;
        curtain.step(0.05, 0., 0.7, 1., &mut world.rng);
        assert!(curtain.light < 0.15, "ten-second fill {}", curtain.light);
        curtain.age = delay + 30. - 0.05;
        curtain.step(0.05, 0., 0.7, 1., &mut world.rng);
        assert!((0.65..0.80).contains(&curtain.light));
        curtain.age = delay + 45. - 0.05;
        curtain.step(0.05, 0., 0.7, 1., &mut world.rng);
        assert!(curtain.light > 0.999);
    }
}

#[test]
fn recovery_patches_yield_to_curtains_and_to_coronas() {
    let mut world = Aurora::new(input(false, false));
    world.enact(SUBSTORM, Some([650., 500.]), 1.);
    world.weather.time += 180.;
    world.weather.recovery = 1.;
    world.weather.activity = 0.4;
    let flux = |world: &mut Aurora| {
        let snap = world.snapshot();
        records(&snap, &snap, world.sky.bounds, 0., 1)
            .iter()
            .filter(|q| q.material[0] == 4.)
            .map(|q| q.field[2])
            .sum::<f32>()
    };
    let ordinary = flux(&mut world);
    assert!(ordinary > 0.);
    world.weather.activity = 1.;
    let maximum = flux(&mut world);
    assert!(maximum < ordinary * 0.60);
    world.enact(CORONA, Some([1200., 110.]), 1.);
    world.weather.crown.as_mut().unwrap().born -= 15.;
    let corona = flux(&mut world);
    println!("patch flux: activity 40={ordinary:.6}, 100={maximum:.6}, corona={corona:.6}");
    assert!(
        corona < maximum * 0.10,
        "corona {corona}, maximum {maximum}"
    );
}

#[test]
fn corona_exports_an_eased_diffuse_material_envelope() {
    let mut world = Aurora::new(input(false, false));
    world.enact(CORONA, Some([1830., 110.]), 1.);
    let born = world.weather.crown.unwrap().born;
    let life = world.weather.crown.unwrap().life;
    let mut envelope = |age: f64| {
        world.weather.time = born + age;
        let snap = Arc::new(world.snapshot());
        let lease = super::super::ffi::Lease {
            sample: super::super::common::clock::Sample {
                lower: snap.clone(),
                upper: snap,
                alpha: 0.5,
                missed: false,
            },
            view: view(world.sky.bounds),
            bounds: world.sky.bounds,
        };
        let mut out = render::Atmosphere::default();
        assert!(unsafe { render::visuals_aurora_atmosphere(&lease, -1, &mut out) } > 0.);
        out.phases[2]
    };
    assert_eq!(envelope(0.), 0.);
    assert_eq!(envelope(5.5), 0.5);
    assert_eq!(envelope(11.), 1.);
    assert!((envelope(f64::from(life) - 7.5) - 0.5).abs() < 1e-6);
    assert_eq!(envelope(f64::from(life)), 0.);
}

#[test]
fn dense_tall_pastel_hems_keep_curvature_over_four_hundred_pixels() {
    // The reviewed E-max/600 seed, through the production projected cubics.
    let mut spec = input(true, false);
    spec.seed = super::super::common::rng::output_seed(5, 1, "Virtual-0");
    spec.palette = 5;
    spec.parameters = vec![(1, 100), (2, 2)]
        .into_iter()
        .map(|(id, payload)| super::super::schema::Value {
            id,
            kind: PARAMETERS[id as usize - 1].kind,
            payload,
        })
        .collect();
    let mut world = Aurora::new(spec);
    for tick in 1..=12000 {
        world.step(tick);
    }
    let snap = world.snapshot();
    let mut traces: Vec<Vec<[f64; 2]>> = vec![];
    let mut prior = None;
    for q in records(&snap, &snap, world.sky.bounds, 1., 1)
        .iter()
        .filter(|q| q.material[0] == render::CURTAIN)
    {
        let arc = q.material[1] as i32 / 8;
        let steps = ((q.right[0] - q.left[0]).hypot(q.right[1] - q.left[1]) / 4.)
            .ceil()
            .max(1.) as usize;
        for k in 0..steps {
            let u = k as f32 / steps as f32;
            if mix(q.field[2], q.field[3], u) < 0.006 {
                prior = None;
                continue;
            }
            let p = q.column(u);
            let point = [mix(p[0], p[2], 0.12) as f64, mix(p[1], p[3], 0.12) as f64];
            if prior.is_none_or(|(a, x)| a != arc || point[0] < x - 0.1) {
                traces.push(vec![]);
            }
            traces.last_mut().unwrap().push(point);
            prior = Some((arc, point[0]));
        }
    }
    let mut checked = 0;
    let mut closest = f64::INFINITY;
    for mut trace in traces {
        trace.sort_by(|a, b| a[0].total_cmp(&b[0]));
        let start = trace[0][0].ceil().max(0.) as i32;
        let end = trace.last().unwrap()[0].floor().min(2560.) as i32;
        let mut ys = vec![];
        let mut j = 0;
        for x in start..=end {
            let x = f64::from(x);
            while j + 1 < trace.len() - 1 && trace[j + 1][0] < x {
                j += 1;
            }
            let a = trace[j];
            let b = trace[(j + 1).min(trace.len() - 1)];
            let u = (x - a[0]) / (b[0] - a[0]).max(1e-9);
            ys.push(a[1] + u * (b[1] - a[1]));
        }
        for piece in ys.windows(401) {
            if piece.iter().copied().fold(f64::NEG_INFINITY, f64::max) < 0.
                || piece.iter().copied().fold(f64::INFINITY, f64::min) > 1250.
            {
                continue;
            }
            let width = |slope: f64| {
                let residuals = piece
                    .iter()
                    .enumerate()
                    .map(|(i, y)| y - slope * (i as f64 - 200.));
                residuals.clone().fold(f64::NEG_INFINITY, f64::max)
                    - residuals.fold(f64::INFINITY, f64::min)
            };
            let slope = (piece[400] - piece[0]) / 400.;
            let (mut lo, mut hi) = (slope - 0.06, slope + 0.06);
            for _ in 0..15 {
                let a = (2. * lo + hi) / 3.;
                let b = (lo + 2. * hi) / 3.;
                if width(a) < width(b) {
                    hi = b;
                } else {
                    lo = a;
                }
            }
            closest = closest.min(width(0.5 * (lo + hi)) / 2.);
            checked += 1;
        }
    }
    println!("E-max/600: {checked} spans, closest straight fit ±{closest:.4}px");
    assert!(checked > 1000);
    assert!(
        closest > 3.,
        "400px hem fits within ±{closest:.4}px of a line"
    );
}
