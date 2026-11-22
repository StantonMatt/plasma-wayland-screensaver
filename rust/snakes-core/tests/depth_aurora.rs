use snakes_core::visuals::depth::{
    MirroredPolicy,
    aurora::{DESCRIPTOR, PARAMETERS, Params},
};
#[test]
fn northern_sky_contract() {
    assert_eq!(DESCRIPTOR.id, 1);
    assert_eq!(
        snakes_core::visuals::depth::ffi::visuals_depth_available(DESCRIPTOR.id),
        1
    );
    assert_eq!(DESCRIPTOR.rate, 20);
    assert_eq!(DESCRIPTOR.mirrored, MirroredPolicy::SharedCanonical);
    let p = Params::default();
    assert_eq!(
        (p.activity, p.horizon, p.stars, p.skyEvents),
        (40, "forest", true, true)
    );
    assert_eq!(
        PARAMETERS
            .iter()
            .map(|p| p.default_value as i32)
            .collect::<Vec<_>>(),
        [40, 1, 1, 1]
    );
}

#[test]
fn merge_brightness_stays_bounded_for_four_hours() {
    use snakes_core::visuals::depth::{
        aurora::render::{Quad, visuals_aurora_quads},
        common::layout::Rect,
        ffi::*,
        schema::Value,
    };
    use std::ptr;

    fn median(samples: &[f32]) -> f32 {
        let mut sorted = samples.to_vec();
        sorted.sort_unstable_by(f32::total_cmp);
        (sorted[(sorted.len() - 1) / 2] + sorted[sorted.len() / 2]) * 0.5
    }

    let mut failures = Vec::new();
    for stress in [false, true] {
        let outputs = [
            (0., 0., 2560., 1440.),
            (2560., 0., 3440., 1440.),
            (6000., 195., 1920., 1080.),
        ]
        .map(|(x, y, width, height)| Output {
            id: x as u64,
            viewport: Rect {
                x,
                y,
                width,
                height,
            },
            refresh_hz: 60.,
            dpr: 1.,
            primary: u32::from(x == 0.),
            padding: 0,
        });
        let config = Config {
            visual: 1,
            palette: if stress { 5 } else { 0 },
            seed: 73,
            speed: if stress { 3. } else { 1. },
            density: if stress { 100. } else { 50. },
            scale: if stress { 2. } else { 1. },
            trails: if stress { 1. } else { 0.35 },
            reduced_motion: 0,
            monitor_mode: if stress { 2 } else { 0 },
        };
        let env = Env {
            background: 1,
            frame_cap: 0,
            top: [0.; 4],
            bottom: [0.; 4],
        };
        let activity = Value {
            id: 1,
            kind: 1,
            payload: 100,
        };
        let rect = Rect {
            x: 0.,
            y: 0.,
            width: if stress { 7920. } else { 2560. },
            height: 1440.,
        };
        // Advance every 20 Hz tick; inspect the public export once per second,
        // including both endpoints of four hours of presentation time.
        let mut power = Vec::with_capacity(14_401);
        let mut intensity = Vec::with_capacity(14_401);
        unsafe {
            let mut world = ptr::null_mut();
            assert_eq!(
                visuals_depth_create(
                    &config,
                    if stress { &activity } else { ptr::null() },
                    usize::from(stress),
                    outputs.as_ptr(),
                    if stress { 3 } else { 1 },
                    &env,
                    ptr::null(),
                    0,
                    1,
                    100_000_000,
                    &mut world,
                ),
                0
            );
            for seconds in 0..=14_400 {
                let ns = seconds * 1_000_000_000;
                assert_eq!(visuals_depth_ensure_through(world, ns, 32), 0);
                let view = View {
                    output_id: 0,
                    layout_generation: 1,
                    timeline_ns: ns,
                    viewport: rect,
                    uniform_scale: 1.,
                    offset_x: 0.,
                    offset_y: 0.,
                    dpr: 1.,
                };
                let mut lease = ptr::null_mut();
                assert_eq!(visuals_depth_retain_sample(world, &view, &mut lease), 0);
                let n = visuals_aurora_quads(lease, 1, -1, ptr::null_mut(), 0);
                let mut quads = vec![Quad::default(); n];
                assert_eq!(visuals_aurora_quads(lease, 1, -1, quads.as_mut_ptr(), n), n);
                visuals_depth_sample_release(lease);
                let mut peak = 0.0_f32;
                let mut total = 0.;
                let mut count = 0;
                for quad in quads.iter().filter(|q| q.material[0] == 2.) {
                    for value in &quad.field[2..] {
                        assert!(
                            value.is_finite() && *value >= 0.,
                            "stress={stress}, second={seconds}"
                        );
                        peak = peak.max(*value);
                        total += value;
                        count += 1;
                    }
                }
                assert!(count > 0, "stress={stress}, second={seconds}: no curtains");
                power.push(peak);
                intensity.push(total / count as f32);
            }
            visuals_depth_destroy(world);
        }
        for (name, samples) in [("peak_power", power), ("mean_intensity", intensity)] {
            let baseline = &samples[..1800];
            let reference_median = median(baseline);
            let reference_max = baseline.iter().copied().fold(0., f32::max);
            let maximum = samples.iter().copied().fold(0., f32::max);
            let window_ratios: Vec<_> = samples[..14_400]
                .chunks_exact(600)
                .map(|window| median(window) / reference_median)
                .collect();
            let worst_window = window_ratios.iter().copied().fold(0., f32::max);
            let lowest_window = window_ratios.iter().copied().fold(f32::INFINITY, f32::min);
            println!(
                "stress={stress} {name}: start={:.6} end={:.6} first30_median={reference_median:.6} first30_max={reference_max:.6} max={maximum:.6} max_ratio={:.6} worst10_median_ratio={worst_window:.6} lowest10_median_ratio={lowest_window:.6} windows={window_ratios:?}",
                samples[0],
                samples[14_400],
                maximum / reference_max
            );
            // Bound upward drift while retaining naturally dim weather and
            // curtains moving away from the viewer. Report both extremes.
            if worst_window > 1.5 || maximum > 3. * reference_max {
                failures.push(format!(
                    "stress={stress} {name}: median ratio={worst_window}, maximum ratio={}",
                    maximum / reference_max
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("; "));
}

#[cfg(feature = "visual-capture")]
#[test]
fn interleaved_output_leases_match_exact_replay() {
    use snakes_core::visuals::depth::{
        aurora::mesh::{Vertex, visuals_aurora_strip},
        aurora::render::{Quad, visuals_aurora_quad_capacity, visuals_aurora_quads},
        common::layout::Rect,
        ffi::*,
        world::Event,
    };
    use std::ptr;
    let outputs = [
        (0., 0., 2560., 1440., 100),
        (2560., 0., 3440., 1440., 175),
        (6000., 195., 1920., 1080., 240),
    ]
    .map(|(x, y, width, height, hz)| Output {
        id: x as u64,
        viewport: Rect {
            x,
            y,
            width,
            height,
        },
        refresh_hz: hz as f64,
        dpr: 1.,
        primary: 0,
        padding: 0,
    });
    let config = Config {
        visual: 1,
        palette: 0,
        seed: 13,
        speed: 1.,
        density: 50.,
        scale: 1.,
        trails: 0.35,
        reduced_motion: 0,
        monitor_mode: 2,
    };
    let env = Env {
        background: 1,
        frame_cap: 0,
        top: [0.; 4],
        bottom: [0.; 4],
    };
    let create = || unsafe {
        let mut w = ptr::null_mut();
        assert_eq!(
            visuals_depth_create(
                &config,
                ptr::null(),
                0,
                outputs.as_ptr(),
                3,
                &env,
                ptr::null(),
                0,
                1,
                100_000_000,
                &mut w
            ),
            0
        );
        assert_eq!(
            visuals_depth_debug_inject(
                w,
                &Event {
                    id: 1,
                    at_ns: 500_000_000,
                    anchor: [6200., 700.],
                    strength: 1.
                }
            ),
            0
        );
        w
    };
    let sample = |w, o: Output, ns| unsafe {
        let v = View {
            output_id: o.id,
            layout_generation: 1,
            timeline_ns: ns,
            viewport: o.viewport,
            uniform_scale: 1.,
            offset_x: 0.,
            offset_y: 0.,
            dpr: 1.,
        };
        let mut l = ptr::null_mut();
        assert_eq!(visuals_depth_retain_sample(w, &v, &mut l), 0);
        assert!(!(*l).sample.missed);
        let n = visuals_aurora_quads(l, 1, -1, ptr::null_mut(), 0);
        let capacity = visuals_aurora_quad_capacity(l, 1, -1);
        assert!(capacity >= n);
        let mut records = vec![Quad::default(); capacity];
        assert_eq!(
            visuals_aurora_quads(l, 1, -1, records.as_mut_ptr(), capacity),
            n
        );
        records.truncate(n);
        let vertices = visuals_aurora_strip(l, 1, -1, ptr::null_mut(), 0);
        assert!(vertices > records.len());
        let mut strip = vec![Vertex::default(); vertices];
        assert_eq!(
            visuals_aurora_strip(l, 1, -1, strip.as_mut_ptr(), strip.len()),
            vertices
        );
        visuals_depth_sample_release(l);
        strip
    };
    let mut callbacks: Vec<_> = outputs
        .iter()
        .enumerate()
        .flat_map(|(id, o)| {
            let hz = o.refresh_hz as i64;
            (0..=hz * 2).map(move |k| (k * 1_000_000_000 / hz, id))
        })
        .collect();
    callbacks.sort_unstable();
    let live = create();
    let replay = create();
    let mut middle = vec![];
    unsafe {
        for (ns, id) in callbacks {
            assert_eq!(visuals_depth_ensure_through(live, ns, 8), 0);
            let frame = sample(live, outputs[id], ns);
            assert!(!frame.is_empty());
            if id == 1 {
                if !middle.is_empty() {
                    assert_ne!(middle, frame);
                }
                middle = frame;
            }
        }
        assert_eq!(visuals_depth_debug_seek(replay, 2_000_000_000), 0);
        assert_eq!(
            visuals_depth_debug_hash(live),
            visuals_depth_debug_hash(replay)
        );
        for o in outputs {
            assert_eq!(
                sample(live, o, 2_000_000_000),
                sample(replay, o, 2_000_000_000)
            );
        }
        visuals_depth_destroy(replay);
        visuals_depth_destroy(live);
    }
}
