// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
fn config() -> Config {
    Config {
        abi_version: super::super::ABI_VERSION,
        struct_size: size_of::<Config>() as u32,
        visual: VISUAL,
        palette: 0,
        density: 55.0,
        scale: 2.0,
        speed: 3.0,
        trails: 1.0,
        seed: 1,
        reduced_motion: 0,
        epoch_ns: 0,
    }
}
fn settings() -> Settings {
    Settings {
        abi_version: 1,
        struct_size: size_of::<Settings>() as u32,
        count: 20,
        mode: 0,
        gravity: 10.0,
        elasticity: 100.0,
        collisions: 1,
        reserved: 0,
    }
}
#[test]
fn frozen_qml_physics_all_trails_and_ordered_records() {
    let fixtures = include_str!("../../../../../tests/parity/bounce/fixtures.tsv");
    let mut bounce = Bounce::new(config(), settings());
    let mut prior_scenario = -1;
    let mut prior_step = 0;
    let mut frames = 0;
    let mut balls = 0;
    let mut primitives = 0;
    let mut records = [Primitive::default(); MAX_RECORDS];
    for line in fixtures.lines().filter(|s| !s.starts_with('#')) {
        let mut cells = line.split('\t');
        let tag = cells.next().unwrap();
        let v: Vec<f64> = cells.map(|s| s.parse().unwrap()).collect();
        if tag == "F" {
            let scenario = v[0] as i32;
            let step = v[1] as usize;
            if scenario != prior_scenario {
                let mut c = config();
                let mut s = settings();
                c.seed = v[4] as i32;
                s.count = v[5] as u32;
                c.scale = v[6];
                c.speed = v[7];
                c.trails = v[8];
                s.gravity = v[9];
                s.elasticity = v[10];
                s.collisions = v[11] as u32;
                c.palette = v[12] as u32;
                bounce = Bounce::new(c, s);
                bounce.initialize(v[2], v[3]);
                prior_step = 0;
                prior_scenario = scenario;
            }
            for _ in prior_step..step {
                bounce.step(1.0 / 60.0);
            }
            prior_step = step;
            let r = Request {
                width: v[2],
                height: v[3],
                ..Request::default()
            };
            assert_eq!(bounce.frame(&r, 0.0, &[], &mut records), v[13] as usize);
            frames += 1;
        } else if tag == "B" {
            let body = &bounce.bodies[v[0] as usize];
            let b = body.ball;
            let actual = [b.x, b.y, b.vx, b.vy, b.size];
            for (i, a) in actual.into_iter().enumerate() {
                assert!(
                    (a - v[i + 1]).abs() < 1e-6,
                    "scenario {prior_scenario}, step {prior_step}, ball {}, field {i}: {a} != {}",
                    v[0],
                    v[i + 1]
                );
            }
            assert_eq!(body.trail_count, v[6] as usize);
            for (i, xy) in body.trail[..body.trail_count].iter().enumerate() {
                for k in 0..2 {
                    assert!(
                        (xy[k] - v[7 + i * 2 + k]).abs() < 1e-6,
                        "trail scenario {prior_scenario} step {prior_step}"
                    );
                }
            }
            balls += 1;
        } else {
            assert_eq!(tag, "P");
            let p = records[v[0] as usize];
            for k in 0..4 {
                assert!(
                    (p.geometry[k] as f64 - v[1 + k]).abs() < 1e-3,
                    "record geometry scenario {prior_scenario} step {prior_step} index {}",
                    v[0]
                );
            }
            let rgb = v[5] as u32;
            assert_eq!(
                p.color,
                [
                    ((rgb >> 16) & 255) as f32 / 255.0,
                    ((rgb >> 8) & 255) as f32 / 255.0,
                    (rgb & 255) as f32 / 255.0,
                    v[6] as f32
                ]
            );
            assert_eq!(p.shape, [v[7] as f32, v[8] as f32, 0.0, 0.0]);
            if p.shape[1] == 3.0 {
                assert!((v[9] - (v[1] - 0.30 * v[7])).abs() < 1e-6);
                assert!((v[10] - (v[2] - 0.40 * v[7])).abs() < 1e-6);
                assert_eq!(v[11], 1.0);
                assert!((v[12] - 1.12 * v[7]).abs() < 1e-6);
            }
            primitives += 1;
        }
    }
    assert_eq!((frames, balls, primitives), (64, 1128, 5306));
}
#[test]
fn fixed_step_delta_clamp_pause_and_live_configuration() {
    let mut a = Bounce::new(config(), settings());
    let mut b = Bounce::new(config(), settings());
    let mut r = Request {
        width: 640.0,
        height: 360.0,
        advance: 1,
        ..Request::default()
    };
    let mut out = [Primitive::default(); MAX_RECORDS];
    a.frame(&r, 10.0, &[], &mut out);
    b.initialize(640.0, 360.0);
    for _ in 0..6 {
        b.step(1.0 / 60.0);
    }
    assert_eq!(a.bodies[0].ball, b.bodies[0].ball);
    let before = a.bodies[0].ball;
    r.advance = 0;
    a.frame(&r, 100.0, &[], &mut out);
    assert_eq!(a.bodies[0].ball, before);
    let mut c = config();
    c.reduced_motion = 1;
    a.configure(c, settings());
    r.advance = 1;
    a.frame(&r, 1.0, &[], &mut out);
    assert_eq!(a.bodies[0].ball, before);
    c.reduced_motion = 0;
    c.palette = 2;
    a.configure(c, settings());
    r.advance = 0;
    a.frame(&r, 0.0, &[], &mut out);
    assert_eq!(a.bodies[0].ball, before);
    c.scale = 1.0;
    a.configure(c, settings());
    a.frame(&r, 0.0, &[], &mut out);
    assert_ne!(a.bodies[0].ball, before);
    assert_eq!(a.bodies[0].trail_count, 0);
}
#[test]
fn trail_shrink_retains_qml_unshift_pop_behavior() {
    let mut b = Bounce::new(config(), settings());
    b.initialize(640.0, 360.0);
    for _ in 0..20 {
        b.step(1.0 / 60.0);
    }
    assert_eq!(b.bodies[0].trail_count, 8);
    let mut c = config();
    c.trails = 0.12;
    b.configure(c, settings());
    b.step(1.0 / 60.0);
    assert_eq!(b.bodies[0].trail_count, 8);
    c.trails = 0.0;
    b.configure(c, settings());
    b.step(1.0 / 60.0);
    assert_eq!(b.bodies[0].trail_count, 0);
}
#[test]
fn shared_modes_keep_mapping_order_and_distinct_tail_rules() {
    let mut s = settings();
    s.mode = 1;
    let mut b = Bounce::new(config(), s);
    let r = Request {
        width: 640.0,
        height: 360.0,
        world_bounds: [-100.0, 200.0, 1280.0, 1440.0],
        viewport_origin: [-100.0, 200.0],
        ..Request::default()
    };
    let ball = Ball {
        x: 100.0,
        y: 400.0,
        vx: 3.0,
        vy: 4.0,
        size: 100.0,
    };
    let mut out = [Primitive::default(); MAX_RECORDS];
    assert_eq!(b.frame(&r, 0.0, &[ball], &mut out), 9);
    assert_eq!(out[8].geometry, [75.0, 112.5, 0.5, 0.25]);
    assert_eq!(out[8].shape, [50.0, 3.0, 0.0, 0.0]);
    assert_eq!(out[0].shape[0], 42.0);
    assert_eq!(out[0].color[3], 0.02);
    s.mode = 2;
    b.configure(config(), s);
    assert_eq!(b.frame(&r, 0.0, &[ball], &mut out), 6);
    assert_eq!(out[5].geometry, [250.0, 250.0, 1.0, 1.0]);
    assert_eq!(out[5].shape[1], 4.0);
    assert_eq!(out[0].geometry, [219.2, 213.6, 1.0, 1.0]);
    assert_eq!(out[0].shape[0], 36.0);
    assert_eq!(out[0].color[3], 0.22);
    assert_eq!(out[4].color[3], 0.08);
}
#[test]
fn abi_errors_leave_output_untouched() {
    let c = config();
    let s = settings();
    let p = unsafe { visuals_bounce_create(&c, &s) };
    assert!(!p.is_null());
    let r = Request {
        width: 640.0,
        height: 360.0,
        ..Request::default()
    };
    let mut count = 123;
    let mut out = [Primitive::default(); MAX_RECORDS];
    assert_eq!(
        unsafe {
            visuals_bounce_frame(
                p,
                &r,
                0.0,
                std::ptr::null(),
                0,
                out.as_mut_ptr(),
                MAX_RECORDS - 1,
                &mut count,
            )
        },
        -1
    );
    assert_eq!(count, 123);
    assert_eq!(out, [Primitive::default(); MAX_RECORDS]);
    let bad = Ball {
        x: f64::NAN,
        ..Ball::default()
    };
    assert_eq!(
        unsafe {
            visuals_bounce_frame(
                p,
                &r,
                0.0,
                &bad,
                1,
                out.as_mut_ptr(),
                MAX_RECORDS,
                &mut count,
            )
        },
        -1
    );
    assert_eq!(count, 123);
    unsafe { visuals_bounce_destroy(p) };
}
#[test]
fn independent_48_and_60_hz_have_identical_fixed_step_state() {
    let mut a = Bounce::new(config(), settings());
    let mut b = Bounce::new(config(), settings());
    let r = Request {
        width: 640.0,
        height: 360.0,
        advance: 1,
        ..Request::default()
    };
    let mut out = [Primitive::default(); MAX_RECORDS];
    for _ in 0..60 {
        a.frame(&r, 1.0 / 60.0, &[], &mut out);
    }
    for _ in 0..48 {
        b.frame(&r, 1.0 / 48.0, &[], &mut out);
    }
    for (a, b) in a.bodies.iter().zip(&b.bodies) {
        assert_eq!(a.ball, b.ball);
        assert_eq!(a.trail_count, b.trail_count);
        assert_eq!(a.trail, b.trail);
    }
}

