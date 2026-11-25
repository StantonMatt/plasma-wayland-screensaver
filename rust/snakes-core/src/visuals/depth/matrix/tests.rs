use super::*;
use crate::visuals::depth::{
    common::layout::{Layout, OutputRect, Rect},
    schema::Value,
};
use std::collections::VecDeque;
pub(super) fn input(seed: u64, desk: bool) -> WorldInput {
    let mut rects = vec![(0., 0., 2560., 1440.)];
    if desk {
        rects.extend([(2560., 0., 3440., 1440.), (6000., 195., 1920., 1080.)]);
    }
    let outputs = rects
        .into_iter()
        .enumerate()
        .map(|(i, (x, y, width, height))| OutputRect {
            id: i as u64,
            rect: Rect {
                x,
                y,
                width,
                height,
            },
            primary: i == 0,
        })
        .collect();
    WorldInput {
        visual_id: 6,
        seed,
        mode: if desk {
            MonitorMode::Spanning
        } else {
            MonitorMode::Independent
        },
        layout: Layout::new(outputs, 1).unwrap(),
        speed: 1.,
        scale: 1.,
        density: 50.,
        trails: 0.35,
        palette: 0,
        reduced_motion: false,
        parameters: vec![],
        environment: Environment::default(),
    }
}
fn event(id: u32, t: f32, x: f64) -> Event {
    Event {
        id,
        at_ns: (t * 1e9) as i64,
        anchor: [x, 0.],
        strength: 1.,
    }
}
#[test]
fn mature_seeded_field_and_lifecycle() {
    let mut a = MatrixWorld::new(input(7, true));
    let mut b = MatrixWorld::new(input(7, true));
    assert_eq!(a.state_hash(), b.state_hash());
    assert_ne!(
        a.state_hash(),
        MatrixWorld::new(input(8, true)).state_hash()
    );
    assert!(a.layers.iter().all(|l| l.cells.iter().any(|c| c.power > 0)));
    for i in 1..=1800 {
        a.step(i);
        b.step(i);
    }
    assert_eq!(a.state_hash(), b.state_hash());
    assert!(a.layers.iter().any(|l| l.hops > 0));
    assert!(a.layers.iter().any(|l| l.merges > 0));
    assert!(a.layers.iter().all(|l| l.splashes > 0));
    assert!(a.layers.iter().any(|l| l.evaporations > 0));
}
#[test]
fn columns_follow_real_and_gap_floors() {
    let mut i = input(3, true);
    let mut outputs = i.layout.outputs.clone();
    outputs[2].rect.x += 100.;
    i.layout = Layout::new(outputs, 2).unwrap();
    let w = MatrixWorld::new(i);
    for l in &w.layers {
        for (x, expected) in [
            (2500., 1440.),
            (2800., 1440.),
            (6048., 1440.),
            (6300., 1275.),
        ] {
            let actual = l.cols[(x / l.pitch[0]) as usize].floor * l.pitch[1];
            assert!((actual - expected).abs() < 0.001);
        }
    }
}
#[test]
fn calm_is_frozen_and_depth_is_independent_of_rainfall() {
    let mut i = input(2, false);
    i.reduced_motion = true;
    let mut w = MatrixWorld::new(i.clone());
    let h = w.state_hash();
    for t in [1, 300, 9000] {
        w.step(t);
        assert_eq!(h, w.state_hash());
    }
    i.density = 100.;
    i.parameters.push(Value {
        id: 5,
        kind: 1,
        payload: 0,
    });
    let flat = MatrixWorld::new(i);
    assert_eq!(w.layers[1].pitch, flat.layers[1].pitch);
    assert!(flat.layers[0].streams.is_empty() && flat.layers[2].streams.is_empty());
}
#[test]
fn mirrored_columns_share_spawn_history_away_from_local_floor() {
    let mut i = input(72, false);
    i.mode = MonitorMode::Mirrored;
    let mut j = i.clone();
    let mut rect = j.layout.outputs[0].clone();
    rect.rect.width = 3440.;
    j.layout = Layout::new(vec![rect], 1).unwrap();
    let a = MatrixWorld::new(i);
    let b = MatrixWorld::new(j);
    for (a, b) in a.layers.iter().zip(&b.layers) {
        assert_eq!(a.pitch, b.pitch);
        let end = (a.cols.len() / 2) * a.rows;
        for k in 0..end {
            assert_eq!(
                a.cells[k],
                b.cells[k],
                "layer {} column {} row {}",
                a.index,
                k / a.rows,
                k % a.rows
            );
        }
    }
}
#[test]
fn forced_bolts_obey_switches_without_rationing_strikes() {
    let mut i = input(7, true);
    for light in [false, true] {
        i.parameters = vec![Value {
            id: 3,
            kind: 2,
            payload: light as u64,
        }];
        let mut w = MatrixWorld::new(i.clone());
        for t in [1., 1.2, 1.4, 1.7, 2., 3.] {
            w.inject(event(3, t, 5800.)).unwrap();
        }
        for t in 1..=100 {
            w.step(t);
        }
        assert_eq!(w.weather.counts[LIGHTNING], if light { 6 } else { 0 });
        let l = &w.layers[1];
        assert_eq!(l.cells.iter().any(|c| c.style & 16 != 0), light);
    }
}
#[test]
fn four_hours_of_weather() {
    for mode in 0..=2 {
        let p = Params {
            weather: mode,
            ..Params::default()
        };
        let mut weather = Weather::new(7, 7920., 1., &p);
        for t in 0..14400 {
            weather.step(t as f32, &p);
        }
        eprintln!("4h weather {mode} counts {:?}", weather.counts);
        for kind in [GUST, SWEEP, LANTERN] {
            assert!(weather.counts[kind] >= 12);
        }
        if mode == 0 {
            assert_eq!(
                (
                    weather.counts[DOWNPOUR],
                    weather.counts[LIGHTNING],
                    weather.counts[LULL]
                ),
                (0, 0, 0)
            );
        } else {
            assert!(weather.counts[LIGHTNING] > 0 && weather.counts[LULL] > 0);
        }
        if mode == 2 {
            assert!(weather.counts[DOWNPOUR] > 40);
        }
    }
}
#[cfg(target_os = "linux")]
fn thread_ns() -> u64 {
    #[repr(C)]
    struct Timespec {
        sec: i64,
        nsec: i64,
    }
    unsafe extern "C" {
        fn clock_gettime(clock: i32, t: *mut Timespec) -> i32;
    }
    let mut t = Timespec { sec: 0, nsec: 0 };
    assert_eq!(unsafe { clock_gettime(3, &mut t) }, 0);
    (t.sec as u64) * 1_000_000_000 + t.nsec as u64
}
#[cfg(target_os = "linux")]
#[test]
#[ignore = "release stress timing"]
fn tick_export_budget() {
    // Linux LP64 rusage ends with voluntary and involuntary context switches.
    fn switches() -> i64 {
        unsafe extern "C" {
            fn getrusage(who: i32, out: *mut [i64; 18]) -> i32;
        }
        let mut usage = [0; 18];
        assert_eq!(unsafe { getrusage(1, &mut usage) }, 0);
        usage[16] + usage[17]
    }
    let mut i = input(7, true);
    i.speed = 3.;
    i.density = 100.;
    let start = std::time::Instant::now();
    let mut w = MatrixWorld::new(i);
    eprintln!("stress preroll {:?}", start.elapsed());
    let mut history = VecDeque::new();
    let mut times = Vec::new();
    let mut max_cpu = 0;
    let mut slow = (0, 0, 0);
    for tick in 1..=18000 {
        let start = std::time::Instant::now();
        let context = switches();
        let cpu = thread_ns();
        w.step(tick);
        history.push_back(w.snapshot());
        if history.len() > 5 {
            history.pop_front();
        }
        let taken = thread_ns() - cpu;
        let wall = start.elapsed().as_nanos() as u64;
        if tick > 120 {
            times.push(wall);
            max_cpu = max_cpu.max(taken);
            if wall > slow.0 {
                slow = (wall, taken, switches() - context);
            }
        }
    }
    times.sort_unstable();
    let p99 = times[times.len() * 99 / 100];
    eprintln!(
        "stress tick+snapshot ns p99 wall {p99}, max wall {} same-call cpu {} switches {}, max CPU {max_cpu}",
        slow.0, slow.1, slow.2
    );
    assert!(p99 <= 500000 && max_cpu <= 2000000);
}
#[test]
#[ignore = "two-hour release soak and OLED dwell"]
fn two_hour_soak() {
    let mut i = input(11, true);
    i.speed = 3.;
    i.density = 100.;
    i.trails = 1.;
    let mut w = MatrixWorld::new(i);
    let mut history = VecDeque::new();
    let mut dwells: Vec<_> = w.layers.iter().map(|l| vec![0u8; l.cells.len()]).collect();
    let mut max_dwell = 0;
    let (mut min_lit, mut max_lit) = (usize::MAX, 0);
    let mut heap = vec![];
    let fixed: usize = w.layers.iter().map(|l| l.cells.capacity()).sum();
    for tick in 1..=216000 {
        w.step(tick);
        history.push_back(w.snapshot());
        if history.len() > 5 {
            history.pop_front();
        }
        if tick % 30 == 0 {
            let mut lit = 0;
            for (l, dwell) in w.layers.iter().zip(&mut dwells) {
                for (c, d) in l.cells.iter().zip(dwell) {
                    let age = (tick as u32 + PREWARM - u32::from_le_bytes(c.stamp)) as f32 / 30.;
                    let half = if c.style & 16 != 0 {
                        1.
                    } else if c.style & 64 != 0 {
                        1.25
                    } else if c.style & 8 != 0 {
                        3. * l.half
                    } else {
                        l.half
                    };
                    let age = if c.style & 16 != 0 {
                        (age - 0.15).max(0.)
                    } else {
                        age
                    };
                    let above = |level: f32| {
                        c.power as f32 > level * 255.
                            && age < half * (c.power as f32 / (255. * level)).log2()
                    };
                    if above(0.4) {
                        *d = d.saturating_add(1);
                        max_dwell = max_dwell.max(*d);
                    } else {
                        *d = 0;
                    }
                    lit += above(0.025) as usize;
                }
            }
            min_lit = min_lit.min(lit);
            max_lit = max_lit.max(lit);
        }
        if tick % 54000 == 0 {
            let rss = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
            heap.push(
                rss.lines()
                    .find(|l| l.starts_with("VmRSS:"))
                    .unwrap_or("")
                    .to_string(),
            );
        }
    }
    eprintln!(
        "2h stress: fixed cells {fixed}, lit min/max {min_lit}/{max_lit}, maximum >40% dwell {max_dwell}s, counters {:?}, RSS quarters {heap:?}",
        w.weather.counts
    );
    assert!(max_dwell < 60);
    assert!(max_lit > min_lit);
    assert!(w.frames.borrow().len() <= 7);
}

