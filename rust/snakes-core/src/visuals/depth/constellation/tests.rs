use super::*;
use crate::visuals::depth::{
    common::layout::{Layout, OutputRect},
    schema::Value,
};
pub(super) fn input(seed: u64) -> WorldInput {
    let rect = Rect {
        x: 0.,
        y: 0.,
        width: 2560.,
        height: 1440.,
    };
    WorldInput {
        visual_id: 2,
        seed,
        mode: MonitorMode::Independent,
        layout: Layout::new(
            vec![OutputRect {
                id: 1,
                rect,
                primary: true,
            }],
            1,
        )
        .unwrap(),
        speed: 1.,
        scale: 1.,
        density: 50.,
        trails: 0.35,
        palette: 5,
        reduced_motion: false,
        parameters: Vec::new(),
        environment: Environment::default(),
    }
}
#[test]
fn graph_survives_motion_and_shocks() {
    let mut sky = ConstellationWorld::new(input(3));
    assert!(sky.graph.edges.len() > 4);
    let count = sky
        .graph
        .edges
        .iter()
        .filter(|e| e.wave.is_finite())
        .count();
    eprintln!("prewarmed recognised edges {count}");
    for (id, t) in [
        (4, 1),
        (3, 3),
        (1, 6),
        (2, 12),
        (5, 20),
        (6, 25),
        (7, 30),
        (8, 40),
    ] {
        sky.inject(Event {
            id,
            at_ns: t * 1_000_000_000,
            anchor: [1280., 720.],
            strength: 1.,
        })
        .unwrap();
    }
    let mut seen = [false; 8];
    for k in 1..=4800 {
        sky.step(k);
        sky.graph.verify(&sky.stars);
        for e in &sky.events {
            seen[e.kind as usize - 1] = true
        }
    }
    assert!(seen.iter().all(|&s| s));
}
#[test]
fn opt_out_and_still() {
    let mut i = input(1);
    i.reduced_motion = true;
    let mut sky = ConstellationWorld::new(i);
    assert!(sky.graph.edges.iter().any(|e| e.wave.is_finite()));
    let positions = sky.stars.iter().map(|s| s.p).collect::<Vec<_>>();
    for k in 1..=400 {
        sky.step(k)
    }
    assert_eq!(positions, sky.stars.iter().map(|s| s.p).collect::<Vec<_>>());
    assert!(sky.events.is_empty());
    let mut i = input(1);
    i.trails = 0.;
    i.parameters = vec![
        Value {
            id: 3,
            kind: 1,
            payload: 0,
        },
        Value {
            id: 4,
            kind: 2,
            payload: 0,
        },
        Value {
            id: 5,
            kind: 2,
            payload: 0,
        },
    ];
    let mut sky = ConstellationWorld::new(i);
    assert!(sky.art.band.is_none());
    assert!(sky.graph.edges.is_empty());
    let age = sky.stars[0].age;
    for id in 1..=8 {
        sky.inject(Event {
            id,
            at_ns: 1,
            anchor: [100., 100.],
            strength: 1.,
        })
        .unwrap();
    }
    for k in 1..=800 {
        sky.step(k)
    }
    assert!(sky.events.is_empty());
    assert_eq!(sky.stars[0].age, age);
}
#[test]
fn binaries_orbit_and_unbind() {
    let mut sky = ConstellationWorld::new(input(7));
    let p = pt(650., 520.);
    for (i, x) in [0., 8.].into_iter().enumerate() {
        sky.stars[i].p = p + pt(x, 0.);
        sky.stars[i].v = Point::default();
        sky.stars[i].mag = 1.;
    }
    sky.regrid();
    for t in 1..120 {
        sky.tick = t * 60;
        sky.binaries();
        if sky.binaries.iter().any(|b| b.a == 0 && b.b == 1) {
            break;
        }
    }
    assert!(sky.binaries.iter().any(|b| b.a == 0 && b.b == 1));
    sky.spawn(Kind::Comet, Some(p), 120.);
    sky.events_step(120., 1. / 60.);
    assert!(!sky.binaries.iter().any(|b| b.a == 0 && b.b == 1));
}

