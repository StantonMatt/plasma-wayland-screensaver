// SPDX-License-Identifier: GPL-3.0-or-later
//! Seamless reference: union of the user's three configured overlay viewports.
use snakes_core::{ai::AiController, Config, World, RuleSet, EventKind, MAX_SNAKES};
use std::time::Instant;
fn main() {
    let ticks=std::env::args().find_map(|s|s.strip_prefix("--ticks=").and_then(|v|v.parse::<usize>().ok())).unwrap_or(600000);
    let min_capsules=std::env::args().find_map(|s|s.strip_prefix("--min-capsules=").and_then(|v|v.parse::<u64>().ok())).unwrap_or(0);
    let seed=std::env::args().find_map(|s|s.strip_prefix("--seed=").and_then(|v|v.parse::<i32>().ok())).unwrap_or(20261004);
    let config=Config {width:7920.0,height:1440.0,density:80.0,scale:200.0,speed:300.0,trails:100.0,intelligence:100.0,self_collisions:true,deadly_walls:true,rules:RuleSet::V2,aggression:100,palette_size:6,seed,..Config::default()};
    let mut w=World::new(Config {width:3440.0,..config}).unwrap();
    w.resize(5360.0,1440.0).unwrap();w.resize(7920.0,1440.0).unwrap(); let mut ai=AiController::new();
    for _ in 0..9000 {w.step(&mut ai);}
    let initial_stats=w.stats();let mut measured=0;
    let (mut spawned,mut picked,mut freezes,mut kills,mut cuts,mut bites)=(0u64,0u64,0u64,0u64,0u64,0u64);
    let mut frozen=[(0u32,0u16);MAX_SNAKES];let mut ms=0.0;
    for t in 0..ticks {
        for s in w.snakes() {frozen[s.id as usize]=(s.generation,s.face.frozen_ticks);}
        let now=Instant::now();w.step(&mut ai);ms+=now.elapsed().as_secs_f64()*1000.0;
        for e in w.frame_events().filter(|e|e.tick==w.tick()) {
            if e.kind==EventKind::ItemSpawn && e.other_snake_id==5 {spawned+=1;}
            if e.kind==EventKind::Pickup && e.other_snake_id==5 {picked+=1;}
            if e.kind==EventKind::Sever && w.snake(e.snake_id as usize).is_some_and(|s|s.generation==e.generation && s.face.frozen_ticks>0) {bites+=1;}
        }
        for s in w.snakes() {let (generation,ticks)=frozen[s.id as usize];if s.generation==generation && s.face.frozen_ticks>ticks {freezes+=1;}}
        for e in w.collision_events() {
            if e.owner_mask & !(1<<e.victim)!=0 && w.snake(e.victim as usize).is_some_and(|s|s.generation==e.generation && s.face.frozen_ticks>0) {
                kills+=1;if e.reason==snakes_core::DeathReason::Body {cuts+=1;}
            }
        }
        measured=t+1;
        if min_capsules>0 && spawned>=min_capsules {break;}
        if (t+1)%100000==0 {println!("progress ticks={} frost_capsules={} pickups={} freezes={} frozen_kills={} mean_ms={:.6}",t+1,spawned,picked,freezes,kills,ms/(t+1) as f64);}
    }
    let ticks=measured;
    println!("reference 7920x1440 80/200/300/100/100 seed={seed} ticks={ticks} mean_ms={:.6} frost_capsules={spawned} pickups={picked} pickup_rate={:.4} freezes={freezes} freezes_per_pickup={:.4} frozen_prey_kills={kills} frozen_prey_cutoffs={cuts} frozen_prey_bites={bites} deaths={} self_deaths={} walls={} segments={}",ms/ticks as f64,picked as f64/spawned.max(1) as f64,freezes as f64/picked.max(1) as f64,w.stats().deaths-initial_stats.deaths,w.stats().self_deaths-initial_stats.self_deaths,w.stats().wall_deaths-initial_stats.wall_deaths,w.stats().total_segments);
}
