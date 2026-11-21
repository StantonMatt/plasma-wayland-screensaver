use super::*;
use common::layout::{Layout, OutputRect, Rect};
fn input(boxes: &[(f64, f64, f64, f64)], count: i32, gravity: i32) -> WorldInput {
    let outputs = boxes
        .iter()
        .enumerate()
        .map(|(i, &(x, y, width, height))| OutputRect {
            id: i as u64,
            primary: i == 0,
            rect: Rect {
                x,
                y,
                width,
                height,
            },
        })
        .collect();
    let param = |id, x: i32| super::super::schema::Value {
        id,
        kind: 1,
        payload: x as i64 as u64,
    };
    WorldInput {
        visual_id: 7,
        seed: 19,
        mode: world::MonitorMode::Spanning,
        layout: Layout::new(outputs, 1).unwrap(),
        speed: 1.,
        density: 100.,
        scale: 1.,
        trails: 1.,
        palette: 0,
        reduced_motion: false,
        parameters: vec![param(1, count), param(2, gravity)],
        environment: Default::default(),
    }
}
const L3: &[(f64, f64, f64, f64)] = &[
    (0., 0., 2560., 1440.),
    (2560., 0., 3440., 1440.),
    (6000., 195., 1920., 1080.),
];
#[test]
fn swept_ledge_open_seam_and_corner() {
    let w = BounceWorld::new(input(L3, 1, 0));
    let r = &w.rooms[0];
    assert!(r.sweep(v(5900., 800.), v(1200., 0.), 24.).is_none());
    let (t, n, _) = r.sweep(v(5800., 1360.), v(1600., 0.), 24.).unwrap();
    assert!((t - 176. / 1600.).abs() < 1e-9);
    assert_eq!(n, v(-1., 0.));
    let (_, n, _) = r.sweep(v(5800., 1260.), v(400., 0.), 24.).unwrap();
    assert!(n.x < 0. && n.y < 0.);
    let (_, n, wall) = r.sweep(v(6500., 1100.), v(0., 900.), 24.).unwrap();
    assert_eq!(n, v(0., -1.));
    assert_eq!(wall.a.y, 1275.);
}
#[test]
fn cradle_and_spin() {
    let mut w = BounceWorld::new(input(&[(0., 0., 1600., 800.)], 5, 0));
    for (i, b) in w.balls.iter_mut().enumerate() {
        *b = Ball::new(&mut w.rng, 0, 30., Material::Rubber, i);
        b.p = v(200. + 60. * i as f64, 350.);
        b.v = V::default();
        b.spin = 0.;
    }
    w.params.elasticity = 100;
    w.balls[0].v = v(460., 0.);
    w.integrate(V::default());
    assert!(w.balls[4].v.x > 410., "{}", w.balls[4].v.x);
    assert!(w.balls[0].v.x.abs() < 15.);
    let ball = &mut w.balls[0];
    ball.v = v(240., 280.);
    let wall = w.rooms[0].walls.iter().find(|w| w.inward.y < 0.).unwrap();
    ball.wall_impulse(wall.inward, *wall, 0.9, 800.);
    assert!(ball.spin.abs() > 0.2);
}
#[test]
fn rest_charge_hop_and_stacking() {
    let mut w = BounceWorld::new(input(&[(0., 0., 700., 500.)], 2, 35));
    for (i, b) in w.balls.iter_mut().enumerate() {
        *b = Ball::new(&mut w.rng, 0, 30., Material::Metal, i);
        b.p = v(350., 470. - 60. * i as f64);
        b.v = V::default();
        b.spin = 0.;
        b.period = 7.;
    }
    for tick in 1..240 {
        w.step(tick);
    }
    assert!(
        w.balls.iter().all(|b| b.sleeping),
        "states {:?}",
        w.balls
            .iter()
            .map(|b| (b.p, b.v, b.rest, b.charge))
            .collect::<Vec<_>>()
    );
    assert!((w.balls[0].p.y - w.balls[1].p.y - 60.).abs() < 3.);
    let mut hopped = false;
    for t in 240..1400 {
        w.step(t);
        hopped |= w.balls.iter().any(|b| b.v.y < -100.);
    }
    assert!(hopped);
}
#[test]
fn islands_soak_and_still() {
    let boxes = [L3[0], L3[1], L3[2], (8300., 100., 620., 480.)];
    let mut config = input(&boxes, 60, 35);
    config.speed = 3.;
    config.scale = 2.;
    let mut w = BounceWorld::new(config.clone());
    assert_eq!(w.rooms.len(), 2);
    assert!(w.balls.iter().any(|b| b.room == 1));
    for t in 1..1800 {
        w.step(t);
        if t % 4 == 0 {
            for b in &w.balls {
                assert!(
                    w.rooms[b.room]
                        .outputs
                        .iter()
                        .any(|o| o.contains(b.p.into())),
                    "escaped room {} {:?}",
                    b.room,
                    b.p
                );
                assert!(
                    w.input.layout.circle_fits(b.p.into(), (b.r - 0.1).max(0.)),
                    "t={t}, p={:?},r={}",
                    b.p,
                    b.r
                );
                assert!(b.v.len() < 2.2 * w.height + 0.01);
            }
        }
    }
    config.reduced_motion = true;
    let mut w = BounceWorld::new(config.clone());
    let other = BounceWorld::new(config);
    let initial = w.state_hash();
    assert_eq!(initial, other.state_hash());
    w.step(3600);
    assert_eq!(initial, w.state_hash());
    assert!(w.balls.iter().all(|b| b.trail.length == 0 && b.sleeping));
}
#[test]
fn event_times_are_wall_time() {
    for speed in [0.1, 3.] {
        let mut config = input(L3, 5, 0);
        config.speed = speed;
        let mut w = BounceWorld::new(config);
        w.inject(Event {
            id: 2,
            at_ns: 1_000_000_000,
            anchor: [400., 500.],
            strength: 1.,
        })
        .unwrap();
        for t in 1..120 {
            w.step(t);
        }
        assert!(w.moment.is_none());
        w.step(120);
        assert_eq!(w.moment.unwrap().kind, events::Kind::Float);
        w.step(720);
        assert!(w.gravity(6.).x.abs() > 0.);
    }
}
#[test]
fn right_tilt_carries_balls_onto_the_balcony() {
    let mut config = input(L3, 20, 35);
    config.seed = 21;
    let mut w = BounceWorld::new(config);
    w.inject(Event {
        id: 1,
        at_ns: 30_000_000_000,
        anchor: [5800., 1200.],
        strength: 1.,
    })
    .unwrap();
    for tick in 1..=3600 {
        w.step(tick);
    }
    let initial: Vec<_> = w.balls.iter().map(|b| b.p.x).collect();
    for tick in 3601..=6000 {
        w.step(tick);
    }
    let crossed = w
        .balls
        .iter()
        .zip(initial)
        .filter(|(b, x)| *x < 6000. && b.p.x > 6000.)
        .count();
    assert!(
        crossed > 0,
        "right tilt never moved a ball up the 165 px step"
    );
}