pub(super) fn spanning() -> WorldInput {
    let mut i = input(1);
    i.mode = MonitorMode::Spanning;
    i.palette = 0;
    let rects = [
        (0., 0., 2560., 1440.),
        (2560., 0., 3440., 1440.),
        (6000., 195., 1920., 1080.),
    ];
    i.layout = Layout::new(
        rects
            .into_iter()
            .enumerate()
            .map(|(j, (x, y, width, height))| OutputRect {
                id: j as u64,
                rect: Rect {
                    x,
                    y,
                    width,
                    height,
                },
                primary: j == 1,
            })
            .collect(),
        1,
    )
    .unwrap();
    i
}
#[test]
fn recording_opens_with_an_active_atlas() {
    let mut world = ConstellationWorld::new(spanning());
    // These are the events in the Recording scene, in absolute world time.
    for (id, seconds, anchor) in [
        (3, 80, [3400., 600.]),
        (10, 103, [2560., 600.]),
        (1, 132, [2560., 700.]),
        (4, 146, [2700., 600.]),
        (5, 160, [-30., 405.]),
    ] {
        world
            .inject(Event {
                id,
                at_ns: seconds * 1_000_000_000,
                anchor,
                strength: 1.,
            })
            .unwrap();
    }
    let (mut trace, mut wave) = (None, None);
    let mut previous = Vec::with_capacity(world.stars.len());
    let mut max_step = 0f32;
    let mut waves = Vec::new();
    for tick in 1..=10_800 {
        previous.clone_from(&world.stars);
        world.step(tick);
        for (id, (a, b)) in previous.iter().zip(&world.stars).enumerate() {
            if a.alive && b.alive && a.born == b.born {
                let step = (b.p - a.p).len();
                assert!(step < 2., "tick {tick}: star {id} jumped {step:.2} px");
                max_step = max_step.max(step);
            }
        }
        let t = tick as f32 / 60.;
        if (120. ..=135.).contains(&t) {
            for e in &world.graph.edges {
                if e.born >= 120. && e.born <= t {
                    trace.get_or_insert(e.born);
                }
                if e.live()
                    && e.wave >= 120.
                    && e.wave <= t
                    && !world.graph.edges.iter().any(|other| {
                        other.live() && other.figure == e.figure && other.wave < e.wave
                    })
                {
                    if !waves.contains(&(e.figure, e.wave.to_bits()))
                        && (world.stars[e.figure].p.x - 2560.).abs() < 800.
                    {
                        waves.push((e.figure, e.wave.to_bits()));
                        let pts: Vec<_> = world
                            .graph
                            .edges
                            .iter()
                            .filter(|f| f.live() && f.figure == e.figure)
                            .map(|f| (world.stars[f.a].p, world.stars[f.b].p))
                            .collect();
                        eprintln!(
                            "seam recognition start {} figure {} edges {:?}",
                            e.wave, e.figure, pts
                        );
                    }
                    if wave.is_none() {
                        eprintln!(
                            "opening wave at {} world {:?}",
                            e.wave, world.stars[e.figure].p
                        );
                    }
                    wave.get_or_insert(e.wave);
                }
            }
        }
    }
    eprintln!(
        "first trace {trace:?} and recognition wave {wave:?}, maximum continuous displacement {max_step:.3} px/tick"
    );
    assert!(trace.is_some() && wave.is_some());
}

#[test]
fn records_replay_and_continue_across_the_seam() {
    use super::render::{Frame, Primitive};
    use crate::visuals::depth::ffi::View;
    let capture = || {
        let mut world = ConstellationWorld::new(spanning());
        world
            .inject(Event {
                id: 1,
                at_ns: 5_000_000_000,
                anchor: [2560., 600.],
                strength: 1.,
            })
            .unwrap();
        for k in 1..=319 {
            world.step(k)
        }
        let lo = world.snapshot();
        world.step(320);
        let hi = world.snapshot();
        let f = lo.typed.unwrap().downcast::<Frame>().unwrap();
        let g = hi.typed.unwrap().downcast::<Frame>().unwrap();
        let mut out = vec![Primitive::default(); 4096];
        [0, 1].map(|i| {
            let v = View {
                output_id: i,
                layout_generation: 1,
                timeline_ns: 5_320_000_000,
                viewport: world.input.layout.outputs[i as usize].rect,
                uniform_scale: 1.,
                offset_x: 0.,
                offset_y: 0.,
                dpr: 1.,
            };
            let n = super::render::draw(&f, &g, &v, 0.2, false, &mut out);
            out[..n].to_vec()
        })
    };
    let views = capture();
    assert_eq!(views, capture());
    let meteor = views[0]
        .iter()
        .find(|p| p.shape[1] == 7. && p.ends[0] < 2560. && p.ends[2] > 2560.)
        .expect("meteor straddles seam");
    assert!(views[1].iter().any(|p| p.shape == meteor.shape
        && p.rgba == meteor.rgba
        && (p.ends[0] + 2560. - meteor.ends[0]).abs() < 0.001
        && (p.ends[2] + 2560. - meteor.ends[2]).abs() < 0.001));
}

#[test]
fn a_star_outside_a_scaled_crop_can_still_cast_a_spike_into_it() {
    use super::render::{Frame, Primitive};
    use crate::visuals::depth::ffi::View;

    let mut profile = input(2);
    profile.scale = 2.;
    let world = ConstellationWorld::new(profile);
    let mut frame = Frame::new(world.art.clone());
    frame.still = true;
    frame.life = false;
    let mut giant = world.stars[0];
    giant.mag = 0.;
    giant.flux = 1.;
    giant.core_light = 1.;
    frame.stars.push(giant);
    let view = View {
        output_id: 1,
        layout_generation: 1,
        timeline_ns: 0,
        viewport: Rect {
            x: 2000.,
            y: 100.,
            width: 300.,
            height: 200.,
        },
        uniform_scale: 1.5,
        offset_x: -75.,
        offset_y: -45.,
        dpr: 1.,
    };
    // The crop sees world x = 2050..2250. Only one spike reaches it from either side.
    for x in [2020., 2280.] {
        frame.stars[0].p = pt(x, 200.);
        let mut records = [Primitive::default(); 8];
        let count = super::render::draw(&frame, &frame, &view, 0., false, &mut records);
        assert_eq!(count, 1);
        assert_eq!(records[0].shape[1], 7.);
        assert!((0. ..=300.).contains(&records[0].ends[0]));
    }
}

#[test]
fn still_night_preserves_the_settled_chart_while_the_current_slows() {
    let mut i = input(13);
    i.parameters = vec![Value {
        id: 4,
        kind: 2,
        payload: 0,
    }];
    let mut sky = ConstellationWorld::new(i);
    sky.spawn(Kind::Still, Some(pt(880., 600.)), -4.);
    let held = sky
        .graph
        .edges
        .iter()
        .filter(|e| e.live())
        .map(|e| (e.a, e.b))
        .collect::<Vec<_>>();
    assert!(!held.is_empty());
    for t in 1..=1800 {
        sky.step(t);
        sky.graph.verify(&sky.stars);
    }
    let retained = held
        .iter()
        .filter(|&&(a, b)| {
            sky.graph
                .edges
                .iter()
                .any(|e| e.a == a && e.b == b && e.live())
        })
        .count();
    assert!(retained * 2 >= held.len());
    assert!(sky.calm > 0.99);
}

