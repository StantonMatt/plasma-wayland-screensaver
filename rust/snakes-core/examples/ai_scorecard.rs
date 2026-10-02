// SPDX-License-Identifier: GPL-3.0-or-later
//! Reproducible ecosystem scorecard. Optional args: minutes, seed, IQ, deadly/wrap.
//! Flags: --ai-only, --diagnostics (observer replay), --profile (AI phase timing).
//! Contest metrics are physical near-food races (two heads within 160 pixels),
//! not cooperative AI claims. Kill attribution uses last collision reason and
//! swept geometry; body killer attribution is nearest colliding body sample.
use snakes_core::{ai::AiController, controller::{BaselineController}, Config, DeathReason, Point, World, MAX_FOOD, MAX_SNAKES, STEP_SECONDS, normalize_angle};
use std::time::Instant;
#[path="support/diagnostics.rs"] mod diagnostics;
use diagnostics::{ScoreController,Diagnostics};
#[derive(Clone,Copy,Default)]
struct Before {alive:bool,len:usize,head:Point,angle:f64,nutrition:f64,radius:f64,generation:u32}
fn percentile(v:&[f64],p:f64)->f64 {v[((v.len()-1) as f64*p) as usize]}
fn run<C:ScoreController>(label:&str,cfg:Config,minutes:usize,mut controller:C,diagnostic:bool) {
    let mut w=World::new(cfg).unwrap();
    let mut diagnostics=diagnostic.then(Diagnostics::new);
    let ticks=minutes*1800;
    let mut times=Vec::with_capacity(ticks);
    let mut before=[Before::default();MAX_SNAKES];
    let mut foods=[None;MAX_FOOD];
    let mut deaths=[0u64;5];let mut eaten=0u64;let mut contests=0u64;let mut contests_lost=0u64;
    let mut bigger_kills=0u64;let mut oscillations=0u64;let mut reversals=0u64;
    let mut sign=[0i8;MAX_SNAKES];let mut sign_tick=[0usize;MAX_SNAKES];
    let mut signed_turn=[0.0f64;MAX_SNAKES];let mut abs_turn=[0.0f64;MAX_SNAKES];
    let mut circle_origin=[Point::default();MAX_SNAKES];let mut circle_nutrition=[0.0;MAX_SNAKES];
    let mut circle_ticks=0usize;let mut live_ticks=0usize;let mut sum_length=0u64;let mut max_length=0usize;
    let mut heading_origin=[0.0f64;MAX_SNAKES];
    for tick in 0..ticks {
        for s in w.snakes() {
            let id=s.id as usize;
            before[id]=Before {alive:s.alive,len:s.segments.len(),head:s.segments.first().map(|p|p.current).unwrap_or_default(),
                angle:s.angle,nutrition:w.nutrition(id).unwrap(),radius:s.radius,generation:s.generation};
        }
        foods.fill(None);for (i,f) in w.foods().enumerate() {foods[i]=Some(f);}
        if let Some(d)=&mut diagnostics {d.before(tick,&w,&controller);}
        let start=Instant::now();w.step(&mut controller);times.push(start.elapsed().as_secs_f64()*1000.0);
        if let Some(d)=&mut diagnostics {d.after(tick,&controller);}
        // A disappeared particle is counted only if nutrition increased in a
        // physical capture region, excluding expiry and death-food eviction.
        for f in foods.iter().flatten() {
            if w.foods().any(|after|after.id==f.id) {continue;}
            let eater=(0..w.snake_count()).filter(|i|before[*i].alive && w.nutrition(*i).unwrap()>before[*i].nutrition)
                .filter(|i|w.distance_squared(before[*i].head,f.position)<(before[*i].radius*4.0+20.0).powi(2))
                .min_by(|a,b|w.distance_squared(before[*a].head,f.position).total_cmp(&w.distance_squared(before[*b].head,f.position)));
            if let Some(e)=eater {
                eaten+=1;
                let challengers=(0..w.snake_count()).filter(|i|*i!=e && before[*i].alive && w.distance_squared(before[*i].head,f.position)<160.0*160.0).count();
                if challengers>0 {contests+=1;contests_lost+=challengers as u64;}
            }
        }
        for s in w.snakes() {
            let id=s.id as usize;let b=before[id];
            if b.alive && !s.alive {
                let reason=w.last_death_reason(id).unwrap();
                if matches!(reason,DeathReason::Wall|DeathReason::SelfHit|DeathReason::Body) {
                    if let Some(d)=&mut diagnostics {d.death(tick,id,b.generation,reason);}
                }
                match w.last_death_reason(id).unwrap() {
                    DeathReason::Wall=>deaths[0]+=1,
                    DeathReason::SelfHit=>deaths[2]+=1,
                    DeathReason::Body=>{
                        deaths[1]+=1;
                        let attacker=w.snakes().filter(|o|o.id!=s.id && o.alive).filter(|o|o.segments.iter().skip(1).any(|p|
                            w.segment_distance_squared(p.current,b.head,w.canonical_point(Point{x:b.head.x+b.angle.cos()*8.0,y:b.head.y+b.angle.sin()*8.0}))<(b.radius+o.radius).powi(2))).max_by_key(|o|o.segments.len());
                        if attacker.is_some_and(|a|a.segments.len()>=b.len+4) {bigger_kills+=1;}
                    },
                    DeathReason::Head=>{
                        let opponent=(0..w.snake_count()).filter(|i|*i!=id && before[*i].alive)
                            .min_by(|a,c|w.distance_squared(b.head,before[*a].head).total_cmp(&w.distance_squared(b.head,before[*c].head)));
                        if opponent.is_some_and(|o|before[o].len>=b.len+4) {deaths[3]+=1;bigger_kills+=1;}
                        else {deaths[4]+=1;}
                    },DeathReason::None=>{}
                }
            }
            if !s.alive {continue;}
            live_ticks+=1;sum_length+=s.segments.len() as u64;max_length=max_length.max(s.segments.len());
            if !b.alive || b.generation!=s.generation || tick%150==0 {
                signed_turn[id]=0.0;abs_turn[id]=0.0;circle_origin[id]=s.segments[0].current;
                circle_nutrition[id]=w.nutrition(id).unwrap();heading_origin[id]=s.angle;sign[id]=0;
            } else {
                let d=normalize_angle(s.angle-b.angle);let direction=if d>0.008 {1} else if d< -0.008 {-1} else {0};
                if direction!=0 && direction!=sign[id] {
                    if sign[id]!=0 && tick-sign_tick[id]<15 {oscillations+=1;}
                    sign[id]=direction;sign_tick[id]=tick;
                }
                signed_turn[id]+=d;abs_turn[id]+=d.abs();
                if normalize_angle(s.angle-heading_origin[id]).abs()>2.8 {reversals+=1;heading_origin[id]=s.angle;}
                if tick%150==149 && abs_turn[id]>5.0 && signed_turn[id].abs()>4.8
                    && w.distance_squared(circle_origin[id],s.segments[0].current)<150.0*150.0
                    && w.nutrition(id).unwrap()-circle_nutrition[id]<0.5 {circle_ticks+=150;}
            }
        }
    }
    if let Some(ai)=controller.ai().filter(|ai|ai.profile()[4]!=0) {println!("ai_profile_ns {:?}",ai.profile());}
    if let Some(d)=diagnostics {d.report();}
    let avg=times.iter().sum::<f64>()/ticks as f64;times.sort_unstable_by(f64::total_cmp);
    let snake_minutes=live_ticks as f64*STEP_SECONDS/60.0;
    println!("{label} seed={} intelligence={} walls={} minutes={minutes} deaths={} wall={} body={} self={} head_lost={} head_tie={} deaths/snake_min={:.4} food/min={:.2} contests_won={contests} contests_lost={contests_lost} bigger_kills={bigger_kills} reversals/snake_min={:.2} oscillations/snake_min={:.2} circling_snake_s={:.2} mean_len={:.2} max_len={max_length} ms_avg={avg:.4} ms_p95={:.4} ms_p99={:.4} ms_max={:.4}",
        cfg.seed,cfg.intelligence,if cfg.deadly_walls {"deadly"} else {"wrap"},w.stats().deaths,deaths[0],deaths[1],deaths[2],deaths[3],deaths[4],w.stats().deaths as f64/snake_minutes,eaten as f64/minutes as f64,reversals as f64/snake_minutes,oscillations as f64/snake_minutes,circle_ticks as f64*STEP_SECONDS,sum_length as f64/live_ticks as f64,percentile(&times,0.95),percentile(&times,0.99),times.last().unwrap());
}
fn main() {
    let args:Vec<String>=std::env::args().collect();
    if args.iter().any(|s|s=="--help") {
        println!("ai_scorecard [minutes=8] [seed] [IQ=100/50] [deadly/wrap] [--ai-only] [--diagnostics] [--profile]");return;
    }
    let positional:Vec<&String>=args.iter().skip(1).filter(|s|!s.starts_with("--")).collect();
    let minutes=positional.first().map(|s|s.parse().unwrap()).unwrap_or(8);
    assert!(minutes>0);
    let seeds=if let Some(seed)=positional.get(1) {vec![seed.parse().unwrap()]} else {vec![73,20260814,991]};
    for seed in seeds {for intelligence in [100.0,50.0] {for deadly_walls in [true,false] {
        let cfg=Config {width:3440.0,height:1440.0,density:100.0,trails:100.0,intelligence,
            self_collisions:true,deadly_walls,seed,..Config::default()};
        if let Some(iq)=positional.get(2) {if intelligence!=iq.parse::<f64>().unwrap() {continue;}}
        if let Some(walls)=positional.get(3) {if deadly_walls!=(walls.as_str()=="deadly") {continue;}}
        let diagnostic=args.iter().any(|s|s=="--diagnostics");
        let mut ai=AiController::new();if args.iter().any(|s|s=="--profile") {ai.enable_profile();}
        if diagnostic {ai.enable_diagnostics();}
        run("ai",cfg,minutes,ai,diagnostic);
        if !args.iter().any(|s|s=="--ai-only") {run("baseline",cfg,minutes,BaselineController,false);}
    }}}
}