#[test]
fn recording_has_time_for_the_return_to_gravity() {
    let mut config = input(&[L3[0]], 12, 35);
    config.seed = common::rng::output_seed(7, 7, "Virtual-0");
    let mut w = BounceWorld::new(config);
    let mut natural = None;
    let mut cascade = Vec::new();
    for (id, second, strength) in [(5, 15, 1.), (1, 20, 1.), (2, 45, 0.8)] {
        w.inject(Event {
            id,
            at_ns: second * 1_000_000_000,
            anchor: [1280., 720.],
            strength,
        })
        .unwrap();
    }
    for tick in 1..=9000 {
        let charged: [bool; 12] = std::array::from_fn(|i| {
            let b = &w.balls[i];
            b.sleeping && b.charge > b.period * 0.5
        });
        w.step(tick);
        for (i, b) in w.balls.iter().enumerate() {
            if charged[i] && !b.sleeping && b.v.y < -100. {
                if (1800..2040).contains(&tick) {
                    cascade.push((tick, i));
                } else if w.moment.is_none() && natural.is_none() {
                    natural = Some(tick);
                    let hero = &w.balls[i];
                    println!(
                        "First charged hop ball {i}, center {:.3},{:.3}, radius {:.3}",
                        hero.p.x, hero.p.y, hero.r
                    );
                }
            }
        }
        if tick == 13 * 120 {
            let hero = w.balls.iter().max_by(|a, b| a.r.total_cmp(&b.r)).unwrap();
            println!(
                "Resting hero at 13 s: center {:.3},{:.3}, radius {:.3}, charge {:.3}/{:.3}",
                hero.p.x, hero.p.y, hero.r, hero.charge, hero.period
            );
        }
        if tick == 44 * 120 || tick == 71 * 120 {
            assert!(w.moment.is_none());
            assert_eq!(w.gravity(tick as f64 / 120.).y, w.base_g);
        }
        if tick == 55 * 120 {
            assert_eq!(w.gravity(55.).y, 0.);
        }
    }
    println!("recording natural charged hop at tick {natural:?}, cascade wakes {cascade:?}");
    assert!(natural.is_some());
    let woken: std::collections::BTreeSet<_> = cascade.iter().map(|&(_, ball)| ball).collect();
    assert!(woken.len() >= 3);
}
#[test]
#[ignore = "release timing and 30 minute stepped-world energy soak"]
fn stress_soak() {
    use std::time::Instant;
    let mut config = input(L3, 60, 35);
    config.scale = 2.;
    config.speed = 3.;
    for elasticity in [50, 100] {
        config.parameters.retain(|p| p.id != 3);
        config.parameters.push(super::super::schema::Value {
            id: 3,
            kind: 1,
            payload: elasticity,
        });
        let start = Instant::now();
        let mut w = BounceWorld::new(config.clone());
        let activation = start.elapsed();
        let mut samples = Vec::with_capacity(216_000);
        let mut peaks = 0f64;
        let mut hops = 0;
        let mut peak_mass_speed = 0f64;
        let mut frames = std::collections::VecDeque::with_capacity(6);
        for tick in 1..=216_000 {
            let before: [bool; 61] =
                std::array::from_fn(|i| w.balls.get(i).is_some_and(|b| b.sleeping));
            let start = Instant::now();
            w.step(tick);
            if frames.len() == 6 {
                frames.pop_front();
            }
            frames.push_back(w.snapshot());
            samples.push(start.elapsed().as_nanos());
            for (i, b) in w.balls.iter().enumerate() {
                assert!(
                    b.v.len2().is_finite()
                        && b.spin.is_finite()
                        && b.p.x.is_finite()
                        && b.p.y.is_finite()
                );
                assert!(
                    w.input.layout.circle_fits(b.p.into(), (b.r - 0.2).max(0.)),
                    "escape {tick} {:?}, {}",
                    b.p,
                    b.r
                );
                peaks = peaks.max(b.v.len() / w.height);
                hops +=
                    usize::from(before.get(i) == Some(&true) && !b.sleeping && b.v.len() > 100.);
            }
            let energy = w.balls.iter().map(|b| b.v.len2() / b.inv_mass).sum::<f64>();
            peak_mass_speed = peak_mass_speed.max(energy / w.kinetic_limit);
        }
        samples.sort_unstable();
        println!(
            "elasticity={elasticity} ticks=216000, speed=300, create={activation:?}, p99={} ns, max={} ns, peak speed={peaks:.3} heights/s, peak translational energy/nominal={peak_mass_speed:.2}, hops={hops}",
            samples[samples.len() * 99 / 100 - 1],
            samples.last().unwrap()
        );
        assert!(peaks <= 2.201 && hops > 0 && peak_mass_speed < 12.);
    }
}
#[test]
fn seam_records_and_replay() {
    let config = input(L3, 1, 0);
    let mut w = BounceWorld::new(config.clone());
    let mut replay = BounceWorld::new(config);
    for world in [&mut w, &mut replay] {
        world.balls[0].p = v(2549., 700.);
        world.balls[0].v = v(160., 0.);
        for t in 1..12 {
            world.step(t);
        }
    }
    let left = w.input.layout.outputs[0].rect;
    let right = w.input.layout.outputs[1].rect;
    let (a, ia, _) = render::test_records(&w, left);
    let (b, ib, _) = render::test_records(&w, right);
    assert_eq!(a[ia..], b[ib..]);
    assert_eq!(a.len() - ia, 1);
    assert!(a[ia].geom[0] > 2560.);
    assert_eq!(a, render::test_records(&replay, left).0);
}
#[test]
fn upload_and_additive_coverage() {
    let boxes = [(0., 0., 3440., 1440.)];
    let mut config = input(&boxes, 60, 35);
    config.speed = 3.;
    config.scale = 2.;
    config.trails = 1.;
    let rect = config.layout.bounds;
    let mut w = BounceWorld::new(config);
    let mut prior = [0u64; 61];
    let mut peak_area = 0f64;
    let mut peak_bytes = 0;
    for t in 1..=7200 {
        w.step(t);
        if t % 2 != 0 {
            continue;
        }
        let (records, _, area) = render::test_records(&w, rect);
        let mut bytes = records.len() * 64;
        for r in records.iter().filter(|r| r.look[3] == 5.) {
            let i = r.axis[2] as usize;
            let serial = w.balls[i].trail.serial;
            bytes += size_of::<Knot>() * (serial - prior[i]).min(32) as usize;
            prior[i] = serial;
        }
        peak_area = peak_area.max(area);
        peak_bytes = peak_bytes.max(bytes);
    }
    println!(
        "60 balls 3440x1440 speed 300 scale 200 trails 100; max dynamic bytes={peak_bytes} max additive area={peak_area:.3}x"
    );
    assert!(peak_bytes <= 48 * 1024);
    assert!(peak_area <= 1.5);
}

