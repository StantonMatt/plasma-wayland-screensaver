// SPDX-License-Identifier: GPL-3.0-or-later
//! Same benchmark also runs unchanged against the previous release archive.
use snakes_core::{Config,RuleSet,World,ai::AiController};
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
    let cpu_start=cpu_seconds();let mut seconds=0.0;let mut segments=0u64;
    for _ in 0..ticks {let start=Instant::now();w.step(&mut ai);seconds+=start.elapsed().as_secs_f64();segments+=w.stats().total_segments as u64;}
    println!("events={} seed={} ticks={} mean_ms={:.6} cpu_ms={:.6} mean_segments={:.2} deaths={}",enabled,seed,ticks,seconds*1000.0/ticks as f64,(cpu_seconds()-cpu_start)*1000.0/ticks as f64,segments as f64/ticks as f64,w.stats().deaths);
}

// Linux process CPU time excludes scheduler contention on shared runners.
fn cpu_seconds()->f64 {
    #[repr(C)] struct Timespec {seconds:i64,nanos:i64}
    unsafe extern "C" {fn clock_gettime(clock:i32,time:*mut Timespec)->i32;}
    let mut t=Timespec {seconds:0,nanos:0};
    assert_eq!(unsafe {clock_gettime(2,&mut t)},0);
    t.seconds as f64+t.nanos as f64*1e-9
}
