// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::visuals::depth::{
    common::{
        layout::{Layout, OutputRect, Rect},
        rng::{identity_hash, output_seed},
    },
    ribbons::Ribbons,
    world::{Environment, Event, LivingWorld, MonitorMode, WorldInput},
};

fn input(seed: u64, width: f64, height: f64) -> WorldInput {
    WorldInput {
        visual_id: 4,
        seed: output_seed(seed, 4, "Virtual-0"),
        mode: MonitorMode::Independent,
        layout: Layout::new(
            vec![OutputRect {
                id: identity_hash("Virtual-0"),
                rect: Rect {
                    x: 0.,
                    y: 0.,
                    width,
                    height,
                },
                primary: true,
            }],
            1,
        )
        .unwrap(),
        speed: 1.,
        density: 50.,
        scale: 1.,
        trails: 0.35,
        palette: 0,
        reduced_motion: false,
        parameters: vec![],
        environment: Environment::default(),
    }
}

#[test]
fn motes_leave_the_shedding_curve() {
    let mut w = Ribbons::new(input(7, 2560., 1440.));
    w.params.world_events = false;
    let mut smallest = f64::INFINITY;
    let mut samples = 0;
    for tick in 1..=5400 {
        w.step(tick);
        for m in w.sea.motes.iter().filter(|m| m.age <= 1. / 30. + 1e-8) {
            let Some(r) = w.sea.agents.iter().find(|r| r.id == m.id / 64) else {
                continue;
            };
            let knot = r.extent() * (r.count - 1) as f64;
            let i = (knot as usize).min(r.count - 2);
            let tangent = (r.nodes[i + 1].pos - r.nodes[i].pos).unit();
            let tail = r.nodes[i].pos + (r.nodes[i + 1].pos - r.nodes[i].pos) * (knot - i as f64);
            smallest = smallest.min(
                (m.pos - tail - w.sea.flow.sample(tail).velocity * (0.33 / 30.))
                    .dot(tangent.perpendicular())
                    .abs()
                    / r.width,
            );
            samples += 1;
        }
    }
    println!("mote birth scatter: {samples} samples, smallest transverse offset {smallest:.4} W");
    assert!(samples >= 30 && smallest >= 0.45);
}

#[test]
fn gust_curvature_reaches_the_head_half_of_every_lane() {
    let mut w = Ribbons::new(input(23, 2560., 1440.));
    w.inject(Event {
        id: 1,
        at_ns: 600_000_000_000,
        anchor: [0., 720.],
        strength: 1.,
    })
    .unwrap();
    for tick in 1..=18120 {
        w.step(tick);
    }
    let mut gust = Frame::new(w.art.clone(), 40, false);
    gust.fill_with_wave(&w.sea, w.params.twist, 0.);
    let event = w.sea.weather.active.take().unwrap();
    let mut control = Frame::new(w.art.clone(), 40, false);
    control.fill_with_wave(&w.sea, w.params.twist, 0.);
    w.sea.weather.active = Some(event);
    let mut lanes = 0;
    for strip in &gust.strips {
        let r = w.sea.agents.iter().find(|r| r.id == strip.id).unwrap();
        if strip.outline.length < 1000. || event.gust(r.head(), w.sea.left) < 0.5 {
            continue;
        }
        let mut peak = 0_f64;
        for i in 1..strip.count - 1 {
            if strip.outline.arc(i) > strip.outline.length * 0.4 {
                break;
            }
            let a = gust.vertices[strip.offset + i].pos;
            let b = control.vertices[strip.offset + i].pos;
            peak = peak.max(((a[0] - b[0]) as f64).hypot((a[1] - b[1]) as f64) / r.width);
        }
        println!(
            "gust ribbon {} layer {} head-half sway {peak:.4} W",
            r.id, r.layer
        );
        assert!(peak >= 0.15);
        lanes += 1;
    }
    assert!(lanes >= 3);
}

#[test]
fn maelstrom_contour_is_non_circular_and_breathes_by_layer() {
    let mut w = Ribbons::new(input(23, 2560., 1440.));
    w.inject(Event {
        id: 2,
        at_ns: 0,
        anchor: [1130., 715.],
        strength: 1.,
    })
    .unwrap();
    for tick in 1..=300 {
        w.step(tick);
    }
    let m = w.sea.weather.active.unwrap();
    for layer in 0..3 {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for i in 0..32 {
            let radial = super::super::agent::polar(i as f64 * std::f64::consts::TAU / 32.);
            let point = m.centre + radial * m.orbit_radius(layer);
            let drive = m.swirl(point, layer).0.dot(radial);
            lo = lo.min(drive);
            hi = hi.max(drive);
        }
        println!(
            "maelstrom layer {layer} angular radial-drive range {:.3} px/s",
            hi - lo
        );
        assert!(hi - lo >= 40.);
        let mut smallest_peak = f64::INFINITY;
        let mut largest_peak = 0_f64;
        for tenth in 0..=130 {
            let mut moment = m;
            moment.age = tenth as f64 / 10.;
            let contour = moment.orbit_contour(layer);
            let peak = (0..64)
                .map(|i| {
                    contour.radius(super::super::agent::polar(
                        i as f64 * std::f64::consts::TAU / 64.,
                    ))
                })
                .fold(0_f64, f64::max);
            smallest_peak = smallest_peak.min(peak);
            largest_peak = largest_peak.max(peak);
        }
        let breathing = largest_peak / smallest_peak;
        println!("maelstrom layer {layer} temporal major-radius ratio {breathing:.6}");
        assert!((1.10..=1.14).contains(&breathing));
    }
}