#[test]
fn glancing_hit_spins_light_ball_and_charged_contacts_hop() {
    let mut w = BounceWorld::new(input(&[(0., 0., 1000., 800.)], 2, 35));
    for (i, b) in w.balls.iter_mut().enumerate() {
        *b = Ball::new(
            &mut w.rng,
            0,
            35.,
            if i == 0 {
                Material::Metal
            } else {
                Material::Glass
            },
            i,
        );
        b.p = v(400. + 70. * i as f64, 400.);
        b.v = V::default();
        b.spin = 0.;
    }
    w.balls[0].v = v(200., 140.);
    w.integrate(v(0., 0.));
    assert!(w.balls[1].spin.abs() > 0.35);
    assert!(w.balls[1].v.x > w.balls[0].v.x);
    for (i, b) in w.balls.iter_mut().enumerate() {
        b.p = v(400. + 70. * i as f64, 765.);
        b.v = V::default();
        b.spin = 0.;
    }
    w.balls[0].v = v(200., 0.);
    w.balls[1].sleeping = true;
    w.balls[1].charge = w.balls[1].period * 0.8;
    w.integrate(v(0., w.base_g));
    assert!(!w.balls[1].sleeping && w.balls[1].v.y < -100.);
}

#[test]
fn material_quotas_and_wall_time_wobble() {
    for (count, quota) in [(3, [1, 1, 1]), (12, [6, 4, 2]), (20, [10, 6, 4])] {
        let w = BounceWorld::new(input(&[L3[0]], count, 0));
        let mut counts = [0; 3];
        for b in &w.balls {
            counts[b.mat as usize] += 1;
        }
        assert_eq!(counts, quota);
    }
    let curve = |substeps, material, pair| {
        let w = BounceWorld::new(input(&[L3[0]], 1, 0));
        let mut ball = w.balls[0].clone();
        ball.set_material(material);
        ball.strike(v(0., -1.), 1440., 1440., pair);
        assert_eq!(ball.squash, 0.);
        let dt = 1. / (120. * f64::from(substeps));
        let decay = Relaxation::new(dt);
        let mut curve = Vec::new();
        for _ in 0..72 {
            for _ in 0..substeps {
                ball.relax_light(dt, decay);
            }
            curve.push(ball.squash);
        }
        curve
    };
    let rubber = curve(1, Material::Rubber, false);
    for (a, b) in rubber.iter().zip(curve(3, Material::Rubber, false)) {
        assert!((a - b).abs() < 1e-10);
    }
    let (peak, maximum) = rubber
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .unwrap();
    assert!(peak <= 2 && (0.21..0.225).contains(maximum));
    assert!(rubber.windows(2).filter(|p| p[0] * p[1] < 0.).count() >= 4);
    assert!(curve(1, Material::Glass, false).iter().all(|&x| x == 0.));
    assert!((curve(1, Material::Metal, false)[peak] - 0.04).abs() < 0.002);
    assert!((curve(1, Material::Rubber, true)[peak] / maximum - 0.4).abs() < 1e-6);
}

