//! Measured release acceptance. Passive drops disable intentional charged launches.
use super::{
    tests::{L3, input},
    *,
};

fn passive_apices(mat: Material, elasticity: i32) -> Vec<f64> {
    let mut config = input(&[L3[0]], 1, 35);
    config.parameters.push(super::super::schema::Value {
        id: 4,
        kind: 2,
        payload: mat as u64 + 1,
    });
    config.parameters.push(super::super::schema::Value {
        id: 3,
        kind: 1,
        payload: elasticity as u64,
    });
    let mut w = BounceWorld::new(config);
    w.params.collisions = false;
    w.params.worldEvents = false;
    let b = &mut w.balls[0];
    assert_eq!(b.mat, mat);
    b.p = v(1280., b.r + 1.);
    b.v = V::default();
    b.spin = 0.;
    b.period = f64::INFINITY;
    let floor = w.height - b.r;
    let drop = floor - b.p.y;
    let mut rising = false;
    let mut highest = floor;
    let mut apices = Vec::new();
    for tick in 1..=120 * 1800 {
        w.step(tick);
        let b = &w.balls[0];
        if b.v.y < 0. {
            rising = true;
            highest = highest.min(b.p.y);
        } else if rising {
            let apex = (floor - highest) / drop;
            apices.push(apex);
            if apex < 0.05 || apices.len() == 200 {
                break;
            }
            highest = floor;
            rising = false;
        } else if b.v.len2() < 1. && b.supported {
            break;
        }
    }
    apices
}

#[test]
fn passive_drop_lifetime() {
    let measure_only = std::env::var_os("BOUNCE_MEASURE_ONLY").is_some();
    let mut defaults = Vec::new();
    for (material, minimum) in [
        (Material::Rubber, 40),
        (Material::Glass, 25),
        (Material::Metal, 15),
    ] {
        let mut counts = Vec::new();
        for elasticity in [50, 60, 70, 80, 90, 100] {
            let apices = passive_apices(material, elasticity);
            let count = apices.iter().filter(|&&h| h >= 0.05).count();
            println!("DROP {material:?} elasticity={elasticity} visible={count} apices={apices:?}");
            if !measure_only {
                if elasticity == 90 {
                    assert!(count >= minimum);
                }
                if elasticity == 50 && material == Material::Metal {
                    assert!(count >= 6);
                }
                if elasticity == 100 {
                    assert_eq!(count, 200);
                    // The room governor first brings a full-height drop into
                    // its safe band. At the elastic endpoint that mature band
                    // must remain stable for every material.
                    let earlier = apices[100..150].iter().sum::<f64>();
                    let later = apices[150..200].iter().sum::<f64>();
                    assert!(later >= 0.90 * earlier);
                }
            }
            if elasticity == 90 {
                defaults.push(count);
            }
            counts.push(count);
        }
        if !measure_only {
            assert!(counts.windows(2).all(|w| w[0] <= w[1]));
        }
    }
    if !measure_only {
        assert!(defaults.windows(2).all(|w| w[0] > w[1]));
    }
}