fn radius(a: Vec2, b: Vec2, c: Vec2) -> f64 {
    let x = b - a;
    let y = c - b;
    let area = (x.x * y.y - x.y * y.x).abs();
    if area < 1e-10 {
        f64::INFINITY
    } else {
        x.length() * y.length() * (c - a).length() / (2. * area)
    }
}

#[test]
fn far_heads_keep_a_two_hundred_pixel_turning_radius() {
    let mut w = Ribbons::new(input(7, 1920., 1080.));
    w.params.world_events = false;
    let mut history = std::collections::BTreeMap::new();
    let mut tightest = f64::INFINITY;
    let mut samples = 0;
    let mut closed = 0;
    for tick in 1..=9000 {
        w.step(tick);
        for r in w.sea.agents.iter().filter(|r| r.alive && r.layer == 0) {
            let h = history.entry(r.id).or_insert([r.head(); 3]);
            h.rotate_left(1);
            h[2] = r.head();
            if tick > 2 {
                tightest = tightest.min(radius(h[0], h[1], h[2]));
                samples += 1;
            }
            if tick % 150 == 0 || ((540..=900).contains(&tick) && tick % 15 == 0) {
                let end = (r.extent() * (r.count - 1) as f64) as usize;
                let points = &r.nodes[..=end];
                let cross = |a: Vec2, b: Vec2| a.x * b.y - a.y * b.x;
                for (i, a) in points.windows(2).enumerate() {
                    for b in points.windows(2).skip(i + 3) {
                        let d = a[1].pos - a[0].pos;
                        let e = b[1].pos - b[0].pos;
                        let z = b[0].pos - a[0].pos;
                        let den = cross(d, e);
                        if den.abs() < 1e-8 {
                            continue;
                        }
                        let (t, u) = (cross(z, e) / den, cross(z, d) / den);
                        if (0.0..1.).contains(&t)
                            && (0.0..1.).contains(&u)
                            && w.input.layout.bounds.contains(a[0].pos + d * t)
                            && r.knot_for == 0.
                        {
                            closed += 1;
                            println!("far closed body at tick {tick}, ribbon {}", r.id);
                        }
                    }
                }
            }
        }
    }
    println!(
        "normal-flow far head path: {samples} samples, minimum radius {tightest:.4} px; unpermitted visible far crossings {closed}"
    );
    assert!(samples > 1000 && tightest >= 199.9 && closed == 0);
}

#[test]
fn each_mote_has_one_brief_thirty_percent_twinkle() {
    let mut w = Ribbons::new(input(7, 2560., 1440.));
    for tick in 1..=1800 {
        w.step(tick);
    }
    assert!(!w.sea.motes.is_empty());
    for original in &w.sea.motes {
        let mut m = *original;
        let mut bursts = 0;
        let mut inside = false;
        let mut peak = 1_f64;
        let mut duration = 0.;
        for i in 0..=(m.life * 1000.) as usize {
            m.age = i as f64 / 1000.;
            let gain = m.twinkle();
            assert!((1.0..=1.3).contains(&gain));
            peak = peak.max(gain);
            if gain > 1. {
                duration += 0.001;
                if !inside {
                    bursts += 1;
                }
                inside = true;
            } else {
                inside = false;
            }
        }
        assert!(bursts == 1 && peak >= 1.299 && (duration - 0.3_f64).abs() <= 0.002);
    }
}

#[test]
fn gust_onset_has_a_wall_time_easing_floor() {
    for speed in [1., 3.] {
        let mut cfg = input(1, 2560., 1440.);
        cfg.speed = speed;
        let mut w = Ribbons::new(cfg);
        w.params.world_events = false;
        w.inject(Event {
            id: 1,
            at_ns: 1_000_000_000,
            anchor: [0., 720.],
            strength: 1.,
        })
        .unwrap();
        for tick in 1..=45 {
            w.step(tick);
        }
        let m = w.sea.weather.active.unwrap();
        let gain = m.gust(p(0., 720.), 0.);
        println!("Gust speed={speed}, at wall onset+0.5s: gain={gain:.9}");
        // The measured 1.25-second minimum keeps onset below 36% after 0.5s.
        // The previous event-clock rise reached 0.74 / 1.0 at these Speeds.
        assert!(gain <= 0.353);
    }
}