#[test]
fn float_entry_stays_in_motion() {
    let mut config = input(&[(0., 0., 1920., 1080.)], 16, 35);
    config.seed = common::rng::output_seed(5, 7, "Virtual-0");
    let mut w = BounceWorld::new(config);
    w.inject(Event {
        id: 2,
        at_ns: 5_000_000_000,
        anchor: [1280., 720.],
        strength: 1.,
    })
    .unwrap();
    for tick in 1..=1800 {
        w.step(tick);
    }
    let mut speeds = w.balls.iter().map(|b| b.v.len()).collect::<Vec<_>>();
    speeds.sort_by(f64::total_cmp);
    let ceiling = w.balls.iter().filter(|b| b.p.y < 270.).count();
    println!(
        "scene E at 15s median={:.2}px/s, upper quartile={ceiling}/16",
        speeds[8]
    );
    assert!(speeds[8] >= 120. && ceiling < 12);
}

#[test]
fn heavy_drop_displaces_the_pit() {
    let mut config = input(&[L3[0]], 60, 60);
    config.seed = common::rng::output_seed(11, 7, "Virtual-0");
    config.scale = 0.6;
    config.parameters.push(super::super::schema::Value {
        id: 3,
        kind: 1,
        payload: 80,
    });
    let mut w = BounceWorld::new(config);
    w.inject(Event {
        id: 3,
        at_ns: 40_000_000_000,
        anchor: [1280., 720.],
        strength: 1.,
    })
    .unwrap();
    let mut contact = None;
    let mut fastest = [0f64; 60];
    let mut near = [false; 60];
    let mut deep = 0f64;
    for tick in 1..=5400 {
        w.step(tick);
        if w.balls.len() != 61 {
            continue;
        }
        let drop = &w.balls[60];
        if !drop.landed {
            continue;
        }
        let (first, initial_y) = *contact.get_or_insert_with(|| {
            for (i, b) in w.balls[..60].iter().enumerate() {
                near[i] = (b.p - drop.p).len() < 2.7 * drop.r + b.r;
            }
            (tick, drop.p.y)
        });
        deep = deep.max(drop.p.y - initial_y);
        if tick - first < 90 {
            for (i, b) in w.balls[..60].iter().enumerate() {
                if near[i] {
                    fastest[i] = fastest[i].max(-b.v.y);
                }
            }
        }
    }
    let throws = fastest.iter().filter(|&&speed| speed > 300.).count();
    println!(
        "scene C impact {contact:?}, penetration={deep:.1}px, median diameter={:.1}px, upward launches over 300px/s={throws}",
        2. * w.medians[0]
    );
    assert!(contact.is_some() && deep >= 2. * w.medians[0] && (3..=6).contains(&throws));
}

