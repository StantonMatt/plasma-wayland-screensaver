use super::super::{
    common::layout::{Layout, OutputRect, Rect},
    schema::Value,
    world::Environment,
};
use super::*;
use std::f32::consts::TAU;
fn input(speed: f64, density: f64, params: &[(u32, u64)]) -> WorldInput {
    WorldInput {
        visual_id: 9,
        seed: 7,
        mode: MonitorMode::Independent,
        layout: Layout::new(
            vec![OutputRect {
                id: 5,
                rect: Rect {
                    x: 0.,
                    y: 0.,
                    width: 2560.,
                    height: 1440.,
                },
                primary: true,
            }],
            1,
        )
        .unwrap(),
        speed,
        density,
        scale: 1.,
        trails: 0.35,
        palette: 0,
        reduced_motion: false,
        parameters: params
            .iter()
            .map(|&(id, payload)| Value {
                id,
                kind: if id == 1 { 1 } else { 2 },
                payload,
            })
            .collect(),
        environment: Environment::default(),
    }
}
#[test]
fn descriptor_and_settings() {
    assert_eq!(DESCRIPTOR.rate, 30);
    assert_eq!(DESCRIPTOR.mirrored, MirroredPolicy::SharedCanonical);
    assert!(Params::default().hunters && Params::default().grass);
    let w = World::new(input(3., 50., &[(4, 0), (5, 0)]));
    assert!(w.flies.iter().all(|f| f.species != Species::Hunter));
    assert!(w.art.blades.is_empty());
}
#[test]
fn sample_history_and_still_are_deterministic() {
    let mut a = World::new(input(1., 50., &[]));
    let mut b = World::new(input(1., 50., &[]));
    let first = a.snapshot().typed.unwrap();
    for t in 1..999 {
        a.step(t);
        b.step(t);
    }
    assert_eq!(a.state_hash(), b.state_hash());
    assert_ne!(
        a.frame.as_ref().unwrap().head_serial,
        first.downcast_ref::<Frame>().unwrap().head_serial
    );
    let mut i = input(1., 50., &[]);
    i.reduced_motion = true;
    let mut s = World::new(i);
    let before = s.state_hash();
    s.step(900);
    assert_eq!(before, s.state_hash());
    assert_eq!(s.history.len(), 60);
    assert!(s.history.iter().all(|row| !row.is_empty()));
}
#[test]
fn banks_gaps_and_independent_offsets() {
    let mut i = input(1., 50., &[]);
    i.layout = Layout::new(
        vec![
            OutputRect {
                id: 1,
                rect: Rect {
                    x: 0.,
                    y: 0.,
                    width: 2560.,
                    height: 1440.,
                },
                primary: false,
            },
            OutputRect {
                id: 2,
                rect: Rect {
                    x: 2560.,
                    y: 0.,
                    width: 3440.,
                    height: 1440.,
                },
                primary: true,
            },
            OutputRect {
                id: 3,
                rect: Rect {
                    x: 6000.,
                    y: 195.,
                    width: 1920.,
                    height: 1080.,
                },
                primary: false,
            },
            OutputRect {
                id: 4,
                rect: Rect {
                    x: 8500.,
                    y: 0.,
                    width: 2560.,
                    height: 1440.,
                },
                primary: false,
            },
        ],
        1,
    )
    .unwrap();
    let m = Meadow::new(&i.layout);
    assert_eq!(m.column(5800.).floor, 1440.);
    assert_eq!(m.column(6100.).floor, 1275.);
    assert!(m.column(5980.).flight_floor < 1440.);
    assert_ne!(m.column(8130.).escape, 0.);
    i.environment.independent_index = 2;
    let world = World::new(i);
    assert_eq!(world.night_age, 1440.);
    assert!(
        world
            .art
            .blades
            .iter()
            .filter(|b| b[0] > 6000. && b[0] < 7920.)
            .all(|b| b[1] == 1275.)
    );
}
#[test]
fn forced_gathering_and_shower_remain_lively() {
    let mut i = input(3., 100., &[(1, 100), (2, 0)]);
    i.scale = 2.;
    let mut w = World::new(i);
    let mut last = vec![None; w.flies.len() + w.far_count];
    let mut peak = 0_f32;
    let mut seen_tree = false;
    let mut seen_bloom = false;
    let mut minimum = u64::MAX;
    for tick in 1..3600 {
        if tick == 30 {
            w.begin_event(events::TREE, [1280., 1200.]);
        }
        if tick == 2100 {
            w.begin_event(events::SHOWER, [1280., 1200.]);
        }
        w.step(tick);
        seen_tree |= w.tree_ready;
        seen_bloom |= w.after_rain > 0.;
        peak = peak.max(w.emission);
        for (i, since) in w
            .flies
            .iter()
            .map(|f| f.since)
            .chain(w.distant[..w.far_count].iter().map(|f| f.since))
            .enumerate()
        {
            if since == 0. {
                if let Some(old) = last[i] {
                    minimum = minimum.min(tick - old);
                }
                last[i] = Some(tick);
            }
        }
    }
    println!("Tree + shower, peak weighted area {peak:.6}, shortest onset spacing {minimum} ticks");
    assert!(seen_tree && seen_bloom && minimum < u64::MAX && peak.is_finite() && peak > 0.);
}

