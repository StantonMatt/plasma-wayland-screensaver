// SPDX-License-Identifier: GPL-3.0-or-later
//! Real seamless desktop dimensions; all kinds come from the shared Rust enums.
use snakes_core::{Config,RuleSet,World,EventKind,FoodKind,ai::AiController};
use std::time::Instant;
fn main() {
    let args:Vec<_>=std::env::args().collect();
    let ticks=args.iter().find_map(|a|a.strip_prefix("--ticks=").and_then(|s|s.parse().ok())).unwrap_or(54000);
    let seed=args.iter().find_map(|a|a.strip_prefix("--seed=").and_then(|s|s.parse().ok())).unwrap_or(20261004);
    let enabled=!args.iter().any(|a|a=="--events-off");
    let mut w=World::new(Config {width:3440.0,height:1440.0,density:80.0,scale:200.0,speed:300.0,trails:100.0,
        intelligence:100.0,self_collisions:true,deadly_walls:true,rules:RuleSet::V2,world_events:enabled,seed,..Default::default()}).unwrap();
    w.resize(5360.0,1440.0).unwrap();w.resize(7920.0,1440.0).unwrap();
    let mut ai=AiController::new();
    for _ in 0..9000 {w.step(&mut ai);}
    let before=w.world_event_stats();let initial=w.stats();
    let mut previous=before;let mut measured=snakes_core::EventStats::default();
    let mut counted_shower=false;
    let mut seconds=0.0;let mut peak_meteors=0;
    let mut night_hunt=0u64;let mut night_sleep=0u64;
    let mut last_star=None;let mut last_night=None;let mut star_intervals=true;let mut night_intervals=true;
    for _ in 0..ticks {
        let start=Instant::now();w.step(&mut ai);seconds+=start.elapsed().as_secs_f64();
        if w.world_event.night>0.5 {
            night_hunt+=w.snakes().filter(|s|s.alive && s.flags & snakes_core::flags::HUNTING!=0).count() as u64;
            night_sleep+=w.snakes().filter(|s|s.alive && s.face.mood==snakes_core::Mood::Sleepy).count() as u64;
        }
        peak_meteors=peak_meteors.max(w.foods().filter(|f|f.kind==FoodKind::Meteor).count());
        for e in w.frame_events().filter(|e|e.kind==EventKind::WorldEvent) {
            println!("event kind={} {} tick={} seconds={:.3} end={}",e.other_snake_id,if e.duration_ticks>0 {"start"} else {"end"},e.tick,e.tick as f64/30.0,e.release_tick);
            if e.duration_ticks==0 {continue;}
            if e.other_snake_id==1 {counted_shower=true;}
            let (last,valid,range)=if e.other_snake_id==1 {(&mut last_star,&mut star_intervals,5400..=9000)} else {(&mut last_night,&mut night_intervals,12600..=18000)};
            if let Some(t)=*last {*valid&=range.contains(&(e.tick-t));}*last=Some(e.tick);
        }
        let now=w.world_event_stats();
        measured.starfalls+=now.starfalls-previous.starfalls;
        measured.nightfalls+=now.nightfalls-previous.nightfalls;
        // Exclude the tail of any shower that began during the warm-up.
        if counted_shower {
            measured.participants+=now.participants-previous.participants;
            measured.stars_eaten+=now.stars_eaten-previous.stars_eaten;
            measured.frenzy_kills+=now.frenzy_kills-previous.frenzy_kills;
            measured.zone_kills+=now.zone_kills-previous.zone_kills;
            measured.commitments+=now.commitments-previous.commitments;
        }
        previous=now;
    }
    let e=measured;let s=w.stats();
    println!("events={} seed={} ticks={} mean_ms={:.6} starfalls={} nightfalls={} participants={} stars_eaten={} frenzy_kills={} peak_meteors={} schedule_star={} schedule_night={} segments={} deaths={} ambient={:.6}",
        enabled,seed,ticks,seconds*1000.0/ticks as f64,e.starfalls,e.nightfalls,e.participants,e.stars_eaten,e.frenzy_kills,peak_meteors,star_intervals,night_intervals,s.total_segments,s.deaths-initial.deaths,w.world_event.ambient);
    println!("telegraph_commits={} zone_kills={} night_hunter_ticks={} night_sleepy_ticks={} arrivals_per_starfall={:.3} stars_eaten_pct={:.3}",e.commitments,e.zone_kills,night_hunt,night_sleep,(e.participants) as f64/(e.starfalls).max(1) as f64,(e.stars_eaten) as f64*100.0/((e.starfalls)*24).max(1) as f64);
    if enabled {assert!(e.starfalls>0 && e.nightfalls>0 && star_intervals && night_intervals);}
}