#[test]
fn maelstrom_release_preserves_head_motion_at_handoff() {
    let mut w = Ribbons::new(input(1, 2560., 1440.));
    let r = w.sea.agents.iter_mut().find(|r| r.alive).unwrap();
    r.caught = true;
    r.heading = p(1., 0.);
    r.speed = 90.;
    let before = r.heading;
    let speed = r.speed;
    r.release_orbit(r.head() - p(100., 0.));
    let rotation = super::super::agent::turn(before, r.heading).abs();
    let jump = (r.speed - speed).abs();
    println!("Maelstrom launch handoff: heading jump={rotation:.9}rad, speed jump={jump:.9}px/s");
    assert!(rotation < 1e-12 && jump < 1e-12);
    assert!(!r.caught && r.release_age > 0. && r.release.x > 0. && r.release.y > 0.);
}

#[test]
fn maelstrom_orbital_influence_fades_before_handoff() {
    let mut w = Ribbons::new(input(1, 2560., 1440.));
    w.params.world_events = false;
    w.inject(Event {
        id: 2,
        at_ns: 1_000_000_000,
        anchor: [1280., 720.],
        strength: 1.,
    })
    .unwrap();
    for tick in 1..=1200 {
        w.step(tick);
        if let Some(m) = w.sea.weather.active.filter(|m| m.collapsing()) {
            assert!(m.weight().abs() < 1e-12);
            return;
        }
    }
    panic!("Maelstrom did not reach its handoff");
}

#[test]
fn opposing_gust_does_not_erase_a_launch_heading() {
    let mut w = Ribbons::new(input(1, 2560., 1440.));
    let lanes = super::super::water::Lanes::new(Rect {
        x: 0.,
        y: 0.,
        width: 2560.,
        height: 1440.,
    });
    let sea = &mut w.sea;
    let r = sea.agents.iter_mut().find(|r| r.alive).unwrap();
    r.layer = 2;
    r.caught = false;
    r.release_age = 20.;
    let wind = p(395., -10.);
    let gust = 0.5;
    let dt = 1. / 30.;
    let blend = dt * 1.25;
    r.heading = -wind.unit();
    r.release = r.heading;
    let before = r.heading;
    r.speed = (gust * wind.length() - 315. * blend) / (1. - blend);
    r.swim(
        super::super::agent::Steering {
            desired: p(0., 0.),
            curl: 0.,
            gust,
            gust_force_limit: f64::INFINITY,
            maelstrom_motor_limit: f64::INFINITY,
            launch: 1.,
            gust_front: None,
            turbulence: 0.4,
            still: 0.,
            smooth_twist: false,
            spanning: true,
            braid_phase: None,
            orbit: None,
        },
        &sea.flow,
        &lanes,
        0.55,
        dt,
        false,
    );
    println!(
        "Opposing Gust: heading length={:.12}, alignment={:.12}",
        r.heading.length(),
        r.heading.dot(before)
    );
    assert!(r.heading.length() > 0.999999 && r.heading.dot(before) > 0.999999);
}

#[test]
fn gust_added_velocity_has_one_screen_space_budget() {
    let mut largest = 0_f64;
    for speed in [1., 2., 3.] {
        let mut cfg = input(7, 2560., 1440.);
        cfg.speed = speed;
        cfg.density = 80.;
        cfg.scale = 2.;
        let mut w = Ribbons::new(cfg);
        w.params.world_events = false;
        w.inject(Event {
            id: 1,
            at_ns: 2_000_000_000,
            anchor: [0., 720.],
            strength: 1.,
        })
        .unwrap();
        let mut maximum = 0_f64;
        for tick in 1..=360 {
            let before: Vec<_> = w
                .sea
                .agents
                .iter()
                .filter(|r| r.alive && r.layer > 0)
                .map(|r| (r.id, r.head()))
                .collect();
            w.step(tick);
            for r in w.sea.agents.iter().filter(|r| r.alive && r.layer > 0) {
                let Some((_, old)) = before.iter().find(|(id, _)| *id == r.id) else {
                    continue;
                };
                // Independent mid/near heads have no resultant-velocity projection.
                // Subtract swimming to measure the actual event translation in pixels/s.
                let added = (r.head() - *old) * 30. - r.heading * (r.speed * speed);
                maximum = maximum.max(added.length());
            }
        }
        println!(
            "Gust added velocity at Speed{}: {maximum:.9} screen px/s",
            speed * 100.
        );
        assert!(maximum > 140.);
        largest = largest.max(maximum);
    }
    assert!(
        largest <= 150.000001,
        "Gust added velocity {largest:.9} exceeds its screen-space budget"
    );
}

