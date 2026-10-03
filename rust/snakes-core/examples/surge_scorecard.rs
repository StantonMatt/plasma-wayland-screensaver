// SPDX-License-Identifier: GPL-3.0-or-later
//! Run through surge_scorecard.py to select only SURGE in an isolated crate.
use snakes_core::{ai::AiController, Config, EventKind, RuleSet, World, MAX_SNAKES};
use std::time::Instant;
#[allow(dead_code)]
#[path = "support/diagnostics.rs"] mod diagnostics;
#[allow(dead_code)]
#[path = "support/accounting.rs"] mod accounting;
use accounting::{CombatTotals, Observed};

fn main() {
    let minutes: usize = std::env::args().nth(1).unwrap_or_else(|| "8".into()).parse().unwrap();
    assert!(minutes > 0);
    let mut total_kills = 0;
    let mut total_self = 0;
    let mut total_wall = 0;
    let mut total_pickups = 0;
    let mut total_used = 0;
    let mut total_chained = 0;
    let mut means = 0.0;
    for seed in [73, 20260814, 991] { for intelligence in [100.0, 50.0] { for deadly_walls in [true, false] {
        let mut w = World::new(Config { width: 3440.0, height: 1440.0, density: 100.0, trails: 100.0,
            seed, intelligence, deadly_walls, self_collisions: true, rules: RuleSet::V2,
            ..Config::default() }).unwrap();
        let mut ai = Observed::new(AiController::new());
        let mut combat = CombatTotals::default();
        let mut times = Vec::with_capacity(minutes * 1800);
        let mut before = [(0u32, 0u8, 0u16, 0u8); MAX_SNAKES];
        let mut used = [false; MAX_SNAKES];
        let mut staged_bursts = [0usize; MAX_SNAKES];
        let mut pickups = 0;
        let mut used_pickups = 0;
        let mut chained_pickups = 0;
        let mut attack_ticks = 0;
        let mut burst_starts = 0;
        let mut staged_starts = 0;
        for _ in 0..minutes * 1800 {
            for s in w.snakes() { before[s.id as usize] = (s.generation, s.effect_kind, s.effect_ticks, s.boost_ticks); }
            ai.begin();
            let start = Instant::now();
            w.step(&mut ai);
            times.push(start.elapsed().as_secs_f64() * 1000.0);
            for event in w.collision_events() { combat.record(event, &ai.tactics); }
            for s in w.snakes() {
                let id = s.id as usize;
                let (generation, kind, ticks, boost) = before[id];
                if generation != s.generation || kind != 1 || ticks <= 1 { continue; }
                let staged = ai.tactics[id].generation == generation && ai.tactics[id].staged;
                if staged {
                    attack_ticks += 1;
                    if !used[id] { used_pickups += 1; used[id] = true; }
                }
                if boost == 0 && s.boost_ticks == 24 {
                    burst_starts += 1;
                    if staged {
                        staged_starts += 1;
                        staged_bursts[id] += 1;
                        if staged_bursts[id] == 2 { chained_pickups += 1; }
                    }
                }
            }
            for e in w.frame_events().filter(|e| e.tick == w.tick() && e.kind == EventKind::Pickup && e.other_snake_id == 1) {
                pickups += 1;
                used[e.snake_id as usize] = false;
                staged_bursts[e.snake_id as usize] = 0;
            }
        }
        let mean = times.iter().sum::<f64>() / times.len() as f64;
        times.sort_unstable_by(f64::total_cmp);
        let p99 = times[(times.len() - 1) * 99 / 100];
        let kills = combat.opponent_kills - combat.ambiguous_kills;
        println!("surge seed={seed} iq={intelligence} walls={} minutes={minutes} pickups={pickups} pickups_min={:.3} used_pickups={used_pickups} chained_pickups={chained_pickups} attack_ticks={attack_ticks} burst_starts={burst_starts} staged_starts={staged_starts} kills={kills} self={} wall={} ms_avg={mean:.6} ms_p99={p99:.6}",
            if deadly_walls { "deadly" } else { "wrap" }, pickups as f64 / minutes as f64, combat.deaths[2], combat.deaths[0]);
        total_kills += kills; total_self += combat.deaths[2]; total_wall += combat.deaths[0];
        total_pickups += pickups; total_used += used_pickups; total_chained += chained_pickups; means += mean;
    } } }
    let mean = means / 12.0;
    println!("surge_total kills={total_kills} self={total_self} wall={total_wall} pickups={total_pickups} pickups_min={:.3} used_pickups={total_used} used_pct={:.2} chained_pickups={total_chained} ms_avg={mean:.6} delta_ms={:.6} gate={}",
        total_pickups as f64 / (12 * minutes) as f64, total_used as f64 * 100.0 / (total_pickups as f64).max(1.0),
        mean - 0.221, if mean <= 0.241 { "PASS" } else { "FAIL" });
    if mean > 0.241 { std::process::exit(1); }
}