#[test]
fn neighbouring_lights_do_not_dim_an_existing_flash() {
    let mut i = input(3., 100., &[(2, 0), (3, 0), (4, 0)]);
    i.scale = 2.;
    let mut w = World::new(i);
    for f in &mut w.flies {
        f.species = Species::Syncer;
        f.z = 0.84;
        f.x = 1280.;
        f.y = 1000.;
        f.light = 0.;
    }
    for f in &mut w.distant {
        f.light = 0.;
    }
    w.flies[0].light = 1.;
    w.export(false);
    let isolated = w.frame.as_ref().unwrap().heads[0][2];
    for f in &mut w.flies {
        f.light = 1.;
    }
    w.export(false);
    assert_eq!(w.frame.as_ref().unwrap().heads[0][2], isolated);
    assert!(
        w.emission > 0.045,
        "fixture must exceed the retired ceiling"
    );
}
#[test]
fn synchrony_locks_local_oscillators() {
    fn measure(gain: u64) -> (f32, f32) {
        let mut w = World::new(input(1., 75., &[(1, gain), (2, 0), (3, 0)]));
        // Measure each sync island against its immediate visible neighbours.
        let order = |w: &World| {
            let mut total = 0.;
            let mut n = 0;
            for f in w.flies.iter().filter(|f| {
                f.species == Species::Syncer && !f.female && f.state == fly::State::Patrol
            }) {
                let mut x = 0.;
                let mut y = 0.;
                let mut count = 0;
                for q in &w.flies {
                    if q.species == f.species
                        && !q.female
                        && q.state == fly::State::Patrol
                        && (q.x - f.x).powi(2) + (q.y - f.y).powi(2) < 260_f32.powi(2)
                        && (f.z - q.z).abs() < 0.35
                    {
                        let (s, c) = (q.phase * TAU).sin_cos();
                        x += c;
                        y += s;
                        count += 1;
                    }
                }
                if count >= 4 {
                    total += (x * x + y * y).sqrt() / count as f32;
                    n += 1;
                }
            }
            total / (n as f32).max(1.)
        };
        let initial = order(&w);
        let mut sum = 0.;
        let end = if gain > 0 && gain < 100 { 5400 } else { 1200 };
        for t in 1..=end {
            w.step(t);
            if t > end - 150 && t % 30 == 0 {
                sum += order(&w);
            }
        }
        (initial, sum / 5.)
    }
    let off = measure(0);
    let on = measure(100);
    let middle = measure(55);
    println!("Medium synchrony order {middle:?}");
    println!("Local circular order: uncoupled {off:?}, strongly coupled {on:?}");
    assert!(on.1 >= 0.85 && on.1 > off.1 + 0.20 && middle.1 >= 0.58);
}
#[test]
#[ignore = "eight-hour release soak"]
fn eight_hour_soak() {
    let mut w = World::new(input(1., 50., &[]));
    let night_signature = |w: &World| {
        w.flies
            .iter()
            .map(|f| (f.species, f.period.to_bits(), f.home))
            .collect::<Vec<_>>()
    };
    let mut previous_population = night_signature(&w);
    let mut prev_night = 0;
    let mut min_lit = usize::MAX;
    let mut min_embers = usize::MAX;
    let mut wet_samples = 0;
    let mut wet_peak = 0_f32;
    for t in 1..=8 * 3600 * 30 {
        w.step(t);
        if t % 900 == 0 {
            let f = &w.frame.as_ref().unwrap().heads;
            assert!(f.iter().flatten().all(|v| v.is_finite()));
            let lights = &f[..w.flies.len() + w.distant.len()];
            let bright = lights.iter().filter(|r| r[2] > 0.12).count();
            let embers = lights.iter().filter(|r| r[2] > 0.04).count();
            min_embers = min_embers.min(embers);
            if w.weather.rain() > 0.2
                || w.night_age > w.night_length
                || (w.night > 0 && w.night_age < 18.)
            {
                if bright == 0 {
                    wet_samples += 1;
                    wet_peak = wet_peak.max(lights.iter().map(|r| r[2]).fold(0_f32, f32::max));
                }
            } else {
                assert!(
                    bright > 0,
                    "dry meadow went dark on night {} at {} s",
                    w.night,
                    w.night_age
                );
                min_lit = min_lit.min(bright);
            }
            assert_eq!(w.flies.len(), w.mid_count);
            if w.night != prev_night {
                let population = night_signature(&w);
                assert_ne!(population, previous_population);
                previous_population = population;
                prev_night = w.night;
            }
        }
    }
    println!(
        "864000 ticks, {} complete nights, dry minimum {} bright, all-weather minimum {min_embers} embers, {} mid, {} far; {wet_samples} wet/hushed samples below 12% (brightest {wet_peak:.3})",
        w.night, min_lit, w.mid_count, w.far_count
    );
    assert!(w.night >= 12 && min_lit > 0 && min_embers > 0);
}

