// SPDX-License-Identifier: GPL-3.0-or-later
//! Reproducible ecosystem scorecard. Optional args: minutes, seed, IQ, deadly/wrap.
//! Flags: --ai-only, --diagnostics (observer replay), --profile (AI phase timing).
//! Contest metrics are physical near-food races (two heads within 160 pixels),
//! not cooperative AI claims. Kill attribution uses exact mechanics events.
use snakes_core::{ai::AiController, controller::{BaselineController}, Config, DeathReason, Point, World, MAX_SNAKES, STEP_SECONDS, normalize_angle};
use std::time::Instant;
#[path="support/phase_contact.rs"] mod phase_contact;
#[path="support/diagnostics.rs"] mod diagnostics;
use diagnostics::{ScoreController,Diagnostics};
#[allow(dead_code)]
#[path="support/accounting.rs"] mod accounting;
use accounting::{Observed,CombatTotals};
#[derive(Clone,Copy,Default)]
struct Before {alive:bool,head:Point,angle:f64,generation:u32,flags:u32}
#[derive(Clone,Copy,Debug,Default)]
#[allow(dead_code)]
struct TraceSnake {head:Point,angle:f64,rush:f64,prey:Option<usize>,side:i8,stage:u8,exit:Point}
fn percentile(v:&[f64],p:f64)->f64 {v[((v.len()-1) as f64*p) as usize]}
fn run<C:ScoreController>(label:&str,cfg:Config,minutes:usize,controller:C,diagnostic:bool,trace:bool) {
    let mut controller=Observed::new(controller);
    let mut w=World::new(cfg).unwrap();
    let mut diagnostics=diagnostic.then(Diagnostics::new);
    let ticks=minutes*1800;
    let mut times=Vec::with_capacity(ticks);
    let mut before=[Before::default();MAX_SNAKES];
    let mut combat=CombatTotals::default();let mut eaten=0u64;let mut contests=0u64;let mut contests_lost=0u64;
    let mut history=[[TraceSnake::default();MAX_SNAKES];16];let mut traces=0;
    let mut oscillations=0u64;let mut reversals=0u64;
    let mut sign=[0i8;MAX_SNAKES];let mut sign_tick=[0usize;MAX_SNAKES];
    let mut signed_turn=[0.0f64;MAX_SNAKES];let mut abs_turn=[0.0f64;MAX_SNAKES];
    let mut circle_origin=[Point::default();MAX_SNAKES];let mut circle_nutrition=[0.0;MAX_SNAKES];
    let mut hunting_ticks=0usize;let mut coil_ticks=0usize;let mut rush_ticks=0usize;
    let mut circle_ticks=0usize;let mut live_ticks=0usize;let mut sum_length=0u64;let mut max_length=0usize;
    let mut heading_origin=[0.0f64;MAX_SNAKES];
    // Observations live outside the timed tick. An intended Phase use means an
    // actual otherwise-lethal body/head contact survived while intangible.
    let mut pickups=0u64; let mut phase_pickups=0u64; let mut trapped_phase_pickups=0u64;
    let mut phase_used=0u64; let mut phase_expiries=0u64; let mut phase_clear_expiries=0u64;
    let mut phase_contact_ticks=0u64; let mut used=[false;MAX_SNAKES];
    for tick in 0..ticks {
        for s in w.snakes() {
            let id=s.id as usize;
            before[id]=Before {alive:s.alive,head:s.segments.first().map(|p|p.current).unwrap_or_default(),
                angle:s.angle,generation:s.generation,flags:s.flags};
        }
        if trace && tick%6==0 {
            for s in w.snakes().filter(|s|s.alive) {
                let id=s.id as usize;let ai=controller.ai().unwrap();let d=ai.competition_debug(id).unwrap();let debug=ai.debug(id).unwrap();
                history[tick/6%16][id]=TraceSnake {head:s.segments[0].current,angle:s.angle,rush:w.observed_rush(id).unwrap(),prey:d.prey,side:d.attack_side,stage:d.attack_stage,
                    exit:debug.path[debug.path_count.saturating_sub(1) as usize]};
            }
        }
        controller.begin();
        if let Some(d)=&mut diagnostics {d.before(tick,&w,&controller);}
        let start=Instant::now();w.step(&mut controller);times.push(start.elapsed().as_secs_f64()*1000.0);
        if let Some(d)=&mut diagnostics {d.after(tick,&controller);}
        for event in w.frame_events() {
            if event.kind==snakes_core::EventKind::Pickup {
                pickups+=1;
                if event.other_snake_id==snakes_core::effects::EffectKind::Phase as u32 {
                    phase_pickups+=1;
                    let id=event.snake_id as usize; used[id]=false;
                    trapped_phase_pickups+=u64::from(before[id].flags & snakes_core::flags::TRAPPED!=0);
                }
            }
            if event.kind==snakes_core::EventKind::EffectExpiry && event.other_snake_id==snakes_core::effects::EffectKind::Phase as u32 {
                phase_expiries+=1;
                phase_clear_expiries+=u64::from(w.snake(event.snake_id as usize).is_some_and(|s|s.alive));
            }
        }
        for s in w.snakes().filter(|s|s.alive && s.flags & snakes_core::flags::PHASED!=0) {
            let id=s.id as usize;
            let contact=phase_contact::otherwise_lethal(&w,s);
            if contact {phase_contact_ticks+=1;if !used[id] {phase_used+=1;used[id]=true;}}
        }
        for (_,e,_,position,_) in w.consumption_events() {
            eaten+=1;
            let challengers=(0..w.snake_count()).filter(|i|*i!=e as usize && before[*i].alive && w.distance_squared(before[*i].head,position)<160.0*160.0).count();
            if challengers>0 {contests+=1;contests_lost+=challengers as u64;}
        }
        for event in w.collision_events() {
            combat.record(event,&controller.tactics);
            let id=event.victim as usize;
            if matches!(event.reason,DeathReason::Wall|DeathReason::SelfHit|DeathReason::Body) {
                if let Some(d)=&mut diagnostics {d.death(tick,id,event.generation,event.reason);}
            }
            let opponents=event.owner_mask & !(1<<id);
            if opponents.count_ones()==1 && trace && traces<3 {
                let owner=opponents.trailing_zeros() as usize;
                traces+=1;println!("combat_event {event:?}");
                let start=(tick/6).saturating_sub(15);
                for t in start..=tick/6 {let h=&history[t%16];println!("  t={:.2} owner={:?} victim={:?}",t as f64*6.0*STEP_SECONDS,h[owner],h[id]);}
            }
        }
        for s in w.snakes() {
            let id=s.id as usize;let b=before[id];
            if !s.alive {continue;}
            let tactic=controller.tactics[id];
            if tactic.generation==s.generation {
                hunting_ticks+=usize::from(tactic.hunting);
                coil_ticks+=usize::from(tactic.coil);
            }
            if w.observed_rush(id).unwrap()>0.0 {rush_ticks+=1;}
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
    let CombatTotals {deaths,opponent_kills,ambiguous_kills,bigger_kills,smaller_kills,hunting_kills,staged_kills,attack_deaths,staged_deaths}=combat;
    let avg=times.iter().sum::<f64>()/ticks as f64;times.sort_unstable_by(f64::total_cmp);
    let snake_minutes=live_ticks as f64*STEP_SECONDS/60.0;
    println!("items pickups={pickups} pickups/min={:.3} phase_pickups={phase_pickups} trapped_phase_pickups={trapped_phase_pickups} phase_used={phase_used} phase_contact_ticks={phase_contact_ticks} phase_expiries={phase_expiries} phase_clear_expiries={phase_clear_expiries}",pickups as f64/minutes as f64);
    println!("{label} seed={} intelligence={} walls={} minutes={minutes} deaths={} wall={} body={} self={} head_lost={} head_tie={} deaths/snake_min={:.4} food/min={:.2} contests_won={contests} contests_lost={contests_lost} opponent_kills={opponent_kills} bigger_kills={bigger_kills} smaller_kills={smaller_kills} ambiguous_kills={ambiguous_kills} hunting_kills={hunting_kills} staged_kills={staged_kills} attack_deaths={attack_deaths} staged_deaths={staged_deaths} reversals/snake_min={:.2} oscillations/snake_min={:.2} circling_snake_s={:.2} mean_len={:.2} max_len={max_length} ms_avg={avg:.4} ms_p50={:.4} ms_p95={:.4} ms_p99={:.4} ms_max={:.4} hunting_snake_s={:.2} coil_snake_s={:.2} rush_snake_s={:.2}",
        cfg.seed,cfg.intelligence,if cfg.deadly_walls {"deadly"} else {"wrap"},w.stats().deaths,deaths[0],deaths[1],deaths[2],deaths[3],deaths[4],w.stats().deaths as f64/snake_minutes,eaten as f64/minutes as f64,reversals as f64/snake_minutes,oscillations as f64/snake_minutes,circle_ticks as f64*STEP_SECONDS,sum_length as f64/live_ticks as f64,percentile(&times,0.50),percentile(&times,0.95),percentile(&times,0.99),times.last().unwrap(),hunting_ticks as f64*STEP_SECONDS,coil_ticks as f64*STEP_SECONDS,rush_ticks as f64*STEP_SECONDS);
}
fn main() {
    let args:Vec<String>=std::env::args().collect();
    if args.iter().any(|s|s=="--help") {
        println!("ai_scorecard [minutes=8] [seed] [IQ=100/50] [deadly/wrap] [--ai-only] [--diagnostics] [--profile] [--trace] [--classic] [--no-power-ups]");return;
    }
    let positional:Vec<&String>=args.iter().skip(1).filter(|s|!s.starts_with("--")).collect();
    let minutes=positional.first().map(|s|s.parse().unwrap()).unwrap_or(8);
    assert!(minutes>0);
    let seeds=if let Some(seed)=positional.get(1) {vec![seed.parse().unwrap()]} else {vec![73,20260814,991]};
    for seed in seeds {for intelligence in [100.0,50.0] {for deadly_walls in [true,false] {
        let cfg=Config {width:3440.0,height:1440.0,density:100.0,trails:100.0,intelligence,
            self_collisions:true,deadly_walls,seed,power_ups:!args.iter().any(|s|s=="--no-power-ups"),rules: if std::env::args().any(|s|s=="--classic") { snakes_core::RuleSet::Classic } else { snakes_core::RuleSet::V2 },..Config::default()};
        if let Some(iq)=positional.get(2) {if intelligence!=iq.parse::<f64>().unwrap() {continue;}}
        if let Some(walls)=positional.get(3) {if deadly_walls!=(walls.as_str()=="deadly") {continue;}}
        let diagnostic=args.iter().any(|s|s=="--diagnostics");
        let mut ai=AiController::new();if args.iter().any(|s|s=="--profile") {ai.enable_profile();}
        if diagnostic {ai.enable_diagnostics();}
        run("ai",cfg,minutes,ai,diagnostic,args.iter().any(|s|s=="--trace"));
        if !args.iter().any(|s|s=="--ai-only") {run("baseline",cfg,minutes,BaselineController,false,false);}
    }}}
}