#[test]
fn quiet_sky_keeps_valid_young_links() {
    let mut i = input(13);
    i.speed = 3.;
    i.trails = 1.;
    i.parameters = [4, 5]
        .map(|id| Value {
            id,
            kind: 2,
            payload: 0,
        })
        .to_vec();
    let mut sky = ConstellationWorld::new(i);
    for tick in 1..=7200 {
        sky.step(tick);
        sky.graph.verify(&sky.stars);
    }
}

#[test]
#[ignore = "release measurement on the three-output stress layout"]
fn prewarm_budget() {
    let mut input = spanning();
    input.seed = 7;
    input.density = 100.;
    input.speed = 3.;
    input.scale = 2.;
    input.trails = 1.;
    let sky = ConstellationWorld::new(input);
    eprintln!(
        "180 simulated seconds of pre-roll at stress: {:?}, {} active stars",
        sky.prewarm_time, sky.target_count
    );
    assert!(sky.prewarm_time < std::time::Duration::from_millis(5));
}

#[test]
fn a_full_sky_makes_room_for_a_new_nursery() {
    let mut sky = ConstellationWorld::new(input(9));
    for s in &mut sky.stars {
        s.alive = true;
    }
    sky.spawn(Kind::Birth, Some(pt(1000., 500.)), 0.);
    let expected = sky.events.last().unwrap().count;
    for tick in 1..=1920 {
        sky.step(tick);
    }
    assert_eq!(
        sky.events
            .iter()
            .find(|e| e.kind == Kind::Birth && e.count > 0)
            .unwrap()
            .emitted,
        expected
    );
    assert!(sky.stars.iter().filter(|s| s.alive && s.born >= 0.).count() >= expected);
}

#[test]
fn a_reborn_slot_cannot_move_its_previous_binary_partner() {
    let mut sky = ConstellationWorld::new(input(1));
    sky.binaries.push(Binary {
        a: 0,
        b: 1,
        r: 8.,
        turn: Point::polar(0.01),
        born: 0.,
    });
    sky.stars[0].age = sky.stars[0].life;
    sky.lifecycle(2.);
    assert!(!sky.stars[0].alive || sky.stars[0].born == 2.);
    assert_eq!(sky.stars[0].group, FIELD);
    assert!(sky.binaries.is_empty());
}

#[test]
fn long_lived_figures_keep_their_spacing_and_drift() {
    use crate::visuals::depth::common::rng::output_seed;
    let mut measurements = Vec::new();
    for (name, seed, gravity) in [("A", 1, 40), ("G", 7, 40), ("gravity100", 1, 100)] {
        let mut i = input(output_seed(seed, 2, "Virtual-0"));
        if name == "G" {
            i.speed = 3.;
            i.scale = 2.;
            i.density = 80.;
            i.trails = 1.;
        }
        i.parameters = vec![Value {
            id: 1,
            kind: 1,
            payload: gravity,
        }];
        let mut sky = ConstellationWorld::new(i);
        let mut sample = Vec::new();
        let mut angles = [0usize; 12];
        let mut peak = 0f32;
        for k in 1..=108000 {
            sky.step(k);
            if k % 300 == 0 {
                for star in sky.stars.iter().filter(|s| s.linkable()) {
                    peak = peak.max(star.target.len());
                }
                for e in sky.graph.edges.iter().filter(|e| e.live()) {
                    let axis = sky.stars[e.b].p - sky.stars[e.a].p;
                    let angle = axis.y.atan2(axis.x).to_degrees().rem_euclid(180.);
                    angles[((angle / 15.) as usize).min(11)] += 1;
                }
            }
            if [35940, 107940].contains(&k) {
                sample = sky.stars.clone();
            }
            if ![36000, 108000].contains(&k) {
                continue;
            }
            sky.graph.verify(&sky.stars);
            let mut drift = Vec::new();
            let mut singles = 0;
            let mut count = 0;
            let mut largest = 0;
            for (j, s) in sky.stars.iter().enumerate().filter(|(_, s)| s.linkable()) {
                count += 1;
                let mut nearest = f32::INFINITY;
                let mut crowd = 1;
                for (_, q) in sky
                    .stars
                    .iter()
                    .enumerate()
                    .filter(|(h, q)| *h != j && q.linkable())
                {
                    let d = (q.p - s.p).len();
                    nearest = nearest.min(d);
                    crowd += usize::from(d < 60.);
                }
                singles += usize::from(nearest >= 28.);
                largest = largest.max(crowd);
                if s.born == sample[j].born {
                    let distance = (s.p - sample[j].p).len();
                    drift.push(distance);
                }
            }
            // Every maximal disk containing at least two distinct stars can
            // be moved until two stars touch its boundary.
            let points: Vec<_> = sky
                .stars
                .iter()
                .filter(|s| s.linkable())
                .map(|s| s.p)
                .collect();
            let mut disk_count = largest;
            for (a, &p) in points.iter().enumerate() {
                for &q in &points[a + 1..] {
                    let d = q - p;
                    if d.len2() > 14400. || d.len2() < 0.001 {
                        continue;
                    }
                    let normal = pt(-d.y, d.x).unit() * (3600. - d.len2() * 0.25).max(0.).sqrt();
                    for center in [p.mix(q, 0.5) + normal, p.mix(q, 0.5) - normal] {
                        disk_count = disk_count.max(
                            points
                                .iter()
                                .filter(|&&x| (x - center).len2() <= 3600.1)
                                .count(),
                        );
                    }
                }
            }
            largest = disk_count;
            drift.sort_unstable_by(f32::total_cmp);
            let mut figures = std::collections::BTreeMap::new();
            for e in sky.graph.edges.iter().filter(|e| e.live()) {
                let entry = figures.entry(sky.graph.root(e.a)).or_insert((0, false));
                entry.0 += 1;
                entry.1 |= sky.stars[e.a].p.y < 720. || sky.stars[e.b].p.y < 720.;
            }
            let readable = figures.values().filter(|v| v.0 >= 2).count();
            let upper = figures.values().filter(|v| v.0 >= 2 && v.1).count();
            eprintln!(
                "{name} {}s: {readable} figures ({upper} upper), {singles}/{count} spaced, largest group in 60px {largest}, drift min/median/max {:.2}/{:.2}/{:.2}",
                k / 60,
                drift[0],
                drift[drift.len() / 2],
                drift.last().unwrap()
            );
            measurements.push((
                name,
                (3..=6).contains(&readable)
                    && upper > 0
                    && singles * 100 >= 95 * count
                    && largest <= 5
                    && drift[drift.len() / 2] > 0.5
                    && (sky.calm > 0. || drift[0] >= 0.3),
            ));
        }
        let mean = angles.iter().sum::<usize>() as f32 / angles.len() as f32;
        assert!(
            angles
                .iter()
                .all(|n| (*n as f32 - mean).abs() <= 0.25 * mean),
            "{name} angles {angles:?}"
        );
        eprintln!("{name} 30-minute peak current target {peak:.2}, angle histogram: {angles:?}");
    }
    assert!(measurements.iter().all(|m| m.1), "{measurements:?}");
}