#[test]
#[ignore = "matched full-cohort Gust material excursion and streaming metrics"]
fn gust_visibility_metrics() {
    for (name, seed, spanning, onset) in [
        ("real-spanning", 1, true, 602_u64),
        ("defaults-canonical", 23, false, 600_u64),
    ] {
        let mut cfg = input(seed, 2560., 1440.);
        cfg.environment.background = 1;
        if spanning {
            cfg.seed = seed;
            cfg.mode = MonitorMode::Spanning;
            cfg.speed = 3.;
            cfg.density = 80.;
            cfg.scale = 2.;
            cfg.trails = 1.;
            cfg.palette = 2;
            cfg.environment.background = 0;
            cfg.layout = Layout::new(
                [
                    (0., 0., 2560., 1440.),
                    (2560., 0., 3440., 1440.),
                    (6000., 195., 1920., 1080.),
                ]
                .into_iter()
                .enumerate()
                .map(|(id, (x, y, width, height))| OutputRect {
                    id: id as u64,
                    rect: Rect {
                        x,
                        y,
                        width,
                        height,
                    },
                    primary: id == 0,
                })
                .collect(),
                1,
            )
            .unwrap();
        }
        let mut w = Ribbons::new(cfg);
        w.inject(Event {
            id: 1,
            at_ns: onset as i64 * 1_000_000_000,
            anchor: [0., 720.],
            strength: 1.,
        })
        .unwrap();
        type Point = super::super::common::layout::Vec2;
        let mut tracks: Vec<(u32, Vec<(usize, Point, f64, f64)>)> = vec![];
        let (mut total, mut aligned, mut east) = (0_u64, 0_u64, 0_u64);
        let mut speeds = vec![];
        for tick in 1..=(onset + 10) * 30 {
            let old: Vec<_> = if tick >= onset * 30 {
                w.sea
                    .agents
                    .iter()
                    .filter(|r| r.alive && r.layer > 0)
                    .map(|r| (r.id, r.head()))
                    .collect()
            } else {
                vec![]
            };
            w.step(tick);
            if tick == onset * 30 {
                tracks = w
                    .sea
                    .agents
                    .iter()
                    .filter(|r| r.alive && r.layer > 0 && r.extent() > 0.15)
                    .map(|r| {
                        let end = (r.extent() * (r.count - 1) as f64).floor() as usize;
                        (
                            r.id,
                            r.nodes[..=end]
                                .iter()
                                .enumerate()
                                .filter(|(_, n)| w.input.layout.contains(n.pos))
                                .map(|(i, n)| (i, n.pos, 0., 0.))
                                .collect(),
                        )
                    })
                    .filter(|(_, p): &(u32, Vec<_>)| !p.is_empty())
                    .collect();
            }
            if tick >= onset * 30 {
                for (id, points) in &mut tracks {
                    let Some(r) = w.sea.agents.iter().find(|r| r.id == *id) else {
                        continue;
                    };
                    for (i, a, x, y) in points {
                        if *i as f64 > r.extent() * (r.count - 1) as f64 {
                            continue;
                        }
                        *x = x.max((r.nodes[*i].pos.x - a.x).abs());
                        *y = y.max((r.nodes[*i].pos.y - a.y).abs());
                    }
                }
            }
            let Some(m) = w
                .sea
                .weather
                .active
                .filter(|m| m.kind == super::super::water::Kind::Gust)
            else {
                continue;
            };
            if tick < onset * 30 {
                continue;
            }
            for r in w.sea.agents.iter().filter(|r| r.alive && r.layer > 0) {
                let Some((_, a)) = old.iter().find(|(id, _)| *id == r.id) else {
                    continue;
                };
                if m.gust(*a, w.sea.left) < 0.8 {
                    continue;
                }
                let v = (r.head() - *a) * 30.;
                if v.length() < 1. {
                    continue;
                }
                total += 1;
                aligned += u64::from(v.y.abs().atan2(v.x.abs()) <= 10_f64.to_radians());
                east += u64::from(v.x > 0.);
                speeds.push(v.length());
            }
        }
        let mut xs = vec![];
        let mut ys = vec![];
        for (_, points) in &tracks {
            for (_, _, x, y) in points {
                xs.push(*x);
                ys.push(*y);
            }
        }
        xs.sort_by(f64::total_cmp);
        ys.sort_by(f64::total_cmp);
        speeds.sort_by(f64::total_cmp);
        assert!(!xs.is_empty() && total > 0);
        let q = |v: &[f64], p: f64| v[((v.len() - 1) as f64 * p).round() as usize];
        println!(
            "GUST_FULL_COHORT {name}: onset={onset},lanes={},initialVisiblePoints={},xExcursionP10={:.6},xExcursionP50={:.6},xExcursionP90={:.6},yExcursionP50={:.6},headSamples={total},horizontalWithin10deg={:.9},eastFraction={:.9},headSpeedP50={:.6},headSpeedP99={:.6}",
            tracks.len(),
            xs.len(),
            q(&xs, 0.1),
            q(&xs, 0.5),
            q(&xs, 0.9),
            q(&ys, 0.5),
            aligned as f64 / total as f64,
            east as f64 / total as f64,
            q(&speeds, 0.5),
            q(&speeds, 0.99)
        );
    }
}

