//! Height and individual coverage acceptance for the large-ball profile.
use super::{
    tests::{L3, input},
    *,
};

#[derive(Default)]
struct Flight {
    serial: u64,
    origin: f64,
    apex: f64,
    active: bool,
}

fn profile_input(profile: &str, seed: u64, count: i32) -> WorldInput {
    let mut config = input(if profile == "G" { &L3[..1] } else { L3 }, count, 35);
    config.seed = seed;
    config.trails = 0.;
    config.density = 50.;
    let mut outputs = config.layout.outputs.clone();
    for (i, output) in outputs.iter_mut().enumerate() {
        output.id = common::rng::identity_hash(&format!("Virtual-{i}"));
    }
    config.layout = common::layout::Layout::new(outputs, config.layout.generation).unwrap();
    if profile == "G" {
        // The capture's HDMI-A-5 role is bound to KWin's Virtual-0 connector.
        // Match VisualWorldCoordinator::independentSeed, including its final
        // generation-zero hash; a raw base seed describes a different world.
        config.mode = world::MonitorMode::Independent;
        config.seed = common::rng::derive_seed(seed, 7, config.layout.outputs[0].id, 0);
        config.speed = 3.;
        config.scale = 2.;
        config.density = 80.;
        config.palette = 2;
    }
    if profile == "SPAN" {
        config.speed = 3.;
        config.scale = 2.;
        config.density = 80.;
        config.palette = 2;
    }
    config
}

fn measure(profile: &str, seed: u64, count: i32, seconds: u64) {
    let speed = if profile == "G" { 3. } else { 1. };
    measure_at_speed(profile, seed, count, seconds, speed);
}

fn measure_at_speed(profile: &str, seed: u64, count: i32, seconds: u64, speed: f64) {
    let mut config = profile_input(profile, seed, count);
    config.speed = speed;
    measure_configuration(profile, seed, count, seconds, speed, config);
}