#[test]
fn capture_double_orbits_and_satellite_flares_on_the_seam() {
    use crate::visuals::depth::common::rng::output_seed;
    let mut world = ConstellationWorld::new(input(output_seed(7, 2, "Virtual-0")));
    world
        .inject(Event {
            id: 9,
            at_ns: 2_000_000_000,
            anchor: [1280., 650.],
            strength: 1.,
        })
        .unwrap();
    let mut axes = Vec::new();
    for tick in 1..=1560 {
        world.step(tick);
        if [1200, 1320, 1440, 1560].contains(&tick) {
            let pair = world
                .binaries
                .iter()
                .find(|b| world.stars[b.a].companion || world.stars[b.b].companion)
                .expect("a co-natal double captured before the first exposure");
            let axis = world.stars[pair.b].p - world.stars[pair.a].p;
            assert!((5.9..=12.1).contains(&axis.len()));
            axes.push(axis);
            eprintln!(
                "Binary {}s: primary {:?}, separation {:.2}, orientation {:.1} degrees",
                tick / 60,
                world.stars[pair.a].p,
                axis.len(),
                axis.y.atan2(axis.x).to_degrees()
            );
        }
    }
    assert!((axes[0] - axes[1]).len() > 3.);
    let mut world = ConstellationWorld::new(spanning());
    world.spawn(Kind::Satellite, Some(pt(2560., 580.)), 5.);
    let sat = world.events.last().unwrap();
    assert_eq!(sat.position(21.), pt(2560., 580.));
    assert!(sat.variant < 0.25);
}

#[test]
fn gravity_stretches_associations_into_filaments() {
    use crate::visuals::depth::common::rng::output_seed;
    let mut widths = Vec::new();
    for gravity in [0, 40, 100] {
        let mut i = input(output_seed(1, 2, "Virtual-0"));
        i.parameters = vec![Value {
            id: 1,
            kind: 1,
            payload: gravity,
        }];
        let mut sky = ConstellationWorld::new(i);
        // Compare the same birth cohorts in all three skies. Later nurseries
        // have different ages and numbers of members and are a separate event.
        let original: std::collections::BTreeSet<_> = sky
            .stars
            .iter()
            .filter(|s| s.offset != Point::default())
            .map(|s| s.group)
            .collect();
        for k in 1..=36000 {
            sky.step(k);
        }
        let mut groups = std::collections::BTreeMap::<usize, Vec<Point>>::new();
        for s in sky.stars.iter().filter(|s| {
            s.born < 0.
                && s.linkable()
                && s.group != FIELD
                && s.offset != Point::default()
                && original.contains(&s.group)
        }) {
            groups.entry(s.group).or_default().push(s.p);
        }
        let mut ratios = Vec::new();
        for members in groups.values().filter(|g| g.len() >= 4) {
            let mean = members.iter().copied().fold(Point::default(), |a, b| a + b)
                / (members.len() as f32);
            let (mut xx, mut yy, mut xy) = (0., 0., 0.);
            for &s in members {
                let d = s - mean;
                xx += d.x * d.x;
                yy += d.y * d.y;
                xy += d.x * d.y;
            }
            let eig = ((xx - yy).powi(2) + 4. * xy * xy).sqrt();
            ratios.push(((xx + yy - eig) / (xx + yy + eig)).max(0.).sqrt());
        }
        ratios.sort_unstable_by(f32::total_cmp);
        let width = ratios[ratios.len() / 2];
        widths.push(width);
        eprintln!("Gravity {gravity}: median minor/major axis ratio {width:.3} at 600s");
    }
    assert!(widths[2] < widths[1] && widths[2] < widths[0]);
}

