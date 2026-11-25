use super::super::{
    common::clock::Sample,
    common::layout::{Layout, OutputRect, Rect},
    ffi::{Lease, View},
    world::{Environment, MonitorMode},
};
use super::*;
use render::{Lens, Pose, visuals_depth_kaleidoscope_sample};
fn input() -> WorldInput {
    let outputs = [
        (0., 0., 2560., 1440.),
        (2560., 0., 3440., 1440.),
        (6000., 195., 1920., 1080.),
    ]
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
        primary: i == 1,
    })
    .collect();
    WorldInput {
        visual_id: 5,
        seed: 7,
        mode: MonitorMode::Spanning,
        layout: Layout::new(outputs, 1).unwrap(),
        speed: 3.,
        density: 100.,
        scale: 2.,
        trails: 1.,
        palette: 2,
        reduced_motion: false,
        parameters: vec![],
        environment: Environment::default(),
    }
}
fn view(r: Rect) -> View {
    View {
        output_id: 1,
        layout_generation: 1,
        timeline_ns: 0,
        viewport: r,
        uniform_scale: 1.,
        offset_x: 0.,
        offset_y: 0.,
        dpr: 1.,
    }
}
fn sample(a: &Arc<Snapshot>, b: &Arc<Snapshot>, v: View, alpha: f64) -> (Lens, [Pose; 65]) {
    let lease = Lease {
        sample: Sample {
            lower: a.clone(),
            upper: b.clone(),
            alpha,
            missed: false,
        },
        view: v,
        bounds: v.viewport,
    };
    let mut lens = Lens::default();
    let mut poses = [Pose::default(); 65];
    let n = unsafe {
        visuals_depth_kaleidoscope_sample(&lease, &mut lens, poses.as_mut_ptr(), poses.len())
    };
    assert_eq!(n, 65);
    (lens, poses)
}
#[test]
fn shared_eye_and_interpolated_glass() {
    let i = input();
    let outputs = i.layout.outputs.clone();
    let mut w = Kaleidoscope::new(i.clone());
    w.hand.begin(Mood::Twist, &mut w.rng);
    for t in 1..240 {
        w.step(t);
    }
    let a = Arc::new(w.snapshot());
    w.step(240);
    let b = Arc::new(w.snapshot());
    let mut shared = None;
    for o in &outputs {
        let v = view(o.rect);
        let (lens, mid) = sample(&a, &b, v, 0.5);
        assert_eq!(
            (lens.eye[0] + o.rect.x as f32, lens.eye[1] + o.rect.y as f32),
            (4280., 720.)
        );
        assert_eq!(lens.eye[2], 676.8);
        if let Some(other) = shared {
            assert_eq!(mid, other);
        }
        shared = Some(mid);
        let lo = sample(&a, &b, v, 0.).1;
        let hi = sample(&a, &b, v, 1.).1;
        assert_ne!(lo, hi);
        for ((l, h), m) in lo.iter().zip(hi).zip(mid) {
            for j in 0..4 {
                assert!((m.0[j] - (l.0[j] + h.0[j]) * 0.5).abs() < 2e-6);
            }
        }
    }
    let mut local = i;
    local.mode = MonitorMode::Mirrored;
    let mut w = Kaleidoscope::new(local);
    let a = Arc::new(w.snapshot());
    w.step(1);
    let b = Arc::new(w.snapshot());
    for o in outputs {
        let (lens, _) = sample(&a, &b, view(o.rect), 0.5);
        assert_eq!(
            lens.eye[..3],
            [
                (o.rect.width / 2.) as f32,
                (o.rect.height / 2.) as f32,
                (0.47 * o.rect.height) as f32
            ]
        );
    }
}
#[test]
fn both_mirror_groups_sample_the_same_point_at_either_bezel() {
    for mirrors in 0..2 {
        let mut i = input();
        i.parameters = vec![super::super::schema::Value {
            id: 1,
            kind: 3,
            payload: mirrors,
        }];
        let w = Kaleidoscope::new(i);
        let frame = Arc::new(w.snapshot());
        for pair in w.input.layout.outputs.windows(2) {
            let left = pair[0].rect;
            let right = pair[1].rect;
            let (a, _) = sample(&frame, &frame, view(left), 0.);
            let (b, _) = sample(&frame, &frame, view(right), 0.);
            for y in [210.5, 720.5, 1190.5] {
                let x = right.x;
                assert_eq!(
                    fold_reference(x - left.x, y - left.y, &a),
                    fold_reference(x - right.x, y - right.y, &b)
                );
            }
        }
    }
}
#[test]
fn reduced_motion_and_disabled_events() {
    let mut i = input();
    i.reduced_motion = true;
    let mut w = Kaleidoscope::new(i);
    let before = Arc::new(w.snapshot());
    let hash = w.state_hash();
    w.inject(Event {
        id: SWAP,
        at_ns: 60_000_000_000,
        anchor: [0.; 2],
        strength: 1.,
    })
    .unwrap();
    w.step(86_400);
    let after = Arc::new(w.snapshot());
    let v = view(w.input.layout.bounds);
    assert_eq!(
        sample(&before, &before, v, 0.),
        sample(&after, &after, v, 1.)
    );
    assert_eq!(hash, w.state_hash());
    assert_eq!(w.next_wake_ns(0), None);
    let mut w = Kaleidoscope::new(input());
    w.params.events = false;
    w.moments.time = 90.;
    for id in DUST..=SWAP {
        w.enact(id);
        assert!(w.moments.age(id) > 1000.);
    }
}
#[test]
fn speed_capped_twist_has_a_sustained_avalanche() {
    let mut i = input();
    i.density = 80.;
    i.parameters = vec![super::super::schema::Value {
        id: 2,
        kind: 1,
        payload: 14,
    }];
    let mut w = Kaleidoscope::new(i);
    for (id, second) in [(DUST, 35), (TWIST, 42)] {
        w.inject(Event {
            id,
            at_ns: second * 1_000_000_000,
            anchor: [4280., 720.],
            strength: 1.,
        })
        .unwrap();
    }
    let mut shifts = Vec::new();
    for frame in 0..48 * 60 {
        let old: Vec<_> = w.chamber.bodies.iter().map(|b| (b.pos, b.angle)).collect();
        w.step(frame * 2 + 1);
        w.step(frame * 2 + 2);
        if frame >= 42 * 60 {
            let mut sum = 0.;
            for (b, (pos, angle)) in w.chamber.bodies.iter().zip(old) {
                let d = b.pos - pos;
                let shift = d.0.hypot(d.1) + (b.angle - angle).abs() * b.cut.reach;
                sum += shift * shift;
            }
            shifts
                .push((sum / w.chamber.bodies.len() as f64).sqrt() * 676.8 * w.input.scale / 1.67);
        }
    }
    let peak = shifts.iter().copied().fold(0., f64::max);
    let moving = shifts
        .split(|s| *s <= 0.20 * peak)
        .map(<[_]>::len)
        .max()
        .unwrap_or(0);
    println!(
        "42 s recording twist: {moving} consecutive frames above 20% of peak glass motion ({peak:.2} eye px/frame)."
    );
    assert!(
        moving >= 66,
        "apparent granular motion should span at least 1.1 s at 60 fps"
    );
}
#[test]
fn physical_time_caps_at_one_and_a_half() {
    let mut at_cap = input();
    at_cap.speed = 1.5;
    let mut capped = Kaleidoscope::new(at_cap);
    let mut fastest = Kaleidoscope::new(input());
    // The seeded hand starts in Hold, so only the physical time scale can
    // change the glass poses over this first half-second.
    for tick in 1..=60 {
        capped.step(tick);
        fastest.step(tick);
        assert_eq!(capped.fraction, fastest.fraction);
        for (a, b) in capped.chamber.bodies.iter().zip(&fastest.chamber.bodies) {
            assert_eq!((a.pos, a.angle), (b.pos, b.angle));
        }
    }
}

