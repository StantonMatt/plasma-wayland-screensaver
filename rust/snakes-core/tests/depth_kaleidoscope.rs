use snakes_core::visuals::depth::{
    MirroredPolicy,
    ffi::visuals_depth_available,
    kaleidoscope::{DESCRIPTOR, PARAMETERS, Params},
};
#[test]
fn chamber_contract() {
    assert_eq!(DESCRIPTOR.rate, 120);
    assert_eq!(DESCRIPTOR.mirrored, MirroredPolicy::SharedCanonical);
    assert_eq!(DESCRIPTOR.id, 5);
    assert_eq!(visuals_depth_available(DESCRIPTOR.id), 1);
    assert_eq!(
        PARAMETERS
            .iter()
            .map(|p| p.default_value as u32)
            .collect::<Vec<_>>(),
        [0, 8, 0, 40, 1]
    );
    let p = Params::default();
    assert_eq!((p.mirrors, p.glass), ("mandala", "mixed"));
}
#[cfg(feature = "visual-capture")]
#[test]
fn interleaved_output_leases_match_exact_replay() {
    use snakes_core::visuals::depth::{
        common::layout::Rect,
        ffi::*,
        kaleidoscope::render::{Lens, Pose, visuals_depth_kaleidoscope_sample},
        world::Event,
    };
    use std::ptr;
    for (refreshes, frame_cap) in [([100., 175., 240.], 0), ([59.95, 174.96, 239.76], 60)] {
        let outputs = [
            (0., 0., 2560., 1440.),
            (2560., 0., 3440., 1440.),
            (6000., 195., 1920., 1080.),
        ]
        .into_iter()
        .zip(refreshes)
        .map(|((x, y, width, height), hz)| Output {
            id: x as u64,
            viewport: Rect {
                x,
                y,
                width,
                height,
            },
            refresh_hz: hz,
            dpr: 1.,
            primary: 0,
            padding: 0,
        })
        .collect::<Vec<_>>();
        let divisor = |hz: f64| {
            if frame_cap == 0 {
                1.
            } else {
                (hz / f64::from(frame_cap)).ceil()
            }
        };
        let lead_ns = outputs
            .iter()
            .map(|o| 1e9 * divisor(o.refresh_hz) / o.refresh_hz)
            .fold(0., f64::max)
            .ceil() as i64;
        let config = Config {
            visual: 5,
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
            frame_cap,
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
                    lead_ns,
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
            let mut lens = Lens::default();
            let n = visuals_depth_kaleidoscope_sample(l, &mut lens, ptr::null_mut(), 0);
            let mut records = vec![Pose::default(); n];
            assert_eq!(
                visuals_depth_kaleidoscope_sample(l, &mut lens, records.as_mut_ptr(), n),
                n
            );
            visuals_depth_sample_release(l);
            records
        };
        let mut callbacks: Vec<_> = outputs
            .iter()
            .enumerate()
            .flat_map(|(id, o)| {
                let hz = o.refresh_hz;
                let span = divisor(hz);
                let count = (2. * hz / span).floor() as u64;
                (0..=count).map(move |k| {
                    let presentation = (k as f64 * 1e9 * span / hz).round() as i64;
                    // A window requests work one refresh before its presentation.
                    (presentation - (1e9 / hz).round() as i64, presentation, id)
                })
            })
            .collect();
        callbacks.extend((0..3).map(|id| (2_000_000_000, 2_000_000_000, id)));
        callbacks.sort_unstable();
        let live = create();
        let replay = create();

        unsafe {
            for (_, ns, id) in callbacks {
                assert_eq!(visuals_depth_ensure_through(live, ns, 8), 0);
                let frame = sample(live, outputs[id], ns);
                assert!(!frame.is_empty());
                assert_eq!(frame.len(), 37);
            }
            assert_eq!(visuals_depth_debug_seek(replay, 2_000_000_000), 0);
            assert_eq!(
                visuals_depth_debug_hash(live),
                visuals_depth_debug_hash(replay)
            );
            for &o in &outputs {
                assert_eq!(
                    sample(live, o, 2_000_000_000),
                    sample(replay, o, 2_000_000_000)
                );
            }
            visuals_depth_destroy(replay);
            visuals_depth_destroy(live);
        }
    }
}