#[test]
fn moving_ink_cannot_cross_retracting_strokes_in_scene_g() {
    use super::render::{Frame, Primitive};
    use crate::visuals::depth::{common::rng::output_seed, ffi::View};
    let mut input = input(output_seed(7, 2, "Virtual-0"));
    input.speed = 3.;
    input.scale = 2.;
    input.density = 80.;
    input.trails = 1.;
    input.palette = 2;
    let rect = input.layout.bounds;
    let mut world = ConstellationWorld::new(input);
    let mut records = vec![Primitive::default(); 4096];
    for k in 0..=1320 {
        if k > 0 {
            world.step(k);
        }
        if k % 15 != 0 {
            continue;
        }
        let upper = world.snapshot().typed.unwrap().downcast::<Frame>().unwrap();
        let view = View {
            output_id: 1,
            layout_generation: 1,
            timeline_ns: k as i64 * 1_000_000_000 / 60,
            viewport: rect,
            uniform_scale: 1.,
            offset_x: 0.,
            offset_y: 0.,
            dpr: 1.,
        };
        let n = render::draw(&upper, &upper, &view, 0., false, &mut records);
        let lines: Vec<_> = records[..n]
            .iter()
            .filter(|r| matches!(r.shape[1], 0. | 9.) && r.rgba[3] > 0.08)
            .collect();
        for (i, a) in lines.iter().enumerate() {
            for b in &lines[..i] {
                let [ax, ay, bx, by] = a.ends;
                let [cx, cy, dx, dy] = b.ends;
                let u = pt(bx - ax, by - ay);
                let v = pt(dx - cx, dy - cy);
                let d = pt(cx - ax, cy - ay);
                let cross = |p: Point, q: Point| p.x * q.y - p.y * q.x;
                let determinant = cross(u, v);
                let (l, m) = (u.len(), v.len());
                if l > 24. && m > 24. && determinant.abs() < 0.258819 * l * m {
                    let start = d.dot(u) / l;
                    let width = u.dot(v) / l;
                    let stop = start + width;
                    let lo = start.min(stop).max(0.);
                    let hi = start.max(stop).min(l);
                    if hi - lo > 12. {
                        let midpoint = (lo + hi) * 0.5;
                        let distance = (cross(u, d) + (midpoint - start) / width * determinant) / l;
                        assert!(distance.abs() >= 39.999, "parallel ink at tick {k}");
                    }
                }
                if determinant.abs() < 0.00001 {
                    continue;
                }
                let a = cross(d, v) / determinant;
                let b = cross(d, u) / determinant;
                assert!(
                    !((0.001..0.999).contains(&a) && (0.001..0.999).contains(&b)),
                    "rendered strokes cross at tick {k}"
                );
            }
        }
    }
}

fn interpolation_stress_input(seed: u64) -> WorldInput {
    let mut input = input(seed);
    input.speed = 3.;
    input.scale = 2.;
    input.density = 100.;
    input.trails = 1.;
    input.palette = 2;
    input.parameters = vec![Value {
        id: 1,
        kind: 1,
        payload: 100,
    }];
    input
}

fn assert_presented_ink_clear(
    lower: &render::Frame,
    upper: &render::Frame,
    seed: u64,
    tick: u64,
    alpha: f32,
) {
    use crate::visuals::depth::ffi::View;
    let view = View {
        output_id: 1,
        layout_generation: 1,
        timeline_ns: (((tick as f64 + alpha as f64) / 60.) * 1e9).round() as i64,
        viewport: input(seed).layout.bounds,
        uniform_scale: 1.,
        offset_x: 0.,
        offset_y: 0.,
        dpr: 1.,
    };
    let mut records = vec![render::Primitive::default(); 4096];
    let count = render::draw(lower, upper, &view, alpha, false, &mut records);
    assert!(count <= records.len());
    let lines: Vec<_> = records[..count]
        .iter()
        .filter(|r| matches!(r.shape[1], 0. | 9.) && r.rgba[3] > 0.001)
        .collect();
    let cross = |a: [f64; 2], b: [f64; 2]| a[0] * b[1] - a[1] * b[0];
    let dot = |a: [f64; 2], b: [f64; 2]| a[0] * b[0] + a[1] * b[1];
    for (i, a) in lines.iter().enumerate() {
        for b in &lines[..i] {
            let [ax, ay, bx, by] = a.ends.map(f64::from);
            let [cx, cy, dx, dy] = b.ends.map(f64::from);
            let u = [bx - ax, by - ay];
            let v = [dx - cx, dy - cy];
            let d = [cx - ax, cy - ay];
            let determinant = cross(u, v);
            let length = dot(u, u).sqrt();
            let other_length = dot(v, v).sqrt();
            let context = || {
                format!(
                    "seed {seed}, tick {tick}, alpha {alpha}: {:?}, {:?}",
                    a.ends, b.ends
                )
            };
            if determinant.abs() > 1e-8 {
                let along = cross(d, v) / determinant;
                let across = cross(d, u) / determinant;
                assert!(
                    !((0.00001..0.99999).contains(&along) && (0.00001..0.99999).contains(&across)),
                    "presented strokes cross: {}",
                    context()
                );
            }
            if length < 0.5
                || other_length < 0.5
                || determinant.abs() >= 15f64.to_radians().sin() * length * other_length
            {
                continue;
            }
            let start = dot(d, u) / length;
            let width = dot(v, u) / length;
            let lo = start.min(start + width).max(0.);
            let hi = start.max(start + width).min(length);
            if hi - lo > 0.001 {
                let distance = |x| (cross(u, d) + (x - start) / width * determinant) / length;
                let (d0, d1) = (distance(lo), distance(hi));
                assert!(
                    d0 * d1 > 0. && d0.abs().min(d1.abs()) >= 39.999,
                    "presented parallel strokes lack clearance: {}, distances {d0}, {d1}",
                    context()
                );
            }
        }
    }
}

#[test]
fn fractional_tick_applies_the_upper_frames_cut() {
    let mut world = ConstellationWorld::new(interpolation_stress_input(7));
    for tick in 1..=319 {
        world.step(tick);
    }
    let lower = world
        .snapshot()
        .typed
        .unwrap()
        .downcast::<render::Frame>()
        .unwrap();
    world.step(320);
    let upper = world
        .snapshot()
        .typed
        .unwrap()
        .downcast::<render::Frame>()
        .unwrap();
    assert_presented_ink_clear(&lower, &lower, 7, 319, 0.);
    assert_presented_ink_clear(&upper, &upper, 7, 320, 0.);
    assert_presented_ink_clear(&lower, &upper, 7, 319, 0.94);
}