#[test]
fn layer_balance_at_the_default_scale() {
    let mut total = [0usize; 3];
    let mut active = 0;
    let mut columns = 0;
    let mut samples = 0;
    for seed in [1, 3, 7, 23] {
        let mut world = MatrixWorld::new(input(seed, false));
        for tick in 1..=9000 {
            world.step(tick);
            if tick % 30 == 0 {
                for (n, l) in total.iter_mut().zip(&world.layers) {
                    *n += l.streams.len();
                }
                let mid = &world.layers[1];
                let coords: std::collections::BTreeSet<_> =
                    mid.streams.iter().map(|s| s.col).collect();
                active += coords.len();
                columns += mid.cols.len();
                samples += 1;
            }
        }
    }
    let mean = total.map(|v| v as f64 / samples as f64);
    let duty = active as f64 / columns as f64;
    eprintln!("default mean heads far/mid/near {mean:?}, fraction mid columns active {duty:.3}");
    assert!(mean[1] > mean[2] && (0.25..0.58).contains(&duty));
    assert!((5.0..=12.0).contains(&mean[2]));
}

#[test]
fn storm_stroke_reaches_the_offset_floor_across_the_seam() {
    let mut w = MatrixWorld::new(input(7, true));
    w.inject(event(3, 1., 5800.)).unwrap();
    for t in 1..=34 {
        w.step(t);
    }
    let layer = &w.layers[1];
    let mut span = [f32::MAX, f32::MIN];
    let mut direction = [0; 3];
    for (col, cells) in layer.cells.chunks(layer.rows).enumerate() {
        for (row, c) in cells.iter().enumerate() {
            if c.style & 16 == 0 {
                continue;
            }
            let x = (col as f32 + 0.5) * layer.pitch[0];
            span[0] = span[0].min(x);
            span[1] = span[1].max(x);
            let id = c.id();
            direction[(id - 209) as usize] += 1;
            assert!(row as f32 <= layer.cols[col].floor + 0.5);
        }
    }
    assert!(span[0] < 6000. && span[1] > 6000., "strike span {span:?}");
    assert!(direction.iter().all(|&v| v > 0));
}