#[test]
fn gust_turns_upstream_swimmers_downwind_without_motor_boost() {
    for speed in [1., 3.] {
        let mut cfg = input(1, 2560., 1440.);
        cfg.speed = speed;
        let mut w = Ribbons::new(cfg);
        w.params.world_events = false;
        let upstream: Vec<_> = w
            .sea
            .agents
            .iter()
            .filter(|r| r.alive && r.course_sign < 0.)
            .map(|r| r.id)
            .collect();
        assert!(!upstream.is_empty());
        w.inject(Event {
            id: 1,
            at_ns: 1_000_000_000,
            anchor: [0., 720.],
            strength: 1.,
        })
        .unwrap();
        let (mut samples, mut east) = (0_u64, 0_u64);
        let mut maximum_motor = 0_f64;
        for tick in 1..=12 * 30 {
            let before: Vec<_> = w
                .sea
                .agents
                .iter()
                .filter(|r| upstream.contains(&r.id))
                .map(|r| (r.id, r.head()))
                .collect();
            w.step(tick);
            let Some(m) = w
                .sea
                .weather
                .active
                .filter(|m| m.kind == super::super::water::Kind::Gust)
            else {
                continue;
            };
            for r in w
                .sea
                .agents
                .iter()
                .filter(|r| r.alive && upstream.contains(&r.id))
            {
                maximum_motor = maximum_motor.max(r.speed);
                let Some((_, old)) = before.iter().find(|(id, _)| *id == r.id) else {
                    continue;
                };
                if tick < 5 * 30 || m.gust(*old, w.sea.left) < 0.8 {
                    continue;
                }
                samples += 1;
                east += u64::from(r.head().x > old.x);
            }
        }
        println!(
            "Gust initially upstream at Speed{}: {east}/{samples} downstream samples; motor max{maximum_motor:.6}",
            speed * 100.
        );
        assert!(samples >= 20 && east as f64 / samples as f64 >= 0.8);
        assert!(maximum_motor <= 230.000001);
    }
}

#[test]
fn gust_keeps_the_ordinary_motor_ceiling_under_strong_steering() {
    let mut w = Ribbons::new(input(1, 2560., 1440.));
    let lanes = super::super::water::Lanes::new(w.input.layout.bounds);
    let sea = &mut w.sea;
    let r = sea.agents.iter_mut().find(|r| r.alive).unwrap();
    r.speed = 90.;
    r.caught = false;
    r.release_age = 0.;
    for _ in 0..480 {
        r.swim(
            super::super::agent::Steering {
                desired: p(1000., 0.),
                curl: 0.,
                gust: 1.,
                gust_force_limit: 50.,
                maelstrom_motor_limit: f64::INFINITY,
                launch: 0.,
                gust_front: None,
                turbulence: 0.4,
                still: 0.,
                smooth_twist: false,
                spanning: false,
                braid_phase: None,
                orbit: None,
            },
            &sea.flow,
            &lanes,
            0.55,
            1. / 120.,
            false,
        );
    }
    println!(
        "Gust motor after four seconds of strong steering: {:.9}",
        r.speed
    );
    assert!(r.speed > 220. && r.speed <= 230.000001);
}

#[test]
fn maelstrom_motor_budget_includes_the_release_floor() {
    for speed in [1., 3.] {
        let mut cfg = input(1, 2560., 1440.);
        cfg.speed = speed;
        cfg.density = 80.;
        cfg.scale = 2.;
        cfg.trails = 1.;
        cfg.palette = 2;
        cfg.environment.background = 0;
        let mut w = Ribbons::new(cfg);
        w.params.world_events = false;
        w.inject(Event {
            id: 2,
            at_ns: 1_000_000_000,
            anchor: [1280., 720.],
            strength: 1.,
        })
        .unwrap();
        let mut peak = 0_f64;
        let mut event_samples = 0;
        let mut release_samples = 0;
        for tick in 1..=35 * 30 {
            w.step(tick);
            if tick < 5 * 30 {
                continue;
            }
            for r in w
                .sea
                .agents
                .iter()
                .filter(|r| r.alive && (r.caught || r.release_age > 0.))
            {
                peak = peak.max(r.speed * speed);
                event_samples += usize::from(r.caught);
                release_samples += usize::from(r.release_age > 0.);
            }
        }
        println!(
            "Maelstrom motor at Speed{}: peak{peak:.9}wallpx/s; orbitSamples{event_samples},releaseSamples{release_samples}",
            speed * 100.
        );
        assert!(event_samples > 100 && release_samples > 100);
        if speed == 1. {
            assert!(peak > 235. && peak <= 315.0001);
        } else {
            let budget = super::super::water::MAELSTROM_MOTOR_PIXELS_PER_SECOND;
            assert!(peak > budget - 20. && peak <= budget + 0.001);
        }
    }
}

