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
            if let Some(s)=self.snapshots.last_mut().filter(|s|s.tick==tick) {
                s.decision=self.history[tick%91];
            }
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
        for k in 0..d.candidates.len() {
            let mut w=snapshot.world.diagnostic_snapshot();
            let candidate=d.candidates[k];
            let mut ai=snapshot.ai.clone();ai.disable_observers();
            let held_ticks=30.max(candidate.turn_ticks+16);
            let mut c=Probe {ai,id,generation,desired:candidate.desired,exit_angle:candidate.exit_angle,
                turn_ticks:candidate.turn_ticks,track_goal:candidate.track_goal,goal:d.goal,rush:candidate.rush,coil_center:candidate.coil_center,coil_radius:candidate.coil_radius,coil_sign:candidate.coil_sign,coil_pitch:candidate.coil_pitch,coil_progress:candidate.coil_progress,
                coil_last_angle:None,attack_valid:candidate.attack_valid,attack_turn_ticks:candidate.attack_turn_ticks,attack_crossing:candidate.attack_crossing,
                attack_crossing_rush:candidate.attack_crossing_rush,total:held_ticks,remaining:held_ticks};
            // All rivals respond normally. Hold the alternative steering for
            // at least one second (up to 1.6s for compound escapes), honoring
            // its control schedule, then return
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
struct Probe { ai:AiController,id:usize,generation:u32,desired:f64,exit_angle:f64,turn_ticks:usize,track_goal:bool,goal:snakes_core::Point,rush:f64,coil_center:snakes_core::Point,coil_radius:f64,coil_sign:f64,coil_pitch:f64,coil_progress:f64,coil_last_angle:Option<f64>,
    attack_valid:bool,attack_turn_ticks:usize,attack_crossing:f64,attack_crossing_rush:f64,total:usize,remaining:usize }
impl Controller for Probe {
    fn steer(&mut self,w:&World,s:SnakeView<'_>) -> Steering {
        let steering=self.ai.steer(w,s);
        if s.id as usize==self.id && s.generation==self.generation && self.remaining>0 {
            let elapsed=self.total-self.remaining;
            self.remaining-=1;
            // Attack controls override ordinary goal and compound-turn exits
            // during both approach and crossing, including mid-crossing replay.
            if self.attack_valid {
                return if elapsed>=self.attack_turn_ticks {
                    Steering {desired_angle:self.attack_crossing,rush:self.attack_crossing_rush}
                } else {Steering {desired_angle:self.desired,rush:self.rush}};
            }
            let desired_angle=if self.track_goal {
                let goal=if self.coil_radius>0.0 {
                    let d=w.displacement(self.coil_center,s.segments[0].current);
                    let theta=d.y.atan2(d.x);let radius=(d.x*d.x+d.y*d.y).sqrt().max(1.0);
                    if let Some(last)=self.coil_last_angle {
                        let before=(self.coil_progress-0.9).max(0.0);
                        self.coil_progress+=snakes_core::normalize_angle(theta-last)*self.coil_sign;
                        self.coil_radius-=self.coil_pitch/std::f64::consts::TAU*((self.coil_progress-0.9).max(0.0)-before);
                    }
                    self.coil_last_angle=Some(theta);
                    let k=if self.coil_progress>0.9 {self.coil_pitch/std::f64::consts::TAU} else {0.0};
                    let angle=theta+self.coil_sign*(std::f64::consts::FRAC_PI_2+(k/radius).atan()+((radius-self.coil_radius)/30.0).atan().clamp(-0.35,0.35));
                    w.canonical_point(snakes_core::Point{x:s.segments[0].current.x+angle.cos()*80.0,y:s.segments[0].current.y+angle.sin()*80.0})
                } else {self.goal};
                let d=w.displacement(s.segments[0].current,goal);d.y.atan2(d.x)
            } else if self.turn_ticks>0 && elapsed>=self.turn_ticks {self.exit_angle} else {self.desired};
            Steering {desired_angle,rush:self.rush}
        } else {steering}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn advisory_v2_mid_crossing_replay_keeps_attack_over_compound_exit() {
        let candidate=snakes_core::ai::CandidateDiagnostic {desired:1.3,rush:0.0,
            attack_valid:true,attack_crossing:1.3,attack_crossing_rush:0.0,turn_ticks:2,exit_angle:-1.2,..Default::default()};
        let w=World::diagnostic_arena(snakes_core::Config {density:0.0,rules:snakes_core::RuleSet::V2,..Default::default()},
            &[(snakes_core::Point{x:400.0,y:400.0},0.0,24,1.0)],&[]).unwrap();
        let mut probe=Probe {ai:AiController::new(),id:0,generation:w.snake(0).unwrap().generation,
            desired:candidate.desired,exit_angle:candidate.exit_angle,turn_ticks:candidate.turn_ticks,
            track_goal:false,goal:Default::default(),rush:candidate.rush,coil_center:Default::default(),
            coil_radius:0.0,coil_sign:0.0,coil_pitch:0.0,coil_progress:0.0,coil_last_angle:None,
            attack_valid:candidate.attack_valid,attack_turn_ticks:0,
            attack_crossing:candidate.attack_crossing,attack_crossing_rush:candidate.attack_crossing_rush,total:12,remaining:12};
        for _ in 0..12 {
            let control=probe.steer(&w,w.snake(0).unwrap());
            assert_eq!(control.desired_angle,1.3,"crossing must override the pending ordinary exit");
            assert_eq!(control.rush,0.0);
        }
        // A cancelled record retains no attack override, even if all the
        // headings and rushes still look like a crossing maneuver.
        probe.attack_valid=false;probe.remaining=probe.total;
        for elapsed in 0..12 {
            assert_eq!(probe.steer(&w,w.snake(0).unwrap()).desired_angle,
                if elapsed>=candidate.turn_ticks {candidate.exit_angle} else {candidate.desired});
        }
    }
    #[test]
    fn replay_attack_overrides_ordinary_exit_in_approach_and_crossing() {
        let w=World::diagnostic_arena(snakes_core::Config {density:0.0,..Default::default()},
            &[(snakes_core::Point{x:400.0,y:400.0},0.0,24,1.0)],&[]).unwrap();
        let s=w.snake(0).unwrap();
        for (switch,rush) in [(0,0.15),(6,1.0)] {
            let mut c=Probe {ai:AiController::new(),id:0,generation:1,desired:if switch==0 {1.3} else {0.7},exit_angle:-1.2,
                turn_ticks:2,track_goal:false,goal:Default::default(),rush,coil_center:Default::default(),
                coil_radius:0.0,coil_sign:0.0,coil_pitch:0.0,coil_progress:0.0,coil_last_angle:None,
                attack_valid:true,attack_turn_ticks:switch,attack_crossing:1.3,attack_crossing_rush:0.15,total:12,remaining:12};
            for elapsed in 0..12 {
                let control=c.steer(&w,s);
                assert_eq!(control.desired_angle,if elapsed>=switch {1.3} else {0.7});
                assert_eq!(control.rush,if elapsed>=switch {0.15} else {rush});
            }
        }
    }
}
