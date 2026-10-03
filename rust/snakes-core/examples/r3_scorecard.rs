// SPDX-License-Identifier: GPL-3.0-or-later
//! Paired R3 ecosystem scorecard. Run through r3_scorecard.py under heavy.
use snakes_core::{ai::AiController, Config, EventKind, RuleSet, World, MAX_SNAKES};
use std::time::Instant;
#[path="support/phase_contact.rs"] mod phase_contact;
#[allow(dead_code)]
#[path = "support/diagnostics.rs"] mod diagnostics;
#[allow(dead_code)]
#[path = "support/accounting.rs"] mod accounting;
use accounting::{CombatTotals, Observed};

fn main() {
    let minutes: usize = std::env::args().nth(1).unwrap_or_else(|| "8".into()).parse().unwrap();
    assert!(minutes > 0);
    let case=std::env::args().find_map(|arg|arg.strip_prefix("--case=").map(str::to_owned));
    let diagnostic = std::env::args().any(|a| a == "--attack-diagnostics");
    let death_diagnostic = std::env::args().any(|a| a == "--death-diagnostics");
    let mut total_kills = 0;
    let mut total_self = 0;
    let mut total_wall = 0;
    let mut total_pickups = 0;
    let mut total_used = 0;
    let mut total_chained = 0;
    let mut effect_pickups = [0usize; 4]; let mut effect_used = [0usize; 4];
    let mut means = 0.0;
    let mut cases = 0;
    for seed in [73, 20260814, 991] { for intelligence in [100.0, 50.0] { for deadly_walls in [true, false] {
        if case.as_ref().is_some_and(|c|*c!=format!("{seed},{intelligence},{deadly_walls}")) {continue;}
        cases += 1;
        let mut w = World::new(Config { width: 3440.0, height: 1440.0, density: 100.0, trails: 100.0,
            seed, intelligence, deadly_walls, self_collisions: true, rules: RuleSet::V2,
            ..Config::default() }).unwrap();
        let mut inner=AiController::new();
        if std::env::args().any(|a|a=="--profile") {inner.enable_profile();}
        if diagnostic || death_diagnostic { inner.enable_diagnostics(); }
        let mut ai = Observed::new(inner);
        let mut deaths = death_diagnostic.then(diagnostics::Diagnostics::new);
        // [finalists, safe, viable, selected, reply_sum, viable_rejected,
        //  viable_zero_reply, viable_with_reply]. Diagnostic runs are separate
        // from paired performance runs; enabling replay computes extra controls.
        let mut attacks = [[0usize; 8]; 4];
        let mut rejected_replies = [[0usize; 7]; 4];
        let mut rejected_gap = [0.0; 4];
        let mut combat = CombatTotals::default();
        let mut times = Vec::with_capacity(minutes * 1800);
        let mut before = [(0u32, 0u8, 0u16, 0u8); MAX_SNAKES];
        let mut used = [false; MAX_SNAKES];
        let mut episode_used = [false; MAX_SNAKES];
        let mut food_ids = [0u64; snakes_core::MAX_FOOD];
        let mut magnet_before = [false; MAX_SNAKES];
        let mut kind_pickups = [0usize;4]; let mut kind_used = [0usize;4];
        let mut staged_bursts = [0usize; MAX_SNAKES];
        let mut pickups = 0;
        let mut used_pickups = 0;
        let mut chained_pickups = 0;
        let mut attack_ticks = 0;
        let mut burst_starts = 0;
        let mut staged_starts = 0;
        for tick in 0..minutes * 1800 {
            for s in w.snakes() { before[s.id as usize] = (s.generation, s.effect_kind, s.effect_ticks, s.boost_ticks); }
            food_ids.fill(0);
            for (i,f) in w.foods().enumerate() {
                if f.feast_id != 0 || f.kind == snakes_core::FoodKind::Prism {food_ids[i]=f.id;}
            }
            for s in w.snakes() {magnet_before[s.id as usize]=s.alive && s.effect_kind==2 && s.effect_ticks>1;}
            ai.begin();
            if let Some(d) = &mut deaths { d.before(tick, &w, &ai); }
            let start = Instant::now();
            w.step(&mut ai);
            times.push(start.elapsed().as_secs_f64() * 1000.0);
            if let Some(d) = &mut deaths { d.after(tick, &ai); }
            if diagnostic {
                for s in w.snakes() {
                    let d = ai.inner.decision(s.id as usize);
                    // Only this tick's actual control calls can contribute:
                    // dead/respawning slots retain their last decision record.
                    if ai.tactics[s.id as usize].generation != before[s.id as usize].0
                        || d.generation != before[s.id as usize].0 || d.reused_plan { continue; }
                    let effect=(before[s.id as usize].1 as usize).min(3);
                    let row = &mut attacks[effect];
                    for k in [7, 8] {
                        let c = d.candidates[k];
                        if !c.attack_valid { continue; }
                        row[0] += 1;
                        if c.safe_ticks == d.horizon {
                            row[1] += 1;
                            row[4] += c.attack_replies;
                            if c.area >= d.required_cells || c.area_capped {
                                row[2] += 1;
                                row[5] += usize::from(d.selected != k);
                                row[6 + usize::from(c.attack_replies > 0)] += 1;
                                if d.selected!=k {
                                    rejected_replies[effect][c.attack_replies.min(6)]+=1;
                                    if c.attack_replies>0 {rejected_gap[effect]+=d.candidates[d.selected].score-c.score;}
                                }
                            }
                        }
                        row[3] += usize::from(d.selected == k);
                    }
                }
            }
            for (food_id,id,_,_,_) in w.consumption_events() {
                let id=id as usize;
                if magnet_before[id] && food_ids.contains(&food_id) && !episode_used[id] {kind_used[2]+=1;episode_used[id]=true;}
            }
            for s in w.snakes().filter(|s|s.alive && s.effect_ticks>0) {
                let id=s.id as usize;
                if episode_used[id] || before[id].0!=s.generation || before[id].1!=s.effect_kind || before[id].2<=1
                    || w.frame_events().any(|e|e.kind==EventKind::Pickup && e.snake_id==s.id) {continue;}
                let intended=match s.effect_kind {
                    1=>ai.tactics[id].generation==s.generation && ai.tactics[id].staged,
                    2=>ai.inner.debug(id).is_some_and(|d|d.flags&8==0 && d.target_count>0 && food_ids.contains(&d.target_food_ids[0])),
                    3=>phase_contact::otherwise_lethal(&w,s),
                    _=>false,
                };
                if intended {kind_used[s.effect_kind as usize]+=1;episode_used[id]=true;}
            }
            for e in w.frame_events().filter(|e|e.kind==EventKind::Pickup) {
                let kind=e.other_snake_id as usize;
                if kind<4 {kind_pickups[kind]+=1;episode_used[e.snake_id as usize]=false;}
            }
            for event in w.collision_events() {
                combat.record(event, &ai.tactics);
                if event.reason == snakes_core::DeathReason::SelfHit {
                    if let Some(d) = &mut deaths { d.death(tick, event.victim as usize, event.generation, event.reason); }
                }
            }
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
        if diagnostic {
            println!("attack_diagnostics seed={seed} iq={intelligence} walls={deadly_walls} by_effect={attacks:?}");
            println!("rejected_attack_diagnostics seed={seed} iq={intelligence} walls={deadly_walls} reply_histogram={rejected_replies:?} positive_reply_score_gap_sum={rejected_gap:?}");
        }
        if let Some(d) = deaths { d.report(); }
        println!("combat_diagnostics seed={seed} iq={intelligence} walls={deadly_walls} hunting_kills={} staged_kills={} hunting_deaths={} staged_deaths={} ambiguous={}",
            combat.hunting_kills, combat.staged_kills, combat.attack_deaths, combat.staged_deaths, combat.ambiguous_kills);
        for k in 1..4 {effect_pickups[k]+=kind_pickups[k];effect_used[k]+=kind_used[k];}
        println!("effect_metrics seed={seed} iq={intelligence} walls={deadly_walls} pickups={kind_pickups:?} used={kind_used:?}");
        if ai.inner.profile()[4]>0 {println!("ai_profile_ns {:?}",ai.inner.profile());println!("ai_forecast_profile_ns {:?}",ai.inner.forecast_profile());}
        let mean = times.iter().sum::<f64>() / times.len() as f64;
        times.sort_unstable_by(f64::total_cmp);
        let p99 = times[(times.len() - 1) * 99 / 100];
        let kills = combat.opponent_kills - combat.ambiguous_kills;
        println!("r3 seed={seed} iq={intelligence} walls={} minutes={minutes} pickups={pickups} pickups_min={:.3} used_pickups={used_pickups} chained_pickups={chained_pickups} attack_ticks={attack_ticks} burst_starts={burst_starts} staged_starts={staged_starts} kills={kills} self={} wall={} ms_avg={mean:.6} ms_p99={p99:.6}",
            if deadly_walls { "deadly" } else { "wrap" }, pickups as f64 / minutes as f64, combat.deaths[2], combat.deaths[0]);
        total_kills += kills; total_self += combat.deaths[2]; total_wall += combat.deaths[0];
        total_pickups += pickups; total_used += used_pickups; total_chained += chained_pickups; means += mean;
    } } }
    assert!(cases>0,"--case must match seed,IQ,deadly_walls, e.g. --case=991,50,true");
    let mean = means / cases as f64;
    println!("r3_total kills={total_kills} self={total_self} wall={total_wall} pickups={total_pickups} pickups_min={:.3} used_pickups={total_used} used_pct={:.2} chained_pickups={total_chained} ms_avg={mean:.6}",
        total_pickups as f64 / (cases * minutes) as f64, total_used as f64 * 100.0 / (total_pickups as f64).max(1.0));
    println!("effect_total pickups={effect_pickups:?} used={effect_used:?}");
}