#[test]
fn real_spanning_maelstrom_release_does_not_recoil() {
    let mut cfg = input(7, 2560., 1440.);
    cfg.seed = 7;
    cfg.mode = MonitorMode::Spanning;
    cfg.speed = 3.;
    cfg.density = 80.;
    cfg.scale = 2.;
    cfg.trails = 1.;
    cfg.palette = 2;
    cfg.environment.background = 0;
    cfg.layout = Layout::new(
        [
            (0., 0., 2560., 1440.),
            (2560., 0., 3440., 1440.),
            (6000., 195., 1920., 1080.),
        ]
        .into_iter()
        .enumerate()
        .map(|(id, (x, y, width, height))| OutputRect {
            id: id as u64,
            rect: Rect {
                x,
                y,
                width,
                height,
            },
            primary: id == 0,
        })
        .collect(),
        1,
    )
    .unwrap();
    let mut w = Ribbons::new(cfg);
    w.inject(Event {
        id: 2,
        at_ns: 602_000_000_000,
        anchor: [4280., 720.],
        strength: 1.,
    })
    .unwrap();
    let mut captured = std::collections::BTreeMap::<u32, Option<u64>>::new();
    let mut maximum = 0_f64;
    let mut worst = (0_u64, 0_u32, 0_f64, 0_usize, 0_f64, 0_f64, 0_u64);
    let mut samples = 0;
    for tick in 1..=19800 {
        w.step(tick);
        for r in w.sea.agents.iter().filter(|r| r.alive) {
            if (18060..=18120).contains(&tick) && r.caught {
                captured.entry(r.id).or_insert(None);
            }
            let Some(release) = captured.get_mut(&r.id) else {
                continue;
            };
            if release.is_none() && r.release_age > 0. {
                *release = Some(tick);
            }
            let Some(start) = *release else {
                continue;
            };
            let elapsed = tick - start;
            if !(60..=450).contains(&elapsed) || elapsed % 15 != 0 {
                continue;
            }
            let end = r.extent() * (r.count - 1) as f64;
            let n = end.floor() as usize;
            let mut body: Vec<_> = r.nodes[..=n].iter().map(|n| n.pos).collect();
            if n + 1 < r.count && end.fract() > 1e-8 {
                body.push(r.nodes[n].pos + (r.nodes[n + 1].pos - r.nodes[n].pos) * end.fract());
            }
            if !body.iter().any(|q| w.input.layout.contains(*q)) {
                continue;
            }
            samples += 1;
            let (mut total, mut lo, mut hi) = (0_f64, 0_f64, 0_f64);
            for q in body.windows(3) {
                total += super::super::agent::turn(q[1] - q[0], q[2] - q[1]);
                lo = lo.min(total);
                hi = hi.max(total);
            }
            let excursion = (hi - lo).to_degrees();
            if excursion > maximum {
                maximum = excursion;
                worst = (
                    tick,
                    r.id,
                    r.release_age,
                    r.layer,
                    r.head().x,
                    r.head().y,
                    elapsed,
                );
            }
        }
    }
    println!(
        "Real S7 Maelstrom: captured={},samples={samples},maxWholeTurn={maximum:.9},worstTickStrandReleaseAgeLayerXYElapsed={worst:?}",
        captured.len()
    );
    assert!(captured.len() >= 3 && samples > 20);
    assert!(
        maximum <= 270.,
        "released body recoils through {maximum} degrees"
    );
}

// Colour reference for native RGB8 checks. The private GPU captures verify the
// shader expression; this also exercises the real material's selector, so the
// shipped pure-red material fails the same single-byte regression.
fn ember_hsv_saturation(rgb: [u8; 3]) -> f64 {
    let maximum = *rgb.iter().max().unwrap() as f64;
    let minimum = *rgb.iter().min().unwrap() as f64;
    (maximum - minimum) / maximum.max(1.)
}

#[test]
fn ember_warm_saturation_uses_the_minimum_channel() {
    assert!((ember_hsv_saturation([3, 2, 1]) - 2. / 3.).abs() < 1e-12);
}

fn ember_integer_update(rgb: [u8; 3], weights: [u32; 3]) -> [u8; 3] {
    let [wr, wg, wb] = weights;
    let [mut r, mut g, mut b] = rgb.map(u32::from);
    let mut allowed = (wg * g + wb * b) / wr;
    if allowed + 1 < r {
        g = (g + 1).min(255);
        b = (b + 1).min(255);
        allowed = (wg * g + wb * b) / wr;
    }
    r = r.min(allowed);
    [r as u8, g as u8, b as u8]
}

fn ember_linear_byte(s: u8) -> u32 {
    let v = s as f64 / 255.;
    ((if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }) * 4096.)
        .round() as u32
}

