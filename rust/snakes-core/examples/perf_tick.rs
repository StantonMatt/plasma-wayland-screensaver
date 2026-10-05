// SPDX-License-Identifier: GPL-3.0-or-later
//! Pinned paired benchmark and exact simulation/controller replay fingerprint.
use snakes_core::{ai::AiController,controller::{Controller,Steering,FaceIntent},Config,World,RuleSet,Point,SnakeView};
use std::time::Instant;
#[path="support/long_fixtures.rs"] mod long_fixtures;
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
 let ticks:usize=args.get(2).map(|x|x.parse().unwrap()).unwrap_or(if case=="desktop" {54000} else {9000});let seed:i32=args.get(3).map(|x|x.parse().unwrap()).unwrap_or(if case=="desktop" {1} else {73});
 let prof=args.iter().any(|s|s=="profile");let diagnostics=args.iter().any(|s|s=="trace");
 let mut cfg=Config {width:3440.,height:1440.,density:100.,trails:100.,intelligence:100.,self_collisions:true,seed,rules:RuleSet::V2,..Config::default()};
 if case=="reference" || case=="reference-wide" {cfg.density=30.;cfg.scale=185.;cfg.speed=230.;}
 if case=="reference-wide" {cfg.width=7920.;}
 if case=="desktop" || case.starts_with("giant-") {cfg.width=7920.;cfg.density=80.;cfg.scale=200.;cfg.speed=300.;cfg.deadly_walls=true;cfg.palette_size=6;}
 if args.iter().any(|s|s=="classic") {cfg.rules=RuleSet::Classic;}
 if args.iter().any(|s|s=="off") {cfg.power_ups=false;cfg.world_events=false;}
 if args.iter().any(|s|s=="wrap") {cfg.deadly_walls=false;}
 if let Some(i)=args.iter().position(|a|a=="--aggression") {cfg.aggression=args[i+1].parse().unwrap();}
 let desktop_layout=case=="desktop" || case.starts_with("giant-");
 let mut w=if case=="crowded" {
  let snakes:Vec<_>=(0..cfg.snake_count()).map(|id|(Point{x:1300.+(id%4) as f64*110.,y:450.+(id/4) as f64*120.},0.2+id as f64*0.35,400,0.8)).collect();
  World::diagnostic_arena(cfg,&snakes,&[]).unwrap()
 } else {World::new(if desktop_layout {Config {width:3440.,..cfg}} else {cfg}).unwrap()};
 if desktop_layout {w.resize(5360.,1440.).unwrap();w.resize(7920.,1440.).unwrap();}
 let giant=case.strip_prefix("giant-").map(|s|s.parse::<usize>().unwrap());
 let fixture=giant.map(|n|long_fixtures::spiral(&w,0,n));
 if let Some((points,angle))=&fixture {w.diagnostic_body(0,points,*angle).unwrap();}
 let mut giant_ticks=0;
 let mut ai=Observed {ai:AiController::new(),hash:0,diagnostics,replay:args.iter().any(|s|s=="replay")};
 if diagnostics {ai.ai.enable_diagnostics();}
 let warmup=args.iter().position(|a|a=="--warmup").map_or(900,|i|args[i+1].parse::<usize>().unwrap());
 if case!="crowded" && giant.is_none() {for _ in 0..warmup {w.step(&mut ai);}}
 if prof {ai.ai.enable_profile();}
 let mut times=Vec::with_capacity(ticks);let mut world_hash=0;let mut segments=0;let mut live=0;
 for _ in 0..ticks {
  // Reinstall after death or loss of half the body, outside timing. V2
  // corpses retain their length but do not perform steering or movement.
  if let Some((points,angle))=&fixture {
   let s=w.snake(0).unwrap();
   if !s.alive || s.segments.len()<points.len()/2 {w.diagnostic_body(0,points,*angle).unwrap();}
   let s=w.snake(0).unwrap();
   if s.alive && s.segments.len()>=points.len()/2 {giant_ticks+=1;}
  }
  let t=Instant::now();w.step(&mut ai);times.push(t.elapsed().as_secs_f64()*1e6);
  for s in w.snakes(){hash(&mut world_hash,s.generation as u64);hash(&mut world_hash,s.angle.to_bits());hash(&mut world_hash,s.desired_angle.to_bits());hash(&mut world_hash,s.flags as u64);hash(&mut world_hash,s.effect_kind as u64);hash(&mut world_hash,s.effect_ticks as u64);hash(&mut world_hash,s.segments.len() as u64);for p in s.segments {hash(&mut world_hash,p.current.x.to_bits());hash(&mut world_hash,p.current.y.to_bits());}}
  for f in w.foods(){hash(&mut world_hash,f.id);hash(&mut world_hash,f.position.x.to_bits());hash(&mut world_hash,f.position.y.to_bits());}
  hash(&mut world_hash,w.rng_state() as u64);segments+=w.stats().total_segments as u64;live+=w.stats().alive as u64;
 }
 println!("giant_ticks={giant_ticks}");
 let mean=times.iter().sum::<f64>()/ticks as f64;times.sort_unstable_by(f64::total_cmp);
 println!("case={case} seed={seed} ticks={ticks} mean_us={mean:.3} p99_us={:.3} world_hash={world_hash} decision_hash={} deaths={} segments={:.1} alive={:.1}",times[(ticks-1)*99/100],ai.hash,w.stats().deaths,segments as f64/ticks as f64,live as f64/ticks as f64);
 if prof {println!("profile_ns={:?} forecast_ns={:?}",ai.ai.profile(),ai.ai.forecast_profile());println!("spatial_ns_counts={:?}",ai.ai.spatial_profile());println!("strategy_ns={:?} race_ns={:?}",ai.ai.strategy_profile(),ai.ai.race_profile());}
}
