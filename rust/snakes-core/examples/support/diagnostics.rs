// SPDX-License-Identifier: GPL-3.0-or-later
use snakes_core::{ai::{AiController, DecisionDiagnostic}, controller::{Controller, Steering}, World, SnakeView, DeathReason, MAX_SNAKES};

pub trait ScoreController: Controller {
    fn ai(&self) -> Option<&AiController> { None }
}
impl ScoreController for AiController { fn ai(&self) -> Option<&AiController> { Some(self) } }
impl ScoreController for snakes_core::controller::BaselineController {}
struct Snapshot { tick:usize, world:World, ai:AiController, decision:[DecisionDiagnostic;MAX_SNAKES] }
pub struct Diagnostics {
    snapshots: Vec<Snapshot>,
    history: Vec<[DecisionDiagnostic;MAX_SNAKES]>,
    pub counts: [usize;8],
}
impl Diagnostics {
    pub fn new() -> Self { Self { snapshots:Vec::new(),history:vec![[DecisionDiagnostic::default();MAX_SNAKES];91],counts:[0;8] } }
    pub fn before<C:ScoreController>(&mut self,tick:usize,w:&World,c:&C) {
        if tick%30!=0 {return;}
        if let Some(ai)=c.ai() {
            if self.snapshots.len()==5 {self.snapshots.remove(0);}
            self.snapshots.push(Snapshot { tick,world:w.diagnostic_snapshot(),ai:ai.clone(),decision:[DecisionDiagnostic::default();MAX_SNAKES] });
        }
    }
    pub fn after<C:ScoreController>(&mut self,tick:usize,c:&C) {
        if let Some(ai)=c.ai() {
            for id in 0..MAX_SNAKES {self.history[tick%91][id]=ai.decision(id);}
            if let Some(s)=self.snapshots.last_mut().filter(|s|s.tick==tick) {s.decision=self.history[tick%91];}
        }
    }
    pub fn death(&mut self,tick:usize,id:usize,generation:u32,reason:DeathReason) {
        let valid=|s:&&Snapshot|s.world.snake(id).is_some_and(|s|s.alive && s.generation==generation);
        let Some(snapshot)=self.snapshots.iter().rev().filter(valid).find(|s|s.tick+60<=tick)
            .or_else(||self.snapshots.iter().find(valid)) else {
                self.counts[7]+=1;
                println!("diagnostic tick={tick} snake={id} generation={generation} death={reason:?} class=no_snapshot");return;
            };
        let d=snapshot.decision[id];
        let mut survived=0u16;
        for k in 0..9 {
            let mut w=snapshot.world.diagnostic_snapshot();
            let candidate=d.candidates[k];
            let mut ai=snapshot.ai.clone();ai.disable_observers();
            let mut c=Probe {ai,id,generation,desired:candidate.desired,exit_angle:candidate.exit_angle,
                turn_ticks:candidate.turn_ticks,track_goal:candidate.track_goal,goal:d.goal,remaining:30};
            // All rivals respond normally. Hold the alternative steering for
            // one second, honoring its turn/straight schedule, then return
            // control to the planner. A survivor proves an available escape;
            // failed probes cannot prove no other trajectory could survive.
            let mut alive=true;
            for _ in 0..120 {
                w.step(&mut c);
                let s=w.snake(id).unwrap();
                if !s.alive || s.generation!=generation {alive=false;break;}
            }
            if alive {survived|=1<<k;}
        }
        let capped=d.candidates.iter().any(|c|c.capped);
        let chosen=d.candidates[d.selected];
        let open_escape=d.candidates.iter().enumerate().any(|(i,c)|survived&(1<<i)!=0 && c.area>=d.required_cells);
        let category=if capped {6} else if survived==0 {0} else if chosen.area<d.required_cells {
            if open_escape {2} else {3}
        } else if reason==DeathReason::SelfHit {4}
            else if chosen.safe_ticks>=d.horizon && reason==DeathReason::Body {5} else {1};
        self.counts[category]+=1;
        println!("diagnostic tick={tick} snake={id} generation={generation} death={reason:?} class={} replay_from={} survives_4s_mask={survived:#x}",
            ["no_replay_escape","late_detection","bad_pocket","fill_underestimate","fill_overestimate_or_turnability","rival_prediction","query_cap"][category],snapshot.tick);
        println!("  replay_decision lookback_ticks={} chosen={} required_cells={} length={} horizon={} candidates={:?}",
            tick-snapshot.tick,d.selected,d.required_cells,d.length,d.horizon,d.candidates);
        for seconds in [1,2,3] {
            if tick<seconds*30 {continue;}
            let at=tick-seconds*30; let d=self.history[at%91][id];
            if d.generation!=generation {continue;}
            println!("  lookback_s={seconds} chosen={} required_cells={} length={} horizon={} candidates={:?}",d.selected,d.required_cells,d.length,d.horizon,d.candidates);
        }
    }
    pub fn report(&self) {
        println!("diagnostic_breakdown no_replay_escape={} late_detection={} bad_pocket={} fill_underestimate={} fill_overestimate_or_turnability={} rival_prediction={} query_cap={} no_snapshot={}",
            self.counts[0],self.counts[1],self.counts[2],self.counts[3],self.counts[4],self.counts[5],self.counts[6],self.counts[7]);
    }
}
struct Probe { ai:AiController,id:usize,generation:u32,desired:f64,exit_angle:f64,turn_ticks:usize,track_goal:bool,goal:snakes_core::Point,remaining:usize }
impl Controller for Probe {
    fn steer(&mut self,w:&World,s:SnakeView<'_>) -> Steering {
        let steering=self.ai.steer(w,s);
        if s.id as usize==self.id && s.generation==self.generation && self.remaining>0 {
            let elapsed=30-self.remaining;
            self.remaining-=1;
            let desired_angle=if self.track_goal {let d=w.displacement(s.segments[0].current,self.goal);d.y.atan2(d.x)} else if self.turn_ticks>0 && elapsed>=self.turn_ticks {self.exit_angle} else {self.desired};
            Steering {desired_angle,rush:0.0}
        } else {steering}
    }
}