#[test]
fn magnified_eye_tracks_the_heap_and_keeps_the_mote_in_view() {
    let mut i = input();
    i.density = 80.;
    i.parameters = vec![super::super::schema::Value {
        id: 2,
        kind: 1,
        payload: 14,
    }];
    let mut w = Kaleidoscope::new(i);
    for (id, second) in [(DUST, 35), (TWIST, 42), (GLINT, 49), (HOLD, 65)] {
        w.inject(Event {
            id,
            at_ns: second * 1_000_000_000,
            anchor: [4280., 720.],
            strength: 1.,
        })
        .unwrap();
    }
    let mut alive = 0;
    let mut visible = 0;
    let mut max_aim: f64 = 0.;
    for tick in 1..95 * 120 {
        let before = w.focus;
        w.step(tick);
        let d = w.focus - before;
        assert!(d.0.hypot(d.1) <= 0.04001);
        max_aim = max_aim.max(w.focus.0.hypot(w.focus.1));
        if tick % 120 != 0 || tick < 20 * 120 {
            continue;
        }
        let age = w.moments.age(DUST);
        let radius = 0.58 + 0.23 * (age * 0.20).sin();
        let side = 0.50 + 0.32 * (age * 0.37).sin();
        if age <= w.moments.mote_duration {
            alive += 1;
            assert!((0.3..0.85).contains(&radius) && (0.15..0.85).contains(&side));
            visible += u32::from(w.moments.dust() >= 0.93);
        }
        println!(
            "optics,{},{:?},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6}",
            tick / 120,
            w.hand.mood,
            w.hand.angle * 0.3,
            age,
            w.moments.mote_duration,
            radius,
            side,
            w.moments.dust(),
            w.focus.0.hypot(w.focus.1)
        );
    }
    assert!((0.07..=0.201).contains(&max_aim));
    assert!(alive >= 20 && visible * 100 >= 80 * alive);
}
#[test]
fn replay_bounds_and_recipe_change() {
    let i = input();
    let mut a = Kaleidoscope::new(i.clone());
    let mut b = Kaleidoscope::new(i);
    for w in [&mut a, &mut b] {
        for (id, t) in [(TWIST, 59), (SWAP, 60)] {
            w.inject(Event {
                id,
                at_ns: t * 1_000_000_000,
                anchor: [0.; 2],
                strength: 1.,
            })
            .unwrap();
        }
    }
    for t in 1..8200 {
        a.step(t);
        b.step(t);
        if t % 61 == 0 {
            assert_eq!(a.state_hash(), b.state_hash());
            for p in &a.chamber.bodies {
                for v in &p.cut.core[..p.cut.sides] {
                    let (s, c) = p.angle.sin_cos();
                    let x = p.pos.0 + v.0 * c - v.1 * s;
                    let y = p.pos.1 + v.0 * s + v.1 * c;
                    assert!(x.hypot(y) + p.cut.rounding < 1.001);
                }
            }
        }
    }
    assert_ne!(a.cell.revision, 1);
    assert_eq!(a.moments.pace, 1.5);
}
#[test]
fn all_moods_occur_and_holds_are_still() {
    let mut r = Pcg32::new(7, 19);
    let mut h = Hand::new(&mut r, 0.4);
    let mut moods = [0; 5];
    for _ in 0..120 * 3600 {
        let angle = h.angle;
        h.step(1. / 120., 1., &mut r);
        moods[h.mood as usize] += 1;
        if h.mood == Mood::Hold {
            assert_eq!(h.angle, angle);
        }
    }
    assert!(moods.iter().all(|x| *x > 0));
    println!("hour of hand moods {moods:?}");
}
#[test]
fn turning_still_fraction_over_two_minutes() {
    // Match the single 1920×1080 capture's Independent seed.
    let mut fractions = Vec::new();
    for turning in [0, 40, 100] {
        let mut i = input();
        i.seed = super::super::common::rng::output_seed(5, 5, "Virtual-0");
        i.speed = 1.;
        i.density = 50.;
        i.scale = 1.;
        i.parameters = vec![super::super::schema::Value {
            id: 4,
            kind: 1,
            payload: turning,
        }];
        let mut world = Kaleidoscope::new(i);
        let mut holds = 0;
        let mut stills = 0;
        for frame in 0..7200 {
            let before: Vec<_> = world
                .chamber
                .bodies
                .iter()
                .map(|b| (b.pos, b.angle))
                .collect();
            let tube = world.hand.angle;
            world.step(2 * frame + 1);
            world.step(2 * frame + 2);
            holds += usize::from(world.hand.mood == Mood::Hold);
            // Eyepiece coordinates at R=507.6. Ignore the slow echo drift.
            let mut shift = 0.;
            for (b, (p, angle)) in world.chamber.bodies.iter().zip(before) {
                let d = b.pos - p;
                let motion = d.0.hypot(d.1) + (b.angle - angle).abs() * b.cut.reach;
                shift += motion * motion;
            }
            let rms = (shift / world.chamber.bodies.len() as f64).sqrt() * 507.6 / 1.67;
            let spin = (world.hand.angle - tube).abs() * 0.30 * 507.6;
            stills += usize::from(rms + spin < 0.25);
        }
        println!("Turning {turning}: {stills}/7200 frames below 0.25 px; {holds}/7200 in Hold.");
        fractions.push(stills);
    }
    assert!(fractions.windows(2).all(|pair| pair[0] > pair[1]));
}
#[test]
#[ignore = "profile first mapping with three default-density independent chambers"]
fn measure_prewarm() {
    let mut samples = Vec::new();
    for activation in 0..32 {
        let mut i = input();
        i.mode = MonitorMode::Independent;
        i.speed = 1.;
        i.density = 50.;
        let start = std::time::Instant::now();
        for output in 0..3 {
            i.seed = 3 * activation + output;
            let world = Kaleidoscope::new(i.clone());
            std::hint::black_box(world.snapshot());
        }
        samples.push(start.elapsed().as_micros());
    }
    samples.sort_unstable();
    println!(
        "Time to construct, pre-roll and snapshot three chambers (µs): median {} p90 {} max {}",
        samples[16], samples[28], samples[31]
    );
}
#[test]
#[ignore = "release timing on the 7920 world"]
fn stress_tick_export() {
    let mut i = input();
    i.palette = 0;
    i.parameters = vec![super::super::schema::Value {
        id: 2,
        kind: 1,
        payload: 14,
    }];
    let views = i
        .layout
        .outputs
        .iter()
        .map(|o| view(o.rect))
        .collect::<Vec<_>>();
    let mut w = Kaleidoscope::new(i);
    inject_event_fixture(&mut w);
    let mut last = Arc::new(w.snapshot());
    const TICKS: usize = 95 * 120;
    let mut samples = Vec::with_capacity(TICKS);
    let mut cpu_samples = Vec::with_capacity(TICKS);
    for t in 1..=TICKS as u64 {
        let cpu = thread_cpu_ns();
        let now = std::time::Instant::now();
        w.step(t);
        let frame = Arc::new(w.snapshot());
        for &v in &views {
            std::hint::black_box(sample(&last, &frame, v, 0.5));
        }
        last = frame;
        samples.push(now.elapsed().as_nanos() as u64);
        if let (Some(start), Some(end)) = (cpu, thread_cpu_ns()) {
            cpu_samples.push(end - start);
        }
    }
    samples.sort_unstable();
    println!(
        "tick + snapshot + three exports (ns) median {} p99 {} max {}",
        samples[TICKS / 2],
        samples[TICKS * 99 / 100],
        samples[TICKS - 1]
    );
    cpu_samples.sort_unstable();
    let cpu_max = cpu_samples.last().copied().unwrap_or(samples[TICKS - 1]);
    let cpu_p99 = cpu_samples
        .get(cpu_samples.len() * 99 / 100)
        .copied()
        .unwrap_or(samples[TICKS * 99 / 100]);
    let cpu_mean = cpu_samples.iter().sum::<u64>() as f64 / cpu_samples.len().max(1) as f64;
    println!(
        "STRESS_METRICS wall_p99={} wall_max={} cpu_p99={} cpu_max={} cpu_mean={}",
        samples[TICKS * 99 / 100],
        samples[TICKS - 1],
        cpu_p99,
        cpu_max,
        cpu_mean
    );
    println!("max same-call thread CPU (ns) {cpu_max}");
    assert!(samples[TICKS * 99 / 100] <= 500_000 && cpu_max <= 2_000_000);
}

// The max gate excludes descheduling, per the foundation's timing contract.
fn thread_cpu_ns() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        #[repr(C)]
        struct Time {
            seconds: std::ffi::c_long,
            nanoseconds: std::ffi::c_long,
        }
        unsafe extern "C" {
            fn clock_gettime(id: std::ffi::c_int, time: *mut Time) -> std::ffi::c_int;
        }
        let mut time = Time {
            seconds: 0,
            nanoseconds: 0,
        };
        // Linux CLOCK_THREAD_CPUTIME_ID, with a valid writable timespec.
        if unsafe { clock_gettime(3, &mut time) } == 0 {
            return Some(time.seconds as u64 * 1_000_000_000 + time.nanoseconds as u64);
        }
    }
    None
}
#[test]
#[ignore = "two simulated hours, 864000 fixed ticks"]
fn two_hour_soak() {
    let mut w = Kaleidoscope::new(input());
    let mut held = std::collections::VecDeque::new();
    let mut rss = vec![];
    let v = view(w.input.layout.bounds);
    for t in 1..=864_000 {
        w.step(t);
        let frame = Arc::new(w.snapshot());
        if let Some(last) = held.back() {
            std::hint::black_box(sample(last, &frame, v, 0.5));
        }
        if held.len() == 5 {
            held.pop_front();
        }
        held.push_back(frame);
        if t % 108_000 == 0 {
            let mem = std::fs::read_to_string("/proc/self/status").unwrap();
            let size = mem
                .lines()
                .find(|s| s.starts_with("VmRSS:"))
                .unwrap()
                .split_whitespace()
                .nth(1)
                .unwrap()
                .parse::<usize>()
                .unwrap();
            rss.push(size);
            assert!(w.frames.borrow().len() <= 6);
            assert_eq!(w.chamber.bodies.len(), 64);
        }
    }
    println!("RSS KiB at 15 minute intervals {rss:?}");
    assert!(rss.iter().max().unwrap() - rss.iter().min().unwrap() < 1024);
}