#[test]
fn sparse_capture_has_visible_drizzle() {
    let seed = crate::visuals::depth::common::rng::output_seed(11, 6, "Virtual-0");
    let mut i = input(seed, false);
    i.density = 10.;
    i.scale = 2.;
    i.trails = 0.;
    i.parameters = vec![Value {
        id: 5,
        kind: 1,
        payload: 0,
    }];
    let mut w = MatrixWorld::new(i);
    for tick in 1..=1800 {
        w.step(tick);
    }
    let mid = &w.layers[1];
    let visible: Vec<_> = mid
        .streams
        .iter()
        .filter(|s| s.position[1] > 0.5 && s.position[1] < mid.cols[s.col].floor - 0.5)
        .collect();
    eprintln!("S5 sparse head count {}", visible.len());
    assert!((2..=22).contains(&visible.len()));
}

#[test]
fn downpour_at_the_capture_anchor() {
    let seed = crate::visuals::depth::common::rng::output_seed(23, 6, "Virtual-0");
    let mut w = MatrixWorld::new(input(seed, false));
    w.inject(event(1, 28., 1280.)).unwrap();
    w.inject(event(2, 30., 1280.)).unwrap();
    let mut before = [0.; 3];
    let mut after = [0.; 3];
    for t in 1..=45 * 30 {
        w.step(t);
        if t % 30 != 0 {
            continue;
        }
        let occ = |l: &field::Layer, radius: f32| {
            let within = |i: usize| ((i as f32 + 0.5) * l.pitch[0] - 1280.).abs() < radius;
            let active: std::collections::BTreeSet<_> = l
                .streams
                .iter()
                .filter(|s| within(s.col) && s.y >= 0.)
                .map(|s| s.col)
                .collect();
            active.len() as f32 / (0..l.cols.len()).filter(|&c| within(c)).count() as f32
        };
        let local = w.layers.each_ref().map(|l| occ(l, 600.));
        let all = w.layers.each_ref().map(|l| occ(l, 2000.));
        if (21 * 30..=27 * 30).contains(&t) {
            for i in 0..3 {
                before[i] += local[i] / 7.;
            }
        }
        if t == 40 * 30 {
            after = local
        }
        if t == 34 * 30 {
            assert!((0.35..=0.45).contains(&all[1]));
        }
        if t >= 21 * 30 {
            eprintln!(
                "capture sec {} local {:?} whole {:?} event {}",
                t / 30,
                local,
                all,
                w.weather.counts[DOWNPOUR]
            );
        }
    }
    eprintln!("pre-event local {before:?}, 40s {after:?}");
    assert_eq!(w.weather.counts[DOWNPOUR], 1);
    assert!(after[1] >= 1.8 * before[1] && after[0] > 0.55);
}