fn ember_glow_reference(silk: &Silk, exposure: f64) -> [f64; 3] {
    let rgb = [
        silk.halo[0] as f64,
        silk.halo[1] as f64,
        silk.halo[2] as f64,
    ];
    if silk.accent[3] >= -1.5 {
        return rgb.map(|c| c * exposure);
    }
    let emitted = rgb[0] * exposure;
    let balanced = silk.accent[3] < -3.5;
    let colour = if silk.accent[3] < -2.5 {
        [
            emitted,
            (rgb[1] * exposure).max(emitted.min(1. / 255.)),
            if balanced {
                (rgb[2] * exposure).max(emitted.min(1. / 255.))
            } else {
                rgb[2] * exposure
            },
        ]
    } else {
        // Exact saved float-warm6 selector, used by the negative control.
        [
            emitted,
            (0.4202 * emitted).max(emitted.min(1. / 255.)),
            0.3562 * emitted,
        ]
    };
    if silk.accent[3] < -2.5 {
        let bytes = colour.map(|v| (v * 255.).round().clamp(0., 255.) as u8);
        let weights = if balanced {
            [777, 1000, 1000]
        } else {
            [709, 1000, 817]
        };
        return ember_integer_update(bytes, weights).map(|v| v as f64 / 255.);
    }
    colour
}

#[test]
fn ember_glow_quantisation_does_not_leave_single_byte_red() {
    let mut cfg = input(1, 2560., 1440.);
    cfg.palette = 2;
    cfg.trails = 1.;
    let art = Art::new(&cfg);
    let linear_byte = |s: u8| {
        let v = s as f64 / 255.;
        let linear = if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        };
        (linear * 4096.).round() as u32
    };
    let mut samples = 0;
    for silk in art.0.iter().flatten() {
        let old_y = silk.halo[0] as f64 * 0.2126
            + silk.halo[1] as f64 * 0.7152
            + silk.halo[2] as f64 * 0.0722;
        for step in 0..=4096 {
            let exposure = step as f64 / (4096. * old_y);
            let colour = ember_glow_reference(silk, exposure);
            let bytes = colour.map(|v| (v * 255.).round().clamp(0., 255.) as u8);
            let [r, g, b] = bytes.map(linear_byte);
            assert!(
                r == 0 || r < 4 * (g + b),
                "halo is saturated red: {bytes:?}"
            );
            let old_red = (silk.halo[0] as f64 * exposure * 255.)
                .round()
                .clamp(0., 255.) as u8;
            assert!(
                bytes[0] <= old_red && old_red - bytes[0] <= 1,
                "integer guard changed more than one red byte"
            );
            let y = colour[0] * 0.2126 + colour[1] * 0.7152 + colour[2] * 0.0722;
            assert!(
                bytes[0] >= bytes[1] && bytes[1] >= bytes[2],
                "red hue changed: {bytes:?}"
            );
            if y >= 2. / 255. && y <= 20. / 255. {
                let saturation = ember_hsv_saturation(bytes);
                assert!(
                    saturation >= 0.45,
                    "visible halo lost red chroma: {bytes:?}"
                );
            }
            if (16..=128).contains(&bytes[0]) {
                let chroma = (bytes[0] - bytes[2]) as f64;
                let hue = 60. * (bytes[1] - bytes[2]) as f64 / chroma.max(1.);
                let saturation = chroma / bytes[0] as f64;
                assert!((0. ..=15.).contains(&hue), "warm halo hue: {bytes:?}");
                assert!(
                    (if silk.accent[3] < -3.5 {
                        0.55..=0.67
                    } else {
                        0.60..=0.70
                    })
                    .contains(&saturation),
                    "warm halo chroma: {bytes:?}"
                );
            }
            samples += 1;
        }
        // Guarded emissions are on the native byte grid; addition is exact.
        let levels: Vec<_> = (0..=128)
            .map(|i| ember_glow_reference(silk, i as f64 / 128.))
            .collect();
        for a in &levels {
            for b in &levels {
                let bytes = std::array::from_fn::<_, 3, _>(|i| {
                    ((a[i] + b[i]) * 255.).round().clamp(0., 255.) as u8
                });
                let [r, g, b] = bytes.map(linear_byte);
                assert!(
                    r == 0 || r < 4 * (g + b),
                    "overlapping halos became saturated red: {bytes:?}"
                );
            }
        }
    }
    cfg.palette = 5;
    cfg.environment.background = 0;
    assert!(Art::new(&cfg)
        .0
        .iter()
        .flatten()
        .all(|s| s.accent[3] == -1.));
    println!(
        "Ember native RGB8: {samples} samples, zero saturated-red halo colours; red hue and overlaps preserved; red correction at most one native byte"
    );
}

