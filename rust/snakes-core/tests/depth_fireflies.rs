use snakes_core::visuals::depth::fireflies::PARAMETERS;
use snakes_core::visuals::depth::{common::layout::Rect, ffi::*};
use std::{ptr, time::Instant};

fn create() -> *mut World {
    let config = Config {
        visual: 9,
        seed: 7,
        palette: 2,
        speed: 3.,
        density: 100.,
        scale: 2.,
        trails: 1.,
        monitor_mode: 2,
        reduced_motion: 0,
    };
    let outputs = [
        (0., 0., 2560., 1440., 100.),
        (2560., 0., 3440., 1440., 175.),
        (6000., 195., 1920., 1080., 240.),
    ]
    .map(|(x, y, width, height, hz)| Output {
        id: x as u64,
        viewport: Rect {
            x,
            y,
            width,
            height,
        },
        dpr: 1.,
        refresh_hz: hz,
        primary: 0,
        padding: 0,
    });
    let env = Env {
        background: 0,
        frame_cap: 60,
        top: [0.; 4],
        bottom: [0.; 4],
    };
    let mut world = ptr::null_mut();
    assert_eq!(
        unsafe {
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
                50_000_000,
                &mut world,
            )
        },
        0
    );
    world
}
#[test]
fn schema() {
    assert_eq!(PARAMETERS.len(), 5);
    assert_eq!(
        PARAMETERS
            .iter()
            .map(|p| p.default_value as i32)
            .collect::<Vec<_>>(),
        [55, 1, 1, 1, 1]
    );
}
#[test]
fn released_entrypoint_creates_a_world() {
    assert_eq!(visuals_depth_available(9), 1);
    let world = create();
    assert!(!world.is_null());
    unsafe { visuals_depth_destroy(world) };
}

#[cfg(feature = "visual-capture")]
#[test]
fn mixed_hz_outputs_share_one_simulation() {
    unsafe {
        let world = create();
        let reference = create();
        let mut times: Vec<_> = [
            (100, 0., 2560., 0., 1440.),
            (175, 2560., 3440., 0., 1440.),
            (240, 6000., 1920., 195., 1080.),
        ]
        .into_iter()
        .flat_map(|(hz, x, width, y, height)| {
            (0..=hz).map(move |i| {
                (
                    i * 1_000_000_000i64 / hz,
                    Rect {
                        x,
                        y,
                        width,
                        height,
                    },
                )
            })
        })
        .collect();
        times.sort_by_key(|(ns, _)| *ns);
        let mut prior: Option<(
            i64,
            std::sync::Arc<snakes_core::visuals::depth::world::Snapshot>,
        )> = None;
        for (ns, viewport) in times {
            assert_eq!(visuals_depth_ensure_through(world, ns, 8), 0);
            let view = View {
                output_id: viewport.x as u64,
                layout_generation: 1,
                timeline_ns: ns,
                viewport,
                uniform_scale: 1.,
                offset_x: 0.,
                offset_y: 0.,
                dpr: 1.,
            };
            let mut lease = ptr::null_mut();
            assert_eq!(visuals_depth_retain_sample(world, &view, &mut lease), 0);
            let sample = &(*lease).sample;
            assert!(!sample.missed);
            assert_eq!(sample.lower.tick, ns as u64 * 30 / 1_000_000_000);
            if let Some((time, lower)) = &prior
                && *time == ns
            {
                assert!(std::sync::Arc::ptr_eq(lower, &sample.lower));
                let fa = lower
                    .typed
                    .as_ref()
                    .unwrap()
                    .downcast_ref::<snakes_core::visuals::depth::fireflies::frame::Frame>()
                    .unwrap();
                let fb = sample
                    .lower
                    .typed
                    .as_ref()
                    .unwrap()
                    .downcast_ref::<snakes_core::visuals::depth::fireflies::frame::Frame>()
                    .unwrap();
                assert!(std::sync::Arc::ptr_eq(&fa.history, &fb.history));
                assert!(std::sync::Arc::ptr_eq(&fa.heads, &fb.heads));
            }
            prior = Some((ns, sample.lower.clone()));
            visuals_depth_sample_release(lease);
        }
        assert_eq!(
            visuals_depth_ensure_through(reference, 1_000_000_000, 60),
            0
        );
        assert_eq!(
            visuals_depth_debug_hash(world),
            visuals_depth_debug_hash(reference)
        );
        let mut stats = Stats::default();
        visuals_depth_stats(world, &mut stats);
        assert_eq!(stats.ticks, 31);
        visuals_depth_destroy(world);
        visuals_depth_destroy(reference);
    }
}
#[test]
#[ignore = "run in Release on an idle core for the stress gate"]
fn stress_gate() {
    unsafe {
        let start = Instant::now();
        let world = create();
        let activation = start.elapsed();
        let begin = Instant::now();
        let mut all_ticks = Vec::with_capacity(18_000);
        for i in 0..18_000u64 {
            let ns = (i * 1_000_000_000).div_ceil(30) as i64;
            let tick = Instant::now();
            assert_eq!(visuals_depth_ensure_through(world, ns, 2), 0);
            all_ticks.push(tick.elapsed().as_nanos());
        }
        let mut s = Stats::default();
        visuals_depth_stats(world, &mut s);
        let elapsed = begin.elapsed();
        all_ticks.sort_unstable();
        let p99 = all_ticks[all_ticks.len() * 99 / 100 - 1];
        println!(
            "world create+complete trail prewarm {:?}, ten minutes at stress, {:?} elapsed, {} ticks, tick+export p99 {:.3} ms max {:.3} ms",
            activation,
            elapsed,
            s.ticks,
            p99 as f64 / 1e6,
            s.max_ns as f64 / 1e6
        );
        visuals_depth_destroy(world);
        assert_eq!(s.ticks, 18_000);
        assert!(p99 <= 500_000 && s.max_ns <= 2_000_000);
    }
}

#[cfg(feature = "visual-capture")]
#[test]
fn capture_ffi_rejects_overflowing_event_anchors() {
    unsafe {
        let world = create();
        let before = visuals_depth_debug_hash(world);
        for anchor in [[f64::MAX, 1000.], [-f64::MAX, 1000.], [1000., f64::MAX]] {
            let event = snakes_core::visuals::depth::world::Event {
                id: 4,
                at_ns: 0,
                anchor,
                strength: 1.,
            };
            assert_eq!(visuals_depth_debug_inject(world, &event), -1);
        }
        assert_eq!(visuals_depth_debug_hash(world), before);
        assert_eq!(visuals_depth_ensure_through(world, 1_000_000_000, 60), 0);
        visuals_depth_destroy(world);
    }
}