fn fold_reference(x: f64, y: f64, lens: &Lens) -> (chamber::V, f64) {
    let p = chamber::V(
        (x - f64::from(lens.eye[0])) / f64::from(lens.eye[2]),
        (y - f64::from(lens.eye[1])) / f64::from(lens.eye[2]),
    );
    let radius = p.0.hypot(p.1);
    let mut band = 1.;
    let (sx, sy) = if lens.motion[3] < 0.5 {
        let k = radius.floor();
        let n = f64::from(lens.eye[3]);
        let a = p.1.atan2(p.0)
            + f64::from(lens.motion[0])
            + if k == 0. {
                0.
            } else {
                (if k % 2. == 0. { -1. } else { 1. }) * f64::from(lens.motion[1])
            };
        let fold =
            ((a * n * (k + 1.) / std::f64::consts::TAU + 0.5).rem_euclid(1.) * 2. - 1.).abs();
        let f = radius.fract();
        let distance = if k == 0. {
            f
        } else {
            0.35 + 0.65 * (if k % 2. == 0. { f } else { 1. - f })
        };
        let theta = fold * std::f64::consts::PI / n + 0.02;
        band = if k == 0. { 1. - f } else { f.min(1. - f) };
        let u = ((distance - 0.25) / 0.28).clamp(0., 1.);
        let guide = f64::from(lens.glass[1]) * (1. - u * u * (3. - 2. * u));
        (
            -0.82 + 1.67 * distance * theta.cos() + guide * f64::from(lens.guide[0]),
            0.46 + 1.67 * distance * (theta.sin() - 0.10) + guide * f64::from(lens.guide[1]),
        )
    } else {
        let (s, c) = f64::from(lens.motion[0]).sin_cos();
        let oy = (s * p.0 + c * p.1) / 0.68 / (3_f64.sqrt() / 2.);
        let ox = (c * p.0 - s * p.1 + f64::from(lens.motion[2] / lens.eye[2])) / 0.68 - oy * 0.5;
        let (a, b) = (ox.rem_euclid(1.), oy.rem_euclid(1.));
        let mut bary = if a + b <= 1. {
            [1. - a - b, a, b]
        } else {
            [a + b - 1., 1. - b, 1. - a]
        };
        bary.rotate_right(((ox.floor() - oy.floor()) as i64).rem_euclid(3) as usize);
        (
            -1.07 + 2.14 * (bary[1] + 0.5 * bary[2]),
            -0.14 + 1.15 * bary[2] * 3_f64.sqrt() * 0.5,
        )
    };
    let mirror = |z: f64, pivot: f64, lo: f64, hi: f64| {
        let z = pivot + (z - pivot) / f64::from(lens.glass[1]);
        lo + (hi - lo) * (1. - (((z - lo) / (hi - lo)).rem_euclid(2.) - 1.).abs())
    };
    (
        chamber::V(mirror(sx, -0.18, -0.98, 0.98), mirror(sy, 0.5, -0.11, 1.02)),
        band,
    )
}

#[test]
fn mirrored_lattice_keeps_one_chamber_phase_across_output_sizes() {
    let mut i = input();
    i.mode = MonitorMode::Mirrored;
    i.speed = 1.;
    i.scale = 1.;
    i.parameters = vec![super::super::schema::Value {
        id: 1,
        kind: 3,
        payload: 1,
    }];
    let outputs = i.layout.outputs.clone();
    let mut w = Kaleidoscope::new(i);
    let mut tick = 0;
    for seconds in [0, 1, 15, 45, 120, 600] {
        while tick < seconds * 120 {
            tick += 1;
            w.step(tick);
        }
        let frame = Arc::new(w.snapshot());
        let mut reference: Option<chamber::V> = None;
        for output in &outputs {
            let (lens, _) = sample(&frame, &frame, view(output.rect), 0.);
            assert_eq!(lens.motion[3], 1.);
            let point = fold_reference(f64::from(lens.eye[0]), f64::from(lens.eye[1]), &lens).0;
            if let Some(other) = reference {
                assert!(
                    (point.0 - other.0).abs() < 2e-6 && (point.1 - other.1).abs() < 2e-6,
                    "{seconds}s on {}x{}: {point:?} != {other:?}",
                    output.rect.width,
                    output.rect.height
                );
            } else {
                reference = Some(point);
            }
            if output.rect.height == 1440. {
                // Preserve the approved pixel drift on the reference displays.
                let expected =
                    ((seconds as f64 * 20.) % (3. * f64::from(lens.eye[2]) * 0.68)) as f32;
                assert_eq!(lens.motion[2], expected);
            }
        }
    }
}

#[test]
fn mirrored_single_output_keeps_its_pixel_drift() {
    let mut i = input();
    i.mode = MonitorMode::Mirrored;
    i.speed = 1.;
    i.parameters = vec![super::super::schema::Value {
        id: 1,
        kind: 3,
        payload: 1,
    }];
    let output = i.layout.outputs[2].clone();
    i.layout = Layout::new(vec![output.clone()], 1).unwrap();
    let mut w = Kaleidoscope::new(i);
    for tick in 1..=5400 {
        w.step(tick);
    }
    let frame = Arc::new(w.snapshot());
    let (lens, _) = sample(&frame, &frame, view(output.rect), 0.);
    let expected = ((45. * 20.) % (3. * f64::from(lens.eye[2]) * 0.68)) as f32;
    assert_eq!(lens.motion[2], expected);
}

#[test]
fn lattice_window_includes_the_air_pocket() {
    let mut i = input();
    i.seed = 3;
    i.speed = 1.;
    i.density = 50.;
    i.scale = 1.;
    i.palette = 5;
    i.parameters = vec![super::super::schema::Value {
        id: 1,
        kind: 3,
        payload: 1,
    }];
    let mut w = Kaleidoscope::new(i);
    for tick in 1..=5400 {
        w.step(tick);
    }
    let frame = Arc::new(w.snapshot());
    let mut clear = 0;
    let mut total = 0;
    for output in &w.input.layout.outputs {
        let v = view(output.rect);
        let lease = Lease {
            sample: Sample {
                lower: frame.clone(),
                upper: frame.clone(),
                alpha: 0.,
                missed: false,
            },
            view: v,
            bounds: v.viewport,
        };
        let mut lens = Lens::default();
        unsafe { visuals_depth_kaleidoscope_sample(&lease, &mut lens, std::ptr::null_mut(), 0) };
        for y in (8..output.rect.height as usize).step_by(16) {
            for x in (8..output.rect.width as usize).step_by(16) {
                let (p, _) = fold_reference(x as f64, y as f64, &lens);
                clear += usize::from(glass_distance(&w, p) > 0.009);
                total += 1;
            }
        }
    }
    println!("Lattice scene C at 45 s: {clear}/{total} samples in clear air.");
    assert!(clear * 4 >= total);
}
#[test]
#[ignore = "point KALEIDOSCOPE_PARITY at the private parity capture directory"]
fn gpu_fold_parity() {
    // Compare classified deep material interiors and clear air, excluding the
    // anti-aliased edge. This tests the transform, physical cut geometry and
    // both mirror groups against real composited output pixels.
    let root = std::path::PathBuf::from(std::env::var("KALEIDOSCOPE_PARITY").unwrap());
    for mirrors in 0..2 {
        let name = if mirrors == 0 {
            "parity-mandala"
        } else {
            "parity-lattice"
        };
        let file = std::fs::read_dir(root.join("scenes").join(name))
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .ends_with("-world.png")
                    && p.extension().is_some_and(|x| x == "png")
            })
            .unwrap();
        let pixels = std::process::Command::new("python3")
            .arg("-c")
            .arg(
                r#"from PIL import Image
import sys
with Image.open(sys.argv[1]) as source:
    image = source.convert("RGB")
assert image.size == (1920, 1080)
pixels = image.load()
sys.stdout.buffer.write(bytes(max(pixels[x, y])
                              for y in range(4, 1080, 8)
                              for x in range(4, 1920, 8)))"#,
            )
            .arg(file)
            .output()
            .unwrap();
        assert!(
            pixels.status.success(),
            "{}",
            String::from_utf8_lossy(&pixels.stderr)
        );
        assert_eq!(pixels.stdout.len(), 240 * 135);
        let mut i = input();
        i.speed = 1.;
        i.density = 50.;
        i.trails = 0.;
        // The wider Lattice field includes clear air as well as glass.
        i.scale = if mirrors == 0 { 1. } else { 0.5 };
        i.palette = 1;
        i.reduced_motion = true;
        i.layout = Layout::new(
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
        .unwrap();
        i.parameters = vec![super::super::schema::Value {
            id: 1,
            kind: 3,
            payload: mirrors,
        }];
        let w = Kaleidoscope::new(i);
        let a = Arc::new(w.snapshot());
        let v = view(w.input.layout.bounds);
        let lease = Lease {
            sample: Sample {
                lower: a.clone(),
                upper: a,
                alpha: 0.,
                missed: false,
            },
            view: v,
            bounds: v.viewport,
        };
        let mut lens = Lens::default();
        let mut poses = [Pose::default(); 65];
        unsafe { visuals_depth_kaleidoscope_sample(&lease, &mut lens, poses.as_mut_ptr(), 65) };
        let mut count = [0; 2];
        let mut wrong = [0; 2];
        for (j, observed) in pixels.stdout.iter().enumerate() {
            let (p, band) = fold_reference(
                (j % 240 * 8) as f64 + 4.5,
                (j / 240 * 8) as f64 + 4.5,
                &lens,
            );
            if band < 0.042 {
                continue;
            }
            let distance = glass_distance(&w, p);
            if distance.abs() < 0.035 {
                continue;
            }
            let filled = usize::from(distance < 0.);
            count[filled] += 1;
            wrong[filled] += usize::from((*observed > 9) != (filled != 0));
        }
        println!(
            "{name}: matched clear/filled {:?} of {count:?}",
            [count[0] - wrong[0], count[1] - wrong[1]]
        );
        for j in 0..2 {
            assert!(count[j] > 200 && wrong[j] * 100 < count[j]);
        }
    }
}