#[test]
fn hunter_sputters_rests_and_returns_as_a_new_fly() {
    let mut w = World::new(input(3., 50., &[(2, 0)]));
    w.begin_event(events::HUNT, [1280., 1100.]);
    let id = w
        .flies
        .iter()
        .position(|f| f.state == fly::State::Lured)
        .unwrap();
    let hunter = w.flies[id].target;
    let mut fading = false;
    let mut dark = false;
    for t in 1..850 {
        w.step(t);
        if w.flies[id].state == fly::State::Fading {
            fading = true;
            assert!(w.flies[hunter].meal > 0.);
        }
        if w.flies[id].state == fly::State::Lost && w.flies[id].light < 0.005 {
            dark = true;
        }
    }
    assert!(fading && dark);
    assert_ne!(w.flies[id].state, fly::State::Lost);
}

#[test]
fn recording_has_cross_seam_signals_and_landings() {
    let mut i = input(1., 50., &[(1, 80)]);
    i.seed = 42;
    i.mode = MonitorMode::Spanning;
    i.layout = Layout::new(
        [
            (0., 0., 2560., 1440.),
            (2560., 0., 3440., 1440.),
            (6000., 195., 1920., 1080.),
        ]
        .into_iter()
        .enumerate()
        .map(|(n, (x, y, width, height))| OutputRect {
            id: n as u64,
            rect: Rect {
                x,
                y,
                width,
                height,
            },
            primary: n == 1,
        })
        .collect(),
        1,
    )
    .unwrap();
    let mut w = World::new(i);
    w.inject(Event {
        id: events::GUST,
        at_ns: 320_000_000_000,
        anchor: [0., 720.],
        strength: 1.,
    })
    .unwrap();
    let mut prev = vec![false; w.mid_count];
    let mut landings = 0;
    let mut seam = [0, 0];
    let mut gust = [false; 3];
    let mut bokeh_ticks = [0u32; 3];
    let mut bokeh_max = [0usize; 3];
    let mut first_landing = None;
    let mut first_signal = None;
    for tick in 1..390 * 30 {
        w.step(tick);
        for (j, f) in w.flies.iter().enumerate() {
            let paired = f.state == fly::State::Paired;
            if tick >= 300 * 30 && paired && !f.female && !prev[j] {
                landings += 1;
                first_landing.get_or_insert(tick);
            }
            prev[j] = paired;
        }
        if tick < 300 * 30 {
            continue;
        }
        for (j, area) in w.meadow.outputs.iter().enumerate() {
            // Include the entire disc, so one straddling a join counts on both panels.
            let visible = w
                .distant
                .iter()
                .filter(|d| {
                    d.near
                        && d.light > 0.01
                        && d.x + 42. >= area.x as f32
                        && d.x - 42. < area.right() as f32
                        && d.y + 42. >= area.y as f32
                        && d.y - 42. < area.bottom() as f32
                })
                .count();
            bokeh_ticks[j] += visible as u32;
            bokeh_max[j] = bokeh_max[j].max(visible);
        }
        for &(from, to) in &w.coupling_edges {
            for (j, x) in [2560., 6000.].into_iter().enumerate() {
                if (w.flies[from].x - x) * (w.flies[to].x - x) < 0. {
                    seam[j] += 1;
                    first_signal.get_or_insert(tick);
                }
            }
        }
        for (j, x) in [1280., 4280., 6960.].into_iter().enumerate() {
            gust[j] |= w.weather.gust(x) > 0.9;
        }
    }
    println!(
        "Recording: {landings} landings (first tick {first_landing:?}), causal seam signals {seam:?} (first tick {first_signal:?}), gust peaks {gust:?}"
    );
    let average = bokeh_ticks.map(|n| n as f32 / 2700.);
    println!("Recording bokeh by output: mean {average:?}, maximum {bokeh_max:?}");
    assert!(average.iter().all(|mean| *mean <= 1.5) && bokeh_max.iter().all(|n| *n <= 3));
    assert!(landings > 0 && seam.iter().all(|n| *n > 0) && gust == [true, true, true]);
}