#[test]
fn personal_profile_has_short_trails_and_open_columns() {
    let seed = crate::visuals::depth::common::rng::output_seed(7, 6, "Virtual-0");
    let mut config = input(seed, false);
    config.speed = 3.;
    config.density = 80.;
    config.trails = 1.;
    config.scale = 2.;
    let mut w = MatrixWorld::new(config);
    for t in 1..=610 * 30 {
        w.step(t);
        if t % 30 != 0 || t < 590 * 30 {
            continue;
        }
        let l = &w.layers[1];
        let mut visible = vec![false; l.cols.len()];
        for s in &l.streams {
            if s.y > 0. {
                visible[s.col] = true;
            }
        }
        let occupied = visible.iter().filter(|s| **s).count() as f32 / visible.len() as f32;
        let dark = l
            .cells
            .chunks(l.rows)
            .zip(&visible)
            .filter(|(col, head)| {
                !**head
                    && col.iter().all(|c| {
                        let age = (PREWARM + t as u32 - u32::from_le_bytes(c.stamp)) as f32 / 30.;
                        c.power as f32 / 255. * (-age / l.half).exp2() < 0.07
                    })
            })
            .count() as f32
            / visible.len() as f32;
        eprintln!(
            "G sec {} occupied {occupied:.3} dark {dark:.3} half {:.3} speed {:.2}",
            t / 30,
            l.half,
            l.speed()
        );
        assert!(occupied <= 0.53 && dark >= 0.25);
    }
}

