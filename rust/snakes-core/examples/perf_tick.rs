// SPDX-License-Identifier: GPL-3.0-or-later
//! Pinned paired benchmark and exact simulation/controller replay fingerprint.
use snakes_core::{ai::AiController,controller::{Controller,Steering,FaceIntent},Config,World,RuleSet,Point,SnakeView};
use std::time::Instant;
struct Observed {ai:AiController,hash:u64,diagnostics:bool,replay:bool}
fn hash(h:&mut u64,x:u64){*h=h.rotate_left(7)^x.wrapping_mul(0x9e3779b97f4a7c15);}
impl Controller for Observed {
 fn face_intent(&self,id:u32)->FaceIntent{self.ai.face_intent(id)}
 fn intent_flags(&self,id:u32)->Option<u32>{self.ai.intent_flags(id)}
 fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
  let id=s.id;let result=self.ai.steer(w,s);
  hash(&mut self.hash,result.desired_angle.to_bits());hash(&mut self.hash,result.rush.to_bits());
  if self.diagnostics {for b in format!("{:?}",self.ai.decision(id as usize)).bytes(){hash(&mut self.hash,b as u64);}}
  if self.replay {snakes_core::controller::BaselineController.steer(w,w.snake(id as usize).unwrap())} else {result}
 }
}
fn main(){
 let args:Vec<_>=std::env::args().collect();let case=args.get(1).map(String::as_str).unwrap_or("standard");
 let ticks:usize=args.get(2).map(|x|x.parse().unwrap()).unwrap_or(9000);let seed:i32=args.get(3).map(|x|x.parse().unwrap()).unwrap_or(73);
 let prof=args.iter().any(|s|s=="profile");let diagnostics=args.iter().any(|s|s=="trace");
 let mut cfg=Config {width:3440.,height:1440.,density:100.,trails:100.,intelligence:100.,self_collisions:true,seed,rules:RuleSet::V2,..Config::default()};
 if case=="reference" || case=="reference-wide" {cfg.density=30.;cfg.scale=185.;cfg.speed=230.;}
 if case=="reference-wide" {cfg.width=7920.;}
 if args.iter().any(|s|s=="classic") {cfg.rules=RuleSet::Classic;}
 if args.iter().any(|s|s=="off") {cfg.power_ups=false;cfg.world_events=false;}
 if args.iter().any(|s|s=="wrap") {cfg.deadly_walls=false;}
 let mut w=if case=="crowded" {
  let snakes:Vec<_>=(0..cfg.snake_count()).map(|id|(Point{x:1300.+(id%4) as f64*110.,y:450.+(id/4) as f64*120.},0.2+id as f64*0.35,400,0.8)).collect();
  World::diagnostic_arena(cfg,&snakes,&[]).unwrap()
 } else {World::new(cfg).unwrap()};
 let mut ai=Observed {ai:AiController::new(),hash:0,diagnostics,replay:args.iter().any(|s|s=="replay")};
 if diagnostics {ai.ai.enable_diagnostics();}
 if case!="crowded" {for _ in 0..900 {w.step(&mut ai);}}
 if prof {ai.ai.enable_profile();}
 let mut times=Vec::with_capacity(ticks);let mut world_hash=0;let mut segments=0;let mut live=0;
 for _ in 0..ticks {
  let t=Instant::now();w.step(&mut ai);times.push(t.elapsed().as_secs_f64()*1e6);
  for s in w.snakes(){hash(&mut world_hash,s.generation as u64);hash(&mut world_hash,s.angle.to_bits());hash(&mut world_hash,s.desired_angle.to_bits());hash(&mut world_hash,s.flags as u64);hash(&mut world_hash,s.effect_kind as u64);hash(&mut world_hash,s.effect_ticks as u64);hash(&mut world_hash,s.segments.len() as u64);for p in s.segments {hash(&mut world_hash,p.current.x.to_bits());hash(&mut world_hash,p.current.y.to_bits());}}
  for f in w.foods(){hash(&mut world_hash,f.id);hash(&mut world_hash,f.position.x.to_bits());hash(&mut world_hash,f.position.y.to_bits());}
  hash(&mut world_hash,w.rng_state() as u64);segments+=w.stats().total_segments as u64;live+=w.stats().alive as u64;
 }
 let mean=times.iter().sum::<f64>()/ticks as f64;times.sort_unstable_by(f64::total_cmp);
 println!("case={case} seed={seed} ticks={ticks} mean_us={mean:.3} p99_us={:.3} world_hash={world_hash} decision_hash={} deaths={} segments={:.1} alive={:.1}",times[(ticks-1)*99/100],ai.hash,w.stats().deaths,segments as f64/ticks as f64,live as f64/ticks as f64);
 if prof {println!("profile_ns={:?} forecast_ns={:?}",ai.ai.profile(),ai.ai.forecast_profile());}
}