#[test]
fn ember_glow_selector_preserves_rendered_motion() {
    let mut cfg = input(1, 2560., 1440.);
    cfg.seed = 1;
    cfg.mode = MonitorMode::Spanning;
    cfg.speed = 3.;
    cfg.density = 80.;
    cfg.scale = 2.;
    cfg.trails = 1.;
    cfg.palette = 2;
    cfg.environment.background = 0;
    cfg.layout = Layout::new(
        [
            (0., 0., 2560., 1440.),
            (2560., 0., 3440., 1440.),
            (6000., 195., 1920., 1080.),
        ]
        .into_iter()
        .enumerate()
        .map(|(id, (x, y, width, height))| OutputRect {
            id: id as u64,
            rect: Rect {
                x,
                y,
                width,
                height,
            },
            primary: id == 0,
        })
        .collect(),
        1,
    )
    .unwrap();
    let mut old_art = Art::new(&cfg);
    let old_halo =
        super::super::material::adjust_oklab(crate::visuals::palette::color(2, 3), 1.25, 0.85);
    for silk in old_art.0.iter_mut().flatten() {
        silk.halo[..3].copy_from_slice(&old_halo[..3]);
        silk.accent[3] = 1.;
    }
    let mut world = Ribbons::new(cfg);
    world
        .inject(Event {
            id: 1,
            at_ns: 602_000_000_000,
            anchor: [0., 720.],
            strength: 1.,
        })
        .unwrap();
    let mut before = Frame::new(Arc::new(old_art), 40, false);
    let mut after = Frame::new(world.art.clone(), 40, false);
    let motion = |frame: &Frame| -> Vec<[u32; 8]> {
        frame
            .vertices
            .iter()
            .map(|n| {
                [
                    n.pos[0],
                    n.pos[1],
                    n.normal[0],
                    n.normal[1],
                    n.half_width,
                    n.facing,
                    n.pulse,
                    n.alpha,
                ]
                .map(f32::to_bits)
            })
            .collect()
    };
    let mut samples = 0;
    for tick in 1..=19110 {
        world.step(tick);
        if tick < 18000 || tick % 15 != 0 {
            continue;
        }
        before.fill(&world.sea, world.params.twist);
        after.fill(&world.sea, world.params.twist);
        assert_eq!(motion(&before), motion(&after));
        assert_eq!(before.caps, after.caps);
        assert_eq!(before.strips.len(), after.strips.len());
        for (a, b) in before.strips.iter().zip(&after.strips) {
            assert_eq!(
                (
                    a.id,
                    a.offset,
                    a.count,
                    a.layer,
                    a.hue,
                    a.margin,
                    a.fitted,
                    a.nominal,
                    a.tip_radius,
                    a.shoulder_gain,
                    a.bounds
                ),
                (
                    b.id,
                    b.offset,
                    b.count,
                    b.layer,
                    b.hue,
                    b.margin,
                    b.fitted,
                    b.nominal,
                    b.tip_radius,
                    b.shoulder_gain,
                    b.bounds
                )
            );
        }
        assert_eq!(before.sparks.len(), after.sparks.len());
        for (a, b) in before.sparks.iter().zip(&after.sparks) {
            assert_eq!(a.id, b.id);
            assert_eq!(a.vertex, b.vertex);
        }
        samples += 1;
    }
    assert_eq!(samples, 75);
    println!(
        "Ember content selector: {samples} complete L3 frames, all motion/caps/shoulder fits/sparks bit-identical"
    );
}

#[test]
fn ember_rgb8_blend_updates_stay_safe_after_repeated_overlaps() {
    let mut cfg = input(1, 2560., 1440.);
    cfg.palette = 2;
    cfg.trails = 1.;
    let art = Art::new(&cfg);
    for silk in art.0.iter().flatten() {
        let exposure = 3.4 / (255. * silk.halo[0] as f64);
        let increment =
            ember_glow_reference(silk, exposure).map(|v| (v * 255.).round().clamp(0., 255.) as u8);
        let mut pixel = [0u8; 3];
        for count in 1..=85 {
            for channel in 0..3 {
                pixel[channel] = pixel[channel].saturating_add(increment[channel]);
            }
            let [r, g, b] = pixel.map(ember_linear_byte);
            assert!(
                r == 0 || r < 4 * (g + b),
                "RGB8 repeated update {count} became saturated: {pixel:?}, increment={increment:?}"
            );
        }
    }
}

#[test]
fn ember_additive_integer_cones_exclude_every_saturated_native_colour() {
    let lut: Vec<_> = (0..=255).map(ember_linear_byte).collect();
    for [wr, wg, wb] in [[777u32, 1000, 1000], [709, 1000, 817]] {
        assert!(wg >= wr && wb >= wr); // Clamping either side to255 also preserves the cone.
        for r in 1..=255u32 {
            for g in 0..=255u32 {
                for b in 0..=255u32 {
                    if wg * g + wb * b >= wr * r {
                        assert!(
                            lut[r as usize] < 4 * (lut[g as usize] + lut[b as usize]),
                            "unsafe cone colour {:?}",
                            [r, g, b]
                        );
                    }
                }
            }
        }
    }
}
