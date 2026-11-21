use super::super::{
    common::layout::{Layout, OutputRect, Rect},
    ffi::View,
};
use super::*;
use optics::{Glass, Packet};
use std::time::Instant;

fn input(still: bool) -> WorldInput {
    let outputs = [
        (0., 0., 2560., 1440.),
        (2560., 0., 3440., 1440.),
        (6000., 195., 1920., 1080.),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (x, y, width, height))| OutputRect {
        id: i as u64,
        primary: i == 1,
        rect: Rect {
            x,
            y,
            width,
            height,
        },
    })
    .collect();
    WorldInput {
        visual_id: 3,
        seed: 23,
        mode: MonitorMode::Spanning,
        layout: Layout::new(outputs, 1).unwrap(),
        speed: 3.,
        density: 100.,
        scale: 2.,
        trails: 1.,
        palette: 2,
        reduced_motion: still,
        parameters: vec![],
        environment: Environment::default(),
    }
}
fn view(rect: Rect, ns: i64) -> View {
    View {
        output_id: 0,
        layout_generation: 1,
        timeline_ns: ns,
        viewport: rect,
        uniform_scale: 1.,
        offset_x: 0.,
        offset_y: 0.,
        dpr: 1.,
    }
}
fn packet() -> Box<Packet> {
    // All fields are numbers or nullable raw pointers.
    Box::new(unsafe { std::mem::zeroed() })
}
#[test]
fn exposure_and_projection_are_per_presentation() {
    let mut world = Voyage::new(input(false));
    world.step(1200);
    let mut left = packet();
    let mut mid = packet();
    let a = &world.frame.art;
    let ns = 40_004_411_764;
    let lv = view(a.input.layout.outputs[0].rect, ns);
    let mv = view(a.input.layout.outputs[1].rect, ns);
    optics::export(&world.frame, &lv, &[0; 250], &mut left);
    optics::export(&world.frame, &mv, &[0; 250], &mut mid);
    assert_eq!(left.uniforms.rotation, mid.uniforms.rotation);
    assert_eq!(left.uniforms.old_rotation, mid.uniforms.old_rotation);
    assert_eq!(left.uniforms.shift, mid.uniforms.shift);
    let l = Glass::new(a, &lv);
    let m = Glass::new(a, &mv);
    assert_eq!(l.anchor, [4280., 720.]);
    assert_eq!(m.anchor, [1720., 720.]);
    assert!((l.f * std::f64::consts::PI - 7920.).abs() < 1e-8);
    let pose = world.frame.pose(ns as f64 * 1e-9);
    let eye = pose.attitude.inverse();
    for x in [-1.8, -0.8, 0., 1.] {
        let target = eye.apply(V(x, 0.6, 1.));
        let p = l.project(target, pose).unwrap();
        let q = m.project(target, pose).unwrap();
        assert!((p[0] - q[0] - 2560.).abs() < 1e-10);
        assert_eq!(p[1], q[1]);
    }
    let later = view(lv.viewport, ns + 2_000_000);
    optics::export(&world.frame, &later, &[0; 250], &mut left);
    assert_ne!(mid.uniforms.eye, left.uniforms.eye);
    let mut mirror = input(false);
    mirror.mode = MonitorMode::Mirrored;
    let mirror = Voyage::new(mirror);
    let g = Glass::new(&mirror.frame.art, &lv);
    assert!(!g.cylinder);
    assert_eq!(g.anchor, [1280., 720.]);
    assert!((g.f * 25f64.to_radians().tan() - 720.).abs() < 1e-9);
}
#[test]
fn retained_stream_records_match_seek_and_still() {
    let mut via = Voyage::new(input(false));
    let mut direct = Voyage::new(input(false));
    let saved = via.frame.atlas.clone();
    for n in 1..=5400 {
        via.step(n)
    }
    direct.step(5400);
    assert_eq!(direct.state_hash(), via.state_hash());
    for (l, r) in direct.frame.atlas.iter().zip(via.frame.atlas.iter()) {
        assert_eq!(l.stamp, r.stamp);
        assert_eq!(l.stars, r.stars)
    }
    assert!(
        saved
            .iter()
            .zip(via.frame.atlas.iter())
            .any(|(a, b)| a.stamp != b.stamp)
    );
    let mut still = Voyage::new(input(true));
    let mut p = packet();
    let mut q = packet();
    let v = still.frame.art.input.layout.outputs[0].rect;
    optics::export(&still.frame, &view(v, 0), &[0; 250], &mut p);
    still.step(900);
    optics::export(&still.frame, &view(v, 30_000_000_000), &[0; 250], &mut q);
    assert_eq!(p.uniforms.eye, q.uniforms.eye);
    assert_eq!(p.uniforms.shift, [0.; 4]);
    assert_eq!(p.uniforms.rotation, p.uniforms.old_rotation);
    assert_eq!(p.groups, q.groups);
    assert_eq!(p.uniforms.optics, q.uniforms.optics);
    assert_eq!(p.debris, 0);
}
#[test]
fn texture_revisions_recover_skipped_rows() {
    let mut w = Voyage::new(input(false));
    let mut p = packet();
    let rect = w.frame.art.input.layout.outputs[1].rect;
    let mut known = [0; 250];
    for time in [0, 3, 4, 15, 90, 300, 7200] {
        w.step(time * 30);
        let v = view(rect, time as i64 * 1_000_000_000);
        optics::export(&w.frame, &v, &known, &mut p);
        assert!(p.group_count > 0 && p.group_count < 1024);
        for group in 0..p.group_count as usize {
            let packed = p.groups[group / 2] >> (16 * (group % 2));
            let row = (packed & 2047) >> 3;
            assert_eq!(row < 125, group < p.near_group_count as usize);
        }
        for row in &p.upload[..p.updates as usize] {
            assert!(row.count <= 1024);
            known[row.row as usize] = row.stamp
        }
        optics::export(&w.frame, &v, &known, &mut p);
        assert_eq!(p.updates, 0);
        for eye in p.uniforms.eye {
            assert!(eye[..3].iter().all(|x| *x >= 0. && *x < 5.));
        }
        let folded = p.uniforms.eye[0];
        let exact = w.frame.pose(time as f64).p;
        let relative = exact.2.rem_euclid(5.);
        assert!((relative - f64::from(folded[2])).abs() < 3e-7);
    }
}
#[cfg(feature = "visual-capture")]
#[test]
fn forced_turn_crosses_bezel_and_returns() {
    let mut profile = input(false);
    profile.seed = 1;
    profile.speed = 1.;
    profile.density = 50.;
    profile.scale = 1.;
    profile.trails = 0.35;
    profile.parameters.push(super::super::schema::Value {
        id: 2,
        kind: 1,
        payload: 100,
    });
    let mut w = Voyage::new(profile);
    for (id, at) in [(3, 40), (1, 62)] {
        w.inject(Event {
            id,
            at_ns: at * 1_000_000_000,
            anchor: [0.; 2],
            strength: 1.,
        })
        .unwrap();
    }
    let g = Glass::new(
        &w.frame.art,
        &view(w.frame.art.input.layout.outputs[0].rect, 0),
    );
    let vanishing = |t: f64| {
        let pose = w.frame.pose(t);
        g.project(w.frame.pose(t + 0.0001).p - pose.p, pose)
            .unwrap()[0]
    };
    println!(
        "Steering 100 world VP at 39, 50 and 61 s: {:?}",
        [39., 50., 61.].map(vanishing)
    );
    assert!(vanishing(39.) > 2560.);
    assert!(vanishing(50.) < 2560.);
    assert!(vanishing(61.) > 2560.);
    // Use translation between real camera positions, rather than relying on
    // the heading supplied to the shader, to measure the focus of expansion.
    let step = 1. / 30.;
    let mut side = 0.;
    let mut hold = 0.;
    let mut yaw_rate = 0f64;
    let mut focus_rate = 0f64;
    let yaw = |t| {
        let eye = w.frame.pose(t).attitude.inverse().apply(V(0., 0., 1.));
        eye.0.atan2(eye.2)
    };
    // A far star in the quiet seam crop exposed a discontinuity in angular
    // velocity at each route knot as the pilot began to turn back.
    for knot in 4 * 51..4 * 55 {
        let t = f64::from(knot) / 4.;
        let h = 0.001;
        let speed_change = g.f * ((yaw(t + h) - yaw(t)) - (yaw(t) - yaw(t - h))) / h;
        assert!(
            speed_change.abs() < 0.05,
            "t={t}, yaw rate step {speed_change} px/s"
        );
    }
    for frame in 30 * 22..30 * 78 {
        let t = frame as f64 * step;
        yaw_rate = yaw_rate.max(((yaw(t + step) - yaw(t)) / step).abs().to_degrees());
        focus_rate = focus_rate.max(
            ((vanishing(t + step) - vanishing(t)) / (step * g.f))
                .abs()
                .to_degrees(),
        );
        let inside_bezel = ((2560. - vanishing(t)) / g.f).to_degrees();
        if (8. ..=12.).contains(&inside_bezel) {
            hold += step;
        }
        if vanishing(t) < 2560. {
            side += step;
        }
    }
    let t = 50.;
    w.step((t * 30.) as u64);
    let now = w.frame.pose(t);
    let later = w.frame.pose(t + step);
    let focus = g.project(now.direction, now).unwrap();
    assert!((0. ..g.height).contains(&focus[1]), "focus {focus:?}");
    let mut flow = Vec::new();
    for row in &w.frame.atlas[..125] {
        for star in &row.stars {
            let position = row.cell.base()
                + V(
                    star.body[0] as f64,
                    star.body[1] as f64,
                    star.body[2] as f64,
                );
            if (position - now.p).length() > 1.82 {
                continue;
            }
            let a = g.project(position - now.p, now).unwrap();
            if !(0. ..6000.).contains(&a[0]) || !(0. ..g.height).contains(&a[1]) {
                continue;
            }
            let radius = |p: [f64; 2]| (p[0] - focus[0]).hypot(p[1] - focus[1]);
            let r = radius(a);
            if (0.2..0.30).contains(&(r / g.h)) {
                let b = g.project(position - later.p, later).unwrap();
                flow.push((radius(b) - r) / step);
            }
        }
    }
    flow.sort_by(f64::total_cmp);
    let radial = flow[flow.len() / 2];
    println!(
        "Excursion side {side:.2}s, deep hold {hold:.2}s, yaw {yaw_rate:.3} deg/s, focus {focus_rate:.3} deg/s, radial {radial:.1} px/s ({} near heads)",
        flow.len()
    );
    assert!((4. ..=8.).contains(&hold) && side > hold);
    assert!(yaw_rate <= 1. && focus_rate <= 3. && radial >= 50.);

    let mut profile = input(false);
    profile.seed = 1;
    profile.speed = 1.;
    let mut recording = Voyage::new(profile);
    for (id, at) in [(1, 45), (3, 15)] {
        recording
            .inject(Event {
                id,
                at_ns: at * 1_000_000_000,
                anchor: [0.; 2],
                strength: 1.,
            })
            .unwrap();
    }
    let pose = recording.frame.pose(55.);
    let vp = g.project(pose.direction, pose).unwrap();
    assert!((2560. ..6000.).contains(&vp[0]));
    println!("Recording at 55 s: world vanishing point {vp:?}");
}
#[test]
#[ignore = "run as an isolated release measurement"]
fn stress_and_stream_bandwidth() {
    let start = Instant::now();
    let mut i = input(false);
    i.parameters.push(super::super::schema::Value {
        id: 1,
        kind: 3,
        payload: 1,
    });
    let mut w = Voyage::new(i);
    let activation = start.elapsed();
    let mut p = packet();
    let mut known = [[0; 250]; 3];
    let mut bytes = [0usize; 3];
    let mut times = Vec::new();
    let mut render_times = Vec::new();
    let n = 18000;
    for tick in 0..n {
        let start = Instant::now();
        w.step(tick);
        let snapshot = w.snapshot();
        std::hint::black_box(snapshot);
        times.push(start.elapsed().as_nanos());
        for i in 0..3 {
            let v = view(
                w.frame.art.input.layout.outputs[i].rect,
                w.rate().tick_ns(tick),
            );
            optics::export(&w.frame, &v, &known[i], &mut p);
            assert!(p.group_count < 1024);
            for u in &p.upload[..p.updates as usize] {
                if tick > 0 {
                    bytes[i] += u.count.div_ceil(4) as usize * 4 * 32
                }
                known[i][u.row as usize] = u.stamp;
            }
        }
        render_times.push(start.elapsed().as_nanos());
    }
    render_times.sort_unstable();
    times.sort_unstable();
    let p99 = times[times.len() * 99 / 100];
    let max = times[times.len() - 1];
    let rates = bytes.map(|b| b as f64 / (n as f64 / 30.) / 1000.);
    println!(
        "activation={activation:?} tick+export p99={p99}ns max={max}ns, streamed decimal KB/s per output={rates:?}"
    );
    println!(
        "including three viewport packers: p99={}ns max={}ns",
        render_times[render_times.len() * 99 / 100],
        render_times[render_times.len() - 1]
    );
    let mut error = 0f64;
    for tick in n..=216000 {
        w.step(tick);
        if tick % 30 == 0 {
            let t = tick as f64 / 30.;
            let h = w.frame.pose(t);
            let here = space::Cell::at(h.p, 1.);
            let row = &w.frame.atlas[here.slot()];
            let first = row.stars[0];
            let local = [first.body[0], first.body[1], first.body[2]];
            for (i, (c, x)) in [(here.0, h.p.0), (here.1, h.p.1), (here.2, h.p.2)]
                .into_iter()
                .enumerate()
            {
                let gpu = (c.rem_euclid(5) as f32 + local[i] - x.rem_euclid(5.) as f32 + 2.5)
                    .rem_euclid(5.)
                    - 2.5;
                let exact = c as f64 - x + f64::from(local[i]);
                error = error.max((f64::from(gpu) - exact).abs());
            }
        }
    }
    println!(
        "continuous two-hour max-warp soak: 216000 ticks, max cell-relative f32 position error={error:e}"
    );
    assert!(error < 1e-6);
    assert!(p99 <= 500_000 && max <= 2_000_000);
    assert!(rates.iter().all(|r| *r <= 100.));
}