#[test]
fn trail_segments_survive_relocations_and_reseeding() {
    let mut w = World::new(input(1., 50., &[]));
    let mut longest = 0_f32;
    for t in 1..1200 {
        if t == 60 {
            w.begin_event(events::HUNT, [1280., 1210.]);
        }
        if t == 1050 {
            w.night_age = w.night_length + 30.;
        }
        w.step(t);
        let f = w.frame.as_ref().unwrap();
        let n = f.history.len();
        let mut next = &*f.heads;
        for age in 0..n {
            let row = &f.history[(f.history_serial as usize + n - age) % n];
            for (a, b) in row.iter().zip(next).take(f.mid) {
                if a[3] as usize == b[3] as usize {
                    longest = longest.max((a[0] - b[0]).hypot(a[1] - b[1]));
                }
            }
            next = row;
        }
    }
    println!(
        "Longest default trail segment across forced relocation, respawn and new night: {longest:.2}px"
    );
    assert!(longest <= 60.);
}

#[test]
fn foreground_lifetime_drift_and_far_duty() {
    let mut w = World::new(input(1., 50., &[(3, 0)]));
    let mut lit = 0;
    let mut rebirths = 0;
    let mut far_on = 0;
    let mut max_far = 0_f32;
    for t in 0..18000 {
        let old = &w.distant[w.far_count];
        let x = old.x;
        let age = old.age;
        w.step(t);
        let f = &w.distant[w.far_count];
        if f.age < age {
            rebirths += 1;
            assert!(f.x > 199. && f.x < 2361. && (20. ..=60.).contains(&f.period));
        }
        if f.light > 0.01 {
            let dx = (f.x - x).abs() * 30.;
            assert!((5.98..12.02).contains(&dx));
            lit += 1;
        }
        let on = w.distant[..w.far_count]
            .iter()
            .filter(|f| f.light > 0.001)
            .count();
        far_on += on;
        max_far = max_far.max(on as f32 / w.far_count as f32);
    }
    println!(
        "Bokeh visible average {:.3}, {} rebirths; far duty {:.3}, maximum concurrent {:.3}",
        lit as f32 / 18000.,
        rebirths,
        far_on as f32 / (18000 * w.far_count) as f32,
        max_far
    );
    assert!(rebirths >= 5 && lit > 0 && max_far <= 0.25);
}

#[test]
fn drawing_the_still_and_the_tree() {
    let mut i = input(1., 50., &[]);
    i.seed = capture_seed(5);
    i.palette = 4;
    i.trails = 1.;
    i.reduced_motion = true;
    let w = World::new(i);
    assert!((w.night_age / w.night_length - 0.35).abs() < 0.001);
    let mut chains = 0;
    let mut hooks = 0;
    let mut ghosts = 0;
    for (id, fly) in w.flies.iter().enumerate() {
        let mut dots: Vec<[f32; 2]> = vec![];
        let mut active = false;
        let mut start = [0.; 2];
        let mut lowest = 0_f32;
        let mut hooked = false;
        let mut extent = [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ];
        for j in 1..=w.history.len() {
            let [x, y, light, _] = w.history[(w.history_serial as usize + j) % w.history.len()][id];
            let lit = light > 0.20;
            if lit {
                extent = [
                    extent[0].min(x),
                    extent[1].min(y),
                    extent[2].max(x),
                    extent[3].max(y),
                ];
                if !active {
                    start = [x, y];
                    lowest = y;
                    if dots.iter().all(|d| (d[0] - x).hypot(d[1] - y) > 5.) {
                        dots.push([x, y]);
                    }
                }
                lowest = lowest.max(y);
                // 100 ms rows first see a pulse partway through its 200 ms dip.
                hooked |=
                    lowest - start[1] >= 4. && lowest - y >= 44. && (x - start[0]).abs() >= 8.;
            }
            active = lit;
        }
        match fly.species {
            Species::Syncer if dots.len() >= 3 && !fly.female => chains += 1,
            Species::Dipper if hooked && !fly.female => hooks += 1,
            Species::Ghost
                if extent[0].is_finite()
                    && (extent[2] - extent[0]).hypot(extent[3] - extent[1]) >= 20. =>
            {
                ghosts += 1
            }
            _ => {}
        }
    }
    println!("Six-second exposure: {chains} dotted chains, {hooks} J-hooks, {ghosts} ghost traces");
    assert!(chains >= 12 && hooks >= 8 && (3..=5).contains(&ghosts));
    let mut tree = World::new(input(1., 50., &[]));
    tree.begin_event(events::TREE, [1280., 1100.]);
    assert!(
        tree.flies
            .iter()
            .all(|f| f.tree.is_none() || f.tree_gain == 1.)
    );
    let points: Vec<_> = tree
        .flies
        .iter()
        .filter_map(|f| f.tree)
        .chain(tree.distant.iter().filter_map(|f| f.tree))
        .collect();
    let mut minimum = f32::INFINITY;
    for (i, p) in points.iter().enumerate() {
        for q in &points[..i] {
            minimum = minimum.min((p[0] - q[0]).hypot(p[1] - q[1]));
        }
    }
    let top = points.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
    let canopy_height = 1440. - 305. - top;
    assert!((505. ..=575.).contains(&canopy_height));
    println!(
        "Crown: {} recruited, {minimum:.2}px closest spacing, canopy {canopy_height:.1}px tall",
        points.len()
    );
    assert!((100..=120).contains(&points.len()) && minimum >= 14.);
}

