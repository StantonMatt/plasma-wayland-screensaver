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

fn input(seed: u64, size: (f64, f64)) -> WorldInput {
    let rect = Rect {
        x: 0.,
        y: 0.,
        width: size.0,
        height: size.1,
    };
    WorldInput {
        visual_id: 4,
        seed: output_seed(seed, 4, "Virtual-0"),
        mode: MonitorMode::Independent,
        layout: Layout::new(
            vec![OutputRect {
                id: identity_hash("Virtual-0"),
                rect,
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
#[ignore = "export the renderer's Hermite centrelines for the 12-width residual check"]
fn flag_centrelines() {
    use std::io::Write;
    let path = std::env::var_os("PVS_RIBBON_CENTRELINES").unwrap();
    let mut law = std::io::BufWriter::new(
        std::fs::File::create(std::path::Path::new(&path).with_extension("wave.csv")).unwrap(),
    );
    let mut csv = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut headings = std::io::BufWriter::new(
        std::fs::File::create(std::env::var_os("PVS_RIBBON_HEADINGS").unwrap()).unwrap(),
    );
    for (scene, times) in [
        ("A", vec![1800, 4650, 5700]),
        ("G", (18000..18090).step_by(5).collect()),
    ] {
        let mut cfg = input(7, (2560., 1440.));
        if scene == "G" {
            cfg.speed = 3.;
            cfg.scale = 2.;
            cfg.trails = 1.;
            cfg.density = 80.;
            cfg.palette = 2;
        }
        let mut w = Ribbons::new(cfg);
        for tick in 0..=*times.last().unwrap() {
            if tick > 0 {
                w.step(tick);
            }
            if times.iter().any(|t| tick <= *t && tick + 360 >= *t) {
                for (i, r) in w
                    .sea
                    .agents
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| r.alive && r.layer > 0)
                {
                    writeln!(
                        headings,
                        "{scene},{tick},{},{},{},{},{}",
                        r.id,
                        r.heading.x,
                        r.heading.y,
                        u8::from(w.sea.partnered(i)),
                        u8::from(w.sea.weather.active.is_some() || r.release_age > 0.)
                    )
                    .unwrap();
                }
            }
            if !times.contains(&tick) {
                continue;
            }
            let snap = w.snapshot();
            let f = snap
                .typed
                .as_ref()
                .unwrap()
                .downcast_ref::<Frame>()
                .unwrap();
            let mut control = Frame::new(w.art.clone(), w.sea.agents.len(), false);
            control.fill_with_wave(&w.sea, w.params.twist, 0.);
            for strip in &f.strips {
                if strip.layer == 0 {
                    continue;
                }
                let r = w.sea.agents.iter().find(|r| r.id == strip.id).unwrap();
                let length = r.extent() * r.spacing * (r.count - 1) as f64;
                let start = std::f64::consts::TAU * 0.6 * length
                    / (0.85 * flag_phase_scale(length, r.width) * r.width);
                writeln!(law, "{scene},{tick},{},{},{}", r.id, start, 0.15 * start).unwrap();
                let nodes = &f.vertices[strip.offset..strip.offset + strip.count];
                let paired = &control.vertices[strip.offset..strip.offset + strip.count];
                for (node, control) in nodes.iter().zip(paired).skip(1) {
                    writeln!(
                        csv,
                        "{scene},{tick},{},{},{},{},{},{},{},{},{},{},{}",
                        r.id,
                        r.layer,
                        r.width,
                        node.pos[0],
                        node.pos[1],
                        node.normal[1],
                        -node.normal[0],
                        control.pos[0],
                        control.pos[1],
                        control.normal[1],
                        -control.normal[0]
                    )
                    .unwrap();
                }
            }
        }
    }
}

use crate::visuals::depth::ribbons::agent::{Ribbon, turn};

fn intersections(points: &[Vec2]) -> Vec<Vec2> {
    let cross = |a: Vec2, b: Vec2| a.x * b.y - a.y * b.x;
    let mut hits = Vec::new();
    for (i, a) in points.windows(2).enumerate() {
        let direction = a[1] - a[0];
        for b in points.windows(2).skip(i + 3) {
            let e = b[1] - b[0];
            let det = cross(direction, e);
            if det.abs() < 1e-10 {
                continue;
            }
            let offset = b[0] - a[0];
            let t = cross(offset, e) / det;
            let u = cross(offset, direction) / det;
            if (0.0..1.).contains(&t) && (0.0..1.).contains(&u) {
                hits.push(a[0] + direction * t);
            }
        }
    }
    hits
}
fn exported_curve(vertices: &[BodyKnot]) -> Vec<Vec2> {
    let mut out = Vec::with_capacity(3 * vertices.len());
    for pair in vertices.windows(2) {
        let pos = |v: &BodyKnot| p(v.pos[0] as f64, v.pos[1] as f64);
        let tangent = |v: &BodyKnot| p(v.normal[1] as f64, -v.normal[0] as f64).unit();
        let a = pos(&pair[0]);
        let delta = pos(&pair[1]) - a;
        let m0 = tangent(&pair[0]) * delta.length();
        let m1 = tangent(&pair[1]) * delta.length();
        let c2 = delta * 3. - m0 * 2. - m1;
        let c3 = m0 + m1 - delta * 2.;
        for t in [0., 1. / 3., 2. / 3.] {
            out.push(a + (m0 + (c2 + c3 * t) * t) * t);
        }
    }
    let end = vertices.last().unwrap().pos;
    out.push(p(end[0] as f64, end[1] as f64));
    out
}
fn visible_body(r: &Ribbon) -> Vec<Vec2> {
    let end = (r.count - 1) as f64 * r.extent();
    let n = end.floor() as usize;
    let mut nodes: Vec<_> = r.nodes[..=n].iter().map(|k| k.pos).collect();
    if n + 1 < r.count && end.fract() > 1e-8 {
        nodes.push(r.nodes[n].pos + (r.nodes[n + 1].pos - r.nodes[n].pos) * end.fract());
    }
    nodes
}
fn radius(a: Vec2, b: Vec2, c: Vec2) -> f64 {
    let u = b - a;
    let v = c - b;
    let sine = turn(u, v).sin().abs();
    if sine < 1e-9 {
        f64::INFINITY
    } else {
        (c - a).length() / (2. * sine)
    }
}
fn spanning(gap: f64) -> WorldInput {
    let mut cfg = input(31, (2560., 1440.));
    cfg.mode = MonitorMode::Spanning;
    cfg.seed = 31;
    let outputs = [
        (0., 0., 2560., 1440.),
        (2560. + gap, 0., 3440., 1440.),
        (6000. + gap, 180., 1920., 1080.),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (x, y, width, height))| OutputRect {
        id: identity_hash(&format!("Virtual-{i}")),
        rect: Rect {
            x,
            y,
            width,
            height,
        },
        primary: i == 0,
    })
    .collect();
    cfg.layout = Layout::new(outputs, 1).unwrap();
    cfg
}
#[test]
#[ignore = "five minute three-output loop census and margin radius fixtures"]
fn spanning_balance_and_gaps() {
    for (name, cfg, until) in [
        ("Balance", input(7, (2560., 1440.)), 9000),
        ("Balance-spanning", spanning(0.), 9000),
        ("D-gap", spanning(60.), 930),
        ("D-spanning", spanning(0.), 6180),
    ] {
        let mut w = Ribbons::new(cfg);
        if name == "D-spanning" {
            w.inject(Event {
                id: 1,
                at_ns: 200_000_000_000,
                anchor: [0., 700.],
                strength: 1.,
            })
            .unwrap();
        }
        let mut samples = [0; 3];
        let mut frames = 0;
        let mut closest = f64::INFINITY;
        let mut smallest = [f64::INFINITY; 3];
        let mut token_max = 0;
        let mut head_min = [f64::INFINITY; 3];
        let mut small_at = [(0, 0, 0, Vec2::default()); 3];
        let mut head_at = small_at;
        let mut heads = std::collections::BTreeMap::new();
        let mut exported_min = [f64::INFINITY; 3];
        let mut exported_at = small_at;
        for tick in 1..=until {
            w.step(tick);
            for r in w
                .sea
                .agents
                .iter()
                .filter(|r| r.alive && name.starts_with("D-"))
            {
                let h = heads.entry(r.id).or_insert([r.head(); 3]);
                h.rotate_left(1);
                h[2] = r.head();
            }
            if !(name.starts_with("Balance") && tick % 150 == 0
                || name == "D-gap" && [900, 915, 930].contains(&tick)
                || name == "D-spanning" && [6000, 6090, 6180].contains(&tick))
            {
                continue;
            }
            frames += 1;
            let mut hits = vec![];
            let mut tokens = [0; 3];
            for r in w.sea.agents.iter().filter(|r| r.alive && r.layer > 0) {
                if r.knot_for > 0.
                    && let Some(j) = w
                        .input
                        .layout
                        .outputs
                        .iter()
                        .position(|o| o.rect.contains(r.head()))
                {
                    tokens[j] += 1;
                }
                let body = visible_body(r);
                if name.starts_with("D-") {
                    if let Some(&[a, b, c]) = heads.get(&r.id)
                        && w.input.layout.contains(b)
                    {
                        let rad = radius(a, b, c);
                        if rad < head_min[r.layer] {
                            head_min[r.layer] = rad;
                            head_at[r.layer] = (tick, r.id, 0, b);
                        }
                    }
                    let end = (r.extent() * (r.count - 1) as f64).floor() as usize;
                    // A clipped final link lies on the previous chord and is
                    // indexed at its material extent for the rendered census.
                    for (i, n) in body[..=end]
                        .windows(3)
                        .enumerate()
                        .filter(|(_, n)| w.input.layout.contains(n[1]))
                    {
                        let rad = radius(n[0], n[1], n[2]);
                        if rad < smallest[r.layer] {
                            smallest[r.layer] = rad;
                            small_at[r.layer] = (tick, r.id, i + 1, n[1]);
                        }
                    }
                }
            }
            // Count the final Hermite centreline that appears in the native frames.
            // Skip the synthetic span ahead of the material tip.
            let snap = w.snapshot();
            let frame = snap.typed.unwrap().downcast::<Frame>().unwrap();
            for s in frame.strips.iter().filter(|s| s.layer > 0) {
                let body = exported_curve(&frame.vertices[s.offset + 1..s.offset + s.count]);
                if name.starts_with("D-") {
                    for (i, n) in body
                        .windows(3)
                        .enumerate()
                        .filter(|(_, n)| w.input.layout.contains(n[1]))
                    {
                        let rad = radius(n[0], n[1], n[2]);
                        if rad < exported_min[s.layer] {
                            exported_min[s.layer] = rad;
                            exported_at[s.layer] = (tick, s.id, i + 1, n[1]);
                        }
                    }
                }
                let crossings: Vec<_> = intersections(&body)
                    .into_iter()
                    .filter(|p| w.input.layout.contains(*p))
                    .collect();
                if !crossings.is_empty() && std::env::var_os("PVS_RIBBON_LOOP_TRACE").is_some() {
                    println!(
                        "{name} tick {tick} strand {} layer {} crossings {crossings:?}",
                        s.id, s.layer
                    );
                }
                hits.extend(crossings);
            }
            token_max = token_max.max(*tokens.iter().max().unwrap());
            for (i, q) in hits.iter().enumerate() {
                for p in &hits[..i] {
                    closest = closest.min((*q - *p).length());
                }
            }
            for (i, o) in w.input.layout.outputs.iter().enumerate() {
                samples[i] += usize::from(hits.iter().any(|p| o.rect.contains(*p)));
            }
        }
        println!(
            "{name} loop samples {samples:?}, nearest pair {closest:.2}, minimum body radii {smallest:?}, max tokens {token_max}"
        );
        println!(
            "{name} native limits far/mid/near 160/220/300. Head minima {head_min:?} at {head_at:?}; body minima at {small_at:?}; raster-subdivision centreline minima {exported_min:?} at {exported_at:?}"
        );
        if name.starts_with("D-") {
            assert_eq!(frames, 3);
            for (layer, limit) in [(1, 220.), (2, 300.)] {
                for measured in [head_min[layer], smallest[layer], exported_min[layer]] {
                    assert!(
                        measured.is_finite() && measured >= limit,
                        "{name} layer {layer}: {measured}"
                    );
                }
            }
        } else {
            assert_eq!(frames, 60);
        }
        assert!(samples.iter().all(|n| *n <= 3) && closest >= 800. && token_max <= 1);
    }
}
#[test]
#[ignore = "two native Maelstrom timelines and every released layer"]
fn released_bodies_clear_orbit() {
    for (name, seed, size, at, centre, end) in [
        ("Recording", 7, (1920., 1080.), 30, [950., 530.], 2121),
        ("C-maelstrom", 23, (2560., 1440.), 120, [1130., 715.], 4800),
    ] {
        let mut w = Ribbons::new(input(seed, size));
        w.inject(Event {
            id: 2,
            at_ns: at * 1_000_000_000,
            anchor: centre,
            strength: 1.,
        })
        .unwrap();
        let origin = p(centre[0], centre[1]);
        let mut histories = std::collections::BTreeMap::new();
        let mut before_exit = std::collections::BTreeMap::new();
        let mut active_orbit_radii = [f64::INFINITY; 3];
        let mut distances = vec![];
        let mut growth = vec![];
        let mut radii = [f64::INFINITY; 3];
        let mut max_turn = 0_f64;
        let mut max_turn_at = (0, 0, 0.);
        let mut closed = 0;
        let mut frames = 0;
        let mut count = 0;
        for tick in 1..=end {
            w.step(tick);
            for r in w
                .sea
                .agents
                .iter()
                .filter(|r| r.alive && (r.caught || r.release_age > 0.))
            {
                let point = r.head();
                let d = (point - origin).length();
                if r.release_age > 0. {
                    let (start, d0, a, b) = histories.entry(r.id).or_insert_with(|| {
                        count += 1;
                        (tick, d, point, point)
                    });
                    let elapsed = tick - *start;
                    if elapsed >= 2 {
                        radii[r.layer] = radii[r.layer].min(radius(*a, *b, point));
                    }
                    *a = *b;
                    *b = point;
                    if elapsed == 90 {
                        growth.push(d - *d0);
                    }
                    if elapsed == 240 {
                        distances.push(d);
                    }
                } else if name == "C-maelstrom" && tick >= 4348 {
                    // The first two requested stills precede this seed's
                    // collapse. Measure the captured, still unwinding bodies
                    // and their actual travelled radius at those instants too.
                    let h = before_exit.entry(r.id).or_insert([point; 3]);
                    h.rotate_left(1);
                    h[2] = point;
                    if tick >= 4350 {
                        active_orbit_radii[r.layer] =
                            active_orbit_radii[r.layer].min(radius(h[0], h[1], h[2]));
                        if tick % 15 == 0 {
                            println!(
                                "Active orbit before fling at {tick}, strand {}, layer {}, radius {:.6}",
                                r.id,
                                r.layer,
                                radius(h[0], h[1], h[2])
                            );
                        }
                    }
                }
                let window = if name == "Recording" {
                    (1731..=2121).contains(&tick) && (tick - 1731) % 15 == 0
                } else {
                    (4350..=4800).contains(&tick) && tick % 15 == 0
                };
                if !window {
                    continue;
                }
                frames += 1;
                let body = visible_body(r);
                closed += intersections(&body).len();
                // Bound the unwrapped tangent range on every contiguous subarc.
                let mut total = 0.;
                let mut lo = 0_f64;
                let mut hi = 0_f64;
                for a in body.windows(3) {
                    total += turn(a[1] - a[0], a[2] - a[1]);
                    lo = lo.min(total);
                    hi = hi.max(total);
                }
                let excursion = (hi - lo).to_degrees();
                if excursion > max_turn {
                    max_turn = excursion;
                    max_turn_at = (tick, r.id, r.release_age);
                }
            }
        }
        if name == "C-maelstrom" {
            println!("Path before the second seed's release, by layer: {active_orbit_radii:?}");
        }
        println!(
            "{name}: released {count}, body samples {frames}, closed {closed}, maximum tangent excursion {max_turn:.2}°, path radius by layer {radii:?}, radial growth at +3 {growth:?}, radii at +8 {distances:?}"
        );
        println!("{name} maximum body turn at tick/strand/release age: {max_turn_at:?}");
        assert!(
            (3..=6).contains(&count)
                && distances.len() == count
                && growth.len() == count
                && distances.iter().all(|d| *d >= 700.)
                && growth.iter().all(|d| *d >= 300.)
        );
        assert_eq!(frames, count * if name == "Recording" { 27 } else { 31 });
        assert!(
            closed == 0
                && max_turn <= 270.
                && radii[0] >= 160.
                && radii[1] >= 220.
                && radii[2] >= 300.
        );
    }
}
#[test]
#[ignore = "every twist link in the Still water event and every filmed spindle"]
fn still_water_angles() {
    let mut w = Ribbons::new(input(23, (2560., 1440.)));
    w.inject(Event {
        id: 3,
        at_ns: 400_000_000_000,
        anchor: [1280., 720.],
        strength: 1.,
    })
    .unwrap();
    let mut worst = 0_f64;
    let mut links = 0;
    let mut min_face = 1_f64;
    let mut measurements = 0;
    let mut actual = 0_f64;
    let mut fitted = 0_f64;
    for tick in 1..=445 * 30 {
        w.step(tick);
        if tick < 400 * 30 {
            continue;
        }
        for r in w.sea.agents.iter().filter(|r| r.alive) {
            let end = (r.count - 1) as f64 * r.extent();
            let phase = |i: usize| {
                let u = (i as f64 / end.max(1.)).min(1.);
                r.appearance_phase(i)
                    + (u * u * u * 0.22 * w.params.twist * r.fringe_strength())
                        * (r.motion_age * 1.1 - i as f64 * 0.31).sin()
            };
            for i in 1..r.count {
                worst = worst.max(
                    (r.appearance_phase(i) - r.appearance_phase(i - 1))
                        .abs()
                        .to_degrees(),
                );
                links += 1;
                if i <= end.ceil() as usize {
                    actual = actual.max((phase(i) - phase(i - 1)).abs().to_degrees());
                }
            }
        }
        let snap = w.snapshot();
        let frame = snap.typed.unwrap().downcast::<Frame>().unwrap();
        for strip in &frame.strips {
            let body = &frame.vertices[strip.offset + 1..strip.offset + strip.count];
            for pair in body.windows(2) {
                fitted = fitted.max(
                    (pair[1].facing.acos() - pair[0].facing.acos())
                        .abs()
                        .to_degrees() as f64,
                );
            }
            if (415 * 30..=417 * 30).contains(&tick) {
                for n in body {
                    min_face = min_face.min(0.12 + 0.88 * n.facing.abs() as f64);
                    measurements += 1;
                }
            }
        }
    }
    println!(
        "Still visible twist including fringe {actual:.6}°, final fitted sections {fitted:.6}°"
    );
    println!(
        "Still 400–445s: {links} phase differences, maximum {worst:.6}°; 415–417s: {measurements} exported transverse sections, narrowest projected fraction {min_face:.6}"
    );
    assert!(worst <= 20. && actual <= 20. && fitted <= 20. && min_face >= 0.25);
}

#[test]
#[ignore = "measure real Speed300 release identity beyond the event lockout"]
fn real_maelstrom_release_keeps_radius_through_recovery() {
    for span in [false, true] {
        for seed in [1, 7, 23] {
            let mut cfg = input(seed, (2560., 1440.));
            cfg.speed = 3.;
            cfg.density = 80.;
            cfg.scale = 2.;
            cfg.trails = 1.;
            cfg.palette = 2;
            cfg.environment.background = 0;
            if span {
                cfg.seed = seed;
                cfg.mode = MonitorMode::Spanning;
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
            let centre = if span { [4280., 720.] } else { [1280., 720.] };
            let origin = p(centre[0], centre[1]);
            let mut w = Ribbons::new(cfg);
            w.inject(Event {
                id: 2,
                at_ns: 602_000_000_000,
                anchor: centre,
                strength: 1.,
            })
            .unwrap();
            let mut tracked = std::collections::BTreeMap::<u32, (Option<u64>, [Vec2; 3])>::new();
            let mut radii = [f64::INFINITY; 3];
            let mut closed = 0;
            let mut max_turn = 0_f64;
            let mut max_visible_turn = 0_f64;
            let mut worst = (0_u64, 0_u32, 0_f64);
            let mut samples = 0;
            let mut distances = vec![];
            for tick in 1..=19800_u64 {
                w.step(tick);
                for r in w.sea.agents.iter().filter(|r| r.alive) {
                    if (18060..=18120).contains(&tick) && r.caught {
                        tracked.entry(r.id).or_insert((None, [r.head(); 3]));
                    }
                    let Some((release, history)) = tracked.get_mut(&r.id) else {
                        continue;
                    };
                    if release.is_none() && r.release_age > 0. {
                        *release = Some(tick);
                        *history = [r.head(); 3];
                    }
                    let Some(start) = *release else {
                        continue;
                    };
                    let elapsed = tick - start;
                    if elapsed > 450 {
                        continue;
                    }
                    history.rotate_left(1);
                    history[2] = r.head();
                    if elapsed >= 2 {
                        radii[r.layer] =
                            radii[r.layer].min(radius(history[0], history[1], history[2]));
                    }
                    if elapsed == 240 {
                        distances.push((r.id, (r.head() - origin).length()));
                    }
                    // At full Speed the released scarf leaves a single output
                    // quickly. Sample the same recovery window at 10 Hz so it
                    // retains coverage without requiring slower translation.
                    if !(60..=450).contains(&elapsed) || elapsed % 3 != 0 {
                        continue;
                    }
                    let body = visible_body(r);
                    if !body.iter().any(|q| w.input.layout.contains(*q)) {
                        continue;
                    }
                    samples += 1;
                    closed += intersections(&body)
                        .iter()
                        .filter(|q| w.input.layout.contains(**q))
                        .count();
                    let (mut total, mut lo, mut hi) = (0_f64, 0_f64, 0_f64);
                    let (mut local, mut local_lo, mut local_hi) = (0_f64, 0_f64, 0_f64);
                    for a in body.windows(3) {
                        let delta = turn(a[1] - a[0], a[2] - a[1]);
                        total += delta;
                        lo = lo.min(total);
                        hi = hi.max(total);
                        if a.iter().all(|q| w.input.layout.contains(*q)) {
                            local += delta;
                            local_lo = local_lo.min(local);
                            local_hi = local_hi.max(local);
                            max_visible_turn =
                                max_visible_turn.max((local_hi - local_lo).to_degrees());
                        } else {
                            local = 0.;
                            local_lo = 0.;
                            local_hi = 0.;
                        }
                    }
                    let excursion = (hi - lo).to_degrees();
                    if excursion > max_turn {
                        max_turn = excursion;
                        worst = (tick, r.id, r.release_age);
                    }
                }
            }

            let released = tracked.values().filter(|(r, _)| r.is_some()).count();
            println!(
                "REAL_RELEASE spanning{span} seed{seed}: captured={},released={released},visibleSamples={samples},closedVisible={closed},maxWholeTurn={max_turn:.6},maxVisibleTurn={max_visible_turn:.6},pathRadii={radii:?},headDistancesAt8={distances:?},worst={worst:?}",
                tracked.len()
            );
            assert!(released >= 3 && samples > 20 && !distances.is_empty());
            assert!(
                closed == 0
                    && max_turn <= 270.
                    && radii[0] >= 160.
                    && radii[1] >= 220.
                    && radii[2] >= 300.,
                "radius or coil regression: {radii:?}, turn {max_turn}, closed {closed}"
            );
        }
    }
}