#[test]
fn legacy_v2_disc_auxiliary_coordinates_remain_ignored() {
    use super::super::primitives::{self, Vertex};
    for kind in [1.0, 2.0] {
        let mut a = Primitive {
            geometry: [24.0, 24.0, 0.0, 0.0],
            color: [0.25, 0.75, 1.0, 0.5],
            shape: [10.0, kind, 0.0, 0.0],
        };
        let mut av = [Vertex::default(); 6];
        let mut bv = [Vertex::default(); 6];
        primitives::vertices(&[a], 1.0, &mut av);
        let mut ap = [0u8; 64 * 64 * 4];
        let mut bp = [0u8; 64 * 64 * 4];
        primitives::raster(&[a], [64.0, 64.0], &mut ap, 64, 64);
        a.geometry[2] = 37.0;
        a.geometry[3] = 11.0;
        primitives::vertices(&[a], 1.0, &mut bv);
        primitives::raster(&[a], [64.0, 64.0], &mut bp, 64, 64);
        for (a, b) in av.iter().zip(&bv) {
            assert_eq!(a.position, b.position);
            assert_eq!(a.local, b.local);
            assert_eq!(a.shape, b.shape);
        }
        assert_eq!(ap, bp, "legacy kind {kind} must ignore unused coordinates");
    }
}
