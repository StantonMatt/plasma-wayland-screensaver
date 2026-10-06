// SPDX-License-Identifier: GPL-3.0-or-later
//! Natural-spawn ecosystem gate; all observers run outside timed ticks.
#[allow(dead_code)]
#[path="support/item_lifecycle.rs"] mod item_lifecycle;
use snakes_core::{ai::AiController, Config, World, RuleSet, EventKind, effects::EffectKind, MAX_SNAKES};
use std::time::Instant;
use item_lifecycle::{lifecycle,replaces_effect};
#[derive(Clone, Copy, Default)]
struct Attempt {generation:u32,active:bool, pursued:bool, diverted:bool, standoff:u32, near:bool}
impl Attempt {
    fn activate(&mut self,e:&snakes_core::FrameEvent)->bool {
        if !replaces_effect(e) {return false;}
        let replaced=self.active && self.generation==e.generation;
        *self=if e.other_snake_id==4 {Self {generation:e.generation,active:true,..Default::default()}} else {Self::default()};
        replaced
    }
    fn bite(&mut self,generation:u32)->bool {
        if !self.active || self.generation!=generation {return false;}
        *self=Self::default();true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use snakes_core::{FrameEvent,event_flags};
    #[test]
    fn venom_outcomes_follow_activation_preserve_frost_and_match_biter_generation() {
        let mut a=Attempt::default();let mut e=FrameEvent {kind:EventKind::Pickup,generation:7,other_snake_id:4,..Default::default()};
        assert!(!a.activate(&e));a.pursued=true;a.standoff=31;
        for (kind,item) in [(EventKind::Stash,1),(EventKind::Stash,4),(EventKind::Pickup,5)] {
            assert!(!a.activate(&FrameEvent {kind,other_snake_id:item,..e}));
            assert!(a.active && a.pursued);assert_eq!(a.standoff,31);
        }
        assert!(!a.bite(8));assert!(a.bite(7));assert!(!a.bite(7));
        e.flags=event_flags::HELD_ACTIVATION;assert!(!a.activate(&e));assert!(a.bite(7));
        a.activate(&e);assert!(a.activate(&FrameEvent {other_snake_id:1,..e}));assert!(!a.active);
        a.activate(&e);assert!(!a.activate(&FrameEvent {generation:8,..e}));assert_eq!(a.generation,8);
    }
}
fn main() {
    let reference=std::env::args().any(|a|a=="--reference");
    let ticks=std::env::args().find_map(|a|a.strip_prefix("--ticks=").and_then(|v|v.parse::<usize>().ok())).unwrap_or(270000);
    let seed=std::env::args().find_map(|a|a.strip_prefix("--seed=").and_then(|v|v.parse::<i32>().ok())).unwrap_or(20261004);
    let mut cfg=Config {width:3440.0,height:1440.0,rules:RuleSet::V2,seed,..Config::default()};
    if std::env::args().any(|a|a=="--no-powerups") {cfg.power_ups=false;}
    if reference {cfg.density=30.0;cfg.trails=100.0;cfg.scale=185.0;cfg.speed=230.0;cfg.intelligence=100.0;cfg.self_collisions=true;}
    let mut w=World::new(cfg).unwrap();let mut ai=AiController::new();
    let (mut spawn,mut pickup,mut sever,mut holder_deaths,mut kills)=(0u64,0u64,0u64,0u64,0u64);
    let mut activations=0u64;let mut venom_activations=0u64;
    let (mut capsule_spawn,mut capsule_pickup,mut prism_pickup,mut snake_ticks)=(0u64,0u64,0u64,0u64);
    let mut sever_value=0.0f64;let mut retained_fraction=0.0f64;let mut victim_ratio=0.0f64;
    let mut active_ticks=0u64;let mut expired_diverted=0u64;let mut expired_near=0u64;
    let mut prism_spawn=0u64;let mut last_prism=0u64;
    let mut ms=0.0;let mut held=[(0u32,false);MAX_SNAKES];let mut bitten=[0u32;MAX_SNAKES];
    let mut attempts=[Attempt::default();MAX_SNAKES];let mut outcomes=[0u64;7];
    let (mut pursued,mut diverted,mut confronted,mut near,mut boosted)=(0u64,0u64,0u64,0u64,0u64);
    for _ in 0..ticks {
        for s in w.snakes() {
            let id=s.id as usize;held[id]=(s.generation,s.alive && s.effect_kind==4 && s.effect_ticks>1);
            snake_ticks+=u64::from(s.alive);
            if !held[id].1 {continue;}
            active_ticks+=1;
            let a=&mut attempts[id];let c=ai.competition_debug(id).unwrap();let debug=ai.debug(id).unwrap();
            if debug.generation==s.generation && c.venom_target.and_then(|id|w.snake(id)).is_some_and(|r|r.alive) {
                a.pursued=true;pursued+=1;
                if debug.flags&4!=0 || !c.track_goal {a.diverted=true;diverted+=1;}
                if c.venom_standoff {a.standoff+=1;confronted+=1;}
            }
            for r in w.snakes().filter(|r|r.alive && r.id!=s.id && r.face.bite_immunity_ticks==0 && !(r.effect_kind==3 && r.effect_ticks>1)) {
                if r.segments.iter().enumerate().skip((r.segments.len()/2+1).max(4)).any(|(_,p)|w.distance_squared(s.segments[0].current,p.current)<(5.0*s.radius).powi(2)) {a.near=true;near+=1;break;}
            }
            boosted+=u64::from(c.rush>0.0 || s.boost_ticks>0);
        }
        let start=Instant::now();w.step(&mut ai);ms+=start.elapsed().as_secs_f64()*1000.0;
        if let Some(prism)=w.foods().find(|f|matches!(f.kind,snakes_core::FoodKind::PrismSeed|snakes_core::FoodKind::Prism)) {
            if prism.id!=last_prism {prism_spawn+=1;last_prism=prism.id;}
        }
        for e in w.frame_events() {
            let stage=lifecycle(e);let id=e.snake_id as usize;
            if stage.acquisition() {capsule_pickup+=1;if e.other_snake_id==4 {pickup+=1;}}
            if stage.activation() {activations+=1;if e.other_snake_id==4 {venom_activations+=1;}}
            if id<MAX_SNAKES {
                outcomes[5]+=u64::from(attempts[id].activate(e));
                if replaces_effect(e) {held[id]=(e.generation,e.other_snake_id==4);}
                if stage==item_lifecycle::ItemLifecycle::EffectExpiry && held[id].0==e.generation && e.other_snake_id==4 {held[id].1=false;}
            }
            match e.kind {
                EventKind::ItemSpawn=>{capsule_spawn+=1;if e.other_snake_id==EffectKind::Venom as u32 {spawn+=1;}},
                EventKind::Feast=>prism_pickup+=1,
                EventKind::Sever=>{
                    sever+=1;sever_value+=e.value as f64;retained_fraction+=e.cut_index as f64/(e.cut_index as f64+e.value as f64);
                    if let Some(biter)=w.snake(e.other_snake_id as usize).filter(|s|s.generation==e.other_generation) {victim_ratio+=(e.cut_index as f64+e.value as f64)/biter.segments.len() as f64;}
                    bitten[id]=e.generation;
                    let biter=e.other_snake_id as usize;
                    outcomes[0]+=u64::from(attempts[biter].bite(e.other_generation));
                    if held[biter].0==e.other_generation {held[biter].1=false;}
                },_=>{}
            }
        }
        // Kill presentation records have no generation; use collision records.
        for e in w.collision_events() {
            let id=e.victim as usize;
            holder_deaths+=u64::from(held[id]==(e.generation,true));
            kills+=u64::from(bitten[id]==e.generation);bitten[id]=0;
            if attempts[id].active && attempts[id].generation==e.generation {outcomes[4]+=1;attempts[id]=Attempt::default();}
        }
        for s in w.snakes() {
            let id=s.id as usize;if s.generation!=bitten[id] || (s.alive && s.face.bite_immunity_ticks==0) {bitten[id]=0;}
            let a=attempts[id];if a.active && (a.generation!=s.generation || s.effect_kind!=4 || s.effect_ticks==0) {
                expired_diverted+=u64::from(a.diverted);expired_near+=u64::from(a.near);
                outcomes[if !a.pursued {1} else if a.standoff>=30 {3} else {2}]+=1;attempts[id]=Attempt::default();
            }
        }
    }
    outcomes[6]=attempts.iter().filter(|a|a.active).count() as u64;
    let s=w.stats();println!("{} seed={} ticks={} mean_ms={:.6} capsules={} pickups={} bites={} pickup_rate={:.4} use_rate={:.4} kills_within_immunity={} holder_deaths={} deaths={} self={} wall={} segments={} all_capsules={} all_pickups={} prism_pickups={} snake_ticks={} deaths_per_snake_hour={:.4}",if reference {"reference"} else {"standard"},seed,ticks,ms/ticks as f64,spawn,pickup,sever,pickup as f64/spawn.max(1) as f64,sever as f64/venom_activations.max(1) as f64,kills,holder_deaths,s.deaths,s.self_deaths,s.wall_deaths,s.total_segments,capsule_spawn,capsule_pickup,prism_pickup,snake_ticks,s.deaths as f64*108000.0/snake_ticks.max(1) as f64);
    println!("activation_counts all={activations} venom={venom_activations}");
    println!("cuts mean_segments={:.2} mean_retained_fraction={:.4} mean_victim_ratio={:.3}",sever_value/sever.max(1) as f64,retained_fraction/sever.max(1) as f64,victim_ratio/sever.max(1) as f64);
    println!("rates capsules={:.4} prism_spawns={} prisms={:.4} holder_ticks={} holder_deaths_per_snake_hour={:.4} expiry_diverted={} expiry_near={}",capsule_pickup as f64/capsule_spawn.max(1) as f64,prism_spawn,prism_pickup as f64/prism_spawn.max(1) as f64,active_ticks,holder_deaths as f64*108000.0/active_ticks.max(1) as f64,expired_diverted,expired_near);
    println!("outcomes bite={} no_pursuit={} approach_expiry={} standoff_expiry={} death={} replaced={} pending={} held_ticks pursuit={} diverted={} confronted={} near={} boosted={}",outcomes[0],outcomes[1],outcomes[2],outcomes[3],outcomes[4],outcomes[5],outcomes[6],pursued,diverted,confronted,near,boosted);
}