#[test]
fn restitution_extremes_and_lifted_crimson() {
    for elasticity in [50, 100] {
        let mut config = input(&[L3[0]], 12, 35);
        config.seed = common::rng::output_seed(7, 7, "Virtual-0");
        config.parameters.push(super::super::schema::Value {
            id: 3,
            kind: 1,
            payload: elasticity,
        });
        let mut w = BounceWorld::new(config);
        let mut first_all = None;
        for tick in 1..=840 {
            w.step(tick);
            if tick == 360 && elasticity == 50 {
                println!(
                    "soft at 3s: {} sleepers",
                    w.balls.iter().filter(|b| b.sleeping).count()
                );
            }
            if elasticity == 100 {
                assert!(
                    w.balls
                        .iter()
                        .filter(|b| b.mat == Material::Rubber)
                        .all(|b| !b.sleeping)
                );
            } else if w.balls.iter().all(|b| b.sleeping) {
                first_all.get_or_insert(tick);
            }
        }
        if elasticity == 50 {
            println!("soft scene all asleep at {first_all:?} ticks");
            assert!(first_all.is_some_and(|tick| tick <= 440));
        }
    }
    let colour = paint::packed(crate::visuals::palette::color(2, 4), true) as u32;
    let [r, g, b] =
        [(colour >> 16) & 255, (colour >> 8) & 255, colour & 255].map(|c| f64::from(c) / 255.);
    let l = |x: f64| ((x + 0.055) / 1.055).powf(2.4);
    let y = 0.2126 * l(r) + 0.7152 * l(g) + 0.0722 * l(b);
    println!("lifted crimson #{colour:06x} relative luminance {y:.4}");
    assert!(y >= 0.30 && r - g > 0.45 && (g - b).abs() < 0.14);
}

#[test]
fn halo_follows_the_compressed_surface() {
    let mut w = BounceWorld::new(input(&[L3[0]], 1, 0));
    let b = &mut w.balls[0];
    b.p = v(1200., 1000.);
    b.v = V::default();
    b.axis = v(0., -1.);
    b.contact_normal = b.axis;
    b.squash = 0.22;
    let r = b.r;
    let (records, _, _) = render::test_records(&w, w.input.layout.bounds);
    let halo = records.iter().find(|rec| rec.look[3] == 1.).unwrap();
    let close = |value: f32, expected: f64| (f64::from(value) - expected).abs() < 0.001;
    assert!(close(halo.geom[0], 1200.) && close(halo.geom[1], 1000. + 0.22 * r));
    assert!(close(halo.geom[2], 1.8 * r * 0.78) && close(halo.geom[3], 1.8 * r / 0.78));
    assert_eq!(&halo.axis[..2], &[0., -1.]);
}