#[test]
fn fractional_tick_random_sweep_preserves_ink_clearance() {
    let mut rng = Pcg32::new(0x1a1fa, 1);
    for seed in [1, 3, 7, 13] {
        let mut world = ConstellationWorld::new(interpolation_stress_input(seed));
        let mut lower = world
            .snapshot()
            .typed
            .unwrap()
            .downcast::<render::Frame>()
            .unwrap();
        for tick in 0..720 {
            world.step(tick + 1);
            let upper = world
                .snapshot()
                .typed
                .unwrap()
                .downcast::<render::Frame>()
                .unwrap();
            for alpha in [0., rng.unit() as f32, 1.] {
                assert_presented_ink_clear(&lower, &upper, seed, tick, alpha);
            }
            lower = upper;
        }
    }
}

fn chart_sites(sky: &ConstellationWorld) -> Vec<Point> {
    let mut chart = std::collections::BTreeMap::<usize, std::collections::BTreeSet<usize>>::new();
    for e in sky.graph.edges.iter().filter(|e| e.live()) {
        chart
            .entry(sky.graph.root(e.a))
            .or_default()
            .extend([e.a, e.b]);
    }
    chart
        .values()
        .filter(|stars| stars.len() >= 3)
        .map(|stars| {
            stars
                .iter()
                .fold(Point::default(), |sum, &i| sum + sky.stars[i].p)
                / stars.len() as f32
        })
        .collect()
}
fn site_cv(sites: &[Point]) -> f32 {
    let near: Vec<_> = sites
        .iter()
        .map(|&p| {
            sites
                .iter()
                .filter(|&&q| q != p)
                .map(|&q| (p - q).len())
                .fold(f32::INFINITY, f32::min)
        })
        .collect();
    let mean = near.iter().sum::<f32>() / near.len() as f32;
    (near.iter().map(|&x| (x - mean).powi(2)).sum::<f32>() / near.len() as f32).sqrt() / mean
}
#[test]
fn sites_advect_turn_over_and_respond_to_gravity() {
    use crate::visuals::depth::common::rng::output_seed;
    let mut bright = Vec::new();
    let mut cv = Vec::new();
    let mut overlap = 0.;
    let mut returns = usize::MAX;
    let mut void = false;
    for gravity in [0, 40, 100] {
        let mut i = input(output_seed(1, 2, "Virtual-0"));
        i.parameters.push(Value {
            id: 1,
            kind: 1,
            payload: gravity,
        });
        let mut sky = ConstellationWorld::new(i);
        let start = chart_sites(&sky);
        let cohorts: Vec<_> = sky.groups.iter().map(|a| (a.p, a.life.to_bits())).collect();
        let mut speeds = Vec::new();
        for tick in 1..=if gravity == 40 { 108000 } else { 36000 } {
            sky.step(tick);
            if tick % 600 == 0 {
                speeds.extend(sky.groups.iter().filter(|a| a.active).map(|a| a.v.len()));
            }
            if tick == 36000 {
                let sites = chart_sites(&sky);
                let mut displacement: Vec<_> = sky
                    .groups
                    .iter()
                    .zip(&cohorts)
                    .filter(|(a, (_, id))| a.active && a.life.to_bits() == *id)
                    .map(|(a, (p, _))| (a.p - *p).len())
                    .collect();
                displacement.sort_unstable_by(f32::total_cmp);
                eprintln!(
                    "g{gravity} original cohorts at 600s: {}/{} surviving, displacement {displacement:?}",
                    displacement.len(),
                    cohorts.len()
                );
                assert!(displacement.len() < cohorts.len());
                if !displacement.is_empty() {
                    assert!(displacement[displacement.len() / 2] > 200.);
                }
                let points: Vec<_> = sky
                    .stars
                    .iter()
                    .filter(|s| s.linkable())
                    .map(|s| s.p)
                    .collect();
                cv.push(site_cv(&sites));
                eprintln!(
                    "gravity {gravity} 600s sites {sites:?} CV {:.3}; associations {:?}",
                    cv.last().unwrap(),
                    sky.groups
                        .iter()
                        .filter(|a| a.active)
                        .map(|a| a.p)
                        .collect::<Vec<_>>()
                );
                if gravity == 0 {
                    bright = points;
                } else if gravity == 40 {
                    overlap = points
                        .iter()
                        .filter(|&&p| bright.iter().any(|&q| (p - q).len() < 30.))
                        .count() as f32
                        / points.len() as f32;
                } else {
                    let mut xs = vec![0.];
                    xs.extend(points.iter().map(|p| p.x + 0.01).filter(|&x| x <= 1560.));
                    void = xs.iter().any(|&x| {
                        let mut ys: Vec<_> = points
                            .iter()
                            .filter(|p| p.x >= x && p.x <= x + 1000.)
                            .map(|p| p.y)
                            .collect();
                        ys.extend([0., 1440.]);
                        ys.sort_unstable_by(f32::total_cmp);
                        ys.windows(2).any(|w| w[1] - w[0] >= 700.)
                    });
                }
            }
            if tick == 108000 {
                let sites = chart_sites(&sky);
                assert!(
                    sky.groups.iter().all(
                        |a| !a.active || !cohorts.iter().any(|(_, id)| *id == a.life.to_bits())
                    )
                );
                returns = sites
                    .iter()
                    .filter(|&&p| start.iter().any(|&q| (q - p).len() < 300.))
                    .count();
                eprintln!("A sites at 0s {start:?}; at 1800s {sites:?}");
            }
        }
        speeds.sort_unstable_by(f32::total_cmp);
        assert!((1.5..=2.5).contains(&speeds[speeds.len() / 2]));
        eprintln!(
            "gravity {gravity} median centre speed {:.2}px/s",
            speeds[speeds.len() / 2]
        );
    }
    eprintln!(
        "gravity CV {cv:?}, 0/40 overlap {overlap:.3}, A returns {returns}, 1000x700 void {void}"
    );
    assert!(overlap <= 0.4 && returns <= 2 && cv[0] <= 0.25 && cv[2] >= 0.45 && void);
}
#[test]
fn nursery_traces_its_branching_walk() {
    use crate::visuals::depth::common::rng::output_seed;
    let mut i = input(output_seed(3, 2, "Virtual-0"));
    let mut out = i.layout.outputs[0].clone();
    out.rect.width = 3440.;
    i.layout = Layout::new(vec![out], 1).unwrap();
    i.density = 55.;
    let mut sky = ConstellationWorld::new(i);
    sky.inject(Event {
        id: 3,
        at_ns: 20_000_000_000,
        anchor: [1720., 720.],
        strength: 1.,
    })
    .unwrap();
    for tick in 1..=3600 {
        sky.step(tick);
        if tick != 1920 && tick != 3600 {
            continue;
        }
        let m = sky
            .events
            .iter()
            .find(|e| e.kind == Kind::Birth && e.start == 20.)
            .unwrap();
        let stars: Vec<_> = sky
            .stars
            .iter()
            .enumerate()
            .filter(|(_, s)| s.linkable() && s.group == m.star.unwrap() && (s.p - m.p).len() < 300.)
            .map(|(i, _)| i)
            .collect();
        let mut ignitions: Vec<_> = sky
            .stars
            .iter()
            .filter(|s| s.alive && !s.companion && s.group == m.star.unwrap())
            .map(|s| s.born)
            .collect();
        ignitions.sort_unstable_by(f32::total_cmp);
        assert!(
            ignitions
                .windows(2)
                .all(|w| (2.5..=3.5).contains(&(w[1] - w[0])))
        );
        let ink = sky
            .graph
            .edges
            .iter()
            .filter(|e| e.live() && stars.contains(&e.a) && stars.contains(&e.b))
            .count();
        let connected = stars
            .iter()
            .map(|&a| {
                stars
                    .iter()
                    .filter(|&&b| sky.graph.root(a) == sky.graph.root(b))
                    .count()
            })
            .max()
            .unwrap();
        eprintln!(
            "C {}s: {} newborns within 300px, {ink} strokes, {connected} connected",
            tick / 60,
            stars.len()
        );
        assert!(stars.len() >= 4 && ink >= 2 && (tick != 3600 || connected >= 4));
    }
}
#[test]
fn gathering_contracts_then_releases_neighbouring_figures() {
    use crate::visuals::depth::common::rng::output_seed;
    let mut sky = ConstellationWorld::new(input(output_seed(1, 2, "Virtual-0")));
    sky.inject(Event {
        id: 7,
        at_ns: 10_000_000_000,
        anchor: [1280., 600.],
        strength: 1.,
    })
    .unwrap();
    let mut before = Vec::new();
    let mut peak = Vec::new();
    for tick in 1..=11400 {
        sky.step(tick);
        if ![600, 3300, 11400].contains(&tick) {
            continue;
        }
        let groups: Vec<_> = sky
            .groups
            .iter()
            .enumerate()
            .filter(|(_, a)| a.gather_partner != FIELD)
            .map(|(i, a)| (i, a.p))
            .collect();
        let areas: Vec<_> = groups
            .iter()
            .map(|&(i, _)| {
                let mut lo = pt(f32::INFINITY, f32::INFINITY);
                let mut hi = pt(f32::NEG_INFINITY, f32::NEG_INFINITY);
                for s in sky.stars.iter().filter(|s| s.group == i && s.linkable()) {
                    lo = pt(lo.x.min(s.p.x), lo.y.min(s.p.y));
                    hi = pt(hi.x.max(s.p.x), hi.y.max(s.p.y));
                }
                (hi.x - lo.x) * (hi.y - lo.y)
            })
            .collect();
        eprintln!(
            "Gathering {}s areas {areas:?}, distance {:.1}px",
            tick / 60,
            (groups[0].1 - groups[1].1).len()
        );
        if tick == 600 {
            before = areas;
        } else if tick == 3300 {
            assert!((groups[0].1 - groups[1].1).len() <= 350.);
            peak = areas;
        } else {
            assert!(peak.iter().zip(&before).all(|(p, b)| p / b <= 0.55));
            assert!(areas.iter().zip(&before).all(|(p, b)| p / b > 0.70));
        }
    }
}