fn glass_distance(w: &Kaleidoscope, p: chamber::V) -> f64 {
    w.chamber
        .bodies
        .iter()
        .map(|b| {
            let (s, c) = b.angle.sin_cos();
            let d = (p - b.pos) * (1. / 0.93);
            let x = c * d.0 + s * d.1;
            let y = c * d.1 - s * d.0;
            match b.cut.sides {
                1 => x.hypot(y) - b.cut.rounding,
                2 => (x - x.clamp(b.cut.core[0].0, b.cut.core[1].0)).hypot(y) - b.cut.rounding,
                n => (0..n)
                    .map(|j| {
                        let a = b.cut.core[j];
                        let edge = b.cut.core[(j + 1) % n] - a;
                        ((x - a.0) * edge.1 - (y - a.1) * edge.0) / edge.0.hypot(edge.1)
                    })
                    .fold(f64::NEG_INFINITY, f64::max),
            }
        })
        .fold(f64::MAX, f64::min)
}

#[test]
fn events_respect_startup_queue_visibility_and_envelopes() {
    let mut w = Kaleidoscope::new(risky_input(7, 3.));
    w.moments.time = 59.;
    w.hand.begin(Mood::Twist, &mut w.rng);
    w.enact(SWAP);
    assert!(w.moments.age(SWAP) > 1e5);
    w.moments.time = 60.;
    w.hand.begin(Mood::Hold, &mut w.rng);
    w.enact(SWAP);
    assert!(w.swap_requested_at.is_some() || w.moments.start[2] == 60.);
    w.moments.time = 68.;
    w.try_begin_swap();
    let swap_start = w.moments.start[2];
    assert!((60.0..=68.0).contains(&swap_start));
    for (time, open) in [(2.5, 0.), (4., 0.), (7., 1.)] {
        w.moments.time = swap_start + time;
        assert_eq!(w.moments.aperture(), open);
    }
    w.enact(CLOUD);
    w.enact(DUST);
    w.enact(GLINT);
    w.moments.time += 0.4;
    assert!((w.moments.glint() - 1.).abs() < 1e-6);
    w.moments.time += 5.6;
    assert!((w.moments.cloud() - 1.).abs() < 1e-6);
    assert!(w.moments.dust() > 0.95);
    w.moments.time += 40.;
    assert_eq!(
        (w.moments.glint(), w.moments.cloud(), w.moments.dust()),
        (0., 0., 0.)
    );
}

/// Ten minutes of the real profile, sampled on each capped output's own grid.
/// CSV keeps the raw measurements so motion changes can be compared honestly.
#[test]
#[ignore = "writes a ten-minute motion diagnostic when PVS_KALEIDOSCOPE_TRACE is set"]
fn real_refresh_motion_trace() {
    use std::{collections::VecDeque, io::Write, time::Instant};
    let path = std::env::var("PVS_KALEIDOSCOPE_TRACE").expect("PVS_KALEIDOSCOPE_TRACE");
    let mut csv = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    writeln!(csv, "output,timeline_ns,ticks,tick_export_ns,fold,hand,guide_x,guide_y,piece_x,piece_y,piece_angle,glint_age,swap_age").unwrap();
    let mut i = input();
    i.palette = 0; // Ocean.
    i.parameters = vec![super::super::schema::Value {
        id: 2,
        kind: 1,
        payload: 14,
    }];
    let views: Vec<_> = i.layout.outputs.iter().map(|o| view(o.rect)).collect();
    let mut w = Kaleidoscope::new(i);
    let mut history = VecDeque::from([(0, Arc::new(w.snapshot()))]);
    let rate = w.rate();
    let mut tick = 0;
    let mut requests = Vec::new();
    for (id, (hz, divisor)) in [(59.95, 1.), (174.96, 3.), (239.76, 4.)]
        .into_iter()
        .enumerate()
    {
        for frame in 0..=(600. * hz / divisor) as u64 {
            requests.push(((frame as f64 * 1e9 * divisor / hz).round() as i64, id));
        }
    }
    requests.sort_unstable();
    for (ns, id) in requests {
        let b = rate.bracket(ns);
        let before = tick;
        let now = Instant::now();
        while tick < b.upper {
            tick += 1;
            w.step(tick);
            history.push_back((tick, Arc::new(w.snapshot())));
            if history.len() > 16 {
                history.pop_front();
            }
        }
        let a = &history.iter().find(|(t, _)| *t == b.lower).unwrap().1;
        let upper = &history.iter().find(|(t, _)| *t == b.upper).unwrap().1;
        let (lens, poses) = sample(a, upper, views[id], b.alpha);
        let cost = now.elapsed().as_nanos();
        writeln!(
            csv,
            "{id},{ns},{},{cost},{},{},{},{},{},{},{},{},{}",
            tick - before,
            lens.motion[1],
            lens.motion[0],
            lens.guide[0],
            lens.guide[1],
            poses[0].0[0],
            poses[0].0[1],
            poses[0].0[2],
            lens.flare[2],
            lens.flare[3]
        )
        .unwrap();
    }
}

#[test]
fn fractional_physics_and_focus_have_constant_presented_velocity() {
    // Isolate sampling from collisions and the deliberately easing hand. Each
    // feature has known constant velocity, including the 15 Hz focus targets.
    for (hz, divisor) in [(59.95, 1.), (174.96, 3.), (239.76, 4.)] {
        let mut w = Kaleidoscope::new(input());
        let v = view(w.input.layout.bounds);
        let mut frames = Vec::new();
        for tick in 0..=242 {
            w.tick = tick;
            w.moments.time = tick as f64 / 120.;
            let physical = tick as f64 * 1.5;
            w.fraction = physical.fract();
            w.chamber.bodies[0].pos = chamber::V(physical.floor() * 0.0001, 0.);
            w.physics_previous[0] = (chamber::V((physical.floor() - 1.) * 0.0001, 0.), 0.);
            w.focus_previous = chamber::V((tick / 8) as f64 * 0.001, 0.);
            w.focus = w.focus_previous + chamber::V(0.001, 0.);
            frames.push(Arc::new(w.snapshot()));
        }
        let mut prior: Option<(f32, f32)> = None;
        for frame in 0..=(2. * hz / divisor) as usize {
            let ns = (frame as f64 * 1e9 * divisor / hz).round() as i64;
            let b = w.rate().bracket(ns);
            let (lens, poses) = sample(
                &frames[b.lower as usize],
                &frames[b.upper as usize],
                v,
                b.alpha,
            );
            if let Some((piece, focus)) = prior {
                let dt = divisor / hz;
                assert!(
                    (f64::from(poses[0].0[0] - piece) - dt * 0.018).abs() < 1e-8,
                    "physics at {hz} Hz"
                );
                assert!(
                    (f64::from(lens.guide[0] - focus) - dt * 0.015).abs() < 1e-8,
                    "focus at {hz} Hz"
                );
            }
            prior = Some((poses[0].0[0], lens.guide[0]));
        }
    }
}