#[test]
fn floating_contacts_release_and_impact_light_has_a_wall_time_attack() {
    let mut config = input(&[L3[0]], 12, 35);
    config.seed = common::rng::output_seed(7, 7, "Virtual-0");
    let mut w = BounceWorld::new(config);
    for (id, second, strength) in [(5, 15, 1.), (1, 20, 1.), (2, 45, 0.8)] {
        w.inject(Event {
            id,
            at_ns: second * 1_000_000_000,
            anchor: [1280., 720.],
            strength,
        })
        .unwrap();
    }
    let mut durations = [[0u32; 12]; 12];
    let mut longest = 0;
    let mut wall_durations = [0u32; 12];
    let mut longest_wall = 0;
    for tick in 1..=70 * 120 {
        w.step(tick);
        if tick < 45 * 120 || w.gravity(tick as f64 / 120.).len2() >= 4. {
            durations.fill([0; 12]);
            wall_durations.fill(0);
            continue;
        }
        for (i, a) in w.balls.iter().enumerate() {
            let wall = &mut wall_durations[i];
            *wall = if w.rooms[a.room].nearest(a.p).2 <= a.r + 0.5 {
                *wall + 1
            } else {
                0
            };
            longest_wall = longest_wall.max(*wall);
            for (j, b) in w.balls.iter().enumerate().skip(i + 1) {
                let count = &mut durations[i][j];
                *count = if (a.p - b.p).len2() <= (a.r + b.r + 0.5).powi(2) {
                    *count + 1
                } else {
                    0
                };
                longest = longest.max(*count);
            }
        }
    }
    println!(
        "Longest continuous Zero-G contacts: pair {:.3} s and wall {:.3} s",
        longest as f64 / 120.,
        longest_wall as f64 / 120.
    );
    assert!(longest > 0 && longest <= 180);
    assert!(longest_wall > 0 && longest_wall <= 180);
    let b = &mut w.balls[0];
    b.light = 0.;
    b.flash_age = 1.;
    for _ in 0..100 {
        b.relax_light(1. / 120., Relaxation::new(1. / 120.));
    }
    b.strike(v(0., -1.), 1440., 1440., false);
    b.emit_contact_light(&mut w.impact_fields[b.room], 100.);
    assert_eq!(b.body_light(), 0.);
    assert!(b.glow() >= 0.99);
    for _ in 0..3 {
        b.relax_light(1. / 120., Relaxation::new(1. / 120.));
    }
    assert!((b.body_light() / b.light - 0.5).abs() < 1e-8);
    for _ in 0..3 {
        b.relax_light(1. / 120., Relaxation::new(1. / 120.));
    }
    assert!((b.body_light() - b.light).abs() < 1e-8);
}
#[test]
fn shared_field_groups_simultaneous_impacts_and_reopens_in_wall_time() {
    let w = BounceWorld::new(input(&[L3[0]], 4, 35));
    let mut field = ImpactField::new(&w.rooms[0]);
    let center = v(150., 1200.);
    assert!(field.admit(0., center, 80.));
    assert!(field.admit(0.02, center + v(100., 0.), 60.));
    assert!(!field.admit(0.48, center + v(850., 0.), 80.));
    assert!(field.admit(0.48, center + v(1650., 0.), 80.));
    assert!(field.admit(0.63, center, 80.));
}