fn measure_configuration(
    profile: &str,
    seed: u64,
    count: i32,
    seconds: u64,
    speed: f64,
    config: WorldInput,
) {
    let world_seed = config.seed;
    let mut w = BounceWorld::new(config);
    if profile == "G"
        && let Ok(pace) = std::env::var("BOUNCE_HEIGHT_PACE")
    {
        w.set_capture_pace(pace.parse().unwrap());
    }
    if profile == "SPAN" {
        let mut radii: Vec<f64> = w.balls.iter().map(|b| b.r).collect();
        radii.sort_by(f64::total_cmp);
        let area: f64 = radii.iter().map(|r| std::f64::consts::PI * r * r).sum();
        let rms = (area / (std::f64::consts::PI * radii.len() as f64)).sqrt();
        let mut independent_input = profile_input("G", seed, count);
        independent_input.scale = w.input.scale;
        let independent = BounceWorld::new(independent_input);
        let mut ir: Vec<f64> = independent.balls.iter().map(|b| b.r).collect();
        ir.sort_by(f64::total_cmp);
        let independent_rms = (ir.iter().map(|r| r * r).sum::<f64>() / ir.len() as f64).sqrt();
        let short = w.rooms[0]
            .outputs
            .iter()
            .map(|o| o.width.min(o.height))
            .fold(0f64, f64::max);
        let nominal = (0.075 * short * w.input.scale)
            .clamp(24., 220.)
            .min(short * 0.28);
        let fit = w.medians[0] / nominal;
        let mut output_areas: Vec<_> = w.rooms[0].outputs.iter().map(|o| o.area()).collect();
        output_areas.sort_by(f64::total_cmp);
        println!(
            "SIZE seed={seed} pace={} short={short} nominal={nominal} fit={fit} raw_area={} room_area={} reference_output_area={} area={area} rms={rms} independent_rms={independent_rms} median={} independent_median={} max={} independent_max={}",
            w.pace,
            area / (fit * fit),
            w.rooms[0].area,
            output_areas[output_areas.len() / 2],
            radii[radii.len() / 2],
            ir[ir.len() / 2],
            radii.last().unwrap(),
            ir.last().unwrap()
        );
        if w.input.scale > 1.5 {
            assert!(
                (rms - independent_rms).abs() < 1e-9,
                "Spanning must match Independent G's fitted RMS radius"
            );
        }
    }
    let mut flights: Vec<Flight> = (0..61).map(|_| Flight::default()).collect();
    let mut rises = Vec::new();
    let mut last_high = [0u64; 61];
    let mut high_gap = [0u64; 61];
    let mut last_seen = [0u64; 61];
    let mut min_high = 61usize;
    let mut rest = [0u64; 61];
    let mut longest_rest = 0u64;
    let mut recent = [(0usize, 0usize); 1200];
    let mut moving_window = 0usize;
    let mut samples = 0usize;
    let mut worst = 1f64;
    let mut apex_sum = [0f64; 2];
    let mut apex_count = [0usize; 2];
    let mut prior_vy = [0f64; 61];
    let mut peak_energy = 0f64;
    let mut peak_speed = 0f64;
    let mut capped = 0usize;
    let mut visits = 0usize;
    let mut gap_context = (0u64, 0usize, 0f64, None);
    for tick in 1..=120 * seconds {
        w.step(tick);
        let slot = (tick as usize - 1) % 1200;
        moving_window -= recent[slot].0;
        samples -= recent[slot].1;
        recent[slot] = (0, 0);
        for (i, b) in w.balls.iter().enumerate() {
            if tick == 72000 && std::env::var_os("BOUNCE_DUMP_POSE").is_some() {
                println!(
                    "POSE profile={profile} seed={seed} id={i} p={:?} r={} mat={:?} heavy={}",
                    b.p, b.r, b.mat, b.heavy
                );
            }
            let moving = !b.sleeping && (!b.supported || b.v.len2() >= 100.);
            moving_window += usize::from(moving);
            samples += 1;
            recent[slot].0 += usize::from(moving);
            recent[slot].1 += 1;
            if moving {
                rest[i] = 0
            } else {
                rest[i] += 1;
                longest_rest = longest_rest.max(rest[i]);
            }
            if b.p.y + b.r < w.height * 0.5 {
                if tick - last_high[i] > 1 {
                    visits += 1;
                }
                last_high[i] = tick;
                last_seen[i] = tick;
            }
            if i < count as usize {
                let gap = tick - last_high[i];
                high_gap[i] = high_gap[i].max(gap);
                if gap > gap_context.0 {
                    gap_context = (
                        gap,
                        i,
                        tick as f64 / 120.,
                        w.moment.map(|m| (m.kind as u8, m.start, m.duration)),
                    );
                }
            }
            let f = &mut flights[i];
            if f.serial != b.hop_serial {
                if f.active {
                    rises.push((f.origin - f.apex).max(0.));
                }
                f.serial = b.hop_serial;
                f.origin = b.hop_origin.y;
                f.apex = b.p.y.min(f.origin);
                f.active = true;
            }
            if f.active {
                f.apex = f.apex.min(b.p.y);
                if b.v.y >= 0. || w.gravity(tick as f64 / 120.).y <= 0. {
                    if w.gravity(tick as f64 / 120.).y > 0. {
                        rises.push((f.origin - f.apex).max(0.));
                    }
                    f.active = false;
                }
            }
            if prior_vy[i] < 0.
                && b.v.y >= 0.
                && !b.supported
                && w.gravity(tick as f64 / 120.).y > 0.
            {
                let bin = if tick <= 36000 {
                    Some(0)
                } else if tick > 120 * (seconds.saturating_sub(300)) {
                    Some(1)
                } else {
                    None
                };
                if let Some(bin) = bin {
                    apex_sum[bin] += w.height - b.r - b.p.y;
                    apex_count[bin] += 1;
                }
            }
            prior_vy[i] = b.v.y;
            peak_speed = peak_speed.max(b.v.len() / w.height);
            capped += usize::from(b.v.len() >= b.flow_limit * 0.99);
            assert!(
                w.input.layout.circle_fits(b.p.into(), (b.r - 0.2).max(0.)),
                "escape seed={seed} count={count} tick={tick}"
            );
        }
        peak_energy = peak_energy
            .max(w.balls.iter().map(|b| b.v.len2() / b.inv_mass).sum::<f64>() / w.kinetic_limit);
        if tick >= 1200 {
            worst = worst.min(moving_window as f64 / samples as f64);
            min_high = min_high.min(
                last_seen[..count as usize]
                    .iter()
                    .filter(|&&last| last > 0 && tick - last < 1200)
                    .count(),
            );
        }
    }
    println!("GAP_CONTEXT {profile} {seed} {count} {gap_context:?}");
    rises.sort_by(f64::total_cmp);
    let median = if rises.is_empty() {
        0.
    } else {
        rises[rises.len() / 2]
    };
    let mean = std::array::from_fn::<_, 2, _>(|i| apex_sum[i] / apex_count[i].max(1) as f64);
    let max_gap = high_gap[..count as usize].iter().max().copied().unwrap() as f64 / 120.;
    println!(
        "HEIGHT profile={profile} seed={seed} world_seed={world_seed} requested_speed={speed} count={count} seconds={seconds} pace={} scale={} median_hop_rise={median:0.6} rise_fraction={:0.6} min_high_10s={min_high} max_individual_gap_s={max_gap:0.6} moving={worst:0.6} rest_s={:0.6} apex_first={} apex_last={} ratio={} peak_energy={peak_energy} peak_speed={peak_speed} hop_samples={} high_visits={visits} capped_samples={capped} radii={:?}",
        w.dt * f64::from(w.substeps) * 120.,
        w.input.scale,
        median / w.height,
        longest_rest as f64 / 120.,
        mean[0],
        mean[1],
        mean[1] / mean[0],
        rises.len(),
        w.balls
            .iter()
            .take(count as usize)
            .map(|b| b.r)
            .collect::<Vec<_>>()
    );
    println!(
        "AIR profile={profile} seed={seed} count={count} pace={} free_updates={} force_errors={} max_error={} max_still={} max_film_vertical_still={}",
        w.pace,
        w.flight_audit.free_updates,
        w.flight_audit.force_errors,
        w.flight_audit.max_error,
        w.flight_audit.max_still,
        w.flight_audit.max_film_vertical_still
    );
    println!(
        "HOVER_CONTEXT id={} end={} gravity_y={}",
        w.flight_audit.film_id, w.flight_audit.film_end, w.flight_audit.film_gravity
    );
    if std::env::var_os("BOUNCE_MEASURE_ONLY").is_none() {
        assert_eq!(
            w.flight_audit.force_errors, 0,
            "unexplained free-flight acceleration"
        );
        if profile == "G" && count == 12 && speed == 3. && (w.pace - 1.).abs() < 1e-9 {
            assert!(w.flight_audit.max_still <= 0.4, "stationary free flight");
            assert!(w.flight_audit.max_film_vertical_still <= 0.4, "film hover");
        }
        assert!(worst >= 0.8 && longest_rest <= 480);
        if seconds >= 600 {
            assert!(mean[1] >= 0.9 * mean[0]);
        }
        assert!(peak_energy < 12. && peak_speed <= 2.201);
        if matches!(profile, "G" | "SPAN") && w.input.scale > 1.5 {
            assert!(min_high >= 3);
            if count == 12 {
                // The release median describes the mature 30-minute scene.
                // The startup smoke test still checks visitors and movement;
                // its first few short contact hops are not that statistic.
                if seconds >= 600 {
                    assert!(median >= 0.20 * w.height);
                }
                assert!(max_gap <= 60.);
            }
        }
    }
}