#[test]
fn large_crown_spreads_perches_and_preserves_isolated_flashes() {
    let mut i = input(3., 100., &[]);
    i.scale = 2.;
    i.palette = 5;
    let mut w = World::new(i);
    w.begin_event(events::TREE, [1280., 1100.]);
    let occupied: Vec<_> = w
        .flies
        .iter()
        .filter_map(|f| f.tree.map(|p| (p, f.tree_gain)))
        .chain(
            w.distant
                .iter()
                .filter_map(|f| f.tree.map(|p| (p, f.tree_gain))),
        )
        .collect();
    assert_eq!(occupied.len(), 112);
    let mut closest = f32::INFINITY;
    let mut lowest = 1_f32;
    let mut highest = 0_f32;
    for (index, &(p, gain)) in occupied.iter().enumerate() {
        assert!(gain > 0. && gain <= 1.);
        lowest = lowest.min(gain);
        highest = highest.max(gain);
        for &(q, _) in &occupied[..index] {
            closest = closest.min((p[0] - q[0]).hypot(p[1] - q[1]));
        }
    }
    println!(
        "Scale-200 crown: closest perches {closest:.2}px, local pulse gain {lowest:.3} to {highest:.3}"
    );
    assert!(closest >= 21. && lowest < 0.4 && highest > lowest * 2.);
}

fn capture_seed(seed: u64) -> u64 {
    let connector = super::super::common::rng::identity_hash("Virtual-0");
    hash64(hash64(hash64(seed ^ 9) ^ connector))
}

#[test]
fn b_wave_crosses_the_meadow() {
    let mut i = input(1., 70., &[(1, 100)]);
    i.seed = capture_seed(7);
    i.trails = 0.60;
    i.palette = 5;
    let mut w = World::new(i);
    // Track the first onset of each real burst. Select only tightly aligned
    // launches that visit at least four distinct 160 px sections of meadow.
    let mut onsets = vec![];
    for tick in 1..=281 * 30 {
        w.step(tick);
        if tick >= 272 * 30 {
            for &id in &w.signals {
                let f = w.flies[id];
                if f.species == Species::Syncer
                    && !f.female
                    && f.state == fly::State::Patrol
                    && f.remaining == f.burst - 1
                {
                    onsets.push((tick as f32 / 30., f.x, f.y));
                }
            }
        }
    }
    let mut best = (0_f32, 0., 0., 0);
    let mut origin = 0.;
    for &(t, x, _) in &onsets {
        let mut bins = 0u32;
        let mut last = (t, x);
        let mut count = 0;
        for &(time, position, _) in &onsets {
            let distance = position - x;
            if time >= t
                && time <= t + 1.5
                && distance >= 0.
                && ((time - t) - distance / 740.).abs() < 0.14
            {
                bins |= 1 << ((distance / 160.) as u32).min(30);
                count += 1;
                if position > last.1 {
                    last = (time, position);
                }
            }
        }
        if bins.count_ones() >= 4 && count >= 6 && last.1 - x > best.0 {
            best = (last.1 - x, t, last.0 - t, count);
            origin = x;
        }
    }
    println!(
        "B-wave front: {:.0}px from {:.3}s in {:.3}s over {} onsets",
        best.0, best.1, best.2, best.3
    );
    for &(t, x, y) in &onsets {
        let dx = x - origin;
        if (best.1..=best.1 + 1.5).contains(&t)
            && dx >= 0.
            && ((t - best.1) - dx / 740.).abs() < 0.14
        {
            println!("B-wave onset {t:.3} {x:.1} {y:.1}");
        }
    }
    assert!(best.0 >= 600.);
}

#[test]
fn forced_courtship_writes_a_spiral_and_leaves_a_lantern_pair() {
    let mut w = World::new(input(1., 50., &[(2, 0)]));
    w.begin_event(events::COURTSHIP, [1280., 1300.]);
    let male = w
        .flies
        .iter()
        .position(|f| f.state == fly::State::Court)
        .unwrap();
    let mate = w.flies[male].target;
    assert!(w.flies[mate].y - w.flies[male].y >= 160.);
    assert!((w.flies[male].x - w.flies[mate].x - 40.).abs() < 0.1);
    let mut onsets = 0;
    let mut path = 0.;
    let mut prior = [w.flies[male].x, w.flies[male].y];
    for t in 1..=80 {
        w.step(t);
        let f = w.flies[male];
        path += (f.x - prior[0]).hypot(f.y - prior[1]);
        prior = [f.x, f.y];
        onsets += u32::from(w.signals.contains(&male) && f.state == fly::State::Court);
    }
    let a = w.flies[male];
    let b = w.flies[mate];
    let separation = (a.x - b.x).hypot(a.y - b.y);
    println!(
        "Courtship: {onsets} flashes over {path:.1}px, final gap {separation:.2}px, pair holds {:.2}s",
        a.timer
    );
    assert_eq!(a.state, fly::State::Paired);
    assert_eq!(b.state, fly::State::Paired);
    assert!((4..=5).contains(&onsets) && path >= 160. && (6. ..=10.).contains(&separation));
    assert!((14.7..=25.).contains(&a.timer) && (0.20..=0.25).contains(&a.light));
}

