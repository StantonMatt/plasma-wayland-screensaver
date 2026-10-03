// SPDX-License-Identifier: GPL-3.0-or-later
use std::time::Instant;
use snakes_core::{ Config, World };
use snakes_core::controller::BaselineController;
fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    sorted[((sorted.len()-1) as f64*fraction).round() as usize]
}
fn main() {
    let config = Config {
        width: 3440.0,
        height: 1440.0,
        density: 100.0,
        trails: 100.0,
        intelligence: 100.0,
        self_collisions: true,
        seed: 20260814,
        rules: if std::env::args().any(|s|s=="--classic") { snakes_core::RuleSet::Classic } else { snakes_core::RuleSet::V2 },..Config::default()
    };
    let mut world = World::new(config).unwrap();
    let mut controller = BaselineController;
    let mut all = Vec::with_capacity(14400);
    let mut minute = Vec::with_capacity(1800);
    println!("3440x1440, 30 Hz, max density/trails, seed 20260814, temporary BaselineController");
    println!("minute ms/tick p50_us p95_us p99_us max_us alive segments food deaths");
    for m in 1..=8 {
        minute.clear();
        for _ in 0..1800 {
            let now = Instant::now();
            world.step(&mut controller);
            let us = now.elapsed().as_secs_f64()*1e6;
            minute.push(us);
            all.push(us);
        }
        let average = minute.iter().sum::<f64>()/minute.len() as f64/1000.0;
        minute.sort_unstable_by(f64::total_cmp);
        let s = world.stats();
        println!("{m} {average:.6} {:.3} {:.3} {:.3} {:.3} {} {} {} {}", percentile(&minute, 0.5), percentile(&minute, 0.95), percentile(&minute, 0.99), minute.last().unwrap(), s.alive, s.total_segments, s.food, s.deaths);
    }
    let average = all.iter().sum::<f64>()/all.len() as f64/1000.0;
    all.sort_unstable_by(f64::total_cmp);
    println!("overall {average:.6} ms/tick; p50 {:.3} us; p95 {:.3} us; p99 {:.3} us; max {:.3} us", percentile(&all, 0.5), percentile(&all, 0.95), percentile(&all, 0.99), all.last().unwrap());
    println!("JS baseline mechanics approx 1.3 ms/tick; populations differ because controllers differ.");
}
