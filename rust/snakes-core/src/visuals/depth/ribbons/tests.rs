use super::*;
use common::{
    layout::{Layout, OutputRect, Rect},
    rng::output_seed,
};
use world::{Environment, MonitorMode};

fn film(seed: u64) -> WorldInput {
    WorldInput {
        visual_id: 4,
        seed: output_seed(seed, 4, "Virtual-0"),
        mode: MonitorMode::Independent,
        layout: Layout::new(
            vec![OutputRect {
                id: 1,
                rect: Rect {
                    x: 0.,
                    y: 0.,
                    width: 1920.,
                    height: 1080.,
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
fn recording_contains_the_lifecycle_dance_and_maelstrom() {
    let mut w = Ribbons::new(film(7));
    w.inject(Event {
        id: 2,
        at_ns: 30_000_000_000,
        anchor: [950., 530.],
        strength: 1.,
    })
    .unwrap();
    let mut vortex_ticks = 0;
    let mut visible_braid_midpoints = 0;
    let mut visible_motes = 0;
    let mut captured = 0;
    let mut free = 0;
    for i in 1..=2250 {
        let before = w.sea.dances;
        w.step(i);
        if w.sea.dances > before {
            println!("bond at {:.3}s", i as f64 / 30.);
        }
        let view = w.input.layout.bounds;
        visible_motes =
            visible_motes.max(w.sea.motes.iter().filter(|m| view.contains(m.pos)).count());
        if i == 910 {
            captured = w.sea.agents.iter().filter(|r| r.caught).count();
            free = w
                .sea
                .agents
                .iter()
                .filter(|r| r.alive && !r.caught && view.contains(r.head()))
                .count();
        }
        for (a, b) in w
            .sea
            .braid_midpoints()
            .filter(|(a, b)| view.contains(*a) && view.contains(*b))
        {
            visible_braid_midpoints += 1;
            println!(
                "braid midpoint {}s at ({:.1},{:.1}) to ({:.1},{:.1})",
                i as f64 / 30.,
                a.x,
                a.y,
                b.x,
                b.y
            );
        }
        if i % 150 == 0 {
            for r in &w.sea.agents {
                for arc in r.pulses.iter().filter(|p| **p > 100.) {
                    let k = (arc / r.spacing) as usize;
                    if k < r.count * 3 / 4 && k < (r.extent() * r.count as f64) as usize {
                        let v = r.nodes[k].pos;
                        if v.x > 100. && v.x < 1820. && v.y > 100. && v.y < 980. && r.layer == 1 {
                            println!("pulse {}s at {:.0},{:.0}", i / 30, v.x, v.y);
                        }
                    }
                }
            }
        }
        if w.sea
            .weather
            .active
            .is_some_and(|m| m.kind == water::Kind::Maelstrom)
        {
            vortex_ticks += 1;
        }
    }
    println!(
        "recording seed 7: {} births, {} frays, {} braids, {} maelstrom ticks",
        w.sea.births, w.sea.frays, w.sea.dances, vortex_ticks
    );
    assert!(visible_braid_midpoints > 0 && visible_motes >= 5 && captured > 0 && free >= 2);
    assert!(vortex_ticks <= 29 * 30);
    assert!(
        w.sea.births > 0
            && w.sea.frays > 0
            && w.sea.dances > 0
            && vortex_ticks >= 25 * 30
            && vortex_ticks <= 40 * 30
    );
}
#[test]
fn event_pace_caps_at_one_and_a_half_while_the_water_keeps_full_speed() {
    let mut input = film(7);
    input.speed = 3.;
    let mut world = Ribbons::new(input);
    world
        .inject(Event {
            id: 3,
            at_ns: 0,
            anchor: [950., 530.],
            strength: 1.,
        })
        .unwrap();
    let age = world.sea.agents[0].age;
    let motion_age = world.sea.agents[0].motion_age;
    for t in 1..=151 {
        world.step(t);
    }
    let event = world.sea.weather.active.unwrap();
    assert!((event.age - 7.5).abs() < 1e-8);
    assert!((world.sea.agents[0].motion_age - motion_age - 15.1).abs() < 1e-8);
    assert!((world.sea.agents[0].age - age - 7.55).abs() < 1e-8);
}
#[test]
fn calm_flat_setting_and_event_bounds() {
    let mut w = Ribbons::new(film(3));
    w.params.twist = 0.;
    w.params.dance = false;
    w.params.world_events = false;
    for id in [0, 4] {
        assert!(
            w.inject(Event {
                id,
                at_ns: 0,
                anchor: [0.; 2],
                strength: 1.
            })
            .is_err()
        );
    }
    for i in 1..9000 {
        w.step(i);
    }
    for r in &w.sea.agents {
        assert!(r.nodes.iter().take(r.count).all(|n| n.phase.abs() < 1e-8));
        assert!(r.pulses.len() <= 3 && r.count <= 128);
        for pair in r.nodes[..r.count].windows(2) {
            assert!(((pair[1].pos - pair[0].pos).length() - r.spacing).abs() < 0.02);
        }
    }
    assert_eq!(w.sea.dances, 0);
    assert!(w.sea.weather.active.is_none() && w.sea.motes.len() <= 512);
}

#[test]
#[ignore = "five-minute composition measurements at five-second intervals"]
fn measure_composition() {
    use common::layout::Vec2;

    // Clip the measured arclength to the viewport, including the visible
    // portion of a segment whose two knots straddle a screen edge.
    fn visible_segment(a: Vec2, b: Vec2) -> f64 {
        let delta = b - a;
        let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
        for (origin, step, limit) in [(a.x, delta.x, 2560.), (a.y, delta.y, 1440.)] {
            if step.abs() < f64::EPSILON {
                if !(0.0..=limit).contains(&origin) {
                    return 0.;
                }
                continue;
            }
            let t0 = -origin / step;
            let t1 = (limit - origin) / step;
            lo = lo.max(t0.min(t1));
            hi = hi.min(t0.max(t1));
        }
        (hi - lo).max(0.) * delta.length()
    }

    let mut input = film(7);
    input.layout = Layout::new(
        vec![OutputRect {
            id: 1,
            primary: true,
            rect: Rect {
                x: 0.,
                y: 0.,
                width: 2560.,
                height: 1440.,
            },
        }],
        1,
    )
    .unwrap();
    let mut w = Ribbons::new(input);
    let mut totals = [0.; 9];
    let mut valid = 0;
    let mut broad_length = 0.;
    let mut visible_length = 0.;
    let mut low = 1f64;
    let mut high = 0f64;
    for tick in 1..=9000 {
        w.step(tick);
        if tick % 150 != 0 {
            continue;
        }
        let mut cell = [0.; 9];
        for r in w.sea.agents.iter().filter(|r| r.alive) {
            let end = (r.extent() * (r.count - 1) as f64) as usize;
            for (i, nodes) in r.nodes[..=end].windows(3).enumerate() {
                let (a, b, c) = (nodes[0].pos, nodes[1].pos, nodes[2].pos);
                let (ab, bc) = (b - a, c - b);
                // Attribute the two neighbouring half-segments to this bend.
                // The first and last bends also own their end caps.
                let from = if i == 0 { a } else { (a + b) * 0.5 };
                let to = if i + 2 == end { c } else { (b + c) * 0.5 };
                let length = visible_segment(from, b) + visible_segment(b, to);
                visible_length += length;
                let twice_area = (ab.x * bc.y - ab.y * bc.x).abs();
                if ab.length() * bc.length() * (c - a).length() >= 500. * twice_area {
                    broad_length += length;
                }
            }
            for n in &r.nodes[..=end] {
                if !(0.0..2560.).contains(&n.pos.x) || !(0.0..1440.).contains(&n.pos.y) {
                    continue;
                }
                let x = (3. * n.pos.x / 2560.) as usize;
                let y = (3. * n.pos.y / 1440.) as usize;
                cell[3 * y + x] +=
                    r.width * r.spacing * (0.12 + 0.88 * n.phase.cos().abs()) * r.light;
            }
        }
        let sum = cell.iter().sum::<f64>().max(0.01);
        let mut shares = [0.; 6];
        for k in 0..9 {
            cell[k] /= sum;
            totals[k] += cell[k];
            shares[k / 3] += cell[k];
            shares[3 + k % 3] += cell[k];
        }
        let balanced =
            shares.iter().all(|v| (0.15..=0.50).contains(v)) && cell.iter().all(|v| *v < 0.35);
        valid += usize::from(balanced);
        low = low.min(shares.iter().copied().reduce(f64::min).unwrap());
        high = high.max(shares.iter().copied().reduce(f64::max).unwrap());
        println!("t={} rows/columns={shares:.2?} cells={cell:.2?}", tick / 30);
    }
    println!(
        "balanced {valid}/60, third extrema {low:.2}..{high:.2}, curvature {:.2}%, mean {:?}",
        100. * broad_length / visible_length,
        totals.map(|v| v / 60.)
    );
}

#[test]
fn gust_cannot_reverse_a_live_body_link_in_one_tick() {
    let mut input = film(7);
    input.density = 80.;
    input.speed = 3.;
    input.scale = 2.;
    input.trails = 1.;
    input.palette = 2;
    let mut output = input.layout.outputs[0].clone();
    output.rect.width = 2560.;
    output.rect.height = 1440.;
    input.layout = Layout::new(vec![output], 1).unwrap();
    let mut world = Ribbons::new(input);
    for tick in 1..=4330 {
        world.step(tick);
    }
    let mut links = 0;
    let mut sharpest = (1.0, 0, 0);
    for tick in 4331..=4360 {
        let previous: Vec<_> = world
            .sea
            .agents
            .iter()
            .filter(|r| r.alive)
            .map(|r| (r.id, r.nodes))
            .collect();
        world.step(tick);
        for ribbon in world.sea.agents.iter().filter(|r| r.alive) {
            let Some((_, nodes)) = previous.iter().find(|(id, _)| *id == ribbon.id) else {
                continue;
            };
            for (a, b) in nodes[..ribbon.count]
                .windows(2)
                .zip(ribbon.nodes[..ribbon.count].windows(2))
            {
                let dot = (a[1].pos - a[0].pos)
                    .unit()
                    .dot((b[1].pos - b[0].pos).unit());
                if dot < sharpest.0 {
                    sharpest = (dot, tick, ribbon.id);
                }
                links += 1;
            }
        }
    }
    assert!(links > 1000);
    let (dot, tick, id) = sharpest;
    assert!(
        dot >= 0.5 - 1e-7,
        "G tick {tick}, ribbon {id}, smallest link dot {dot}"
    );
}

#[test]
fn downstream_retirement_preserves_the_visible_tail() {
    let world = Ribbons::new(film(7));
    for remaining in [120., 11., 8.] {
        let mut ribbon = world.sea.agents[0].clone();
        ribbon.age = ribbon.grow + 20.;
        ribbon.life = ribbon.age + remaining;
        let extent = ribbon.extent();
        ribbon.retire_downstream();
        assert!((ribbon.extent() - extent).abs() < 1e-12);
        assert!((ribbon.life - ribbon.age - remaining.min(ribbon.exit_duration())).abs() < 1e-12);
        ribbon.age += 1. / 30.;
        assert!(ribbon.extent() <= extent);
        assert!((extent - ribbon.extent()) * ((ribbon.count - 1) as f64) < 1.);
    }
}

#[path = "../../../../../../tests/depth/ribbons/art.rs"]
mod art;