#[test]
fn dipper_exposes_both_legs_of_its_hook() {
    let mut w = World::new(input(1., 50., &[(2, 0), (3, 0)]));
    let id = w
        .flies
        .iter()
        .position(|f| f.species == Species::Dipper && !f.female)
        .unwrap();
    let f = &mut w.flies[id];
    f.x = 1280.;
    f.y = 900.;
    f.vx = 35.;
    f.phase = 0.1;
    f.female = false;
    f.activity = 0.;
    f.state = fly::State::Patrol;
    f.since = 2.;
    f.fire(w.flash_time, 1.);
    let swoop = f.swoop;
    let mut bottom = 900_f32;
    let mut peak = (0., 0.);
    let mut bottom_time = 0.;
    for tick in 1..=(swoop.end() * 30.).ceil() as u64 {
        w.step(tick);
        let f = w.flies[id];
        if f.y > bottom {
            bottom = f.y;
            bottom_time = f.since;
        }
        if f.light > peak.1 {
            peak = (f.since, f.light);
        }
    }
    let top = w.flies[id];
    let dip = bottom - 900.;
    let climb = bottom - top.y;
    println!(
        "Dipper dip {dip:.1}px, climb {climb:.1}px, peak {:.3}s after bottom",
        peak.0 - bottom_time
    );
    assert!((7.9..=16.1).contains(&dip) && (37.9..=60.1).contains(&climb));
    assert!((0.066..=0.15).contains(&(peak.0 - bottom_time)) && top.light > 0.5);
    let mut heights = (f32::INFINITY, 0_f32);
    let mut radii = (f32::INFINITY, 0_f32);
    for key in 0..256 {
        let stroke = fly::Swoop::new(key);
        let t = fly::Swoop::BOTTOM;
        let dt = 1. / 60.;
        let accel =
            (2. * stroke.offset(t) - stroke.offset(t - dt) - stroke.offset(t + dt)) / (dt * dt);
        let radius = stroke.across.powi(2) / accel;
        let height = stroke.offset(t) - stroke.offset(stroke.end());
        radii = (radii.0.min(radius), radii.1.max(radius));
        heights = (heights.0.min(height), heights.1.max(height));
        let lean = (stroke.across * dt
            / (stroke.offset(stroke.end() - dt) - stroke.offset(stroke.end())))
        .atan()
        .to_degrees();
        assert!(stroke.across >= 40.5 && (29.9..=45.1).contains(&lean));
    }
    println!("256 flashes: radii {radii:?} px, heights {heights:?} px");
    assert!(radii.0 >= 8. && radii.1 <= 14. && heights.1 >= 1.5 * heights.0);
}

#[test]
fn hunter_catch_and_pending_answer_share_a_timestamp_in_either_order() {
    use fly::State::{Fading, Lured, Perched};
    for hunter in [0, 1] {
        let prey = 1 - hunter;
        let mut w = World::new(input(1., 50., &[(2, 0), (3, 0)]));
        w.flies.truncate(2);
        let now = w.flash_time;
        for f in &mut w.flies {
            f.x = 1280.;
            f.y = 1200.;
            f.vx = 0.;
            f.vy = 0.;
            f.tree = None;
            f.remaining = 0;
            f.since = 1.;
            f.last_flash = now - 1.;
            f.phase = 0.;
            f.perch_age = 60.;
        }
        w.flies[prey].species = Species::Dipper;
        w.flies[prey].female = false;
        w.flies[prey].state = Lured;
        w.flies[prey].target = hunter;
        w.flies[hunter].species = Species::Hunter;
        w.flies[hunter].female = true;
        w.flies[hunter].state = Perched;
        w.flies[hunter].target = prey;
        // This answer expires 14 ticks after the catch. A later-index hunter
        // used to gain an extra tick immediately after the cross-fly flash.
        w.flies[hunter].answer = 0.49;
        let mut onsets = vec![];
        for tick in 0..=15 {
            let before = w.flies[hunter].last_flash;
            w.step(tick + 1);
            if w.flies[hunter].last_flash != before {
                onsets.push(tick);
            }
        }
        assert_eq!(w.flies[prey].state, Fading);
        assert_eq!(onsets, [0, 14], "hunter index {hunter}");
        println!(
            "Hunter index {hunter}: onset ticks {onsets:?}, pending answer follows its own timer"
        );
    }
}