#[test]
fn accelerated_crowds_share_the_swept_area_and_distant_balls_move_freely() {
    let mut config = input(&[L3[0]], 12, 35);
    config.speed = 3.;
    config.scale = 2.;
    let mut w = BounceWorld::new(config);
    for (i, ball) in w.balls.iter_mut().enumerate() {
        ball.p = v(200. + (i % 4) as f64 * 100., 950. + (i / 4) as f64 * 100.);
    }
    w.balls[0].p = v(2450., 120.);
    w.update_flow_limits();
    let max = 2.2 * w.height;
    assert!((w.balls[0].flow_limit - max).abs() < 1e-8);
    assert!(w.balls[6].flow_limit < max / 5.);
    w.input.speed = 1.;
    let mut ordinary = BounceWorld::new(w.input.clone());
    ordinary.update_flow_limits();
    assert!(ordinary.balls.iter().all(|b| b.flow_limit.is_infinite()));
}
#[test]
fn landing_compresses_at_the_wall_and_large_impacts_keep_their_halo() {
    for speed in [1., 3.] {
        let mut config = input(&[L3[0]], 1, 0);
        config.speed = speed;
        config.scale = 2.;
        let mut w = BounceWorld::new(config);
        let b = &mut w.balls[0];
        b.set_material(Material::Rubber);
        b.p = v(900., 1440. - b.r - 0.01);
        b.v = v(0., 1440.);
        b.spin = 0.;
        b.birth = -2.;
        let r = b.r;
        let mut max_squash = 0.;
        let mut gap = 0.;
        let mut peak_tick = 0;
        for tick in 1..=12 {
            w.step(tick);
            let (records, sphere, _) = render::test_records(&w, w.input.layout.bounds);
            let image = records[sphere];
            let k = f64::from(image.axis[2]);
            if k > max_squash {
                max_squash = k;
                peak_tick = tick;
                gap = 1440. - f64::from(image.geom[1]) - r;
            }
            if tick == 8 {
                let ratio = (108. / r).powi(2).clamp(0.3, 1.);
                let expected = 0.75 * w.balls[0].body_light() * ratio;
                assert!((f64::from(-image.look[3]) - expected).abs() < 1e-5);
                let halo = records.iter().find(|rec| rec.look[3] == 1.).unwrap();
                assert!((halo.colour[3] - 0.48 * w.balls[0].glow()).abs() < 0.001);
            }
        }
        println!(
            "Landing at {speed}x: peak tick {peak_tick}, compression {max_squash:.3}, floor gap {:.4} radii",
            gap / r
        );
        assert!(peak_tick <= 4 && max_squash > 0.19 && gap <= 0.12 * r);
    }
}

#[test]
fn floating_corner_releases_after_one_wall_second() {
    for speed in [1., 3.] {
        let mut config = input(&[L3[0]], 1, 0);
        config.speed = speed;
        let mut world = BounceWorld::new(config);
        let ball = &mut world.balls[0];
        let r = ball.r;
        ball.p = v(r, 1440. - r);
        ball.v = V::default();
        ball.spin = 0.;
        ball.sleeping = true;
        for tick in 1..120 {
            world.step(tick);
        }
        assert!(world.balls[0].sleeping);
        world.step(120);
        world.step(121);
        let ball = &world.balls[0];
        assert!(!ball.sleeping && (25. ..=40.).contains(&ball.v.len()));
        assert!(ball.v.x > 0. && ball.v.y < 0.);
        for tick in 122..=140 {
            world.step(tick);
        }
        assert!(world.balls[0].p.x > r + 2. && world.balls[0].p.y < 1440. - r - 2.);
    }
}

#[test]
fn fast_small_pair_cannot_tunnel() {
    let mut config = input(L3, 60, 35);
    config.speed = 3.;
    config.scale = 0.25;
    let mut w = BounceWorld::new(config);
    for (i, b) in w.balls.iter_mut().enumerate() {
        *b = Ball::new(&mut w.rng, 0, 12.15, Material::Rubber, 0);
        b.p = v(
            3000. + 100. * (i % 12) as f64,
            200. + 200. * (i / 12) as f64,
        );
        b.v = V::default();
        b.spin = 0.;
        b.birth = -2.;
        b.sleeping = true;
    }
    w.balls[0].p = v(987., 600.);
    w.balls[1].p = v(1013., 600.);
    w.balls[0].v = v(3168., 0.);
    w.balls[1].v = v(-3168., 0.);
    w.balls[0].sleeping = false;
    w.balls[1].sleeping = false;
    w.integrate(v(0., w.base_g));
    println!(
        "pair after one substep: left x={} vx={}, right x={} vx={}, dt={}, radius={}",
        w.balls[0].p.x, w.balls[0].v.x, w.balls[1].p.x, w.balls[1].v.x, w.dt, w.balls[0].r
    );
    assert!(w.balls[0].v.x < 0. && w.balls[1].v.x > 0.);
    assert!(
        w.balls[0].p.x < w.balls[1].p.x,
        "solid balls crossed without exchanging momentum"
    );
}
