use super::*;
use crate::visuals::depth::common::layout::OutputRect;
use std::time::Instant;
fn input(l3: bool, stress: bool) -> WorldInput {
    let outputs = [
        (0., 0., 2560., 1440.),
        (2560., 0., 3440., 1440.),
        (6000., 195., 1920., 1080.),
    ];
    let l = Layout::new(
        outputs[..if l3 { 3 } else { 1 }]
            .iter()
            .enumerate()
            .map(|(i, &(x, y, width, height))| OutputRect {
                id: crate::visuals::depth::common::rng::identity_hash(
                    ["Virtual-0", "Virtual-1", "Virtual-2"][i],
                ),
                rect: Rect {
                    x,
                    y,
                    width,
                    height,
                },
                primary: i == 0,
            })
            .collect(),
        1,
    )
    .unwrap();
    WorldInput {
        visual_id: 8,
        seed: 1729,
        mode: MonitorMode::Spanning,
        layout: l,
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
#[test]
fn mass_momentum_and_hue_survive_merge_engulf_split_and_burst() {
    let mut w = Colony::new(input(false, false));
    for kind in [LinkKind::Merge, LinkKind::Engulf] {
        w.orbs.fill(Orb::default());
        w.links.clear();
        let a = w.make_orb(
            p(900., 750.),
            0.55,
            0.98,
            if kind == LinkKind::Engulf { 6. } else { 1. },
        );
        let b = w.make_orb(p(912., 760.), 0.53, 0.02, 1.);
        w.orbs[0] = Orb {
            vel: p(12., -11.),
            ..a
        };
        w.orbs[1] = Orb {
            vel: p(-4., 18.),
            ..b
        };
        let momentum = w.orbs[0].vel * a.mass + w.orbs[1].vel * b.mass;
        w.links.push(Link {
            a: 0,
            b: 1,
            ids: [a.id, b.id],
            kind,
            age: 4.,
            duration: 1.,
        });
        w.update_links(1. / 30.);
        let parent = w.orbs[0];
        assert_eq!(parent.mass, a.mass + b.mass);
        assert!((parent.vel * parent.mass - momentum).length() < 1e-9);
        assert!(hue_delta(parent.hue, 0.).abs() < 0.021);
        w.divide(0, 3);
        let m = w.orbs.iter().map(|o| o.mass).sum::<f64>();
        let v = w
            .orbs
            .iter()
            .fold(Vec2::default(), |v, o| v + o.vel * o.mass);
        assert!((m - parent.mass).abs() < 1e-12);
        assert!((v - momentum).length() < 1e-8);
        let q = w.orbs[0];
        w.motes.fill(Mote::default());
        assert!(w.burst(0));
        let m = w.motes.iter().map(|s| s.mass).sum::<f64>();
        let v = w
            .motes
            .iter()
            .fold(Vec2::default(), |v, s| v + s.vel * s.mass);
        assert!((m - q.mass * 0.98).abs() < 1e-9);
        assert!((v - q.vel * m).length() < 1e-8);
        let mut angles = Vec::new();
        let mut speeds = Vec::new();
        for mote in w.motes.iter().filter(|m| m.life > 0.) {
            let droplet = mote.vel - q.vel;
            let ejecta = droplet + mote.ejecta_slip;
            let cross = droplet.x * ejecta.y - droplet.y * ejecta.x;
            let angle = cross.atan2(droplet.dot(ejecta)).to_degrees();
            let ratio = ejecta.length() / droplet.length();
            assert!(angle.abs() <= 12. && (0.70..=1.30).contains(&ratio));
            angles.push(angle);
            speeds.push(ratio);
        }
        angles.sort_by(f64::total_cmp);
        speeds.sort_by(f64::total_cmp);
        assert!(angles.last().unwrap() - angles[0] > 6.);
        assert!(speeds.last().unwrap() - speeds[0] > 0.15);
    }
}
#[test]
fn same_seed_same_records_ten_minutes_with_open_seams_and_walls() {
    let mut a = Colony::new(input(true, false));
    let mut b = Colony::new(input(true, false));
    let mut hashes = std::collections::BTreeSet::new();
    for t in 1..=18000 {
        a.step(t);
        b.step(t);
        if t % 900 == 0 {
            let mut fa = render::Frame::default();
            let mut fb = render::Frame::default();
            a.frame(&mut fa);
            b.frame(&mut fb);
            assert_eq!(fa.records_for_test(), fb.records_for_test());
            assert!(
                hashes.insert(
                    fa.records_for_test()
                        .iter()
                        .fold(0, |h, r| hash64(h ^ u64::from(r.body[0].to_bits())))
                )
            );
            for o in a.orbs.iter().filter(|o| o.alive()) {
                assert!(a.input.layout.contains(o.pos));
                assert!(o.vel.length().is_finite());
            }
        }
    }
    assert_eq!(
        space::wall(&a.input.layout, p(2560., 700.), 250.),
        p(0., 0.)
    );
    assert!(space::wall(&a.input.layout, p(5995., 120.), 80.).x < 0.);
    assert!(space::wall(&a.input.layout, p(6005., 220.), 80.).y > 0.);
}
#[test]
fn events_complete_on_mixed_height_and_small_offset_screens() {
    for screens in [
        vec![(0., 0., 2560., 2160.), (2560., 0., 1920., 720.)],
        vec![(-2560., -1440., 2560., 2160.), (0., -720., 1920., 720.)],
        vec![(-64., -32., 64., 64.), (0., -16., 48., 8.)],
        vec![(-16., -8., 16., 8.)],
    ] {
        let mut spec = input(false, false);
        spec.layout = Layout::new(
            screens
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
                .collect(),
            1,
        )
        .unwrap();
        for kind in [events::Kind::Procession, events::Kind::Dandelion] {
            let anchor = spec.layout.outputs[0].rect.centre();
            let mut w = Colony::new(spec.clone());
            w.inject(Event {
                id: kind as u32,
                at_ns: w.rate().tick_ns(1),
                anchor: [anchor.x, anchor.y],
                strength: 1.,
            })
            .unwrap();
            w.step(1);
            assert_eq!(w.event.as_ref().unwrap().kind, kind, "{screens:?}");
            for tick in 2..=1802 {
                w.step(tick);
            }
            assert!(w.event.is_none(), "{kind:?} did not finish on {screens:?}");
            let mut frame = render::Frame::default();
            w.frame(&mut frame);
            assert!(frame.records_for_test().iter().all(|r| {
                r.body
                    .iter()
                    .chain(&r.lobe)
                    .chain(&r.surface)
                    .all(|v| v.is_finite())
            }));
        }
    }
}
#[test]
fn event_cues_toggles_and_single_heart_bloom() {
    let mut w = Colony::new(input(false, false));
    let p0 = p(1280., 720.);
    let force = |w: &mut Colony, id: u32, tick: u64| {
        w.inject(Event {
            id,
            at_ns: w.rate().tick_ns(tick),
            anchor: [p0.x, p0.y],
            strength: 1.,
        })
        .unwrap()
    };
    for (j, kind) in [
        events::Kind::Dandelion,
        events::Kind::Procession,
        events::Kind::Foam,
        events::Kind::Surfacing,
    ]
    .into_iter()
    .enumerate()
    {
        let t = 1 + j as u64 * 2100;
        force(&mut w, kind as u32, t);
        for tick in t..t + 1801 {
            w.step(tick);
            if tick == t + 180 {
                assert_eq!(w.event.as_ref().unwrap().kind, kind);
            }
        }
    }
    w.params.worldEvents = false;
    force(&mut w, 1, 10000);
    w.step(10000);
    assert!(w.event.is_none());
    w.params.worldEvents = true;
    w.params.depth = 0;
    force(&mut w, 4, 10001);
    w.step(10001);
    assert!(w.event.is_none());
    assert!(
        w.inject(Event {
            id: 99,
            at_ns: 0,
            anchor: [0.; 2],
            strength: 1.
        })
        .is_err()
    );
}
#[test]
fn clock_follows_wall_time_and_fluid_parts_around_its_inflated_bounds() {
    let mut i = input(true, true);
    i.environment.obstacles.push(Obstacle {
        output_id: 0,
        rect: Rect {
            x: 100.,
            y: 100.,
            width: 190.,
            height: 95.,
        },
        inflation: 0.1,
        kind: 1,
        enabled: 1,
        velocity: p(31., 23.),
    });
    let mut fast = Colony::new(i.clone());
    i.speed = 0.1;
    let mut slow = Colony::new(i);
    let before = fast.clock[0].rect;
    for t in 1..=120 {
        fast.step(t);
        slow.step(t);
    }
    let r = fast.clock[0].rect;
    assert_eq!(r, slow.clock[0].rect);
    assert!((r.x - before.x - 124.).abs() < 1e-8);
    assert!(space::repel(r, p(r.x + 3., r.centre().y), 40.).x < 0.);
    assert!(space::repel(r, p(r.right() - 3., r.centre().y), 40.).x > 0.);
    assert_eq!(
        space::repel(r, p(r.right() + 150., r.centre().y), 40.),
        p(0., 0.)
    );
}
#[test]
fn six_hours_thermostat() {
    let mut w = Colony::new(input(false, false));
    let mut bounds = [usize::MAX, 0];
    let mut mass = [f64::MAX, 0f64];
    for t in 1..=6 * 3600 * 30 {
        w.step(t);
        if t % 900 == 0 {
            let n = w.orbs.iter().filter(|o| o.alive()).count();
            bounds = [bounds[0].min(n), bounds[1].max(n)];
            let m = w.orbs.iter().map(|o| o.mass).sum::<f64>() / w.target_mass;
            mass = [mass[0].min(m), mass[1].max(m)];
            assert!(
                (w.target * 7 / 10..=w.target * 13 / 10 + 1).contains(&n),
                "tick {t}: {n}"
            );
        }
    }
    eprintln!(
        "6h population {bounds:?}, mass/target {mass:?}, merges engulf rafts splits bursts births {:?}",
        w.stats
    );
    assert!(mass[0] > 0.65 && mass[1] < 1.30);
    assert!(w.stats.iter().all(|v| *v > 0));
}
#[test]
fn stress_tick_and_export_distribution() {
    let start = Instant::now();
    let mut w = Colony::new(input(true, true));
    let prewarm = start.elapsed();
    let mut times = Vec::with_capacity(4096);
    let mut peak_area = 0f64;
    for tick in 1..=4096 {
        let start = Instant::now();
        w.step(tick);
        let s = w.snapshot();
        times.push(start.elapsed().as_nanos());
        if tick % 300 == 0 {
            let f = s.typed.unwrap();
            let f = f.downcast_ref::<render::Frame>().unwrap();
            peak_area = peak_area.max(f.quad_area(&w.input.layout));
        }
    }
    times.sort_unstable();
    eprintln!(
        "stress prewarm {prewarm:?}, p99 {} us, max {} us, max output fill {:.2}x",
        times[times.len() * 99 / 100] / 1000,
        times[times.len() - 1] / 1000,
        peak_area
    );
    assert!(peak_area < 4.);
}

#[test]
fn recording_has_life_and_pearls_cross_both_screens() {
    let mut w = Colony::new(input(true, false));
    w.inject(Event {
        id: 2,
        at_ns: 640_000_000_000,
        anchor: [4280., 720.],
        strength: 1.,
    })
    .unwrap();
    for tick in 1..=600 * 30 {
        w.step(tick);
    }
    let start = w.stats;
    let mut end = start;
    let mut previous: Vec<_> = w.orbs.iter().map(|o| (o.id, o.pos.x)).collect();
    let mut raft = false;
    let mut crossings = [0; 2];
    let mut early = [0; 2];
    let population = |w: &Colony| {
        let mut screens: Vec<_> = w.input.layout.outputs.iter().map(|o| o.rect).collect();
        screens.sort_by(|a, b| a.x.total_cmp(&b.x));
        screens
            .iter()
            .map(|r| {
                w.orbs
                    .iter()
                    .filter(|o| o.alive() && (0.25..=0.85).contains(&o.z) && r.contains(o.pos))
                    .count()
            })
            .collect::<Vec<_>>()
    };
    let mut baseline = vec![];
    let mut excursions = Vec::new();
    let mut missing_trains = Vec::new();
    for tick in 600 * 30 + 1..=705 * 30 {
        w.step(tick);
        if tick == 690 * 30 {
            end = w.stats;
        }
        if tick == 639 * 30 {
            baseline = population(&w);
        }
        if tick >= 639 * 30 && tick % 30 == 0 {
            let counts = population(&w);
            eprintln!(
                "procession {}s mid {:?} swimmers {}",
                tick / 30,
                counts,
                w.orbs.iter().filter(|o| o.stream && o.alive()).count()
            );
            let mut frame = render::Frame::default();
            w.frame(&mut frame);
            let mut runs = Vec::new();
            let mut gaps = Vec::new();
            for train in 0..2 {
                let mut beads: Vec<_> = w
                    .orbs
                    .iter()
                    .enumerate()
                    .filter(|(_, o)| o.stream && o.alive() && o.train == train)
                    .collect();
                beads.sort_by(|(_, a), (_, b)| a.stream_station.total_cmp(&b.stream_station));
                let mut best = usize::from(!beads.is_empty());
                let mut length = best;
                for pair in beads.windows(2) {
                    let radii = frame.radius_for_slot(pair[0].0) + frame.radius_for_slot(pair[1].0);
                    let gap = (pair[0].1.pos - pair[1].1.pos).length() / radii;
                    gaps.push((gap * 100.).round() as u32);
                    length = if (0.95..=1.8).contains(&gap) {
                        length + 1
                    } else {
                        1
                    };
                    best = best.max(length);
                }
                runs.push(best);
            }
            eprintln!(
                "train runs {}s {:?} gaps-percent {:?}",
                tick / 30,
                runs,
                gaps
            );
            if (650..=675).contains(&(tick / 30)) && runs.iter().any(|r| *r < 5) {
                missing_trains.push(tick / 30);
            }
            if counts
                .iter()
                .zip(&baseline)
                .any(|(n, b)| n.abs_diff(*b) as f64 > 0.30 * *b as f64)
            {
                excursions.push(tick / 30);
            }
        }
        raft |= w.links.iter().any(|l| l.kind == LinkKind::Raft);
        for (o, (id, x)) in w.orbs.iter().zip(&mut previous) {
            if o.alive() && o.id == *id && tick > 643 * 30 && tick < 685 * 30 {
                for (n, seam) in [2560., 6000.].into_iter().enumerate() {
                    if o.stream && *x < seam && o.pos.x >= seam {
                        if tick <= 665 * 30 {
                            early[n] += 1;
                        }
                        crossings[n] += 1;
                    }
                }
            }
            (*id, *x) = (o.id, o.pos.x);
        }
    }
    eprintln!("bead crossings through +25s {early:?}");
    assert!(early.iter().all(|n| *n >= 3));
    let changes: Vec<_> = end.iter().zip(start).map(|(v, s)| v - s).collect();
    eprintln!(
        "recording merges, engulf, rafts, splits, bursts, births {changes:?}; raft visible {raft}, eastward seam crossings {crossings:?}"
    );
    assert!(
        missing_trains.is_empty(),
        "broken trains at {missing_trains:?}"
    );
    assert!(
        excursions.is_empty(),
        "population outside 30% at {excursions:?}"
    );
    assert!(
        raft && changes[0] > 0
            && changes[3] + changes[4] > 0
            && crossings.into_iter().all(|n| n >= 3)
    );
}

#[test]
fn mirrored_stress_fill_in_every_cover_crop() {
    use crate::visuals::depth::schema::Value;
    let mut spec = input(false, true);
    spec.layout = Layout::new(
        vec![OutputRect {
            id: 1,
            primary: true,
            rect: Rect {
                x: 0.,
                y: 0.,
                width: 3440.,
                height: 1440.,
            },
        }],
        1,
    )
    .unwrap();
    spec.mode = MonitorMode::Mirrored;
    spec.seed = 2024;
    spec.palette = 3;
    spec.parameters = vec![
        Value {
            id: 2,
            kind: 3,
            payload: 2,
        },
        Value {
            id: 3,
            kind: 1,
            payload: 100,
        },
        Value {
            id: 4,
            kind: 1,
            payload: 100,
        },
    ];
    let mut world = Colony::new(spec);
    for tick in 1..=18000 {
        world.step(tick);
        if tick != 900 && tick != 18000 {
            continue;
        }
        let mut frame = render::Frame::default();
        world.frame(&mut frame);
        let fills = [(2560., 1440.), (3440., 1440.), (1920., 1080.)].map(|(w, h): (f64, f64)| {
            let scale = (w / 3440.).max(h / 1440.);
            let width = w / scale;
            let height = h / scale;
            let rect = Rect {
                x: (3440. - width) / 2.,
                y: (1440. - height) / 2.,
                width,
                height,
            };
            frame.quad_area(
                &Layout::new(
                    vec![OutputRect {
                        id: 1,
                        primary: true,
                        rect,
                    }],
                    1,
                )
                .unwrap(),
            )
        });
        eprintln!("C5 at {} s, per-output quad area {:?}", tick / 30, fills);
        assert!(fills.iter().all(|f| *f < 4.));
    }
}

#[test]
fn simultaneous_fissions_share_the_pair_budget_with_merges() {
    let mut w = Colony::new(input(true, true));
    w.links.clear();
    w.thermostat = 2.;
    for orb in w.orbs.iter_mut().filter(|orb| orb.alive()) {
        orb.mass = 10.;
        orb.cooldown = 0.;
        orb.split = 0.;
        orb.partner = None;
    }
    for i in 0..2 {
        if !w.orbs[i].alive() {
            w.orbs[i] = w.make_orb(p(1450. + i as f64 * 90., 720.), 0.55, 0.3, 10.);
        }
    }
    w.orbs[0].age = 20.;
    w.orbs[1].age = 20.;
    w.orbs[0].partner = Some(1);
    w.orbs[1].partner = Some(0);
    w.links.push(Link {
        a: 0,
        b: 1,
        ids: [w.orbs[0].id, w.orbs[1].id],
        kind: LinkKind::Merge,
        age: 0.,
        duration: 0.65,
    });
    w.life_cycle(1. / 30., false);
    let mut frame = render::Frame::default();
    w.frame(&mut frame);
    let duos = frame
        .records_for_test()
        .iter()
        .filter(|r| r.lobe[3] == 1.)
        .count();
    assert_eq!(duos, 16);
}

#[test]
fn living_colony_coverage_across_the_spanning_layout() {
    let mut w = Colony::new(input(true, false));
    let mut frames = render::Frame::default();
    let mut all = true;
    for seconds in [10, 120, 900] {
        for tick in w.tick + 1..=seconds * 30 {
            w.step(tick);
        }
        w.frame(&mut frames);
        let rows = frames.records_for_test();
        for screen in &w.input.layout.outputs {
            let rect = screen.rect;
            let mut dark = 0;
            let mid: Vec<_> = w
                .orbs
                .iter()
                .filter(|o| o.alive() && rect.contains(o.pos))
                .collect();
            eprintln!(
                "at {seconds} x{} population {} mass {:.2} near {} wall {} bonds {}",
                rect.x,
                mid.len(),
                mid.iter().map(|o| o.mass).sum::<f64>(),
                mid.iter().filter(|o| o.z > 0.85).count(),
                mid.iter()
                    .filter(|o| !w.input.layout.circle_fits(o.pos, o.radius * 1.2))
                    .count(),
                mid.iter().filter(|o| o.links > 0).count()
            );
            for y in 0..60 {
                for x in 0..100 {
                    let point = p(
                        rect.x + (x as f64 + 0.5) / 100. * rect.width,
                        rect.y + (y as f64 + 0.5) / 60. * rect.height,
                    );
                    let lit = rows.iter().any(|r| {
                        // Approximate the five-code-value optical contour for
                        // the headless geometry check. check_hero.py measures
                        // the actual GPU pixels against the Midnight backdrop.
                        let contour = match r.lobe[3] as i32 {
                            0 | 1 => 1.48,
                            2 => 0.8,
                            _ => 0.,
                        };
                        let centre = p(f64::from(r.body[0]), f64::from(r.body[1]));
                        let diff = centre - point;
                        diff.dot(diff) < (f64::from(r.body[2]) * contour).powi(2)
                    });
                    dark += u32::from(!lit);
                }
            }
            let percent = f64::from(dark) / 60.;
            eprintln!(
                "C1 optical footprint at {seconds} s, x={}, dark {percent:.1}%",
                rect.x
            );
            all &= (53.0..=72.0).contains(&percent);
        }
    }
    assert!(all);
}

#[test]
fn dandelion_children_separate_and_survive() {
    let mut spec = input(true, false);
    spec.seed = 4242;
    spec.palette = 1;
    let mut world = Colony::new(spec);
    world
        .inject(Event {
            id: 1,
            at_ns: 60_000_000_000,
            anchor: [4280., 720.],
            strength: 1.,
        })
        .unwrap();
    let mut children = vec![];
    let mut centre = Vec2::default();
    let mut least_margin = f64::MAX;
    let mut greatest_offset = 0.0_f64;
    for tick in 1..=125 * 30 {
        world.step(tick);
        if (60 * 30..=106 * 30).contains(&tick) {
            let event = world.event.as_ref().unwrap();
            let heart = event.calyx.unwrap_or(world.orbs[event.heart]);
            let screen = event.screen;
            let margin = (heart.pos.x - screen.x)
                .min(screen.right() - heart.pos.x)
                .min(heart.pos.y - screen.y)
                .min(screen.bottom() - heart.pos.y);
            least_margin = least_margin.min(margin);
            greatest_offset = greatest_offset.max((heart.pos - event.anchor).length());
            if tick == 95 * 30 {
                eprintln!("C2 cue heart world {:?}", heart.pos);
            }
            assert!(margin >= 360. && greatest_offset <= 120.000001);
        }
        if world.orbs.iter().any(|o| o.bloom_travel > 0.) && children.is_empty() {
            let live: Vec<_> = world
                .orbs
                .iter()
                .filter(|o| o.alive() && o.bloom_travel > 0.)
                .collect();
            let energy = live.iter().map(|o| o.energy).fold(1.0_f64, f64::min);
            assert!(energy >= 0.8);
            eprintln!("Daughter launch minimum energy {energy}");
            centre = live.iter().fold(p(0., 0.), |p, o| p + o.pos) / live.len() as f64;
            children.extend(live.iter().map(|o| o.id));
            eprintln!(
                "Bloom at {:.2}s: {} daughters, centre {:?}",
                tick as f64 / 30.,
                children.len(),
                centre
            );
        }
        if [106 * 30, 113 * 30, 125 * 30].contains(&tick) {
            let live: Vec<_> = world
                .orbs
                .iter()
                .filter(|o| o.alive() && children.contains(&o.id))
                .collect();
            let spread: f64 = live.iter().map(|o| (o.pos - centre).length()).sum::<f64>()
                / live.len().max(1) as f64;
            eprintln!(
                "Bloom {}s: {} surviving, mean radius {:.0}px",
                tick / 30,
                live.len(),
                spread
            );
            let diameter = live
                .iter()
                .flat_map(|a| live.iter().map(move |b| (a.pos - b.pos).length()))
                .fold(0.0_f64, f64::max);
            let mut angles: Vec<_> = live
                .iter()
                .map(|o| o.pos - centre)
                .filter(|v| (350.0..=1100.0).contains(&v.length()))
                .map(|v| v.y.atan2(v.x).rem_euclid(TAU))
                .collect();
            angles.sort_by(f64::total_cmp);
            let count = angles.len();
            if let Some(first) = angles.first() {
                angles.push(*first + TAU);
            }
            let span = (TAU
                - angles
                    .windows(2)
                    .map(|a| a[1] - a[0])
                    .fold(0.0_f64, f64::max))
            .to_degrees();
            eprintln!(
                "C2 {}s diameter {diameter:.1} band {count} arc {span:.1}",
                tick / 30
            );
            assert!(children.len() >= 6 && live.len() == children.len());
            assert!(diameter >= 900.);
            if tick == 113 * 30 {
                assert!(count >= 6 && span >= 240.);
            }
        }
    }
    eprintln!("C2 heart minimum edge margin {least_margin:.1} maximum offset {greatest_offset:.1}");
}

#[test]
fn foam_and_forced_lifecycle_use_real_colony_events() {
    use crate::visuals::depth::common::rng::output_seed;
    use crate::visuals::depth::schema::Value;
    let mut spec = input(false, false);
    spec.seed = output_seed(9001, 8, "Virtual-0");
    spec.parameters = vec![
        Value {
            id: 1,
            kind: 3,
            payload: 2,
        },
        Value {
            id: 2,
            kind: 3,
            payload: 2,
        },
        Value {
            id: 3,
            kind: 1,
            payload: 30,
        },
        Value {
            id: 4,
            kind: 1,
            payload: 40,
        },
    ];
    let mut w = Colony::new(spec);
    w.inject(Event {
        id: 3,
        at_ns: 600_000_000_000,
        anchor: [1280., 720.],
        strength: 1.,
    })
    .unwrap();
    let mut baseline = 0;
    for t in 1..=646 * 30 {
        w.step(t);
        if t == 639 * 30 {
            baseline = w.stats[0];
        }
    }
    eprintln!("Foam cascade: {} merges", w.stats[0] - baseline);
    assert!(w.stats[0] - baseline >= 6);
    let mut spec = input(false, false);
    spec.seed = output_seed(1729, 8, "Virtual-0");
    let mut w = Colony::new(spec);
    for (id, time) in [(5, 70), (6, 82)] {
        w.inject(Event {
            id,
            at_ns: time * 1_000_000_000,
            anchor: [1280., 720.],
            strength: 1.,
        })
        .unwrap();
    }
    let mut split = false;
    let mut burst = false;
    for t in 1..=85 * 30 {
        w.step(t);
        split |= t == 2175 && w.orbs.iter().any(|o| o.split > 2.);
        burst |= (82 * 30..84 * 30).contains(&t) && !w.rings.is_empty();
    }
    assert!(split && burst, "fixture pinch {split}, ring {burst}");
}

#[test]
fn never_merge_keeps_a_connected_raft() {
    use crate::visuals::depth::{common::rng::output_seed, schema::Value};
    let mut spec = input(false, false);
    spec.seed = output_seed(1729, 8, "Virtual-0");
    spec.parameters = vec![Value {
        id: 3,
        kind: 1,
        payload: 0,
    }];
    let mut w = Colony::new(spec);
    for t in 1..=90 * 30 {
        w.step(t);
    }
    let mut seen = vec![false; w.orbs.len()];
    let mut sizes = vec![];
    for i in 0..w.orbs.len() {
        if seen[i] || !w.orbs[i].alive() {
            continue;
        }
        let mut stack = vec![i];
        let mut size = 0;
        while let Some(j) = stack.pop() {
            if seen[j] {
                continue;
            }
            seen[j] = true;
            size += 1;
            for l in w.links.iter().filter(|l| l.kind == LinkKind::Raft) {
                if l.a == j {
                    stack.push(l.b);
                }
                if l.b == j {
                    stack.push(l.a);
                }
            }
        }
        if size > 1 {
            sizes.push(size);
        }
    }
    sizes.sort_unstable();
    eprintln!(
        "Merging 0 at 90s, raft sizes {sizes:?}. Fusions {}",
        w.stats[0] + w.stats[1]
    );
    assert_eq!(w.stats[0] + w.stats[1], 0);
    assert!(!sizes.is_empty() && (4..=7).contains(&sizes[sizes.len() / 2]));
}

fn independent_capture(
    seed: u64,
    palette: u32,
    look: u64,
    flow: u64,
    merging: u64,
    depth: u64,
) -> WorldInput {
    use crate::visuals::depth::{common::rng::output_seed, schema::Value};
    let mut spec = input(false, false);
    spec.mode = MonitorMode::Independent;
    spec.seed = output_seed(seed, 8, "Virtual-0");
    spec.palette = palette;
    spec.parameters = vec![
        Value {
            id: 1,
            kind: 3,
            payload: look,
        },
        Value {
            id: 2,
            kind: 3,
            payload: flow,
        },
        Value {
            id: 3,
            kind: 1,
            payload: merging,
        },
        Value {
            id: 4,
            kind: 1,
            payload: depth,
        },
    ];
    spec
}

#[test]
fn mature_rise_carries_cool_orbs_down_and_hot_orbs_up() {
    let mut spec = independent_capture(77, 2, 1, 1, 70, 80);
    spec.scale = 1.30;
    spec.density = 70.;
    spec.trails = 0.60;
    let mut w = Colony::new(spec);
    w.inject(Event {
        id: 4,
        at_ns: 290_000_000_000,
        anchor: [1280., 720.],
        strength: 1.,
    })
    .unwrap();
    let mut previous = vec![(0, false); w.orbs.len()];
    let (mut up, mut down) = (0, 0);
    let mut minimum_lower = 1.0_f64;
    let mut lower_count = 0;
    let mut red_count = 0;
    let mut minimum_red = 1.0_f64;
    for tick in 1..=1860 * 30 {
        w.step(tick);
        if tick < 1800 * 30 || tick % 30 != 0 {
            continue;
        }
        let (mut lower, mut red, mut live, mut pool) = (0, 0, 0, 0);
        for (i, o) in w.orbs.iter().enumerate().filter(|(_, o)| o.alive()) {
            let bottom = o.pos.y >= 720.;
            live += 1;
            if bottom {
                lower += 1;
                let c = w.art.at(Ramp::pigment(2, o.hue, o.z, o.energy)) as u32;
                let r = (c >> 16) & 255;
                let g = (c >> 8) & 255;
                let crimson = g * 100 < r * 50;
                red += usize::from(crimson);
                pool += usize::from(o.pos.y >= 0.85 * 1440. && crimson);
            }
            if previous[i].0 == o.id && bottom != previous[i].1 {
                if bottom {
                    down += 1;
                } else {
                    up += 1;
                }
            }
            previous[i] = (o.id, bottom);
        }
        minimum_lower = minimum_lower.min(lower as f64 / live as f64);
        minimum_red = minimum_red.min(red as f64 / lower.max(1) as f64);
        red_count += red;
        lower_count += lower;
        eprintln!(
            "Rise {}s lower {}/{} red {} floor {} up {} down {}",
            tick / 30,
            lower,
            live,
            red,
            pool,
            up,
            down
        );
    }
    eprintln!(
        "thermal summary lower {:.4} red {:.4} minimum red {:.4} up {} down {}",
        minimum_lower,
        red_count as f64 / lower_count as f64,
        minimum_red,
        up,
        down
    );
    assert!(minimum_lower >= 0.20 && minimum_red >= 0.50 && up >= 3 && down >= 3);
}

// An optional TSV supplies the geometry for the native C4 angular pixel
// regression. It follows the same seed, Foam event and optical fill budget.
#[test]
fn pastel_film_capture_circles() {
    use std::fmt::Write;
    let mut spec = independent_capture(9001, 5, 2, 2, 30, 40);
    spec.trails = 0.20;
    let mut w = Colony::new(spec);
    w.inject(Event {
        id: 3,
        at_ns: 600_000_000_000,
        anchor: [1280., 720.],
        strength: 1.,
    })
    .unwrap();
    let mut text = String::new();
    for tick in 1..=660 * 30 {
        w.step(tick);
        if ![60 * 30, 630 * 30, 642 * 30, 660 * 30].contains(&tick) {
            continue;
        }
        let mut frame = render::Frame::default();
        w.frame(&mut frame);
        let records = frame.records_for_test();
        assert!(records.iter().filter(|r| r.lobe[3] < 1.5).count() >= 6);
        for q in records {
            writeln!(
                text,
                "{} {} {} {} {} {} {} {} {}",
                tick / 30,
                q.body[0],
                q.body[1],
                q.body[2],
                q.lobe[0],
                q.lobe[1],
                q.lobe[2],
                q.lobe[3],
                q.surface[3]
            )
            .unwrap();
        }
    }
    if let Some(path) = std::env::var_os("PVS_ORBS_FILM_CIRCLES") {
        std::fs::write(path, text).unwrap();
    }
}