#[test]
fn large_profile_height_short() {
    let seconds = std::env::var("BOUNCE_HEIGHT_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let count = std::env::var("BOUNCE_HEIGHT_COUNT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(12);
    let seed = std::env::var("BOUNCE_HEIGHT_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(7);
    if let Ok(speed) = std::env::var("BOUNCE_ART_SPEED") {
        measure_at_speed("G", seed, count, seconds, speed.parse().unwrap());
    } else {
        measure("G", seed, count, seconds);
    }
}
#[test]
#[ignore = "30-minute height, coverage and liveliness matrix; run through heavy"]
fn height_acceptance_matrix() {
    // Finish every case before failing, so a rejected candidate still leaves
    // the other seeds, dense rooms and unchanged default as useful evidence.
    let mut passed = true;
    for seed in [7, 19, 31] {
        passed &= std::panic::catch_unwind(|| measure("G", seed, 12, 1800)).is_ok();
    }
    for count in [30, 60] {
        passed &= std::panic::catch_unwind(|| measure("G", 7, count, 1800)).is_ok();
    }
    passed &= std::panic::catch_unwind(|| measure("default", 7, 12, 1800)).is_ok();
    assert!(
        passed,
        "height acceptance failed; see all six measurement rows"
    );
}

#[test]
#[ignore = "30-minute wall-time acceptance at Speed 100 and 200; run through heavy"]
fn large_room_speed_acceptance_matrix() {
    let mut passed = true;
    for speed in [1., 2.] {
        for seed in [7, 19, 31] {
            passed &= std::panic::catch_unwind(|| {
                measure_at_speed("G", seed, 12, 1800, speed);
            })
            .is_ok();
        }
    }
    assert!(
        passed,
        "slow-speed height acceptance failed; see all six rows"
    );
}

#[test]
fn capture_profile_uses_the_actual_independent_seed_and_ids() {
    let g = profile_input("G", 7, 12);
    assert_eq!(g.mode, world::MonitorMode::Independent);
    assert_eq!(g.seed, 15_142_783_343_244_961_894);
    assert_eq!(g.layout.outputs[0].id, 0xdc11_9f94_1515_3109);
    let spanning = profile_input("default", 7, 12);
    assert_eq!(spanning.mode, world::MonitorMode::Spanning);
    assert_eq!(spanning.seed, 7);
    assert_eq!(spanning.layout.outputs[0].id, 0xdc11_9d94_1515_2da3);
    assert_eq!(spanning.layout.outputs[0].rect.x, 6000.);
}

#[test]
fn large_other_gravity_modes_skip_downward_lofts() {
    for gravity in [-35, 0] {
        let mut config = input(&L3[..1], 1, gravity);
        config.scale = 2.;
        config.speed = 3.;
        config.trails = 0.;
        let mut w = BounceWorld::new(config);
        w.params.worldEvents = false;
        if gravity == 0 {
            assert!(!w.balls[0].high_arc);
            assert_eq!(w.pace, 3.);
            continue;
        }
        let prior_mass = 1. / w.balls[0].inv_mass;
        let mut b = Ball::new(&mut w.rng, 0, 400., Material::Rubber, 0);
        b.high_arc = true;
        b.p = v(1280., 950.);
        b.v = V::default();
        b.spin = 0.;
        b.period = f64::INFINITY;
        b.birth = -2.;
        w.kinetic_limit *= (1. / b.inv_mass) / prior_mass;
        w.balls[0] = b;
        let mut bounced_down = false;
        for tick in 1..=120 * 30 {
            w.step(tick);
            bounced_down |= w.balls[0].v.y > 100.;
        }
        // Its 800px diameter cannot put the bottom above room mid-height.
        // Upward gravity must still let it bounce away from the ceiling.
        assert!(bounced_down);
        assert_eq!(w.balls[0].below_mid_time, 0.);
        assert!(!w.balls[0].loft_due);
    }
}

#[test]
fn large_room_speed_slider_is_strictly_monotonic() {
    for count in [12, 30, 60] {
        let mut prior = 0.;
        let mut normal = 0.;
        let mut middle = 0.;
        for step in 1..=30 {
            let mut config = input(&L3[..1], count, 35);
            config.scale = 2.;
            config.speed = f64::from(step) / 10.;
            let w = BounceWorld::new(config);
            assert!(
                w.pace > prior,
                "count={count} speed={} pace={} previous={prior}",
                w.input.speed,
                w.pace
            );
            prior = w.pace;
            if step == 10 {
                normal = w.pace;
            } else if step == 20 {
                middle = w.pace;
            }
        }
        println!(
            "SPEED count={count} pace100={normal} pace200={middle} pace300={prior} ratio={}",
            prior / normal
        );
        assert!((normal - 0.6).abs() < 1e-12);
        assert!((middle - 0.8).abs() < 1e-12);
        assert!((prior - 1.).abs() < 1e-12);
        assert!(
            prior >= 1.6 * normal,
            "Speed 100 to 300 must remain clearly visible"
        );
    }
}

#[test]
#[cfg(feature = "visual-capture")]
fn capture_pace_event_updates_the_timebase() {
    let mut config = input(&L3[..1], 12, 35);
    config.scale = 2.;
    config.speed = 3.;
    let mut w = BounceWorld::new(config);
    w.inject(Event {
        id: 9,
        at_ns: 0,
        anchor: [0., 0.],
        strength: 1.25,
    })
    .unwrap();
    w.step(1);
    assert_eq!(w.pace, 1.25);
    assert_eq!(w.substeps, 2);
    assert_eq!(w.dt * 240., 1.25);
}

#[test]
fn large_room_pace_changes_time_without_cutting_height() {
    fn apex(speed: f64) -> (f64, u64) {
        let mut config = input(&L3[..1], 12, 35);
        config.scale = 2.;
        config.speed = speed;
        config.trails = 0.;
        let mut w = BounceWorld::new(config);
        w.params.collisions = false;
        w.params.worldEvents = false;
        w.balls.truncate(1);
        let b = &mut w.balls[0];
        b.p = v(1280., 1100.);
        b.v = v(0., -900.);
        b.spin = 0.;
        b.period = f64::INFINITY;
        // Isolate this passive flight from the wall-time loft recovery.
        b.high_arc = false;
        let mut top = b.p.y;
        // Observe the same physical interval at every effective pace.
        let ticks = (3. * 120. / w.pace).ceil() as u64;
        for tick in 1..=ticks {
            w.step(tick);
            top = top.min(w.balls[0].p.y);
            if w.balls[0].v.y >= 0. {
                return (1100. - top, tick);
            }
        }
        panic!("no apex");
    }
    let (fast_height, fast_tick) = apex(3.);
    let (normal_height, normal_tick) = apex(1.);
    let (slow_height, slow_tick) = apex(0.25);
    // At a maximum physical step of 1/120s, symplectic Euler's
    // displacement error at this apex is at most 900/240 = 3.75px.
    assert!((fast_height - slow_height).abs() < 4.);
    assert!((fast_height - normal_height).abs() < 4.);
    assert!(fast_height > 400. && slow_tick > 2 * fast_tick);
    let ratio = normal_tick as f64 / fast_tick as f64;
    println!(
        "PACE_ARC rise100={normal_height} rise300={fast_height} ticks100={normal_tick} ticks300={fast_tick} time_ratio={ratio}"
    );
    assert!((ratio - 5. / 3.).abs() < 0.10);
    let mut config = input(&L3[..1], 12, 35);
    config.speed = 3.;
    // This seed's smaller fitted bodies allow a little more pace than G.
    // The old Scale-number exemption would incorrectly leave it at three.
    let fitted = BounceWorld::new(config.clone()).pace;
    assert!(fitted > 1. && fitted < 3.);
    config.scale = 2.;
    config
        .parameters
        .iter_mut()
        .find(|p| p.id == 2)
        .unwrap()
        .payload = 0;
    assert_eq!(BounceWorld::new(config).pace, 3.);
}

#[test]
fn upper_half_contacts_store_charge_for_the_next_tall_launch() {
    let mut config = input(&L3[..1], 2, 35);
    config.scale = 2.;
    let mut w = BounceWorld::new(config);
    w.balls.clear();
    for y in [600., 900.] {
        let mut b = Ball::new(&mut w.rng, 0, 150., Material::Rubber, 0);
        b.high_arc = true;
        b.birth = -2.;
        b.p = v(1280., y);
        b.v = V::default();
        b.charge = b.period + 0.18;
        w.balls.push(b);
    }
    let decay = Relaxation::new(1. / 120.);
    w.integrate_substep(v(0., 900.), 0., 1. / 120., decay);
    assert!(w.balls[0].supported);
    assert_eq!(w.balls[0].hop_serial, 0);
    assert!(w.balls[0].charge >= w.balls[0].period + 0.18);
    for b in &mut w.balls {
        b.p.y += 200.;
    }
    w.integrate_substep(v(0., 900.), 0., 1. / 120., decay);
    assert_eq!(w.balls[0].hop_serial, 1);
    assert_eq!(w.balls[0].charge, 0.);
    assert!(w.balls[0].v.y < 0.);
}

#[test]
fn pile_nudges_preserve_large_ball_charge_until_launch() {
    let mut rng = Pcg32::new(7, 1);
    let mut b = Ball::new(&mut rng, 0, 150., Material::Metal, 0);
    b.high_arc = true;
    b.sleeping = true;
    b.charge = b.period + 0.18;
    let charged = b.charge;
    b.wake();
    assert_eq!(b.charge, charged);
    b.hop(v(0., 900.), 1000., &mut rng);
    assert_eq!(b.charge, 0.);
    assert!(!b.sleeping && b.v.y < 0.);
}

#[test]
fn crowd_limit_preserves_vertical_launch_energy() {
    let mut rng = Pcg32::new(7, 1);
    let mut b = Ball::new(&mut rng, 0, 150., Material::Rubber, 0);
    b.high_arc = true;
    b.flow_limit = 200.;
    b.floating = false;
    b.v = v(500., -1200.);
    b.limit_motion(3168., v(0., 1.));
    assert_eq!(b.v, v(200., -1200.));
    b.v = v(500., -1200.);
    b.limit_motion(3168., v(0.966, 0.259));
    assert_eq!(b.v, v(200., -1200.));
    b.floating = true;
    b.limit_motion(3168., V::default());
    assert!((b.v.len() - 200.).abs() < 1e-9);
    b.floating = false;
    b.high_arc = false;
    b.v = v(500., -1200.);
    b.limit_motion(3168., v(0., 1.));
    assert!((b.v.len() - 200.).abs() < 1e-9);
}

#[test]
fn large_permanent_zero_gravity_keeps_horizontal_drift() {
    let mut config = input(&L3[..1], 1, 0);
    config.speed = 3.;
    config.scale = 2.;
    config.trails = 0.;
    let mut w = BounceWorld::new(config);
    w.balls[0].p = v(500., 1000.);
    w.balls[0].v = v(20., 0.);
    w.balls[0].spin = 0.;
    for _ in 0..120 * 30 {
        for _ in 0..w.substeps {
            w.integrate(V::default());
        }
    }
    let b = &w.balls[0];
    assert!((b.p.y - 1000.).abs() < 1e-6);
    assert_eq!(b.v.y, 0.);
    assert!(!b.loft_due && b.below_mid_time == 0.);
}

#[test]
fn horizontal_interlude_drift_recovers_height_smoothly() {
    let mut config = input(&L3[..1], 1, 35);
    config.seed = 7;
    config.speed = 3.;
    config.scale = 2.;
    config.trails = 0.;
    let mut w = BounceWorld::new(config);
    w.params.collisions = false;
    w.balls[0].p = v(1280., 1000.);
    w.balls[0].v = v(120., 0.);
    w.balls[0].spin = 0.;
    w.balls[0].flow_limit = 200.;
    let mut reached = None;
    for tick in 1..=120 * 30 {
        let previous = w.balls[0].p;
        for _ in 0..w.substeps {
            w.integrate(V::default());
        }
        let b = &w.balls[0];
        assert!((b.p - previous).len() <= 2.2 * w.height * w.input.speed / 120. + 1e-3);
        if b.p.y + b.r < w.height * 0.5 {
            reached.get_or_insert(tick);
        }
    }
    assert!(reached.is_some_and(|tick| tick <= 120 * 26), "{reached:?}");
}

#[test]
fn oversized_down_ball_has_no_unreachable_midheight_loft() {
    let mut config = input(&L3[..1], 1, 35);
    config.scale = 2.;
    config.speed = 1.;
    let mut w = BounceWorld::new(config);
    w.params.worldEvents = false;
    let prior_mass = 1. / w.balls[0].inv_mass;
    let b = &mut w.balls[0];
    b.r = 400.;
    b.set_material(Material::Rubber);
    b.p = v(1280., 900.);
    b.v = V::default();
    b.spin = 0.;
    b.period = f64::INFINITY;
    w.kinetic_limit *= (1. / b.inv_mass) / prior_mass;
    let mut ceiling_streak = 0;
    let mut max_ceiling = 0;
    for tick in 1..=120 * 40 {
        w.step(tick);
        let b = &w.balls[0];
        ceiling_streak = if b.p.y - b.r < 1. {
            ceiling_streak + 1
        } else {
            0
        };
        max_ceiling = max_ceiling.max(ceiling_streak);
    }
    let b = &w.balls[0];
    println!(
        "OVERSIZE y={} vy={} below_time={} loft_due={} max_ceiling_s={}",
        b.p.y,
        b.v.y,
        b.below_mid_time,
        b.loft_due,
        max_ceiling as f64 / 120.
    );
    assert!(
        max_ceiling < 120,
        "an unreachable goal must not pin the ball at the ceiling"
    );
    assert_eq!(b.below_mid_time, 0.);
    assert!(!b.loft_due);
}

#[test]
fn sleeping_loft_does_not_store_unintegrated_force() {
    let mut config = input(&L3[..1], 1, 35);
    config.scale = 2.;
    let mut w = BounceWorld::new(config);
    w.params.worldEvents = false;
    let b = &mut w.balls[0];
    b.r = 100.;
    b.p = v(1280., 1340.);
    b.v = V::default();
    b.spin = 0.;
    b.period = f64::INFINITY;
    b.sleeping = true;
    b.below_mid_time = 30.;
    b.loft_due = true;
    w.step(1);
    println!(
        "SLEEP_LOFT sleeping={} velocity={:?}",
        w.balls[0].sleeping, w.balls[0].v
    );
    assert!(w.balls[0].sleeping);
    assert_eq!(w.balls[0].v, V::default());
}

#[test]
fn upward_gravity_spends_charge_at_ceiling_contacts() {
    let mut config = input(&L3[..1], 1, -35);
    config.scale = 2.;
    let mut w = BounceWorld::new(config);
    w.params.worldEvents = false;
    let b = &mut w.balls[0];
    b.p = v(1280., b.r + 0.01);
    b.v = V::default();
    b.spin = 0.;
    b.period = 0.55;
    b.charge = b.period + 0.18;
    w.step(1);
    println!(
        "UP_CONTACT vy={} charge={} hops={}",
        w.balls[0].v.y, w.balls[0].charge, w.balls[0].hop_serial
    );
    assert!(w.balls[0].v.y > 100.);
    assert_eq!(w.balls[0].charge, 0.);
    assert_eq!(w.balls[0].hop_serial, 1);
}

#[test]
fn crowded_profile_uses_the_measured_pace_for_each_seed() {
    for seed in [7, 19, 31] {
        for (speed, expected) in [(1., 0.6), (2., 0.8), (3., CROWDED_PACE_AT_300)] {
            let mut config = profile_input("G", seed, 12);
            config.speed = speed;
            assert_eq!(BounceWorld::new(config).pace, expected);
        }
    }
}

#[test]
fn unsupported_due_loft_follows_gravity() {
    let mut w = BounceWorld::new(profile_input("G", 7, 12));
    w.params.collisions = false;
    w.params.worldEvents = false;
    w.balls.truncate(1);
    w.kinetic_limit = f64::INFINITY;
    let b = &mut w.balls[0];
    b.p = v(1280., 900.);
    b.r = 50.;
    b.v = v(0., -200.);
    b.spin = 0.;
    b.period = 10000.;
    b.charge = 0.;
    b.below_mid_time = 100.;
    for tick in 1..=60 {
        let expected = w.balls[0].v + w.gravity(tick as f64 / 120.) * w.dt;
        w.step(tick);
        assert!(!w.balls[0].supported && !w.balls[0].near_ball);
        assert!(
            (w.balls[0].v - expected).len() < 1e-9,
            "free-air force tick={tick}: actual={:?} expected={expected:?}",
            w.balls[0].v
        );
    }
}

#[test]
fn art_profile_metadata() {
    for seed in [7, 19, 31] {
        for (label, scale) in [
            ("span", 2.),
            ("span-scale100", 1.),
            ("span-scale150", 1.5),
            ("scale100", 1.),
            ("scale150", 1.5),
            ("G", 2.),
        ] {
            let mut config = profile_input(
                if label.starts_with("span") {
                    "default"
                } else {
                    "G"
                },
                seed,
                12,
            );
            config.speed = 3.;
            config.scale = scale;
            config.palette = 2;
            config.density = 80.;
            let w = BounceWorld::new(config);
            let area: f64 = w
                .balls
                .iter()
                .map(|b| std::f64::consts::PI * b.r * b.r)
                .sum();
            let room: f64 = w.rooms.iter().map(|r| r.area).sum();
            let mut radii: Vec<_> = w.balls.iter().map(|b| b.r).collect();
            radii.sort_by(f64::total_cmp);
            println!(
                "PROFILE label={label} seed={seed} scale={scale} world_seed={} pace={} occupancy={} median_r={} max_r={} area={area} room={room}",
                w.input.seed,
                w.pace,
                area / room,
                radii[radii.len() / 2],
                radii.last().unwrap()
            );
        }
    }
}

#[test]
fn art_contact_heavy_drop_leaves_detached_neighbor_untouched() {
    let mut w = BounceWorld::new(profile_input("G", 7, 12));
    w.balls.truncate(3);
    for (i, (x, y, r)) in [
        (1280., 1000., 100.),
        (1500., 1000., 50.),
        (1280., 700., 200.),
    ]
    .into_iter()
    .enumerate()
    {
        let b = &mut w.balls[i];
        b.p = v(x, y);
        b.r = r;
        b.v = V::default();
        b.spin = 0.;
        b.alpha = 1.;
        b.sleeping = false;
        b.charge = 0.;
        b.set_material(Material::Rubber);
    }
    w.balls[2].heavy = true;
    w.balls[2].landed = false;
    w.balls[2].v.y = 1000.;
    let gravity = v(0., w.base_g);
    w.solver.solve(
        &mut w.balls,
        &w.rooms,
        gravity.unit(),
        gravity,
        1. / 120.,
        w.height,
        0.9,
        true,
        true,
        &mut w.rng,
    );
    assert!(
        w.balls[1].v.len() < 1e-9,
        "detached neighbor got an impulse: {:?}",
        w.balls[1].v
    );
    assert!(w.balls[0].v.len() > 0.);
}

#[test]
fn art_contact_interrupted_loft_retains_charge_for_retry() {
    let mut w = BounceWorld::new(profile_input("G", 7, 12));
    w.params.worldEvents = false;
    w.balls.truncate(2);
    for (i, y) in [900., 800.].into_iter().enumerate() {
        let b = &mut w.balls[i];
        b.p = v(1280., y);
        b.r = 50.;
        b.v = V::default();
        b.spin = 0.;
        b.alpha = 1.;
        b.charge = 0.;
        b.set_material(Material::Rubber);
    }
    w.balls[0].below_mid_time = 100.;
    w.balls[0].charge = w.balls[0].period + 0.18;
    w.step(1);
    assert_eq!(w.balls[0].hop_serial, 1);
    assert!(w.balls[0].v.y < -500.);
    assert_eq!(w.balls[0].charge, w.balls[0].period + 0.18);
    w.step(2);
    assert_eq!(
        w.balls[0].hop_serial, 1,
        "an ongoing contact is not a new launch"
    );
    for tick in 3..=18 {
        w.balls[0].p = v(1280., 900.);
        w.balls[0].v = V::default();
        w.balls[1].p = v(1280., 800.);
        w.balls[1].v = V::default();
        w.step(tick);
    }
    assert_eq!(w.balls[0].hop_serial, 2, "retry after the contact responds");
    assert_eq!(w.balls[0].charge, w.balls[0].period + 0.18);
    w.balls.truncate(1);
    w.balls[0].p = v(1280., 600.);
    w.balls[0].v = V::default();
    w.step(19);
    assert!(!w.balls[0].loft_due);
    assert!(w.balls[0].charge < 0.02);
}

#[test]
fn art_actual_fitted_bodies_control_pace() {
    for seed in [7, 19, 31] {
        for scale in [1., 1.5, 2.] {
            let mut config = profile_input("G", seed, 12);
            config.scale = scale;
            let w = BounceWorld::new(config);
            assert_eq!(w.pace, 1., "seed={seed} scale={scale}");
        }
        let mut spanning = profile_input("default", seed, 12);
        spanning.scale = 2.;
        spanning.speed = 3.;
        assert_eq!(
            BounceWorld::new(spanning).pace,
            OUTPUT_SWEEP_PACE_AT_300,
            "spanning seed={seed}"
        );
    }
    let default = BounceWorld::new(profile_input("default", 7, 12));
    assert_eq!(default.pace, 1., "approved default Spanning clock");
    let mut config = profile_input("G", 19, 12);
    config.scale = 1.5;
    let smaller_setting = BounceWorld::new(config.clone());
    config.scale = 2.;
    let larger_setting = BounceWorld::new(config);
    assert!(
        smaller_setting
            .balls
            .iter()
            .zip(&larger_setting.balls)
            .all(|(a, b)| (a.r - b.r).abs() < 1e-9)
    );
    assert_eq!(smaller_setting.pace, larger_setting.pace);
    // Large bodies on an otherwise sparse L3 room still cross an output's
    // visual field. World-average occupancy alone must not release the cap.
    let mut oversized = 0;
    for seed in 1..=64 {
        let mut config = profile_input("default", seed, 1);
        config.scale = 2.;
        config.speed = 3.;
        let w = BounceWorld::new(config);
        if w.balls[0].r > 0.23 * 1080. {
            assert!(w.pace < 1. && w.pace >= 0.6);
            oversized += 1;
        }
    }
    assert!(oversized > 0);
    let mut zero = profile_input("G", 7, 12);
    zero.parameters
        .iter_mut()
        .find(|p| p.id == 2)
        .unwrap()
        .payload = 0;
    assert_eq!(BounceWorld::new(zero).pace, 3.);
}

#[test]
#[ignore = "Three-seed Spanning height acceptance, 30 minutes each; run through heavy"]
fn spanning_height_acceptance_matrix() {
    let mut passed = true;
    for seed in [7, 19, 31] {
        passed &= std::panic::catch_unwind(|| measure_at_speed("SPAN", seed, 12, 1800, 3.)).is_ok();
    }
    assert!(
        passed,
        "Spanning height acceptance failed; all three measurement rows are retained"
    );
}

#[test]
fn spanning_profile_height_short() {
    measure_at_speed("SPAN", 7, 12, 60, 3.);
}

#[test]
fn subnormal_sweep_budget_preserves_a_useful_speed_slider() {
    for cap in [0.6, 0.8, 0.9, 1., 1.8, 3.] {
        let mut prior = 0.;
        for percent in 1..=300 {
            let pace = paced_speed(f64::from(percent) / 100., cap);
            assert!(pace > prior, "cap={cap} Speed={percent}");
            prior = pace;
        }
        assert_eq!(prior, cap);
        if (0.8..=1.).contains(&cap) {
            assert_eq!(paced_speed(1., cap), 0.6);
        }
    }
}

#[test]
fn spanning_fit_uses_a_representative_physical_output() {
    for seed in [7, 19, 31] {
        for count in [12, 30, 60] {
            let spanning = BounceWorld::new(profile_input("SPAN", seed, count));
            let independent = BounceWorld::new(profile_input("G", seed, count));
            let area = |w: &BounceWorld| w.balls.iter().map(|b| b.r * b.r).sum::<f64>();
            assert!(
                (area(&spanning) - area(&independent)).abs() < 1e-8,
                "seed={seed} count={count}"
            );
        }
    }
    let default = BounceWorld::new(profile_input("default", 7, 12));
    let rms = (default.balls.iter().map(|b| b.r * b.r).sum::<f64>() / 12.).sqrt();
    assert!((rms - 132.8076241).abs() < 1e-6, "approved default radii");
    assert_eq!(default.pace, 1., "approved default clock");
}

#[test]
#[ignore = "Spanning wall-time acceptance at Speed 100 and 200; run through heavy"]
fn spanning_speed_acceptance_matrix() {
    let mut passed = true;
    for speed in [1., 2.] {
        for seed in [7, 19, 31] {
            passed &= std::panic::catch_unwind(|| measure_at_speed("SPAN", seed, 12, 1800, speed))
                .is_ok();
        }
    }
    assert!(
        passed,
        "Spanning low-Speed height acceptance failed; all six rows retained"
    );
}

#[test]
#[ignore = "Dense Spanning liveliness at Scale 200; run through heavy"]
fn spanning_dense_acceptance_matrix() {
    let mut passed = true;
    for count in [30, 60] {
        passed &= std::panic::catch_unwind(|| measure_at_speed("SPAN", 7, count, 1800, 3.)).is_ok();
    }
    assert!(
        passed,
        "Dense Spanning acceptance failed; both rows retained"
    );
}

#[test]
fn fitted_spanning_cast_uses_the_calibrated_monotonic_clock() {
    for seed in [7, 19, 31] {
        let mut prior = 0.;
        for percent in 1..=300 {
            let mut config = profile_input("SPAN", seed, 12);
            config.speed = f64::from(percent) / 100.;
            let w = BounceWorld::new(config);
            assert!(w.pace > prior, "seed={seed} Speed={percent}");
            prior = w.pace;
            if percent == 100 {
                assert_eq!(w.pace, 0.6);
            } else if percent == 200 {
                assert!((w.pace - (0.6 + OUTPUT_SWEEP_PACE_AT_300) / 2.).abs() < 1e-12);
            }
        }
        assert_eq!(prior, OUTPUT_SWEEP_PACE_AT_300);
    }
}

#[test]
fn spanning_fast_clock_is_continuous_across_fitted_casts() {
    let mut casts = Vec::new();
    for seed in 1..=64 {
        for step in 0..=20 {
            let mut config = profile_input("SPAN", seed, 12);
            config.scale = 1. + f64::from(step) / 20.;
            let mut paces = [0.; 3];
            let mut rms = 0.;
            for (i, speed) in [1., 2., 3.].into_iter().enumerate() {
                config.speed = speed;
                let w = BounceWorld::new(config.clone());
                paces[i] = w.pace;
                rms = (w.balls.iter().map(|b| b.r * b.r).sum::<f64>() / 12.).sqrt();
            }
            assert!(
                paces[0] < paces[1] && paces[1] < paces[2],
                "seed={seed} scale={} paces={paces:?}",
                config.scale
            );
            casts.push((rms, paces[2], seed, config.scale));
        }
    }
    for a in &casts {
        for b in &casts {
            if a.0.max(b.0) <= 1.1 * a.0.min(b.0) + 1e-10 {
                assert!(
                    a.1.max(b.1) <= 1.35 * a.1.min(b.1),
                    "similar-size casts have different fast clocks: {a:?}, {b:?}"
                );
            }
        }
    }
}

#[test]
#[ignore = "Five affected fast-Scale Spanning cases, 30 minutes each; run through heavy"]
fn spanning_fast_scale_acceptance_matrix() {
    let mut passed = true;
    for (scale, seed) in [(1., 7), (1., 19), (1., 31), (1.2, 19), (1.4, 19)] {
        let mut config = profile_input("SPAN", seed, 12);
        config.scale = scale;
        // These ordinary-size casts use the default arc policy. Measure their
        // motion, energy, retention and containment; the tall-arc G targets
        // apply to Scale 200, which is checked by its separate matrix.
        passed &= std::panic::catch_unwind(|| {
            measure_configuration("SPAN", seed, 12, 1800, 3., config);
        })
        .is_ok();
    }
    assert!(
        passed,
        "fast-Scale Spanning acceptance failed; all five rows retained"
    );
}
