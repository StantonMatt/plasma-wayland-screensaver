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
    let i = input();
    let views = i
        .layout
        .outputs
        .iter()
        .map(|o| view(o.rect))
        .collect::<Vec<_>>();
    let mut w = Kaleidoscope::new(i);
    let mut last = Arc::new(w.snapshot());
    let mut samples = Vec::with_capacity(7200);
    for t in 1..=7200 {
        let now = std::time::Instant::now();
        w.step(t);
        let frame = Arc::new(w.snapshot());
        for &v in &views {
            std::hint::black_box(sample(&last, &frame, v, 0.5));
        }
        last = frame;
        samples.push(now.elapsed().as_nanos() as u64);
    }
    samples.sort_unstable();
    println!(
        "tick + snapshot + three exports (ns) median {} p99 {} max {}",
        samples[3600], samples[7128], samples[7199]
    );
    assert!(samples[7128] <= 500_000 && samples[7199] <= 2_000_000);
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
fn events_respect_startup_holds_and_opt_out() {
    let mut w = Kaleidoscope::new(input());
    w.moments.time = 59.;
    w.hand.begin(Mood::Twist, &mut w.rng);
    w.enact(SWAP);
    assert!(w.moments.age(SWAP) > 1e5);
    w.moments.time = 60.;
    w.hand.begin(Mood::Hold, &mut w.rng);
    w.enact(SWAP);
    assert!(w.moments.age(SWAP) > 1e5);
    w.hand.begin(Mood::Twist, &mut w.rng);
    w.enact(SWAP);
    assert_eq!(w.moments.aperture(), 1.);
    for (time, open) in [(62.5, 0.), (64., 0.), (67., 1.)] {
        w.moments.time = time;
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
