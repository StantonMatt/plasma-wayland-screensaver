use snakes_core::visuals::depth::{
    MirroredPolicy,
    starfield::{DESCRIPTOR, PARAMETERS, Params},
};
#[test]
fn production_contract() {
    assert_eq!(DESCRIPTOR.id, 3);
    assert_eq!(
        snakes_core::visuals::depth::ffi::visuals_depth_available(DESCRIPTOR.id),
        1
    );
    assert_eq!(DESCRIPTOR.rate, 30);
    assert_eq!(DESCRIPTOR.mirrored, MirroredPolicy::SharedCanonical);
    assert_eq!(DESCRIPTOR.fit as u32, 2);
    let p = Params::default();
    assert_eq!(
        (p.flight, p.steering, p.nebulae, p.events),
        ("voyage", 40, 45, true)
    );
    assert_eq!(
        PARAMETERS
            .iter()
            .map(|p| p.default_value as i32)
            .collect::<Vec<_>>(),
        [0, 40, 45, 1]
    );
}
#[cfg(feature = "visual-capture")]
#[test]
fn mixed_refresh_leases_and_replay() {
    use snakes_core::visuals::depth::{common::layout::Rect, ffi::*};
    use std::ptr;
    unsafe {
        let config = Config {
            visual: 3,
            seed: 23,
            palette: 0,
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
        .map(|(x, y, width, height, refresh_hz)| Output {
            id: x as u64,
            viewport: Rect {
                x,
                y,
                width,
                height,
            },
            dpr: 1.,
            refresh_hz,
            primary: u32::from(x == 2560.),
            padding: 0,
        });
        let mut world = ptr::null_mut();
        let env = Env {
            background: 1,
            frame_cap: 60,
            top: [0.; 4],
            bottom: [0.; 4],
        };
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
                50_000_000,
                &mut world
            ),
            0
        );
        let mut times: Vec<_> = outputs
            .iter()
            .flat_map(|o| {
                (0..=o.refresh_hz as u64)
                    .map(move |i| ((1_000_000_000 * i / o.refresh_hz as u64) as i64, o))
            })
            .collect();
        times.sort_by_key(|(t, _)| *t);
        let mut prior = None;
        for (t, o) in times {
            assert_eq!(visuals_depth_ensure_through(world, t, 8), 0);
            let v = View {
                output_id: o.id,
                layout_generation: 1,
                timeline_ns: t,
                viewport: o.viewport,
                uniform_scale: 1.,
                offset_x: 0.,
                offset_y: 0.,
                dpr: 1.,
            };
            let mut lease = ptr::null_mut();
            assert_eq!(visuals_depth_retain_sample(world, &v, &mut lease), 0);
            let s = &(*lease).sample;
            assert!(!s.missed);
            assert_eq!(s.lower.tick, (t as u64 * 30) / 1_000_000_000);
            if let Some((n, ref a)) = prior
                && n == t
            {
                assert!(std::sync::Arc::ptr_eq(a, &s.lower))
            }
            prior = Some((t, s.lower.clone()));
            visuals_depth_sample_release(lease);
        }
        let hash = visuals_depth_debug_hash(world);
        assert_eq!(visuals_depth_debug_seek(world, 1_000_000_000), 0);
        assert_eq!(hash, visuals_depth_debug_hash(world));
        visuals_depth_destroy(world);
    }
}