#[test]
#[ignore = "30 minute measured scene acceptance; run through heavy"]
fn scene_liveliness() {
    let measure_only = std::env::var_os("BOUNCE_MEASURE_ONLY").is_some();
    for profile in ["default", "G"] {
        let mut config = input(if profile == "G" { &L3[..1] } else { L3 }, 12, 35);
        config.seed = 7;
        config.trails = 0.;
        config.density = 50.;
        if profile == "G" {
            config.speed = 3.;
            config.scale = 2.;
            config.density = 80.;
            config.palette = 2;
        }
        let mut w = BounceWorld::new(config);
        let mut still = [0u64; 61];
        let mut longest = 0u64;
        let mut window = 0usize;
        let mut samples = 0usize;
        let mut recent = [(0usize, 0usize); 1200];
        let mut interval_sum = [0f64; 6];
        let mut interval_count = [0usize; 6];
        let mut worst = 1f64;
        let mut peak_energy = 0f64;
        let mut peak_speed = 0f64;
        let mut apex_sum = [0f64; 2];
        let mut apex_count = [0usize; 2];
        let mut prior_vy = [0f64; 61];
        for tick in 1..=120 * 1800 {
            w.step(tick);
            let slot = (tick as usize - 1) % recent.len();
            window -= recent[slot].0;
            samples -= recent[slot].1;
            recent[slot] = (0, 0);
            for (i, b) in w.balls.iter().enumerate() {
                let moving = !b.sleeping && (!b.supported || b.v.len2() >= 100.);
                window += usize::from(moving);
                samples += 1;
                recent[slot].0 += usize::from(moving);
                recent[slot].1 += 1;
                if moving {
                    still[i] = 0;
                } else {
                    still[i] += 1;
                    longest = longest.max(still[i]);
                }
                if prior_vy[i] < 0.
                    && b.v.y >= 0.
                    && !b.supported
                    && w.gravity(tick as f64 / 120.).y > 0.
                {
                    let interval = (tick as usize - 1) / (120 * 300);
                    interval_sum[interval] += w.height - b.r - b.p.y;
                    interval_count[interval] += 1;
                    let period = if tick <= 120 * 300 {
                        Some(0)
                    } else if tick > 120 * 1500 {
                        Some(1)
                    } else {
                        None
                    };
                    if let Some(p) = period {
                        apex_sum[p] += w.height - b.r - b.p.y;
                        apex_count[p] += 1;
                    }
                }
                prior_vy[i] = b.v.y;
                peak_speed = peak_speed.max(b.v.len() / w.height);
                assert!(
                    w.input.layout.circle_fits(b.p.into(), (b.r - 0.2).max(0.)),
                    "escaped {profile} tick {tick}"
                );
            }
            peak_energy = peak_energy.max(
                w.balls.iter().map(|b| b.v.len2() / b.inv_mass).sum::<f64>() / w.kinetic_limit,
            );
            if tick >= 1200 {
                worst = worst.min(window as f64 / samples as f64);
            }
        }
        println!(
            "APEX_INTERVALS {profile} mean={:?} counts={interval_count:?}",
            std::array::from_fn::<_, 6, _>(|i| interval_sum[i] / interval_count[i] as f64)
        );
        let mean = std::array::from_fn::<_, 2, _>(|i| apex_sum[i] / apex_count[i] as f64);
        println!(
            "SCENE {profile} worst_10s={worst:.6} longest_rest_s={:.6} first5_apex={} last5_apex={} ratio={} samples={apex_count:?} peak_energy={peak_energy} peak_speed={peak_speed}",
            longest as f64 / 120.,
            mean[0],
            mean[1],
            mean[1] / mean[0]
        );
        if !measure_only {
            assert!(worst >= 0.8);
            assert!(longest <= 480);
            assert!(mean[1] >= mean[0] * 0.9);
            assert!(peak_speed <= 2.201 && peak_energy < 12.);
        }
    }
}

#[test]
fn recharged_launches_stagger_without_position_jumps() {
    let mut w = BounceWorld::new(input(L3, 12, 35));
    w.params.worldEvents = false;
    w.params.collisions = false;
    for (i, b) in w.balls.iter_mut().enumerate() {
        b.p = v(250. + 470. * i as f64, w.height - b.r);
        b.v = V::default();
        b.spin = 0.;
    }
    let mut launches = [None; 12];
    for tick in 1..=360 {
        let positions: Vec<_> = w.balls.iter().map(|b| b.p).collect();
        w.step(tick);
        for (i, b) in w.balls.iter().enumerate() {
            assert!((b.p - positions[i]).len() <= 2.2 * w.height / 120. + 0.001);
            if b.v.y < -100. {
                launches[i].get_or_insert(tick);
            }
        }
    }
    assert!(launches.iter().all(Option::is_some), "{launches:?}");
    let times: Vec<_> = launches.into_iter().map(Option::unwrap).collect();
    assert!(
        times.iter().max().unwrap() - times.iter().min().unwrap() >= 30,
        "{times:?}"
    );
}

#[test]
fn large_accelerated_profile_keeps_moving() {
    let mut config = input(&L3[..1], 12, 35);
    config.seed = 7;
    config.scale = 2.;
    config.speed = 3.;
    config.trails = 0.;
    let mut w = BounceWorld::new(config);
    let mut moving = 0usize;
    let mut worst = 1f64;
    for tick in 1..=7200 {
        w.step(tick);
        moving += w
            .balls
            .iter()
            .filter(|b| !b.sleeping && (!b.supported || b.v.len2() >= 100.))
            .count();
        if tick % 1200 == 0 {
            let ratio = moving as f64 / (1200. * 12.);
            worst = worst.min(ratio);
            println!(
                "G window {} moving={ratio:.6} bodies={:?}",
                tick / 120,
                w.balls
                    .iter()
                    .map(|b| (b.v.len(), b.sleeping, b.supported, b.charge, b.flow_limit))
                    .collect::<Vec<_>>()
            );
            moving = 0;
        }
    }
    assert!(worst >= 0.8, "{worst}");
}

#[test]
fn short_charge_starts_at_the_quiet_glow() {
    let mut rng = Pcg32::new(7, 1);
    let mut b = Ball::new(&mut rng, 0, 50., Material::Rubber, 0);
    b.sleeping = true;
    b.phase = 0.;
    for period in [0.55, 1.45, 6., 16.] {
        b.period = period;
        b.charge = 0.;
        assert!((b.quiet_glow() - 0.25).abs() < 1e-6, "period={period}");
        b.charge = period;
        assert!((b.quiet_glow() - 0.60).abs() < 1e-6, "period={period}");
    }
}