#[cfg(feature = "visual-capture")]
#[test]
fn forced_events_use_the_shared_world_and_respect_the_toggle() {
    for enabled in [true, false] {
        let mut profile = input(false);
        profile.mode = MonitorMode::Independent;
        profile.speed = 1.;
        profile.parameters.push(super::super::schema::Value {
            id: 4,
            kind: 2,
            payload: u64::from(enabled),
        });
        let mut w = Voyage::new(profile);
        for (id, at) in [(4, 20), (5, 40), (7, 60), (1, 80), (6, 90)] {
            w.inject(Event {
                id,
                at_ns: at * 1_000_000_000,
                anchor: [0.; 2],
                strength: 1.,
            })
            .unwrap()
        }
        let mut p = packet();
        for (t, kind) in [(25, 3), (44, 6), (63, 5), (92, 7)] {
            w.step(t * 30);
            optics::export(
                &w.frame,
                &view(
                    w.frame.art.input.layout.outputs[0].rect,
                    t as i64 * 1_000_000_000,
                ),
                &[0; 250],
                &mut p,
            );
            assert_eq!(
                p.sprites[..p.sprite_count as usize]
                    .iter()
                    .any(|s| s.material[1] as i32 == kind),
                enabled,
                "t={t} kind={kind}"
            );
            assert!(p.sprite_count + if p.debris > 0 { 43 } else { 0 } <= 64);
        }
    }
}