#[test]
fn settled_ink_opens_only_at_a_visible_junction() {
    use super::render::{Frame, Primitive};
    use crate::visuals::depth::ffi::View;
    let mut i = input(11);
    i.palette = 0;
    let rect = i.layout.bounds;
    let mut world = ConstellationWorld::new(i);
    let mut records = vec![Primitive::default(); 4096];
    let mut checked = 0;
    for tick in 1..=3600 {
        world.step(tick);
        if tick % 30 != 0 {
            continue;
        }
        let frame = world.snapshot().typed.unwrap().downcast::<Frame>().unwrap();
        let time = tick as f32 / 60.;
        let view = View {
            output_id: 1,
            layout_generation: 1,
            timeline_ns: tick as i64 * 1_000_000_000 / 60,
            viewport: rect,
            uniform_scale: 1.,
            offset_x: 0.,
            offset_y: 0.,
            dpr: 1.,
        };
        let n = render::draw(&frame, &frame, &view, 0., false, &mut records);
        let output = &records[..n];
        for edge in frame
            .edges
            .iter()
            .filter(|e| e.live() && time - e.born >= 1.2)
        {
            let a = frame.stars[edge.a].p;
            let b = frame.stars[edge.b].p;
            let Some(line) = output.iter().find(|p| {
                p.shape[1] == 0.
                    && (pt(p.ends[0], p.ends[1]) - a).len() < 9.
                    && (pt(p.ends[2], p.ends[3]) - b).len() < 9.
            }) else {
                panic!("missing settled stroke {tick} {edge:?}");
            };
            let luma = |p: &Primitive| {
                (0.2126 * p.rgba[0] + 0.7152 * p.rgba[1] + 0.0722 * p.rgba[2]) * p.rgba[3]
            };
            for (id, end) in [
                (edge.a, pt(line.ends[0], line.ends[1])),
                (edge.b, pt(line.ends[2], line.ends[3])),
            ] {
                let s = frame.stars[id];
                let core = output
                    .iter()
                    .filter(|r| {
                        r.shape[1] == 1. && r.shape[2] == 0. && pt(r.ends[0], r.ends[1]) == s.p
                    })
                    .max_by(|a, b| luma(a).total_cmp(&luma(b)))
                    .unwrap();
                assert!((end - s.p).len() <= core.shape[0] + 4.);
                assert!(
                    luma(core) >= 1.6 * luma(line),
                    "junction {id} tick {tick} core {} vs line {}",
                    luma(core),
                    luma(line)
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 500);
}

#[test]
fn advected_centres_avoid_their_previous_sites() {
    use crate::visuals::depth::common::rng::output_seed;
    let mut sky = ConstellationWorld::new(input(output_seed(1, 2, "Virtual-0")));
    let mut paths = std::collections::BTreeMap::<u32, Vec<(u64, Point)>>::new();
    let mut returns = 0;
    let mut tested = 0;
    for tick in 0..=108000 {
        if tick > 0 {
            sky.step(tick);
        }
        if tick % 900 != 0 {
            continue;
        }
        for a in sky.groups.iter().filter(|a| a.active) {
            let path = paths.entry(a.life.to_bits()).or_default();
            // Nearby consecutive samples are one visit. Check return only
            // after the centre has cleared the site's 400px diameter.
            let mut departed = false;
            for &(when, p) in path
                .iter()
                .rev()
                .take_while(|(when, _)| tick - when <= 54000)
            {
                let d = (p - a.p).len();
                if departed && d < 200. {
                    returns += 1;
                    eprintln!(
                        "site revisit at {}s from {}s, {}px",
                        tick / 60,
                        when / 60,
                        d
                    );
                    break;
                }
                if departed {
                    tested += 1;
                }
                departed |= d > 400.;
            }
            path.push((tick, a.p));
        }
    }
    eprintln!("{returns} site revisits in {tested} displaced centre comparisons");
    assert_eq!(returns, 0);
}

#[test]
fn death_fade_retracts_live_ink_before_the_core_disappears() {
    let mut sky = ConstellationWorld::new(input(19));
    let old = *sky.graph.edges.iter().find(|e| e.live()).unwrap();
    sky.next_nova = 1000.;
    sky.stars[old.a].age = sky.stars[old.a].life - 6.01;
    sky.step(1);
    let star = sky.stars[old.a];
    assert!(star.alive && star.fading && !star.linkable());
    assert!((star.fade_end - 6.01).abs() < 0.01);
    let cut = sky
        .graph
        .edges
        .iter()
        .find(|e| e.a == old.a && e.b == old.b)
        .unwrap();
    assert_eq!(cut.end, 1. / 60.);
    for tick in 2..=60 {
        sky.step(tick);
    }
    assert!(sky.stars[old.a].alive);
    assert!(!sky.graph.edges.iter().any(|e| e.a == old.a || e.b == old.a));
}

#[test]
fn shower_exposure_contains_radial_meteors_above_the_horizon() {
    let mut config = spanning();
    config.seed = 5;
    let mut sky = ConstellationWorld::new(config);
    let radiant = pt(4280., 380.);
    for (id, at, p) in [
        (1, 5, [2560., 600.]),
        (5, 20, [-40., 390.]),
        (6, 60, [4280., 380.]),
    ] {
        sky.inject(Event {
            id,
            at_ns: at * 1_000_000_000,
            anchor: p,
            strength: 1.,
        })
        .unwrap();
    }
    let mut tracks = std::collections::BTreeSet::new();
    let mut expected = 0;
    for tick in 1..=4500 {
        sky.step(tick);
        if tick < 3600 || tick % 15 != 0 {
            continue;
        }
        for m in &sky.events {
            if m.kind == Kind::Shower && m.start == 60. {
                expected = m.emitted;
            }
            if m.kind != Kind::Meteor || m.start < 60. || tick as f32 / 60. - m.start > m.duration {
                continue;
            }
            let axis = m.delta.unit();
            let origin = m.p - radiant;
            if (origin.x * axis.y - origin.y * axis.x).abs() > 0.1 {
                continue;
            }
            assert!(origin.dot(axis) >= 0.);
            let end = m.p + m.delta;
            assert!(end.y <= 1152.01);
            if tracks.insert(m.start.to_bits()) {
                eprintln!(
                    "E shower sample {:.2}s start {:?} end {:?}, head {:?}",
                    tick as f32 / 60.,
                    m.p,
                    end,
                    m.position(tick as f32 / 60.)
                );
            }
        }
    }
    eprintln!(
        "E stack samples {} of {expected} emitted shower meteors",
        tracks.len()
    );
    assert!(tracks.len() >= 2 && tracks.len() == expected);
}
