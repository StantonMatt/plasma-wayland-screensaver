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
#[ignore = "matched full-cohort Gust material excursion and streaming metrics"]
fn gust_visibility_metrics() {
    for (name, seed, spanning, onset) in [
        ("real-spanning", 1, true, 602_u64),
        ("owner-spanning", 1, true, 602_u64),
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
        if name == "owner-spanning" {
            cfg.density = 30.;
            cfg.scale = 0.65;
            cfg.trails = 0.5;
            cfg.parameters = vec![
                crate::visuals::depth::schema::Value {
                    id: 1,
                    kind: 1,
                    payload: 10,
                },
                crate::visuals::depth::schema::Value {
                    id: 2,
                    kind: 1,
                    payload: 100,
                },
            ];
        }
        let mut w = Ribbons::new(cfg);
        let mut shape = Frame::new(w.art.clone(), 40, false);
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
            if spanning && ((onset + 2) * 30..=(onset + 6) * 30).contains(&tick) && tick % 30 == 0 {
                shape.fill(&w.sea, w.params.twist);
                let (mut visible, mut straight) = (0, 0);
                for strip in &shape.strips {
                    let points: Vec<_> = shape.vertices[strip.offset..strip.offset + strip.count]
                        .iter()
                        .map(|v| p(v.pos[0] as f64, v.pos[1] as f64))
                        .filter(|p| w.input.layout.contains(*p))
                        .collect();
                    if points.len() < 6 {
                        continue;
                    }
                    let arc: f64 = points.windows(2).map(|p| (p[1] - p[0]).length()).sum();
                    if arc < 240. {
                        continue;
                    }
                    let chord = (*points.last().unwrap() - points[0]).unit();
                    let deviation = points
                        .iter()
                        .map(|v| (*v - points[0]).dot(chord.perpendicular()).abs())
                        .fold(0_f64, f64::max);
                    visible += 1;
                    straight += usize::from(deviation < (0.02 * arc).max(12.));
                }
                println!(
                    "GUST_BENDS {name}: filmSecond={},visible={visible},straight={straight}",
                    tick / 30 - 600
                );
            }
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
            "GUST_FULL_COHORT {name}: onset={onset},lanes={},initialVisiblePoints={},xExcursionP10={:.6},xExcursionP50={:.6},xExcursionP90={:.6},yExcursionP50={:.6},headSamples={total},horizontalWithin10deg={:.9},eastFraction={:.9},headSpeedP50={:.6},headSpeedP99={:.6},headSpeedMax={:.6}",
            tracks.len(),
            xs.len(),
            q(&xs, 0.1),
            q(&xs, 0.5),
            q(&xs, 0.9),
            q(&ys, 0.5),
            aligned as f64 / total as f64,
            east as f64 / total as f64,
            q(&speeds, 0.5),
            q(&speeds, 0.99),
            q(&speeds, 1.)
        );
    }
}

#[test]
fn gust_total_carry_shoulders_a_strong_inherited_motor_smoothly() {
    let mut w = Ribbons::new(input(1, 7920., 1440.));
    w.params.world_events = false;
    w.inject(Event {
        id: 1,
        at_ns: 0,
        anchor: [0., 720.],
        strength: 1.,
    })
    .unwrap();
    for tick in 1..=90 {
        w.step(tick);
    }
    let front = w.sea.weather.active.unwrap();
    let lanes = super::super::water::Lanes::new(w.input.layout.bounds);
    let sea = &mut w.sea;
    let r = sea.agents.iter_mut().find(|r| r.alive).unwrap();
    r.layer = 1;
    let mut previous = 0.;
    for motor in [100., 150., 250., 350., 500.] {
        r.speed = motor;
        r.heading = p(1., 0.);
        let before = r.head();
        r.swim(
            super::super::agent::Steering {
                desired: p(motor, 0.),
                curl: 0.,
                gust: 1.,
                launch: 0.,
                gust_front: Some(super::super::agent::GustDrive {
                    front,
                    left: 0.,
                    right: 7920.,
                    carry: 1.,
                    force: 150. / 395.,
                    world_rate: 3.,
                }),
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
            0.1,
            false,
        );
        let speed = (r.head() - before).length() * 30.;
        println!("Gust inherited motor={motor}: carry={speed:.6} world px/s");
        assert!(speed > previous && speed < 900.);
        previous = speed;
    }
    assert!(previous > 880., "the shoulder must retain a lively Gust");
}

#[test]
fn gust_turns_upstream_swimmers_downwind() {
    for speed in [1., 3.] {
        // Exercise the open-water carry separately from shore banking. A
        // downstream swimmer is now expected to turn upstream near the edge.
        let mut cfg = input(1, 7920., 1440.);
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
            for (_, r) in w
                .sea
                .agents
                .iter()
                .enumerate()
                .filter(|(i, r)| r.alive && upstream.contains(&r.id) && !w.sea.gust_banking(*i))
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
    }
}

#[test]
fn ordinary_flow_keeps_its_motor_ceiling_under_strong_steering() {
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
                gust: 0.,
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
        "Ordinary motor after four seconds of strong steering: {:.9}",
        r.speed
    );
    assert!(r.speed > 220. && r.speed <= 230.000001);
}

#[test]
fn maelstrom_orbit_and_release_remain_lively_at_high_speed() {
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
        // Keep a motion floor rather than the retired screen-space light budget.
        assert!(peak > 235. * speed);
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
    // Keep this release witness independent of unrelated earlier Gusts. Shore
    // banking changes the ambient cast at 602s; the scripted Maelstrom must
    // still exercise a substantial visible cohort with the same recoil limit.
    w.params.world_events = false;
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