#[test]
fn celestial_overdraw_and_population() {
    for density in [10., 50., 100.] {
        let mut profile = input(false);
        profile.mode = MonitorMode::Independent;
        profile.speed = 1.;
        profile.density = density;
        let mut world = Voyage::new(profile);
        let mut n = 0;
        let mut halos = 0;
        for time in [0, 10, 20, 45] {
            world.step(time * 30);
            let f = &world.frame;
            let pose = f.pose(time as f64);
            let rect = f.art.input.layout.outputs[0].rect;
            let v = view(rect, time as i64 * 1_000_000_000);
            let g = Glass::new(&f.art, &v);
            let mut p = packet();
            optics::export(f, &v, &[0; 250], &mut p);
            let sky_area: f64 = p.sprites[..p.sprite_count as usize]
                .iter()
                .filter(|s| s.material[1] <= 1.)
                .map(|s| 4. * f64::from(s.shape[2]) * f64::from(s.shape[3]))
                .sum();
            assert!(sky_area <= 2.5 * rect.area());
            let mut count = 0;
            let mut bright = 0;
            for (i, row) in f.atlas.iter().enumerate() {
                let size = universe::SHELL[i / 125];
                for star in &row.stars {
                    let delta = (row.cell.base()
                        + V(
                            star.body[0] as f64,
                            star.body[1] as f64,
                            star.body[2] as f64,
                        ))
                        * size
                        - pose.p;
                    if delta.length() > 2. * size || delta.length() < 0.012 {
                        continue;
                    }
                    if let Some([x, y]) = g.project(delta, pose)
                        && x >= 0.
                        && y >= 0.
                        && x < rect.width
                        && y < rect.height
                    {
                        count += 1;
                        if star.body[3] > 0.68 {
                            bright += 1
                        }
                    }
                }
            }
            n += count;
            halos += bright;
        }
        println!(
            "density {density}: mean projected stars {} with {} halos",
            n / 4,
            halos / 4
        );
        assert!(n > 250 && n / 4 < 2600);
        assert!(halos / 4 <= 160);
    }
}