#[test]
fn recipe_change_preserves_the_continuous_optical_timeline() {
    let mut w = Kaleidoscope::new(input());
    w.hand.begin(Mood::Twist, &mut w.rng);
    for tick in 1..=240 {
        w.step(tick);
    }
    let a = Arc::new(w.snapshot());
    w.step(241);
    w.cell = Arc::new(Cell::new(&w.chamber, 2));
    let b = Arc::new(w.snapshot());
    let v = view(w.input.layout.bounds);
    let lo = sample(&a, &b, v, 0.).0;
    let mid = sample(&a, &b, v, 0.5).0;
    let hi = sample(&a, &b, v, 1.).0;
    assert!(hi.motion[1] > lo.motion[1]);
    assert!((mid.motion[1] - (lo.motion[1] + hi.motion[1]) * 0.5).abs() < 1e-8);
    assert_ne!(hi.motion[0], lo.motion[0]);
    assert!((mid.motion[0] - (lo.motion[0] + hi.motion[0]) * 0.5).abs() < 1e-7);
    let light = [lo.light[0] + hi.light[0], lo.light[1] + hi.light[1]];
    let length = light[0].hypot(light[1]);
    for j in 0..2 {
        assert!((mid.light[j] - light[j] / length).abs() < 1e-7);
        assert!((mid.guide[j] - (lo.guide[j] + hi.guide[j]) * 0.5).abs() < 1e-7);
    }
}

fn risky_input(seed: u64, speed: f64) -> WorldInput {
    let mut i = input();
    i.seed = seed;
    i.speed = speed;
    i.palette = 0;
    i.parameters = vec![super::super::schema::Value {
        id: 2,
        kind: 1,
        payload: 14,
    }];
    i
}
fn inject_event_fixture(w: &mut Kaleidoscope) {
    for (id, second) in [
        (DUST, 35),
        (TWIST, 42),
        (GLINT, 49),
        (TWIST, 59),
        (SWAP, 60),
        (HOLD, 65),
    ] {
        w.inject(Event {
            id,
            at_ns: second * 1_000_000_000,
            anchor: [4280., 720.],
            strength: 1.,
        })
        .unwrap();
    }
}
#[test]
fn magnified_physics_pose_is_canonical() {
    let mut w = Kaleidoscope::new(risky_input(7, 3.));
    inject_event_fixture(&mut w);
    let mut max_penetration: f64 = 0.;
    let mut max_error: f64 = 0.;
    let mut max_optical: f64 = 0.;
    for tick in 1..=95 * 120 {
        w.step(tick);
        if tick < 42 * 120 {
            continue;
        }
        let snapshot = Arc::new(w.snapshot());
        let (_, poses) = sample(&snapshot, &snapshot, view(w.input.layout.bounds), 0.);
        for (i, b) in w.chamber.bodies.iter().enumerate() {
            let (before, angle) = w.physics_previous[i];
            let p = before + (b.pos - before) * w.fraction + w.chamber.transport;
            let a = angle + (b.angle - angle) * w.fraction;
            let error = (f64::from(poses[i].0[0]) - p.0).hypot(f64::from(poses[i].0[1]) - p.1)
                + (f64::from(poses[i].0[2]) - a).abs() * b.cut.reach;
            max_error = max_error.max(error);
        }
        let swap = w.swap_phase();
        if !(3.25..4.).contains(&swap) {
            let depth = w.chamber.max_penetration();
            if depth > max_penetration {
                println!(
                    "penetration peak {depth:.6} at {:.3}, mood {:?}, swap {swap:.3}",
                    tick as f64 / 120.,
                    w.hand.mood
                );
            }
            max_penetration = max_penetration.max(depth);
            max_optical = max_optical.max(w.chamber.max_optical_penetration());
        }
    }
    println!("canonical pose max error {max_error}, same-layer penetration {max_penetration}");
    assert!(
        max_error < 2e-6,
        "no presentation-only pose lag: {max_error}"
    );
    println!("visible optical penetration {max_optical}");
    assert!(
        max_optical < 0.001,
        "rendered glass must not cross: {max_optical}"
    );
    assert!(
        max_penetration < 0.01,
        "contact-resolved glass: {max_penetration}"
    );
}
#[test]
fn magnified_physics_material_sweep_diagnostic() {
    use std::collections::VecDeque;
    let mut worst: f64 = 0.;
    for seed in [7, 11, 23] {
        for speed in [1., 3.] {
            let mut w = Kaleidoscope::new(risky_input(seed, speed));
            inject_event_fixture(&mut w);
            let mut old = Vec::new();
            let mut window = VecDeque::new();
            let mut peak: f64 = 0.;
            for tick in 1..=95 * 120 {
                w.step(tick);
                let poses: Vec<_> = w
                    .chamber
                    .bodies
                    .iter()
                    .map(|b| (b.pos, b.angle, b.cut.reach))
                    .collect();
                if old.len() == poses.len() && (42 * 120..=49 * 120).contains(&tick) {
                    let rms = (poses
                        .iter()
                        .zip(&old)
                        .map(|((p, a, r), (q, b, _))| {
                            let d: chamber::V = *p - *q;
                            (d.0.hypot(d.1) + (a - b).abs() * r).powi(2)
                        })
                        .sum::<f64>()
                        / poses.len() as f64)
                        .sqrt();
                    window.push_back(rms);
                    if window.len() > 12 {
                        window.pop_front();
                    }
                    if window.len() == 12 {
                        peak = peak.max(window.iter().sum::<f64>() * 10. * 2. / (1.67 * 0.65));
                    }
                }
                old = poses;
            }
            println!(
                "seed {seed}, speed {speed}: physical 100ms RMS surface sweep {peak:.6} eye radii/s"
            );
            worst = worst.max(peak);
        }
    }
    println!("worst physical surface rate: {worst}");
}
#[test]
fn magnified_swap_key_follows_outward_travel_and_refill_finishes() {
    let mut w = Kaleidoscope::new(risky_input(11, 3.));
    w.moments.time = 60.25;
    w.moments.start[(SWAP - 1) as usize] = 60.;
    for (p, b) in w.swap_origins.iter_mut().zip(&w.chamber.bodies) {
        *p = b.pos;
    }
    assert_eq!(
        w.transit_key(0),
        Some(1.),
        "no fade before outward movement"
    );
    w.moments.time = 60.5;
    w.chamber.transport = chamber::V(0.1, -0.01);
    let k = w.transit_key(0).unwrap();
    assert!(k > 0.05 && k < 1.);
    for scale in [1.5, 2.] {
        let speed = REFILL_SPEED * 1.67 * 0.65 / scale;
        assert_eq!(chamber::transfer_distance(3., speed, 0.4), 0.4);
        assert_eq!(chamber::transfer_speed(0., speed, 0.4), 0.);
        assert_eq!(chamber::transfer_speed(3., speed, 0.4), 0.);
        let mut before = 0.;
        for tick in 1..=360 {
            let d = chamber::transfer_distance(tick as f64 / 120., speed, 0.4);
            assert!(d >= before && d - before <= speed / 120. + 1e-9);
            before = d;
        }
    }
}

#[test]
fn magnified_refill_aim_stays_on_filled_glass() {
    for seed in [7, 11, 23] {
        for speed in [1., 3.] {
            let mut w = Kaleidoscope::new(risky_input(seed, speed));
            for (id, second) in [(TWIST, 59), (SWAP, 60)] {
                w.inject(Event {
                    id,
                    at_ns: second * 1_000_000_000,
                    anchor: [4280., 720.],
                    strength: 1.,
                })
                .unwrap();
            }
            let mut worst: f64 = 1.;
            let mut samples = 0;
            let mut max_pose_error: f64 = 0.;
            for tick in 1..=85 * 120 {
                w.step(tick);
                let age = w.moments.age(SWAP);
                let refill_age = age - w.swap_refill_age;
                if (2.5..=4.5).contains(&refill_age) && tick % 8 == 0 {
                    let frame = Arc::new(w.snapshot());
                    let (lens, poses) = sample(&frame, &frame, view(w.input.layout.bounds), 0.);
                    let focus = chamber::V(f64::from(lens.guide[0]), f64::from(lens.guide[1]));
                    worst = worst.min(w.apex_fill(focus));
                    if refill_age >= 3. {
                        for (i, body) in w.chamber.bodies.iter().enumerate() {
                            let p = poses[i].0;
                            let q = body.pos + w.chamber.transport;
                            let translation = (f64::from(p[0]) - q.0).hypot(f64::from(p[1]) - q.1);
                            let rotation = ((f64::from(p[2]) - body.angle + std::f64::consts::PI)
                                .rem_euclid(TAU)
                                - std::f64::consts::PI)
                                .abs()
                                * body.cut.reach;
                            max_pose_error = max_pose_error
                                .max((translation + rotation) * w.input.scale / (1.67 * 0.65));
                        }
                    }
                    samples += 1;
                }
            }
            assert!(
                samples >= 28,
                "the test must observe two seconds after the actual refill"
            );
            println!(
                "seed {seed}, speed {speed}: refill focus coverage after refill+2.5..4.5 {worst}, focus {:?}, target {:?}",
                w.focus, w.refill_target
            );
            println!("refill material pose error {max_pose_error} reference eye radii");
            assert!(
                max_pose_error <= 0.02,
                "the presented glass has settled by refill+3s"
            );
            assert!(
                worst >= 0.9,
                "the incoming core must fill and stay filled: {worst}"
            );
        }
    }
}

