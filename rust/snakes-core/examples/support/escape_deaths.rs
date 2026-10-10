// SPDX-License-Identifier: GPL-3.0-or-later
//! Death counterfactuals are deliberately outside timed steps and CPU builds.
use snakes_core::{*,ai::AiController,controller::{Controller,Steering}};
use super::item_lifecycle::StepInventory;
struct Snapshot {world:World,ai:AiController}
pub struct EscapeDeaths {
    snapshots:std::collections::VecDeque<Snapshot>,before:[StepInventory;MAX_SNAKES],
    holding:u64,usable:u64,by_kind:[u64;8],by_cause:[[u64;6];8],missing:u64,
}
impl EscapeDeaths {
    pub fn new()->Self {Self {snapshots:std::collections::VecDeque::with_capacity(3),before:[StepInventory::default();MAX_SNAKES],holding:0,usable:0,by_kind:[0;8],by_cause:[[0;6];8],missing:0}}
    pub fn before(&mut self,w:&World,ai:&AiController) {
        for s in w.snakes() {self.before[s.id as usize]=StepInventory::new(s.generation,s.inventory,true);}
        if w.tick()%30==0 {
            if self.snapshots.len()==3 {self.snapshots.pop_front();}
            self.snapshots.push_back(Snapshot {world:w.diagnostic_snapshot(),ai:ai.clone()});
        }
    }
    pub fn after(&mut self,w:&World) {
        for event in w.frame_events() {
            let id=event.snake_id as usize;if id>=MAX_SNAKES {continue;}
            self.before[id].apply(event,w.collision_events().any(|e|e.victim==event.snake_id && e.generation==event.generation));
        }
        for e in w.collision_events() {
            let id=e.victim as usize;let held=self.before[id];
            let mut wanted=0u8;
            for kind in 1..=7 {if held.carries(kind,e.generation) {wanted|=1<<kind;}}
            if wanted==0 {continue;}
            self.holding=self.holding.saturating_add(1);
            let start=e.tick.saturating_sub(30);
            let Some(snapshot)=self.snapshots.iter().rev().find(|s|s.world.tick()<=start) else {self.missing=self.missing.saturating_add(1);continue;};
            let mut replay=snapshot.world.diagnostic_snapshot();
            let mut probe=Probe {ai:snapshot.ai.clone(),id:e.victim,generation:e.generation,start,wanted,usable:0,seen:false};
            // Reproduce actual controls and world updates, then ask the same
            // rollout used for admission at each of the last thirty decisions.
            probe.run(&mut replay,e.tick);
            self.usable=self.usable.saturating_add(u64::from(probe.usable!=0));
            for kind in 1..=7 {if probe.usable&(1<<kind)!=0 {
                self.by_kind[kind]=self.by_kind[kind].saturating_add(1);
                let cause=e.reason as usize;
                self.by_cause[kind][cause]=self.by_cause[kind][cause].saturating_add(1);
            }}
        }
    }
    pub fn print(&self) {
        println!("escape_deaths holding_any={} usable_any={} usable_by_kind={:?} missing_lookback={}",self.holding,self.usable,self.by_kind,self.missing);
        for kind in 1..=7 {println!("escape_death_cause kind={kind} counts={:?}",self.by_cause[kind]);}
    }
}
struct Probe {ai:AiController,id:u32,generation:u32,start:u64,wanted:u8,usable:u8,seen:bool}
impl Probe {
    fn run(&mut self,w:&mut World,end:u64) {
        while w.tick()<end && self.usable!=self.wanted {
            w.step(self);
            if self.seen && w.snake(self.id as usize).is_none_or(|s|!s.alive || s.generation!=self.generation) {break;}
        }
    }
}
impl Controller for Probe {
    fn delegate(&self,_:u32)->Option<&dyn Controller> {Some(&self.ai)}
    fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
        let steering=self.ai.steer(w,s);
        self.seen|=s.id==self.id && s.generation==self.generation;
        if s.id==self.id && s.generation==self.generation && w.tick()>=self.start {
            let mut isolated=self.ai.clone();
            self.usable|=isolated.usable_escape_items(w,s,self.wanted & !self.usable);
        }
        steering
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lookback_replays_through_the_previous_generation_until_current_birth() {
        let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,density:0.0,
            width:2000.0,height:1200.0,deadly_walls:true,world_events:false,..Default::default()},
            &[(Point{x:-100.0,y:600.0},std::f64::consts::PI,20,0.0)],&[]).unwrap();
        let mut probe=Probe {ai:AiController::new(),id:0,generation:2,start:0,wanted:1<<1,usable:0,seen:false};
        probe.run(&mut w,250);
        assert!(probe.seen,"old-generation snapshots must reach the current generation's decisions");
        assert!(w.tick()>1,"the earlier generation cannot terminate the lookback");
        assert_eq!(probe.usable,0,"older generations cannot donate a held escape");
    }
}