#[test]
fn syncer_flash_length_and_chemical_response_follow_cadence() {
    let mut widths = vec![];
    for speed in [1_f64, 3.] {
        let mut w = World::new(input(speed, 50., &[(1, 0), (2, 0), (3, 0), (4, 0), (5, 0)]));
        let mut f = *w
            .flies
            .iter()
            .find(|f| f.species == Species::Syncer)
            .unwrap();
        f.female = false;
        f.state = fly::State::Patrol;
        f.tree = None;
        f.activity = 0.;
        f.phase = 0.;
        f.period = 100.;
        f.remaining = 0;
        f.heard_age = f32::INFINITY;
        f.light = 0.;
        f.fire(0., 1.);
        if speed == 1. {
            for ms in 0..800 {
                f.since = ms as f32 / 1000.;
                let t = f.since;
                let old = if t < 0.085 {
                    terrain::smooth(t / 0.085)
                } else if t < 0.09 {
                    1.
                } else {
                    (1. - terrain::smooth((t - 0.09) / 0.265)).powi(2)
                };
                assert_eq!(f.envelope(0., 0., 1.), old, "default at {ms} ms");
            }
        }
        f.since = 0.;
        let mut continuous = 0;
        for ms in 0..800 {
            f.since = ms as f32 / 1000.;
            continuous += usize::from(f.envelope(0., 0., w.cadence) > 0.5);
        }
        f.since = 0.;
        w.flies = vec![f];
        let mut lit_frames = 0;
        for tick in 1..=30 {
            w.step(tick);
            lit_frames += usize::from(w.flies[0].light > 0.5);
        }
        widths.push((continuous, lit_frames, w.cadence));
        println!(
            "Speed {speed}: half-bright envelope {continuous} ms, observed {lit_frames} ticks"
        );
    }
    assert!((widths[1].0 as f32 - widths[0].0 as f32 / widths[1].2).abs() <= 1.);
    assert!(widths[1].1 < widths[0].1);
    assert!((widths[1].1 as f32 - widths[0].1 as f32 / widths[1].2).abs() <= 1.);
}

#[test]
fn seeded_individual_offsets_break_group_lockstep_within_one_frame() {
    fn onsets() -> (Vec<u64>, Vec<u32>) {
        let mut w = World::new(input(1., 50., &[(1, 0), (2, 0), (3, 0), (4, 0), (5, 0)]));
        let mut syncers: Vec<_> = w
            .flies
            .iter()
            .copied()
            .filter(|f| f.species == Species::Syncer)
            .collect();
        syncers.sort_by(|a, b| a.sync_offset.total_cmp(&b.sync_offset));
        let selected = [
            syncers[0],
            syncers[syncers.len() / 2],
            *syncers.last().unwrap(),
        ];
        let offsets = selected.map(|f| f.sync_offset.to_bits()).to_vec();
        w.flies = selected.to_vec();
        w.sync_gain = 0.; // Supply the same observed group clock to each individual.
        w.phase_coupling = 3.5;
        for f in &mut w.flies {
            assert!(f.sync_offset.abs() <= 1. / 30.);
            f.female = false;
            f.state = fly::State::Patrol;
            f.tree = None;
            f.activity = 0.;
            f.period = 2.;
            f.heard_period = 2.;
            f.phase = 0.1;
            f.heard_phase = 0.1;
            f.heard_age = 0.;
            f.since = 99.;
            f.burst = 1;
            f.remaining = 0;
            f.last_flash = f64::NEG_INFINITY;
        }
        let mut first = vec![0; 3];
        for tick in 1..=90 {
            w.step(tick);
            for (i, f) in w.flies.iter().enumerate() {
                if first[i] == 0 && f.last_flash.is_finite() {
                    first[i] = tick;
                }
            }
        }
        (first, offsets)
    }
    let (first, offsets) = onsets();
    assert_eq!(onsets(), (first.clone(), offsets));
    assert!(first[0] > 0 && first[0] < first[2], "onsets {first:?}");
    assert!(first[2] - first[0] <= 2, "onsets {first:?}");
    println!("Common-clock seeded group onset ticks {first:?}");
}

#[test]
fn syncer_bursts_keep_their_pulses_and_rest_across_the_speed_range() {
    let mut spacings = vec![];
    for speed in [0.1, 0.5, 1., 2., 3.] {
        let mut w = World::new(input(speed, 50., &[(1, 0), (2, 0), (3, 0), (4, 0), (5, 0)]));
        let mut f = *w
            .flies
            .iter()
            .find(|f| f.species == Species::Syncer)
            .unwrap();
        f.female = false;
        f.state = fly::State::Patrol;
        f.phase = 0.;
        f.remaining = 0;
        f.activity = 0.;
        f.heard_age = f32::INFINITY;
        w.flies = vec![f];
        let mut bursts = vec![vec![]];
        let mut gaps = vec![];
        let mut last = None;
        for tick in 1..=3600 {
            let before = w.flies[0].last_flash;
            w.step(tick);
            if w.flies[0].last_flash != before {
                if let Some(previous) = last {
                    let gap = tick - previous;
                    if gap > 60 {
                        bursts.push(vec![]);
                    } else {
                        gaps.push(gap);
                    }
                }
                bursts.last_mut().unwrap().push(tick);
                last = Some(tick);
            }
        }
        assert!(bursts.len() >= 3);
        for burst in &bursts[..bursts.len() - 1] {
            assert_eq!(burst.len(), usize::from(f.burst), "Speed {speed}");
        }
        let shortest = *gaps.iter().min().unwrap();
        let longest = *gaps.iter().max().unwrap();
        assert!(longest - shortest <= 1, "Speed {speed}: {gaps:?}");
        spacings.push(shortest);
        println!(
            "Speed {speed}: {} bursts of {} pulses, spacing {shortest}–{longest} ticks",
            bursts.len(),
            f.burst
        );
    }
    assert!(spacings.windows(2).all(|pair| pair[0] > pair[1]));
}