#[test]
fn an_overlapping_twist_keeps_refined_swap_contacts() {
    let mut w = Kaleidoscope::new(risky_input(11, 3.));
    inject_event_fixture(&mut w);
    for tick in 1..=60 * 120 + 1 {
        w.step(tick);
    }
    assert!(
        w.event_contacts,
        "overlapping twist and swap must keep contact-resolved glass"
    );
}

#[test]
fn queued_swap_waits_for_visible_glass_and_has_a_timeout() {
    let mut w = Kaleidoscope::new(risky_input(7, 3.));
    for (id, second) in [(TWIST, 59), (SWAP, 60)] {
        w.inject(Event {
            id,
            at_ns: second * 1_000_000_000,
            anchor: [4280., 720.],
            strength: 1.,
        })
        .unwrap();
    }
    let mut started = false;
    for tick in 1..=68 * 120 {
        w.step(tick);
        if tick == 60 * 120 {
            assert!(w.swap_requested_at.is_some());
            assert!(w.moments.age(SWAP) > 1e5, "don't pour an invisible heap");
        }
        if w.moments.start[2] >= 60. {
            assert!(w.apex_fill(w.focus) >= SWAP_VISIBLE_COVERAGE || w.swap_forced_sparse);
            assert!(
                !w.swap_forced_sparse
                    || w.moments.time >= w.twist_swap_ready_at + SWAP_POST_TWIST_WAIT
            );
            assert!(w.swap_queue_delay > 0. && w.swap_queue_delay < SWAP_MAX_WAIT);
            let frame = Arc::new(w.snapshot());
            let (lens, _) = sample(&frame, &frame, view(w.input.layout.bounds), 0.);
            assert!((f64::from(lens.mote[3]) - w.swap_queue_delay).abs() < 1e-6);
            started = true;
            break;
        }
    }
    assert!(started);
    let mut empty = Kaleidoscope::new(risky_input(7, 3.));
    empty.chamber.bodies.clear();
    empty.moments.time = 60.;
    empty.enact(SWAP);
    empty.moments.time = 68. - 1. / 120.;
    empty.try_begin_swap();
    assert!(empty.swap_requested_at.is_some());
    empty.moments.time = 68.;
    empty.try_begin_swap();
    assert_eq!(empty.moments.start[2], 68.);
    assert_eq!(empty.swap_queue_delay, 8.);
    assert!(
        empty.swap_forced_sparse,
        "non-Twist empty queues keep their sparse pour"
    );
}

#[test]
fn scheduled_swaps_use_the_same_visibility_queue() {
    let mut w = Kaleidoscope::new(risky_input(7, 3.));
    w.moments.due[2] = 60.;
    w.inject(Event {
        id: TWIST,
        at_ns: 59_000_000_000,
        anchor: [4280., 720.],
        strength: 1.,
    })
    .unwrap();
    for tick in 1..=68 * 120 {
        w.step(tick);
    }
    assert!(w.moments.start[2] > 60. && w.moments.start[2] <= 68.);
    assert!(w.swap_queue_delay > 0. && w.swap_queue_delay <= 8.);
}

#[test]
fn refill_hold_lasts_six_seconds_even_at_restless_turning() {
    let mut rng = Pcg32::new(7, 19);
    let mut hand = Hand::new(&mut rng, 1.);
    hand.hold_for(6., 3., &mut rng);
    let angle = hand.angle;
    for _ in 0..6 * 120 - 1 {
        hand.step(1. / 120., 3., &mut rng);
        assert_eq!(hand.mood, Mood::Hold);
        assert_eq!(hand.angle, angle);
    }
}

#[test]
fn refill_guide_paths_match_position_velocity_and_acceleration() {
    use chamber::V;
    let norm = |p: V| p.0.hypot(p.1);
    let initial = (V(0.1, -0.2), V(0.03, 0.02), V(-0.01, 0.02));
    let end = (V(-0.3, 0.4), V(-0.01, 0.03), V(0.02, -0.01));
    let path = GuidePath::with_end(60., 2.5, initial, end);
    for (time, expected) in [(60., initial), (62.5, end)] {
        let actual = path.sample(time);
        assert!(norm(actual.0 - expected.0) < 1e-12);
        assert!(norm(actual.1 - expected.1) < 1e-12);
        assert!(norm(actual.2 - expected.2) < 1e-12);
    }
    let following = GuidePath::new(62.5, 1.5, end.0, end.1, end.2, V(0., 0.));
    let a = path.sample(62.5);
    let b = following.sample(62.5);
    assert!(norm(a.0 - b.0) < 1e-12 && norm(a.1 - b.1) < 1e-12 && norm(a.2 - b.2) < 1e-12);
    let stopped = following.sample(64.);
    assert!(norm(stopped.0) < 1e-12 && norm(stopped.1) < 1e-12 && norm(stopped.2) < 1e-12);
}

#[test]
fn refill_guide_is_sampled_at_the_presented_time() {
    use chamber::V;
    let mut w = Kaleidoscope::new(risky_input(11, 3.));
    let path = GuidePath::new(60., 2.5, V(0.2, 0.1), V(0., 0.), V(0., 0.), V(-0.3, 0.2));
    w.refill_guide = Some(path);
    w.moments.time = 61.;
    let a = Arc::new(w.snapshot());
    w.moments.time = 61. + 1. / 120.;
    let b = Arc::new(w.snapshot());
    for alpha in [0., 0.25, 0.5, 0.75, 1.] {
        let (lens, _) = sample(&a, &b, view(w.input.layout.bounds), alpha);
        let expected = path.sample(61. + alpha / 120.).0;
        assert!((f64::from(lens.guide[0]) - expected.0).abs() < 1e-7);
        assert!((f64::from(lens.guide[1]) - expected.1).abs() < 1e-7);
    }
}

#[test]
fn a_following_refill_guide_does_not_skip_the_next_recipe_check() {
    use chamber::V;
    let mut w = Kaleidoscope::new(risky_input(11, 3.));
    w.refill_guide = Some(GuidePath::new(
        59.,
        1.,
        w.focus,
        V::default(),
        V::default(),
        w.focus,
    ));
    w.refill_guide_phase = RefillGuide::Following;
    w.moments.time = 60.;
    w.swap_requested_at = Some(52.);
    w.try_begin_swap();
    assert!(w.refill_guide_phase == RefillGuide::Unchecked);
    w.guide_magnified_eye();
    assert!(w.refill_guide_phase == RefillGuide::Unchecked);
}

#[test]
fn sparse_incoming_recipes_aim_continuously_and_hold_the_refill() {
    let mut exercised = 0;
    for angle in [-2.0, -1.5, -1.0, -0.5, 0., 0.5, 1., 1.5, 2.] {
        let mut w = Kaleidoscope::new(risky_input(23, 1.));
        w.hand.angle = angle;
        for _ in 0..800 {
            w.chamber.step_refined(angle, true);
        }
        if w.apex_fill(w.ordinary_guide_offset()) >= 0.9
            || w.apex_fill(w.filled_refill_target()) < 0.95
        {
            continue;
        }
        w.moments.start[2] = 60.;
        w.moments.time = 63.95;
        let before = w.focus;
        w.guide_magnified_eye();
        assert!(w.refill_guide_phase == RefillGuide::Braking);
        let path = w.refill_guide.unwrap();
        let (p, v, a) = path.sample(63.95);
        assert!((p - before).0.hypot((p - before).1) < 1e-12);
        assert!(v.0.hypot(v.1) < 1e-12 && a.0.hypot(a.1) < 1e-12);
        let mut previous = path;
        for step in 1..=125 {
            w.moments.time = 63.95 + step as f64 * (8. / 120.);
            w.guide_magnified_eye();
            let next = w.refill_guide.unwrap();
            if next.start != previous.start {
                let old = previous.sample(next.start);
                let new = next.sample(next.start);
                for d in [old.0 - new.0, old.1 - new.1, old.2 - new.2] {
                    assert!(d.0.hypot(d.1) < 1e-10, "C2 path handoff");
                }
            }
            let age = w.moments.age(SWAP);
            if (6.5..8.5).contains(&age) {
                assert!(
                    w.apex_fill(next.sample(w.moments.time).0) >= 0.9,
                    "refill stays full"
                );
            }
            previous = next;
        }
        exercised += 1;
    }
    assert!(
        exercised > 0,
        "exercise sparse but recoverable incoming glass"
    );
}

#[test]
fn mouth_shadow_floor_is_a_quarter_in_encoded_light() {
    let mut w = Kaleidoscope::new(risky_input(11, 3.));
    w.moments.time = 60.5;
    w.moments.start[2] = 60.;
    for (origin, body) in w.swap_origins.iter_mut().zip(&w.chamber.bodies) {
        *origin = body.pos;
    }
    w.chamber.transport = chamber::V(
        MOUTH_SHADOW_FADE_DISTANCE * (1.67 * 0.65) / w.input.scale,
        0.,
    );
    let linear = w.transit_key(0).unwrap();
    let encoded = 1.055 * linear.powf(1. / 2.4) - 0.055;
    assert!((encoded - 0.25).abs() < 1e-12, "encoded floor: {encoded}");
}