#[test]
fn changed_density_replaces_resident_rows() {
    let before = Voyage::new(input(false));
    let mut setup = input(false);
    setup.density = 10.;
    let after = Voyage::new(setup);
    let known = std::array::from_fn(|i| before.stream.atlas[i].stamp);
    let mut p = packet();
    let v = view(after.frame.art.input.layout.outputs[1].rect, 0);
    optics::export(&after.frame, &v, &known, &mut p);
    assert!(p.updates > 0);
    for row in &p.upload[..p.updates as usize] {
        assert_ne!(row.stamp, known[row.row as usize]);
        assert_eq!(
            before.stream.atlas[row.row as usize].stars[0],
            after.stream.atlas[row.row as usize].stars[0]
        );
    }
}

#[test]
fn culling_keeps_both_shutter_endpoints_at_every_bezel() {
    for mode in [MonitorMode::Spanning, MonitorMode::Mirrored] {
        let mut setup = input(false);
        setup.mode = mode;
        setup.parameters.push(super::super::schema::Value {
            id: 1,
            kind: 3,
            payload: 1,
        });
        let mut w = Voyage::new(setup);
        for t in [0.37, 20.59, 120.9, 7325.13] {
            w.step((t * 30.0_f64).ceil() as u64);
            let frame = &w.frame;
            let head = frame.pose(t);
            let tail = frame.pose(t - frame.art.shutter);
            for output in &frame.art.input.layout.outputs {
                let v = view(output.rect, (t * 1e9) as i64);
                let g = Glass::new(&frame.art, &v);
                let mut p = packet();
                optics::export(frame, &v, &[0; 250], &mut p);
                let mut submitted = [false; 250];
                for i in 0..p.group_count as usize {
                    let id = (p.groups[i / 2] >> (16 * (i % 2))) & 2047;
                    submitted[id as usize / 8] = true;
                }
                for (row, page) in frame
                    .atlas
                    .iter()
                    .enumerate()
                    .filter(|(row, _)| !submitted[*row])
                {
                    let scale = universe::SHELL[row / 125];
                    for star in &page.stars {
                        let position = (page.cell.base()
                            + V(
                                star.body[0] as f64,
                                star.body[1] as f64,
                                star.body[2] as f64,
                            ))
                            * scale;
                        if (position - head.p).length() >= 2. * scale {
                            continue;
                        }
                        let ray = position - head.p;
                        if let (Some(a), Some(b)) = (
                            g.project(ray, head),
                            g.project(ray.unit() + head.direction * 0.018, head),
                        ) {
                            let vp = p.uniforms.field;
                            let radius =
                                (a[0] - f64::from(vp[2])).hypot(a[1] - f64::from(vp[3])) / g.h;
                            let reach = g.h
                                * f64::from(p.uniforms.old_axis[3])
                                * head.exposure
                                * (1.19 * radius).min(1.)
                                * space::smooth((radius - 0.018) / 0.064);
                            let norm = (a[0] - b[0]).hypot(a[1] - b[1]).max(0.0001);
                            // Check the whole optically extended exposure, including
                            // the case where a tail enters from the other output.
                            for step in 0..=8 {
                                let t = (step as f64 / 8.) * reach / norm;
                                let x = a[0] - (a[0] - b[0]) * t;
                                let y = a[1] - (a[1] - b[1]) * t;
                                assert!(
                                    x < -3. || y < -3. || x > g.width + 3. || y > g.height + 3.,
                                    "culled a visible optical exposure at ({x}, {y}) on {}",
                                    output.id
                                );
                            }
                        }
                        for pose in [head, tail] {
                            if let Some([x, y]) = g.project(position - pose.p, pose) {
                                assert!(
                                    x < -3. || y < -3. || x > g.width + 3. || y > g.height + 3.,
                                    "culled a star reaching ({x}, {y}) on output {} at {t}",
                                    output.id
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(feature = "visual-capture")]
#[test]
fn jump_envelope_and_arrival_geometry() {
    for (seed, event, start) in [(1, 1, 30), (23, 2, 80)] {
        let mut profile = input(false);
        profile.seed = super::super::common::rng::output_seed(seed, 3, "Virtual-0");
        profile.speed = 1.;
        profile.scale = 1.;
        profile.mode = MonitorMode::Independent;
        let mut world = Voyage::new(profile);
        world
            .inject(Event {
                id: event,
                at_ns: start * 1_000_000_000,
                anchor: [0.; 2],
                strength: 1.,
            })
            .unwrap();
        let leg = &world.frame.leg;
        let warp = leg.warp_start();
        let arrival = leg.arrival_start();
        let mut spool = packet();
        let v = view(
            world.frame.art.input.layout.outputs[0].rect,
            ((warp - 0.25) * 1e9).round() as i64,
        );
        optics::export(&world.frame, &v, &[0; 250], &mut spool);
        let reach =
            spool.uniforms.axis[3] * spool.uniforms.old_axis[3] * spool.uniforms.viewport[2];
        assert!(
            (10. ..=30.).contains(&reach),
            "long-shutter spool {reach} px"
        );
        assert_eq!(leg.pose(warp).exposure, 1.60);
        assert!(leg.pose(warp + 0.6).exposure < 1.012);
        assert!(leg.pose(warp - 0.2).warp == leg.pose(warp + 0.2).warp);
        assert!(leg.pose(warp + 3.).bow > 0.999);
        assert!((leg.pose(arrival - 0.4).exposure - leg.pose(arrival - 0.2).exposure) > 0.);
        assert!((leg.pose(arrival).exposure + 0.08).abs() < 1e-7);
        assert!(leg.pose(arrival + 0.351).exposure.abs() < 1e-7);
        let drop = leg.pose(arrival);
        let target = leg.destination.p - drop.p;
        let angle = drop.direction.dot(target.unit()).acos().to_degrees();
        let at_pass = leg.pose(arrival + 13.);
        println!(
            "scene seed {seed} arrival {arrival:.2}s; target angle {angle:.2}deg distance {:.4} -> {:.4}, landmark {:?}",
            target.length(),
            (leg.destination.p - at_pass.p).length(),
            leg.destination.kind
        );
        assert!((4.0..8.0).contains(&angle));
        assert!((leg.destination.p - at_pass.p).length() < 0.12 * target.length());
        if event == 2 {
            assert_eq!(leg.destination.kind, Kind::Open);
        }
    }
}

#[test]
fn forward_parallax_at_cruise_speeds() {
    for speed in [1., 3.] {
        let mut profile = input(false);
        profile.seed = super::super::common::rng::output_seed(1, 3, "Virtual-0");
        profile.speed = speed;
        profile.mode = MonitorMode::Independent;
        let mut world = Voyage::new(profile);
        let t = 10.;
        world.step((t * 30.) as u64);
        let f = &world.frame;
        let g = Glass::new(
            &f.art,
            &view(f.art.input.layout.outputs[0].rect, 10_000_000_000),
        );
        let now = f.pose(t);
        let mut later = f.pose(t + 0.013);
        // Remove the camera pan to measure each depth layer's own proper motion.
        later.attitude = now.attitude;
        let vp = g.project(now.direction, now).unwrap();
        let half_diag = 0.5 * g.width.hypot(g.height);
        let mut samples = [Vec::new(), Vec::new()];
        for (row, cell) in f.atlas.iter().enumerate() {
            let layer = row / 125;
            for star in &cell.stars {
                let xyz = star.body;
                let p = (cell.cell.base() + V(xyz[0] as f64, xyz[1] as f64, xyz[2] as f64))
                    * universe::SHELL[layer];
                if (p - now.p).length() > 1.82 * universe::SHELL[layer] {
                    continue;
                }
                let Some(a) = g.project(p - now.p, now) else {
                    continue;
                };
                let Some(b) = g.project(p - later.p, later) else {
                    continue;
                };
                if !(0. ..g.width).contains(&a[0]) || !(0. ..g.height).contains(&a[1]) {
                    continue;
                }
                let radius = |v: [f64; 2]| (v[0] - vp[0]).hypot(v[1] - vp[1]);
                if (0.50..0.70).contains(&(radius(a) / half_diag)) {
                    samples[layer].push((radius(b) - radius(a)) / 0.013);
                }
            }
        }
        let [near, far] = samples.map(|mut s| {
            s.sort_by(f64::total_cmp);
            s[s.len() / 2]
        });
        println!("speed {speed}: median radial px/s at r=0.5–0.7 near {near:.2}, far {far:.2}");
        assert!((15. * speed..=40. * speed).contains(&near));
        assert!(far < 2.);
    }
}

#[cfg(feature = "visual-capture")]
#[test]
fn capture_viewpoints_and_cluster_composition() {
    for (seed, event, start, times) in [
        (1, 1, 30, &[40][..]),
        (23, 2, 80, &[90, 105, 120][..]),
        (1, 0, 0, &[600][..]),
        (1, 0, 0, &[0][..]),
    ] {
        let still = times == [0];
        let mut profile = input(still);
        profile.seed = super::super::common::rng::output_seed(seed, 3, "Virtual-0");
        profile.mode = MonitorMode::Independent;
        profile.speed = if times == [600] { 3. } else { 1. };
        profile.scale = 1.;
        profile.density = if times == [600] { 80. } else { 50. };
        profile.trails = 0.35;
        if event == 2 {
            profile.parameters.push(super::super::schema::Value {
                id: 2,
                kind: 1,
                payload: 60,
            });
        }
        let mut world = Voyage::new(profile);
        if event != 0 {
            world
                .inject(Event {
                    id: event,
                    at_ns: start * 1_000_000_000,
                    anchor: [0.; 2],
                    strength: 1.,
                })
                .unwrap();
        }
        for &t in times {
            world.step(t * 30);
            let frame = &world.frame;
            let pose = frame.pose(t as f64);
            let v = view(
                frame.art.input.layout.outputs[0].rect,
                (t * 1_000_000_000) as i64,
            );
            let g = Glass::new(&frame.art, &v);
            let mark = if still {
                frame.art.vista
            } else {
                frame.leg.destination
            };
            let mut p = packet();
            optics::export(frame, &v, &[0; 250], &mut p);
            println!(
                "seed {seed} at {t}: VP {:?}, warp start {:.2}, destination {:?}, gas {:?}",
                g.project(pose.direction, pose),
                frame.leg.warp_start(),
                mark.kind,
                p.sprites[1].shape
            );
            if t == 600 {
                let next = frame.leg.next(
                    frame.art.input.seed,
                    &frame.art.params,
                    frame.art.input.speed,
                    &[],
                );
                assert!(
                    (639. ..640.).contains(&next.warp_start()),
                    "G-motion covers the next unforced jump"
                );
                assert_eq!(next.pose(630.).exposure, 0.);
                assert!(next.pose(646.).exposure >= 1.);
            }
            if event == 2 || still {
                let mut knots = Vec::new();
                for centre in space::nursery(mark) {
                    let mut count = 0;
                    let mut haloed = 0;
                    for row in &frame.atlas[..125] {
                        for s in &row.stars {
                            let a = s.body;
                            let here = row.cell.base() + V(a[0].into(), a[1].into(), a[2].into());
                            if (here - centre).length() > 0.33 || (here - pose.p).length() > 1.90 {
                                continue;
                            }
                            if let Some([x, y]) = g.project(here - pose.p, pose)
                                && (0. ..g.width).contains(&x)
                                && (0. ..g.height).contains(&y)
                            {
                                count += 1;
                                haloed += usize::from(a[3] > 0.68);
                            }
                        }
                    }
                    knots.push((count, haloed));
                }
                println!("visible knot neighbourhoods at {t}: {knots:?}");
                assert!(
                    knots
                        .iter()
                        .any(|(count, haloed)| *count >= 40 && *haloed >= 8)
                );
                if still {
                    let gas = p.sprites[1].shape;
                    assert!((0.19..0.46).contains(&(f64::from(gas[0]) / g.width)));
                }
            }
        }
    }
}

#[cfg(feature = "visual-capture")]
#[test]
fn gas_footprints_are_continuous_through_the_recorded_handoff() {
    use std::collections::BTreeMap;
    let mut profile = input(false);
    profile.seed = 1;
    profile.speed = 1.;
    profile.trails = 0.35;
    let mut world = Voyage::new(profile);
    for (id, at) in [(1, 45), (3, 15)] {
        world
            .inject(Event {
                id,
                at_ns: at * 1_000_000_000,
                anchor: [0.; 2],
                strength: 1.,
            })
            .unwrap();
    }
    let rect = world.frame.art.input.layout.outputs[1].rect;
    let mut output = packet();
    let mut previous = BTreeMap::<u32, f64>::new();
    let mut worst = 0f64;
    for n in 0..90 * 60 {
        let t = n as f64 / 60.;
        world.step(n / 2);
        output.sprite_count = 0;
        let g = Glass::new(&world.frame.art, &view(rect, 0));
        sky::compose(
            &world.frame,
            world.frame.pose(t),
            world.frame.pose((t - world.frame.art.shutter).max(0.)),
            &g,
            t,
            &mut output,
        );
        let mut current = BTreeMap::new();
        let mut total = 0.;
        for s in &output.sprites[..output.sprite_count as usize] {
            let area = 4. * f64::from(s.shape[2]) * f64::from(s.shape[3]) / rect.area();
            if s.material[1] <= 1. {
                total += area;
            }
            if s.material[1] == 1. {
                current.insert(s.detail[0].to_bits(), area * f64::from(s.material[2]));
            }
        }
        assert!(total <= 2.5 && output.sprite_count < 16);
        if n > 0 {
            for id in current.keys().chain(previous.keys()) {
                let delta =
                    (current.get(id).unwrap_or(&0.) - previous.get(id).unwrap_or(&0.)).abs();
                worst = worst.max(delta);
                assert!(
                    delta < 0.014,
                    "gas footprint changed by {delta:.4} screen areas at {t:.4}"
                );
            }
        }
        previous = current;
    }
    println!(
        "Largest gas exposure-area change over all 5400 presentations: {worst:.6} DP-5 areas/frame"
    );
}

#[cfg(feature = "visual-capture")]
#[test]
fn crowded_events_reserve_a_slot_and_retire_the_faintest_in_advance() {
    let mut world = Voyage::new(input(false));
    for (id, seconds) in [
        (7, 4),
        (4, 5),
        (5, 6),
        (4, 7),
        (4, 11),
        (7, 13),
        (6, 14),
        (7, 15),
    ] {
        world
            .inject(Event {
                id,
                at_ns: seconds * 1_000_000_000,
                anchor: [0.; 2],
                strength: 1.,
            })
            .unwrap();
    }
    let events = &world.frame.moments;
    assert!(
        events
            .iter()
            .any(|e| e.retire > e.start + 1.5 && e.retire < e.start + e.duration)
    );
    for n in 0..1200 {
        let t = n as f64 / 60.;
        let layers: usize = events
            .iter()
            .filter(|e| e.start <= t && e.retire > t)
            .map(|e| if e.id == 5 { 1 } else { 2 })
            .sum();
        assert!(layers <= 7, "{layers} event layers at {t}");
    }
}

#[cfg(feature = "visual-capture")]
#[test]
fn the_arrival_cluster_expands_before_its_side_flyby() {
    for spanning in [false, true] {
        let mut profile = input(false);
        profile.seed = if spanning {
            1
        } else {
            super::super::common::rng::output_seed(1, 3, "Virtual-0")
        };
        profile.speed = 1.;
        profile.scale = 1.;
        if !spanning {
            profile.mode = MonitorMode::Independent;
        }
        let mut world = Voyage::new(profile);
        let spool = if spanning { 45 } else { 30 };
        world
            .inject(Event {
                id: 1,
                at_ns: spool * 1_000_000_000,
                anchor: [0.; 2],
                strength: 1.,
            })
            .unwrap();
        if spanning {
            world
                .inject(Event {
                    id: 3,
                    at_ns: 15_000_000_000,
                    anchor: [0.; 2],
                    strength: 1.,
                })
                .unwrap();
        }
        let landing = world.frame.leg.arrival_start();
        world.step((landing * 30.).round() as u64);
        let frame = &world.frame;
        let target = frame.leg.destination;
        assert_eq!(target.kind, Kind::Open);
        let g = Glass::new(
            &frame.art,
            &view(
                frame.art.input.layout.outputs[if spanning { 1 } else { 0 }].rect,
                0,
            ),
        );
        let positions: Vec<V> = frame.atlas[..125]
            .iter()
            .flat_map(|row| {
                row.stars.iter().filter_map(move |s| {
                    let p =
                        row.cell.base() + V(s.body[0] as f64, s.body[1] as f64, s.body[2] as f64);
                    (s.light[1] == 2. && (p - target.p).length() < 0.035).then_some(p)
                })
            })
            .collect();
        assert_eq!(positions.len(), 12);
        let spread = |age| {
            let pose = frame.pose(landing + age);
            let centre = g.project(target.p - pose.p, pose).unwrap();
            let sum: f64 = positions
                .iter()
                .map(|p| {
                    let point = g.project(*p - pose.p, pose).unwrap();
                    (point[0] - centre[0]).hypot(point[1] - centre[1])
                })
                .sum();
            sum / positions.len() as f64
        };
        let growth = spread(7.5) / spread(0.);
        let exit = (0..=900)
            .map(|n| n as f64 / 60.)
            .find(|&age| {
                let pose = frame.pose(landing + age);
                g.project(target.p - pose.p, pose).is_none_or(|p| {
                    !(0. ..g.width).contains(&p[0]) || !(0. ..g.height).contains(&p[1])
                })
            })
            .unwrap();
        let pose = frame.pose(landing + exit);
        let position = g.project(target.p - pose.p, pose).unwrap();
        println!(
            "{} selected 12-star cluster: span {:.2}x after 7.5s, exits at {:.2}s ({:.1}, {:.1})",
            if spanning { "Spanning" } else { "Independent" },
            growth,
            exit,
            position[0],
            position[1]
        );
        if spanning {
            let mut last = [0.; 2];
            for n in 0..=900 {
                let pose = frame.pose(landing + n as f64 / 60.);
                let p = g.project(target.p - pose.p, pose).unwrap();
                last = [p[0] + 2560., p[1]];
                if last[0] >= 6000. && !(195. ..1275.).contains(&last[1]) || last[0] >= 7920. {
                    break;
                }
            }
            assert!(
                last[0] >= 7920. && (195. ..1275.).contains(&last[1]),
                "outer windshield exit {last:?}"
            );
        }
        assert!((2. ..=3.).contains(&growth));
        assert!((10. ..=15.).contains(&exit) && (0. ..g.height).contains(&position[1]));
    }
}

#[test]
fn gas_keys_replay_at_a_leg_boundary() {
    let mut travelled = Voyage::new(input(false));
    let first = (travelled.frame.leg.end * 30.).ceil() as u64;
    let mut slow = packet();
    let mut sought = packet();
    for tick in 1..=first + 60 {
        travelled.step(tick);
        if tick + 45 < first || tick % 5 != 0 {
            continue;
        }
        let mut direct = Voyage::new(input(false));
        direct.step(tick);
        let t = tick as f64 / 30. - 0.007;
        let rect = direct.frame.art.input.layout.outputs[1].rect;
        let glass = Glass::new(&direct.frame.art, &view(rect, 0));
        for (f, output) in [(&travelled.frame, &mut slow), (&direct.frame, &mut sought)] {
            output.sprite_count = 0;
            sky::compose(f, f.pose(t), f.pose(t - f.art.shutter), &glass, t, output);
        }
        assert_eq!(slow.sprite_count, sought.sprite_count, "at {t}s");
        for (a, b) in slow.sprites[..slow.sprite_count as usize]
            .iter()
            .zip(&sought.sprites)
        {
            assert_eq!(a.shape, b.shape, "at {t}s");
            assert_eq!(a.material, b.material);
            assert_eq!(a.detail, b.detail);
        }
    }
}

#[cfg(feature = "visual-capture")]
#[test]
fn arrival_exposure_joins_consecutive_presentations() {
    for speed in [1., 3.] {
        let mut profile = input(false);
        profile.seed = 1;
        profile.speed = speed;
        profile.trails = 0.35;
        let mut world = Voyage::new(profile);
        for (id, seconds) in [(1, 45), (3, 15)] {
            world
                .inject(Event {
                    id,
                    at_ns: seconds * 1_000_000_000,
                    anchor: [0.; 2],
                    strength: 1.,
                })
                .unwrap();
        }
        let rect = world.frame.art.input.layout.outputs[1].rect;
        let glass = Glass::new(&world.frame.art, &view(rect, 0));
        let mut out = packet();
        for frame in 76 * 30..=79 * 30 {
            let t = f64::from(frame) / 30.;
            world.step(frame as u64);
            let now = world.frame.pose(t);
            let prior = world.frame.pose(t - 1. / 30.);
            out.sprite_count = 0;
            sky::compose(&world.frame, now, prior, &glass, t, &mut out);
            let subject = out.sprites[..out.sprite_count as usize]
                .iter()
                .find(|s| s.material[1] == 4. && s.detail[1] > 0.5)
                .unwrap();
            let tail = glass
                .project(world.frame.leg.destination.p - prior.p, prior)
                .unwrap();
            let angle = f64::from(subject.material[0]);
            let length = f64::from(subject.material[3]);
            let recovered = [
                f64::from(subject.shape[0]) - length * angle.cos(),
                f64::from(subject.shape[1]) - length * angle.sin(),
            ];
            assert!(
                (recovered[0] - tail[0]).hypot(recovered[1] - tail[1]) < 0.003,
                "speed {speed} t={t}: shutter ends at {recovered:?}, previous head {tail:?}"
            );
        }
    }
}

#[test]
fn slow_spanning_cruise_stays_finite_across_a_day_of_legs() {
    let mut profile = input(false);
    profile.seed = 0;
    profile.speed = 0.1;
    profile.parameters = vec![
        super::super::schema::Value {
            id: 1,
            kind: 3,
            payload: 2,
        },
        super::super::schema::Value {
            id: 2,
            kind: 1,
            payload: 100,
        },
    ];
    let mut direct = Voyage::new(profile.clone());
    let mut world = Voyage::new(profile);
    // The original failure preplanned leg 174 at tick 2_482_101. Advance at
    // the actual planning rate, retaining every preceding leg's exit slope.
    let through = 2_600_000;
    for tick in 1..=through {
        world.step(tick);
        let pose = world.frame.pose(tick as f64 / 30.);
        for v in [pose.p, pose.direction, pose.attitude.0] {
            assert!(
                v.0.is_finite() && v.1.is_finite() && v.2.is_finite(),
                "non-finite pose at tick {tick}: {pose:?}"
            );
        }
        assert!(pose.attitude.1.is_finite());
        assert!(
            (pose.direction.length() - 1.).abs() < 1e-12,
            "invalid direction at tick {tick}: {pose:?}"
        );
    }
    assert!(world.frame.leg.serial >= 174);
    direct.step(through);
    assert_eq!(world.state_hash(), direct.state_hash());
}