#[test]
fn randomized_onsets_keep_monotonic_timestamps_and_respond_to_close_answers() {
    let mut minimum = f64::INFINITY;
    let mut onsets = 0;
    let mut compared = 0;
    let mut scripted = 0;
    for seed in 0..16 {
        let mut rng = Pcg32::new(seed, 0xf1);
        let mut i = input(
            between(&mut rng, 0.25, 3.) as f64,
            between(&mut rng, 20., 100.) as f64,
            &[(1, u64::from(rng.next_u32() % 101))],
        );
        i.seed = seed;
        let mut w = World::new(i);
        let mut last = vec![None; w.flies.len() + w.far_count];
        let mut observed = w
            .flies
            .iter()
            .map(|f| f.last_flash)
            .chain(w.distant[..w.far_count].iter().map(|f| f.last_flash))
            .collect::<Vec<_>>();
        let mut night = w.night;
        for tick in 1..=18_000 {
            if rng.next_u32().is_multiple_of(90) {
                let id = 1 + rng.next_u32() % 5;
                let x = between(&mut rng, 100., 2460.);
                w.inject(Event {
                    id,
                    at_ns: w.rate_ns(tick),
                    anchor: [x as f64, 1100.],
                    strength: 1.,
                })
                .unwrap();
                scripted += 1;
            }
            // Exercise answers arbitrarily close to other onset sources.
            if rng.next_u32().is_multiple_of(7) {
                let id = rng.next_u32() as usize % w.flies.len();
                w.flies[id].answer = between(&mut rng, 0.001, 0.10);
            }
            w.tick = tick;
            w.advance(1. / 30., false);
            if w.night != night {
                // A new population has new light identities, as in the trail history.
                night = w.night;
                last.fill(None);
                observed.fill(f64::NEG_INFINITY);
            }
            for (id, time) in w
                .flies
                .iter()
                .map(|f| f.last_flash)
                .chain(w.distant[..w.far_count].iter().map(|f| f.last_flash))
                .enumerate()
            {
                if time > observed[id] {
                    onsets += 1;
                    if let Some(previous) = last[id] {
                        let spacing = time - previous;
                        minimum = minimum.min(spacing);
                        assert!(
                            spacing > 0.,
                            "seed {seed}, light {id}, tick {tick}, spacing {spacing}"
                        );
                        compared += 1;
                    }
                    last[id] = Some(time);
                    observed[id] = time;
                }
            }
        }
    }
    println!(
        "Onset timing: 16 seeds x 18000 ticks, {scripted} scripted events, {onsets} onsets, {compared} consecutive pairs, minimum {minimum:.12} s"
    );
    assert!(compared > 100_000 && minimum > 0. && minimum < 0.5);
}

#[test]
fn event_anchors_must_remain_finite_when_converted() {
    let mut w = World::new(input(1., 50., &[]));
    for anchor in [
        [f64::MAX, 1000.],
        [-f64::MAX, 1000.],
        [1000., f64::MAX],
        [1000., f64::NAN],
    ] {
        assert!(
            w.inject(Event {
                id: events::HUNT,
                at_ns: 0,
                anchor,
                strength: 1.
            })
            .is_err()
        );
    }
    assert!(w.script.is_empty());
    assert!(
        w.inject(Event {
            id: events::HUNT,
            at_ns: 0,
            anchor: [1280., 1000.],
            strength: 1.
        })
        .is_ok()
    );
}

#[test]
fn onset_commit_uses_timestamp_even_when_envelope_age_is_ahead() {
    let mut last = 10.;
    let mut age = 99.;
    assert!(fly::commit_flash(10.1, &mut last, &mut age));
    assert_eq!((last, age), (10.1, 0.));
    age = 99.;
    assert!(fly::commit_flash(10.5, &mut last, &mut age));
    assert_eq!((last, age), (10.5, 0.));
    age = 99.;
    assert!(fly::commit_flash(10.5, &mut last, &mut age));
    assert_eq!((last, age), (10.5, 0.));
}
