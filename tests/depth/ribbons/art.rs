// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[test]
#[ignore = "measure the final seeded art scenes at native scale"]
fn round_three_art_measurements() {
    use crate::visuals::depth::ribbons::agent::Ribbon;
    use common::layout::Vec2;
    fn input(seed: u64) -> WorldInput {
        let mut w = film(seed);
        let mut screen = w.layout.outputs[0].clone();
        screen.rect.width = 2560.;
        screen.rect.height = 1440.;
        w.layout = Layout::new(vec![screen], 1).unwrap();
        w
    }
    fn loops(r: &Ribbon, view: Rect, trace: bool) -> usize {
        let cross = |a: Vec2, b: Vec2| a.x * b.y - a.y * b.x;
        let end = (r.extent() * (r.count - 1) as f64) as usize;
        let body = &r.nodes[..=end];
        let mut count = 0;
        for (i, a) in body.windows(2).enumerate() {
            let d = a[1].pos - a[0].pos;
            for (j, b) in body.windows(2).enumerate().skip(i + 3) {
                let e = b[1].pos - b[0].pos;
                let offset = b[0].pos - a[0].pos;
                let determinant = cross(d, e);
                if determinant.abs() < 1e-8 {
                    continue;
                }
                let t = cross(offset, e) / determinant;
                let u = cross(offset, d) / determinant;
                if (0.0..1.).contains(&t)
                    && (0.0..1.).contains(&u)
                    && view.contains(a[0].pos + d * t)
                {
                    if trace {
                        let q = a[0].pos + d * t;
                        println!(
                            "  ribbon {} layer {} spans {i} {j} at {:.1},{:.1} length {:.0}",
                            r.id,
                            r.layer,
                            q.x,
                            q.y,
                            (j - i) as f64 * r.spacing
                        );
                    }
                    count += 1;
                }
            }
        }
        count
    }
    let mut world = Ribbons::new(input(7));
    let view = world.input.layout.bounds;
    let mut loop_frames = 0;
    for tick in 1..=9000 {
        world.step(tick);
        if tick % 150 == 0 {
            let hits: usize = world
                .sea
                .agents
                .iter()
                .filter(|r| r.alive && r.layer > 0)
                .map(|r| loops(r, view, false))
                .sum();
            if hits > 0 {
                println!(
                    "Balance {:.0}s: {hits} visible mid/near self-crossings",
                    tick as f64 / 30.
                );
                loop_frames += 1;
            }
        }
    }
    println!("Balance loop samples {loop_frames}/60");
    for gap in [0., 60.] {
        let mut span = input(31);
        span.mode = MonitorMode::Spanning;
        span.seed = 31;
        let outputs = [
            (0., 0., 2560., 1440.),
            (2560. + gap, 0., 3440., 1440.),
            (6000. + gap, 180., 1920., 1080.),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (x, y, width, height))| OutputRect {
            id: common::rng::identity_hash(&format!("Virtual-{i}")),
            rect: Rect {
                x,
                y,
                width,
                height,
            },
            primary: i == 0,
        })
        .collect();
        span.layout = Layout::new(outputs, 1).unwrap();
        let mut world = Ribbons::new(span);
        for tick in 1..=900 {
            world.step(tick);
        }
        let per_panel: Vec<_> = world
            .input
            .layout
            .outputs
            .iter()
            .map(|o| {
                world
                    .sea
                    .agents
                    .iter()
                    .filter(|r| r.alive)
                    .map(|r| loops(r, o.rect, true))
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        println!(
            "Spanning at 30s, gap {gap}px, max visible self-crossings on a single ribbon per panel {per_panel:?}"
        );
        assert!(per_panel.into_iter().all(|n| n <= 1));
    }
    let mut calm = input(7);
    calm.parameters.push(crate::visuals::depth::schema::Value {
        id: 1,
        kind: 1,
        payload: 0,
    });
    let mut world = Ribbons::new(calm);
    for tick in 1..=1800 {
        world.step(tick);
    }
    let mut total = 0.;
    let mut horizontal = 0.;
    let mut tightest = f64::INFINITY;
    for r in world.sea.agents.iter().filter(|r| r.alive) {
        let end = (r.extent() * (r.count - 1) as f64) as usize;
        for ns in r.nodes[..=end]
            .windows(3)
            .filter(|s| view.contains(s[1].pos))
        {
            let a = ns[1].pos - ns[0].pos;
            let b = ns[2].pos - ns[1].pos;
            total += b.length();
            if b.y.abs() <= 0.5 * b.length() {
                horizontal += b.length();
            }
            tightest = tightest.min(
                (a + b).length()
                    / (2.
                        * crate::visuals::depth::ribbons::agent::turn(a, b)
                            .sin()
                            .abs()
                            .max(1e-9)),
            );
        }
    }
    println!(
        "Turbulence 0 aligned {:.3}, minimum radius {:.2}",
        horizontal / total,
        tightest
    );
    let mut world = Ribbons::new(input(23));
    world
        .inject(Event {
            id: 3,
            at_ns: 400_000_000_000,
            anchor: [1280., 720.],
            strength: 1.,
        })
        .unwrap();
    for tick in 1..=12450 {
        world.step(tick);
    }
    let mut hanging = 0;
    let mut mid = 0;
    let mut curved = 0;
    for r in world.sea.agents.iter().filter(|r| r.alive && r.layer == 1) {
        let end = (r.extent() * (r.count - 1) as f64) as usize;
        if !r.nodes[..=end].iter().any(|n| view.contains(n.pos)) {
            continue;
        }
        let tip = r.nodes[end].pos;
        let dy = tip.y - r.head().y;
        let bend_radius = r.nodes[end / 2..=end]
            .windows(3)
            .map(|ns| {
                let a = ns[1].pos - ns[0].pos;
                let b = ns[2].pos - ns[1].pos;
                (a + b).length() / (2. * super::super::agent::turn(a, b).sin().abs().max(1e-8))
            })
            .fold(f64::INFINITY, f64::min);
        println!(
            "Still mid {} distal minimum radius {:.1} px",
            r.id, bend_radius
        );
        curved += usize::from(bend_radius <= 1500.);
        mid += 1;
        hanging += usize::from((150.0..=300.).contains(&dy));
        println!(
            "Still mid {} drop {:.1} head {:.0},{:.0} tail {:.0},{:.0}",
            r.id,
            dy,
            r.head().x,
            r.head().y,
            tip.x,
            tip.y
        );
    }
    let mut world = Ribbons::new(film(7));
    world
        .inject(Event {
            id: 2,
            at_ns: 30_000_000_000,
            anchor: [950., 530.],
            strength: 1.,
        })
        .unwrap();
    let centre = Vec2 { x: 950., y: 530. };
    let mut released = vec![];
    let mut orbits = 0;
    let mut layers = [false; 3];
    let mut flings = 0;
    for tick in 1..=2250 {
        let before: Vec<_> = world
            .sea
            .agents
            .iter()
            .filter(|r| r.caught)
            .map(|r| r.id)
            .collect();
        world.step(tick);
        if tick == 900 {
            for r in world.sea.agents.iter().filter(|r| r.alive) {
                println!(
                    "Maelstrom admission {} layer={} visible={} life={:.1} xy={:.0},{:.0} caught={}",
                    r.id,
                    r.layer,
                    world.input.layout.bounds.contains(r.head()),
                    r.life - r.age,
                    r.head().x,
                    r.head().y,
                    r.caught
                );
            }
        }
        if tick == 1440 {
            for r in world.sea.agents.iter().filter(|r| r.caught) {
                orbits += 1;
                layers[r.layer] = true;
                println!(
                    "Orbit at 48s id {} layer {} radius {:.1}, head {:.0},{:.0}",
                    r.id,
                    r.layer,
                    (r.head() - centre).length(),
                    r.head().x,
                    r.head().y
                );
            }
        }
        for r in world.sea.agents.iter().filter(|r| r.alive) {
            if before.contains(&r.id) && !r.caught && r.release_age > 0. {
                assert!((55 * 30..=60 * 30).contains(&tick));
                assert!(r.pulses.iter().any(|p| (0.0..14.).contains(p)));
                released.push((r.id, tick, (r.head() - centre).length()));
                println!(
                    "Release {:.2}s id {} pulse {:.2}",
                    tick as f64 / 30.,
                    r.id,
                    r.pulses
                        .iter()
                        .copied()
                        .filter(|p| *p >= 0.)
                        .fold(f64::INFINITY, f64::min)
                );
            }
            for &(id, t, starting_radius) in &released {
                if id == r.id && tick == t + 90 {
                    flings += 1;
                    assert!((r.head() - centre).length() - starting_radius >= 300.);
                    println!(
                        "Fling id {id} in 3s {:.2} px",
                        (r.head() - centre).length() - starting_radius
                    );
                }
            }
        }
    }
    assert!(layers.iter().all(|b| *b) && flings >= 3 && flings == released.len());
    println!(
        "Still hanging {hanging}/{mid}, curved distal halves {curved}/{mid}; recording captured {orbits}"
    );
    assert!(loop_frames <= 3 && horizontal >= total * 0.8 && tightest >= 400.);
    assert!(mid > 0 && hanging * 5 >= mid * 3 && curved == mid && (3..=6).contains(&orbits));
}

#[test]
#[ignore = "locate native review crops in the seeded product timelines"]
fn native_art_subjects() {
    for (name, seed, seconds, size) in [
        ("A", 7, 60, (2560., 1440.)),
        ("G", 7, 640, (2560., 1440.)),
        ("Gust", 23, 604, (2560., 1440.)),
        ("Recording", 7, 75, (1920., 1080.)),
    ] {
        let mut input = film(seed);
        let mut output = input.layout.outputs[0].clone();
        output.rect.width = size.0;
        output.rect.height = size.1;
        input.layout = Layout::new(vec![output], 1).unwrap();
        if name == "G" {
            input.speed = 3.;
            input.density = 80.;
            input.scale = 2.;
            input.trails = 1.;
            input.palette = 2;
        }
        let mut world = Ribbons::new(input);
        if name == "Recording" || name == "Gust" {
            world
                .inject(if name == "Gust" {
                    Event {
                        id: 1,
                        at_ns: 600_000_000_000,
                        anchor: [0., 720.],
                        strength: 1.,
                    }
                } else {
                    Event {
                        id: 2,
                        at_ns: 30_000_000_000,
                        anchor: [950., 530.],
                        strength: 1.,
                    }
                })
                .unwrap();
        }
        for tick in 1..=seconds * 30 {
            world.step(tick);
            if tick % 5 != 0 {
                continue;
            }
            let t = tick as f64 / 30.;
            for r in world
                .sea
                .agents
                .iter()
                .filter(|r| r.alive && world.input.layout.bounds.contains(r.head()))
            {
                let h = r.head();
                let length = r.extent() * (r.count - 1) as f64 * r.spacing;
                let head_frame = match name {
                    "A" => t == 60.,
                    "G" => t == 600.,
                    "Gust" => t == 604.,
                    _ => false,
                };
                let short_body = (2. ..200.).contains(&length)
                    && (name == "Recording" || name == "G" && t >= 570.);
                if head_frame || short_body {
                    let phase = if r.age < r.grow {
                        "birth"
                    } else if r.extent() < 1. {
                        "fray"
                    } else {
                        "mature"
                    };
                    println!(
                        "subject,{name},{t:.4},{},{},{phase},{:.1},{:.1},{length:.2},{:.2},{:.3},{:.3}",
                        r.id, r.layer, h.x, h.y, r.width, r.heading.x, r.heading.y
                    );
                }
            }
        }
    }
}
