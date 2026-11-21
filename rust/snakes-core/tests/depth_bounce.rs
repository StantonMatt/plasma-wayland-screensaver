use snakes_core::visuals::depth::bounce::PARAMETERS;
#[cfg(feature = "visual-capture")]
use snakes_core::visuals::depth::{common::layout::Rect, ffi::*, schema::Value};
#[cfg(feature = "visual-capture")]
use std::{ptr, time::Instant};

#[cfg(feature = "visual-capture")]
fn create(count: u64) -> *mut World {
    create_scaled(count, 2.)
}
#[cfg(feature = "visual-capture")]
fn create_scaled(count: u64, scale: f64) -> *mut World {
    let config = Config {
        visual: 7,
        seed: 7,
        palette: 2,
        speed: 3.,
        density: 100.,
        scale,
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
    let population = Value {
        id: 1,
        kind: 1,
        payload: count,
    };
    let mut world = ptr::null_mut();
    assert_eq!(
        unsafe {
            visuals_depth_create(
                &config,
                &population,
                1,
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
    assert_eq!(
        snakes_core::visuals::depth::ffi::visuals_depth_available(7),
        1
    );
    assert_eq!(PARAMETERS.len(), 6);
    assert_eq!(
        PARAMETERS
            .iter()
            .map(|p| p.default_value as i32)
            .collect::<Vec<_>>(),
        [12, 35, 90, 0, 1, 1]
    );
}
#[cfg(feature = "visual-capture")]
#[test]
fn mixed_hz_outputs_share_one_simulation() {
    unsafe {
        let world = create(12);
        let reference = create(12);
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
            assert_eq!(sample.lower.tick, ns as u64 * 120 / 1_000_000_000);
            if let Some((time, lower)) = &prior
                && *time == ns
            {
                assert!(std::sync::Arc::ptr_eq(lower, &sample.lower));
            }
            prior = Some((ns, sample.lower.clone()));
            visuals_depth_sample_release(lease);
        }
        assert_eq!(
            visuals_depth_ensure_through(reference, 1_000_000_000, 121),
            0
        );
        assert_eq!(
            visuals_depth_debug_hash(world),
            visuals_depth_debug_hash(reference)
        );
        let mut stats = Stats::default();
        visuals_depth_stats(world, &mut stats);
        assert_eq!(stats.ticks, 121);
        visuals_depth_destroy(world);
        visuals_depth_destroy(reference);
    }
}

#[cfg(all(feature = "visual-capture", target_os = "linux"))]
fn thread_ns() -> u64 {
    #[repr(C)]
    struct Timespec {
        sec: i64,
        nsec: i64,
    }
    unsafe extern "C" {
        fn clock_gettime(clock: i32, time: *mut Timespec) -> i32;
    }
    let mut time = Timespec { sec: 0, nsec: 0 };
    assert_eq!(unsafe { clock_gettime(3, &mut time) }, 0);
    time.sec as u64 * 1_000_000_000 + time.nsec as u64
}
#[cfg(feature = "visual-capture")]
#[test]
#[ignore = "run the release timing gate on a quiet core"]
fn stress_tick_and_export() {
    for scale in [2., 0.25] {
        unsafe {
            let start = Instant::now();
            let world = create_scaled(60, scale);
            let activation = start.elapsed();
            let mut elapsed = Vec::with_capacity(36_000);
            #[cfg(target_os = "linux")]
            let mut max_cpu = 0;
            for i in 0..36_000u64 {
                let ns = (i * 1_000_000_000).div_ceil(120) as i64;
                let tick = Instant::now();
                #[cfg(target_os = "linux")]
                let cpu = thread_ns();
                assert_eq!(visuals_depth_ensure_through(world, ns, 2), 0);
                elapsed.push(tick.elapsed().as_nanos());
                #[cfg(target_os = "linux")]
                {
                    max_cpu = max_cpu.max(thread_ns() - cpu);
                }
            }
            let mut stats = Stats::default();
            visuals_depth_stats(world, &mut stats);
            visuals_depth_destroy(world);
            elapsed.sort_unstable();
            let p99 = elapsed[elapsed.len() * 99 / 100 - 1];
            println!(
                "60 bodies, 7920px union, speed 300, scale={scale}; creation={activation:?}, {} ticks, tick+export p99={p99} ns max={} ns",
                stats.ticks, stats.max_ns
            );
            assert_eq!(stats.ticks, 36_000);
            assert!(p99 <= 500_000);
            #[cfg(target_os = "linux")]
            {
                println!("max thread CPU={max_cpu} ns");
                assert!(max_cpu <= 2_000_000);
            }
            #[cfg(not(target_os = "linux"))]
            assert!(stats.max_ns <= 2_000_000);
        }
    }
}