#[test]
fn palette_capture_has_two_fronts() {
    let master = 11;
    let seed = crate::visuals::depth::common::rng::output_seed(master, 6, "Virtual-0");
    let mut config = input(seed, false);
    let mut outputs = config.layout.outputs.clone();
    outputs[0].rect.width = 1920.;
    outputs[0].rect.height = 1080.;
    config.layout = Layout::new(outputs, 1).unwrap();
    let mut w = MatrixWorld::new(config);
    for t in 1..=120 * 30 {
        w.step(t);
    }
    let mut heads = Vec::new();
    w.layers[1].heads(&mut heads);
    let mut hues = [0; 6];
    let mut lattice = 0;
    for h in &heads {
        if h.light[1] != 1. {
            hues[h.ink[3] as usize] += 1;
            lattice += usize::from((209. ..219.).contains(&h.ink[2]));
        }
    }
    eprintln!("S4 master {master} hues {hues:?}, lattice {lattice}");
    assert_eq!(lattice, 0);
    assert!(hues.iter().filter(|&&n| n >= 5).count() >= 2);
}

#[test]
fn steady_seed_keeps_gentle_rain_through_the_dry_front() {
    let seed = crate::visuals::depth::common::rng::output_seed(23, 6, "Virtual-0");
    let mut i = input(seed, false);
    i.parameters.push(Value {
        id: 1,
        kind: 0,
        payload: 0,
    });
    let mut w = MatrixWorld::new(i);
    w.inject(event(1, 28., 1280.)).unwrap();
    w.inject(event(2, 30., 1280.)).unwrap();
    w.inject(event(4, 60., 1280.)).unwrap();
    let mut occupancy = vec![];
    for t in 1..=300 * 30 {
        w.step(t);
        if [34 * 30, 120 * 30, 300 * 30].contains(&t) {
            let l = &w.layers[1];
            let active: std::collections::BTreeSet<_> = l
                .streams
                .iter()
                .filter(|s| s.y >= 0.)
                .map(|s| s.col)
                .collect();
            occupancy.push(active.len() as f32 / l.cols.len() as f32);
        }
    }
    eprintln!("Steady seed 23 mid occupancy at 34/120/300s: {occupancy:?}");
    assert!(occupancy.iter().all(|v| (0.25..=0.50).contains(v)));
    assert_eq!(w.weather.counts[LULL], 0);
}