#[test]
fn swap_shadow_phase_uses_presented_time_and_only_magnified_mandala() {
    let mut w = Kaleidoscope::new(risky_input(11, 3.));
    w.moments.time = 60.;
    let before = Arc::new(w.snapshot());
    w.moments.start[2] = 60. + 1. / 120.;
    w.moments.time = 60. + 1. / 120.;
    let start = Arc::new(w.snapshot());
    let (lens, _) = sample(&before, &start, view(w.input.layout.bounds), 0.5);
    assert!(lens.motion[2] >= 0., "the future swap must not start early");
    w.moments.time = 60.5;
    let a = Arc::new(w.snapshot());
    w.moments.time = 60.5 + 1. / 120.;
    let b = Arc::new(w.snapshot());
    for alpha in [0., 0.25, 0.5, 1.] {
        let (lens, _) = sample(&a, &b, view(w.input.layout.bounds), alpha);
        let age = 60.5 + alpha / 120. - w.moments.start[2];
        assert!((f64::from(lens.motion[2]) + 1. + age).abs() < 1e-6);
    }
    let mut normal = input();
    normal.scale = 1.;
    normal.parameters.clear();
    let mut w = Kaleidoscope::new(normal);
    w.moments.time = 60.5;
    w.moments.start[2] = 60.;
    let frame = Arc::new(w.snapshot());
    let (lens, _) = sample(&frame, &frame, view(w.input.layout.bounds), 0.);
    assert!(
        lens.motion[2] >= 0.,
        "ordinary Mandala retains its drift value"
    );
    for age in [0., 0.44, 1., 2.5, 3.5, 5., 7.] {
        w.moments.time = 60. + age;
        assert_eq!(
            w.transit_key(0),
            None,
            "ordinary optics never use the mouth fade"
        );
    }
}

#[test]
fn mouth_shadow_waits_for_the_actual_pour() {
    let mut w = Kaleidoscope::new(risky_input(11, 3.));
    w.moments.start[2] = 60.;
    for (origin, body) in w.swap_origins.iter_mut().zip(&w.chamber.bodies) {
        *origin = body.pos;
    }
    w.chamber.bodies[0].pos.0 += 0.1;
    for age in [0., 0.1, 0.25, 0.4, 0.43999] {
        w.moments.time = 60. + age;
        assert_eq!(
            w.transit_key(0),
            Some(1.),
            "no shadow from the hand's leading turn"
        );
    }
    let origin = w.chamber.bodies[0].pos;
    w.step(60 * 120 + 53);
    assert!(w.swap_shadow_started);
    assert_eq!(w.swap_origins[0].0, origin.0);
    assert_eq!(w.swap_origins[0].1, origin.1);
}

#[test]
fn outgoing_mouth_shadow_preserves_the_visible_pour() {
    let mut w = Kaleidoscope::new(risky_input(7, 3.));
    w.moments.start[2] = 60.;
    w.moments.time = 61.;
    for (origin, body) in w.swap_origins.iter_mut().zip(&w.chamber.bodies) {
        *origin = body.pos;
    }
    let radius = (1.67 * 0.65) / w.input.scale;
    w.chamber.transport = chamber::V(0.65 * radius, 0.);
    assert!(
        w.transit_key(0).unwrap() >= 0.20,
        "the outgoing glass must remain visible through the first 0.65 eye radii"
    );
    w.chamber.transport = chamber::V((MOUTH_SHADOW_FADE_DISTANCE + 0.15) * radius, 0.);
    assert!(
        w.transit_key(0).unwrap() <= 0.08,
        "the departing glass must still fall into the mouth shadow"
    );
}

#[test]
fn outgoing_tail_keeps_the_leading_beat_and_follows_the_mouth_order() {
    let mut w = Kaleidoscope::new(risky_input(11, 3.));
    w.moments.start[2] = 60.;
    let rear = -0.18 - 0.64 / w.input.scale;
    let front = rear + 1.67 / w.input.scale;
    for (i, x) in [front, rear].into_iter().enumerate() {
        w.chamber.bodies[i].pos = chamber::V(x, 0.5);
        w.swap_origins[i] = w.chamber.bodies[i].pos;
    }
    w.moments.time = 60. + 0.44 + 0.7 - 1e-8;
    assert_eq!(w.transit_key(0), Some(1.));
    assert_eq!(w.transit_key(1), Some(1.));
    w.moments.time = 61.65;
    assert!(
        w.transit_key(0).unwrap() < 1e-10,
        "front glass leaves first"
    );
    assert!(
        w.transit_key(1).unwrap() > 0.8,
        "rear glass still travels visibly"
    );
    w.moments.time = 62.05;
    assert!(w.transit_key(1).unwrap() < 1e-10, "a bounded tail");
}

#[test]
fn empty_eye_light_estimate_sees_dimming_without_a_geometry_change() {
    let mut w = Kaleidoscope::new(risky_input(11, 3.));
    w.moments.start[2] = 60.;
    w.moments.time = 61.;
    for (origin, body) in w.swap_origins.iter_mut().zip(&w.chamber.bodies) {
        *origin = body.pos;
    }
    let full = w.visible_pour_light();
    assert!(full > 0.);
    let radius = 1.67 * 0.65 / w.input.scale;
    for origin in &mut w.swap_origins {
        origin.0 -= MOUTH_SHADOW_FADE_DISTANCE * radius;
    }
    let dim = w.visible_pour_light();
    assert!(dim > 0. && dim < full * 0.3, "same glass, less light");
    for origin in &mut w.swap_origins {
        origin.0 -= 2. * radius;
    }
    assert_eq!(w.visible_pour_light(), 0.);
}

#[test]
fn hidden_refill_position_does_not_reveal_glass_early() {
    let mut w = Kaleidoscope::new(risky_input(11, 3.));
    w.moments.start[2] = 60.;
    w.moments.time = 63.9;
    w.chamber.transport = chamber::V(REFILL_OFFSET, -0.1 * REFILL_OFFSET);
    assert_eq!(w.transit_key(0), Some(0.));
    w.moments.time = 64.;
    assert_eq!(w.transit_key(0), Some(0.), "no opacity jump at entry");
    w.moments.time = 64.1;
    w.chamber.transport.0 -= 0.01;
    assert!(
        (0.0..0.01).contains(&w.transit_key(0).unwrap()),
        "entry eases from black"
    );
}

#[test]
fn magnified_refill_starts_after_the_empty_eye_and_never_later_than_four_seconds() {
    let mut w = Kaleidoscope::new(risky_input(11, 3.));
    w.moments.start[2] = 60.;
    w.moments.time = 61.4;
    w.note_pour_light(0.05);
    assert_eq!(
        w.swap_refill_age, 4.,
        "five percent is still visibly filled"
    );
    w.note_pour_light(0.049);
    assert!((w.swap_refill_age - 2.6).abs() < 1e-10);
    assert_eq!(w.swap_empty_age, Some(w.moments.age(SWAP)));
    assert!((w.swap_phase_at(2.6) - 4.).abs() < 1e-10);
    assert!((w.swap_phase_at(2.6 + 3.) - 7.).abs() < 1e-10);
    w.moments.time = 62.6;
    assert!(w.swap_aperture().abs() < 1e-10);
    assert!(w.emission_estimate().abs() < 1e-10);
    w.moments.time = 63.6;
    assert!(
        w.swap_aperture() > 0.1,
        "the earlier refill opens its light envelope too"
    );
    w.moments.time = 62.;
    w.note_pour_light(0.);
    assert!(
        (w.swap_refill_age - 2.6).abs() < 1e-10,
        "first empty crossing owns the beat"
    );

    w.swap_empty_age = None;
    w.swap_refill_age = 4.;
    w.moments.time = 63.1;
    w.note_pour_light(0.);
    assert_eq!(
        w.swap_refill_age, 4.,
        "late drains retain the four-second maximum"
    );
    w.input.scale = 1.;
    w.swap_empty_age = None;
    w.moments.time = 61.;
    w.note_pour_light(0.);
    assert_eq!(
        w.swap_empty_age, None,
        "ordinary optics retain their timeline"
    );
}

