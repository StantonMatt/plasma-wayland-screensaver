// SPDX-License-Identifier: GPL-3.0-or-later
//! F3 seed-991 corner lead-up, fixed production desktop startup.
//! Run through heavy: this reconstructs 266174 ticks. --forced-exit tests a
//! physical alternative after the identified respawn; it is not AI policy.
use snakes_core::{ai::AiController,Config,World,RuleSet,SnakeView};
use snakes_core::controller::{Controller,Steering};
struct Observed {ai:AiController,forced:bool}
impl Controller for Observed {
    fn delegate(&self,_id:u32)->Option<&dyn Controller> {Some(&self.ai)}
 fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
  let advice=self.ai.steer(w,s);
  if self.forced && s.id==9 && s.generation==47 && w.tick()>=266157 {
   Steering {desired_angle:2.7509339480197843,rush:0.0}
  } else {advice}
 }
}
fn main() {
 let cfg=Config {width:3440.,height:1440.,density:80.,trails:100.,scale:200.,speed:300.,intelligence:100.,self_collisions:true,deadly_walls:true,palette_size:6,rules:RuleSet::V2,seed:991,..Config::default()};
 let mut w=World::new(cfg).unwrap();w.resize(5360.,1440.).unwrap();w.resize(7920.,1440.).unwrap();
 let mut ai=Observed {ai:AiController::new(),forced:std::env::args().any(|s|s=="--forced-exit")};ai.ai.enable_diagnostics();
 for _ in 0..if ai.forced {266229} else {266174} {
  let s=w.snake(9).unwrap();let p=s.segments.first().map_or(Default::default(),|p|p.current);
  let (speed,turn)=w.motion_limits(9,0.).unwrap();
  if w.tick()>=266140 && !s.segments.is_empty() {println!("pre tick={} gen={} alive={} len={} head={:?} angle={} desired={} radius={} speed={} turn={} body0={:?} body10={:?}",w.tick(),s.generation,s.alive,s.segments.len(),p,s.angle,s.desired_angle,s.radius,speed,turn,s.segments[0],s.segments.get(10));}
  w.step(&mut ai);
  if w.tick()>=266140 {let d=ai.ai.decision(9);println!("decision tick={} selected={} safe={:?} desired={:?} areas={:?} goals={:?}",w.tick(),d.selected,d.candidates.map(|c|c.safe_ticks),d.candidates.map(|c|c.desired),d.candidates.map(|c|c.area),d.goal);}
 }
 println!("stats {:?} original_alive={} original_generation={}",w.stats(),w.snake(9).unwrap().alive,w.snake(9).unwrap().generation);
}