#[test]
fn magnified_mote_never_outlives_or_exceeds_its_glass_key() {
    let mut w = Kaleidoscope::new(risky_input(11, 3.));
    w.moments.start[2] = 60.;
    w.moments.start[0] = 35.;
    w.moments.mote_duration = 40.;
    w.moments.time = 60.8;
    let age = w.moments.age(DUST);
    let radius = 0.58 + 0.23 * (age * 0.20).sin();
    let side = 0.50 + 0.32 * (age * 0.37).sin();
    let point = w.eye_sample_point(radius, side);
    let selected = w.chamber.bodies.iter().position(|b| !b.cut.back).unwrap();
    for (i, body) in w.chamber.bodies.iter_mut().enumerate() {
        body.pos = chamber::V(10. + i as f64, 10.);
    }
    w.chamber.transport = chamber::V(0.1, -0.01);
    w.chamber.bodies[selected].pos = point - w.chamber.transport;
    let unit = 1.67 * 0.65 / w.input.scale;
    w.swap_origins[selected] = point - chamber::V(0.75 * unit, 0.);
    for (previous, body) in w.physics_previous.iter_mut().zip(&w.chamber.bodies) {
        *previous = (body.pos, body.angle);
    }
    let glass = w.glass_key_at(point);
    assert!(glass > 0. && glass < 0.5);
    let frame = Arc::new(w.snapshot());
    let (lens, poses) = sample(&frame, &frame, view(w.input.layout.bounds), 0.);
    assert!(lens.mote[2] > 0. && f64::from(lens.mote[2]) <= glass + 1e-7);
    assert_eq!(poses[w.chamber.bodies.len()].0[3], lens.mote[2]);
    w.chamber.bodies[selected].pos.0 += 10.;
    let frame = Arc::new(w.snapshot());
    let (lens, poses) = sample(&frame, &frame, view(w.input.layout.bounds), 0.);
    assert_eq!(
        lens.mote[2], 0.,
        "no floating silver after its glass leaves"
    );
    assert_eq!(poses[w.chamber.bodies.len()].0[3], 0.);
}

#[test]
fn magnified_swap_waits_past_the_twist_peak_when_glass_is_visible() {
    let mut w = Kaleidoscope::new(risky_input(11, 3.));
    w.moments.time = 60.;
    w.enact(TWIST);
    let peak = 60. + w.hand.twist_peak_delay(w.input.speed).unwrap();
    w.focus = w.filled_refill_target();
    assert!(w.apex_fill(w.focus) >= SWAP_VISIBLE_COVERAGE);
    w.enact(SWAP);
    assert!(w.swap_requested_at.is_some());
    w.moments.time = peak + 0.5 - 1. / 120.;
    w.try_begin_swap();
    assert!(w.swap_requested_at.is_some());
    w.moments.time = peak + 0.5;
    w.try_begin_swap();
    assert_eq!(w.swap_requested_at, None);
    assert!((w.moments.start[2] - peak - 0.5).abs() < 1e-10);
    assert!(!w.swap_forced_sparse);
}

#[test]
fn a_swap_queued_behind_twist_gives_the_empty_eye_only_one_more_second() {
    let mut w = Kaleidoscope::new(risky_input(11, 3.));
    w.moments.time = 60.;
    w.enact(TWIST);
    let ready = w.twist_swap_ready_at;
    w.chamber.bodies.clear();
    w.enact(SWAP);
    w.moments.time = ready;
    w.try_begin_swap();
    assert!(w.swap_requested_at.is_some(), "allow the visibility window");
    w.moments.time = ready + 1. - 1. / 120.;
    w.try_begin_swap();
    assert!(w.swap_requested_at.is_some());
    w.moments.time = ready + 1.;
    w.try_begin_swap();
    assert_eq!(
        w.swap_requested_at, None,
        "a static empty eye must not consume the eight-second escape"
    );
    assert!((w.moments.start[2] - ready - 1.).abs() < 1e-10);
    assert!(w.swap_queue_delay < SWAP_MAX_WAIT);
    let approved_sparse_pace = 0.36 * 1.67 * 0.65 / w.input.scale;
    assert!(
        (w.pour_speed() - approved_sparse_pace).abs() < 1e-12,
        "short under-gate queues use the actual C4 sparse settings"
    );
    // A later empty view did not wait behind that old Twist. It must retain
    // the ordinary eight-second escape rather than inheriting its deadline.
    let later = w.moments.time + 10.;
    w.moments.time = later;
    w.enact(SWAP);
    w.moments.time = later + 1.;
    w.try_begin_swap();
    assert!(w.swap_requested_at.is_some());
    w.moments.time = later + 8. - 1. / 120.;
    w.try_begin_swap();
    assert!(w.swap_requested_at.is_some());
    w.moments.time = later + 8.;
    w.try_begin_swap();
    assert_eq!(w.swap_requested_at, None);
    assert!((w.swap_queue_delay - 8.).abs() < 1e-10);
}

#[test]
fn magnified_slow_twist_cannot_extend_the_eight_second_swap_queue() {
    let input = risky_input(11, 0.25);
    let viewport = input.layout.bounds;
    let mut w = (DESCRIPTOR.create)(input).unwrap();
    for (id, second) in [(TWIST, 59), (SWAP, 60)] {
        w.inject(Event {
            id,
            at_ns: second * 1_000_000_000,
            anchor: [4280., 720.],
            strength: 1.,
        })
        .unwrap();
    }
    for tick in 1..=80 * 120 {
        w.step(tick);
        if tick < 60 * 120 {
            continue;
        }
        let frame = Arc::new(w.snapshot());
        let (lens, _) = sample(&frame, &frame, view(viewport), 0.);
        if lens.flare[3] < 7. {
            let start = frame.time_ns as f64 / 1e9 - f64::from(lens.flare[3]);
            let queue = f64::from(lens.mote[3]);
            println!("QUEUE_PROBE speed=0.25, request=60.000, start={start:.3}, queue={queue:.3}s");
            assert!(
                queue <= 8. + 1. / 120. + 1e-6,
                "the absolute timeout must bound a slow Twist: {queue}s"
            );
            assert!(start <= 68. + 1. / 120. + 1e-6);
            return;
        }
    }
    panic!("the requested swap never started");
}

#[test]
fn magnified_pour_schedule_observes_both_requested_swaps() {
    let mut w = Kaleidoscope::new(risky_input(11, 3.));
    for (id, second) in [
        (DUST, 35),
        (TWIST, 42),
        (GLINT, 49),
        (TWIST, 59),
        (SWAP, 60),
        (TWIST, 74),
        (SWAP, 76),
    ] {
        w.inject(Event {
            id,
            at_ns: second * 1_000_000_000,
            anchor: [4280., 720.],
            strength: 1.,
        })
        .unwrap();
    }
    let mut last = -1.;
    let mut completed = 0;
    for tick in 1..=95 * 120 {
        w.step(tick);
        let start = w.moments.start[2];
        if start >= 60. && start != last && w.swap_phase() >= 4. {
            let film_onset = start + 0.44 - 20.;
            let approved_onset = [44.57333333333334, 57.465][completed];
            assert!(
                (film_onset - approved_onset).abs() < 1e-9,
                "approved Speed-300 pour onset changed: {film_onset}"
            );
            assert!(w.swap_refill_age <= 4.);
            if let Some(empty) = w.swap_empty_age {
                assert!((w.swap_refill_age - (empty + 1.2).min(4.)).abs() < 1e-10);
            }
            println!(
                "R1_SWAP start={start} empty={:?} refill={} queue={} twist_ready={} forced_sparse={}",
                w.swap_empty_age,
                w.swap_refill_age,
                w.swap_queue_delay,
                w.twist_swap_ready_at,
                w.swap_forced_sparse
            );
            last = start;
            completed += 1;
        }
    }
    assert_eq!(completed, 2, "neither queued request may disappear");
}

#[test]
fn magnified_event_replay_is_deterministic() {
    let mut previous = None;
    for seed in [7, 11, 19] {
        let mut a = Kaleidoscope::new(risky_input(seed, 3.));
        let mut b = Kaleidoscope::new(risky_input(seed, 3.));
        inject_event_fixture(&mut a);
        inject_event_fixture(&mut b);
        for tick in 1..=70 * 120 {
            a.step(tick);
            b.step(tick);
            if tick.is_multiple_of(120) {
                assert_eq!(a.state_hash(), b.state_hash(), "seed {seed}, tick {tick}");
            }
        }
        assert_ne!(
            previous,
            Some(a.state_hash()),
            "seeds must produce distinct worlds"
        );
        previous = Some(a.state_hash());
    }
}

#[test]
fn swap_queue_and_hold_changes_are_limited_to_magnified_mandala() {
    for (points, scale) in [(8, 2.), (14, 1.)] {
        let mut config = risky_input(7, 3.);
        config.parameters[0].payload = points;
        config.scale = scale;
        let mut w = Kaleidoscope::new(config);
        assert!(!w.magnified_mandala());
        w.chamber.bodies.clear();
        w.moments.time = 60.;
        w.hand.begin(Mood::Hold, &mut w.rng);
        w.enact(SWAP);
        assert!(w.moments.age(SWAP) > 1e5);
        assert!(w.swap_requested_at.is_none());
        w.hand.begin(Mood::Twist, &mut w.rng);
        w.enact(SWAP);
        assert_eq!(w.moments.start[2], 60.);
        assert!(w.swap_requested_at.is_none());
    }
    let mut w = Kaleidoscope::new(risky_input(7, 3.));
    w.chamber.bodies.clear();
    w.moments.time = 60.;
    w.hand.begin(Mood::Hold, &mut w.rng);
    w.enact(SWAP);
    assert_eq!(w.swap_requested_at, Some(60.));
    assert!(w.moments.age(SWAP) > 1e5);
}
