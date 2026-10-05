// SPDX-License-Identifier: GPL-3.0-or-later
//! Evidence-based lead-up categories, not a proof over every feasible control.
//! An unresolved bounded area search never certifies a continuation. Rings are
//! per generation; earliest loss is censored at the 10-second window boundary.
use snakes_core::{ai::DesktopObservation, *};
use std::{collections::VecDeque, fs::File, io::{BufWriter, Write}};
#[derive(Clone, Copy)]
struct Decision {tick:u64, generation:u32, safe:usize, continuation:bool, unresolved:bool, first:bool, engaged:bool, prey:u32, prey_generation:u32, cutoff:bool, boosted:bool, power:u8}
pub struct Leadups {
    ring:[VecDeque<Decision>;MAX_SNAKES], live:[u64;3], deaths:[[u64;5];3],
    lengths:[usize;MAX_SNAKES], alive:[bool;MAX_SNAKES], out:BufWriter<File>,
    pub max_length:usize, pub first_lengths:[u64;4],
    recent_contest:u64, recent_non_contest:u64, recent_stupid:u64,
    contest:u64, non_contest:u64, stupid:u64, kills:u64, attacker_kills:u64, cutoffs:u64, cutoff_kills:u64, boost_kills:u64, power_kills:[u64;8],
    last_cutoff:[u64;MAX_SNAKES], last_cutoff_generation:[u32;MAX_SNAKES],
}
impl Leadups {
    pub fn new(prefix:&str)->Self {
        let mut out=BufWriter::new(File::create(format!("{prefix}.deaths.csv")).unwrap());
        writeln!(out,"tick,snake,generation,length,reason,category,first_loss_tick,loss_censored,selected_safe_ticks,continuation_unresolved,rival_changed,predicted_head_error,contest_recent_60,contest_at_death,killer,killer_generation,owner_mask,attacker_kill,cutoff_kill,boost_assisted,power_up_assisted").unwrap();
        Self {ring:std::array::from_fn(|_|VecDeque::with_capacity(301)),live:[0;3],deaths:[[0;5];3],lengths:[0;MAX_SNAKES],alive:[false;MAX_SNAKES],out,max_length:0,first_lengths:[0;4],recent_contest:0,recent_non_contest:0,recent_stupid:0,contest:0,non_contest:0,stupid:0,kills:0,attacker_kills:0,cutoffs:0,cutoff_kills:0,boost_kills:0,power_kills:[0;8],last_cutoff:[0;MAX_SNAKES],last_cutoff_generation:[0;MAX_SNAKES]}
    }
    pub fn before(&mut self,w:&World) {
        for s in w.snakes() {let id=s.id as usize;self.lengths[id]=s.segments.len();self.alive[id]=s.alive;
            if s.alive {self.live[cohort(s.segments.len())]+=1;self.max_length=self.max_length.max(s.segments.len());
                for (i,n) in [1000,2000,4000,6000].into_iter().enumerate() {if s.segments.len()>=n && self.first_lengths[i]==0 {self.first_lengths[i]=w.tick().max(1);}}
            }
        }
    }
    pub fn after(&mut self,w:&World,obs:&[DesktopObservation;MAX_SNAKES],called:&[bool;MAX_SNAKES]) {
        for id in 0..w.snake_count() {
            let generation=w.snake(id).unwrap().generation;let h=&mut self.ring[id];
            if h.back().is_some_and(|e|e.generation!=generation) {h.clear();}
            if !called[id] || obs[id].generation!=generation {continue;} let d=obs[id];
            if h.back().is_some_and(|e|e.generation!=d.generation) {h.clear();}
            if h.len()==300 {h.pop_front();}
            let hunted=obs.iter().enumerate().any(|(rid,other)|called[rid] && self.alive[rid]
                && w.snake(rid).is_some_and(|s|s.generation==other.generation)
                && other.prey==id as u32+1 && other.prey_generation==d.generation);
            let engaged=d.contested || d.fleeing || d.prey!=0 || d.venom_target!=0 || hunted;
            if d.cutoff {
                if self.last_cutoff_generation[id]!=d.generation || w.tick().saturating_sub(self.last_cutoff[id])>15 {self.cutoffs+=1;}
                self.last_cutoff[id]=w.tick();self.last_cutoff_generation[id]=d.generation;
            }
            h.push_back(Decision {tick:w.tick(),generation:d.generation,safe:d.safe_ticks,continuation:d.continuation,unresolved:d.continuation_unresolved,first:d.any_first_step,engaged,prey:if d.venom_target!=0 {d.venom_target} else {d.prey},prey_generation:d.prey_generation,cutoff:d.cutoff,boosted:d.boosted,power:d.power_up});
        }
        for e in w.collision_events() {self.record_death(w,e,obs,called);}
    }
    fn record_death(&mut self,w:&World,e:&CollisionEvent,obs:&[DesktopObservation;MAX_SNAKES],called:&[bool;MAX_SNAKES]) {
            let id=e.victim as usize;
            let current=called[id] && obs[id].generation==e.generation;
            if self.ring[id].back().is_some_and(|p|p.generation!=e.generation) {self.ring[id].clear();}
            let d=if current {obs[id]} else {DesktopObservation::default()};let h=&self.ring[id];
            let head_error=if current {w.distance_squared(d.next_head,e.head.current).sqrt()} else {f64::NAN};
            let mut rival_changed=false;
            for rid in 0..w.snake_count() {if !current || rid==id || e.owner_mask&(1<<rid)==0 {continue;}
                if let Some(head)=w.collision_head(rid) {
                    // More than half a tick's travel is a material forecast change.
                    let threshold=w.motion_limits(rid,0.0).map_or(1.0,|(speed,_)|(speed*STEP_SECONDS*0.5).max(1.0));
                    rival_changed |= d.rival_generation[rid]!=e.owner_generations[rid] || w.distance_squared(d.rival_next[rid],head.current)>threshold*threshold;
                }
            }
            let category=if !current {4} else if rival_changed {2} else if d.safe_ticks>0 {0} else if h.iter().all(|p|!p.first) {3} else {1};
            self.deaths[cohort(e.victim_length)][category]+=1;
            let last_viable=h.iter().rposition(|p|p.continuation);
            let loss=last_viable.map_or(0,|i|i+1);
            let first_loss=h.get(loss).map_or(0,|p|p.tick);
            let censored=last_viable.is_none();
            let contest_at_death=current && h.back().is_some_and(|p|p.tick==e.tick && p.engaged);
            let contest=current && h.iter().rev().take(60).any(|p|p.engaged);
            self.contest+=u64::from(contest_at_death);self.non_contest+=u64::from(!contest_at_death);
            self.stupid+=u64::from(!contest_at_death && category<=1);
            self.recent_contest+=u64::from(contest);self.recent_non_contest+=u64::from(!contest);
            self.recent_stupid+=u64::from(!contest && category<=1);
            // Match the mechanics' credited owner, preserving ties/masks too.
            let owners=e.owner_mask & !(1<<id);
            let killer=if owners==0 {u32::MAX} else {owners.trailing_zeros()};
            let mut attacker=false;let mut cutoff=false;let mut boosted=false;let mut power=0;
            let killer_generation=if (killer as usize)<MAX_SNAKES {e.owner_generations[killer as usize]} else {0};
            if (killer as usize)<MAX_SNAKES {
                self.kills+=1;
                for p in self.ring[killer as usize].iter().rev().take(60).filter(|p|p.generation==killer_generation && p.prey==id as u32+1 && p.prey_generation==e.generation) {
                    attacker=true;cutoff|=p.cutoff;boosted|=p.boosted;
                    if (1..=4).contains(&p.power) {power=p.power;}
                }
            }
            self.attacker_kills+=u64::from(attacker);self.cutoff_kills+=u64::from(cutoff);self.boost_kills+=u64::from(attacker && boosted);
            if attacker && power>0 {self.power_kills[power as usize]+=1;}
            writeln!(self.out,"{},{},{},{},{:?},{},{},{},{},{},{},{:.4},{},{},{},{},{},{},{},{},{}",e.tick,id,e.generation,e.victim_length,e.reason,
                ["prediction_mismatch","lost_continuation","rival_changed","already_unrecoverable_candidates","no_current_decision"][category],first_loss,censored,d.safe_ticks,d.continuation_unresolved,rival_changed,head_error,contest,contest_at_death,killer,killer_generation,e.owner_mask,attacker,cutoff,attacker && boosted,power).unwrap();
            // Keep all recent decisions so later workers can inspect the transition.
            for p in h {writeln!(self.out,"#decision,{},{},{},{},{},{},{}",id,p.tick,p.generation,p.safe,p.continuation,p.unresolved,p.first).unwrap();}
    }
    pub fn print(&mut self) {
        self.out.flush().unwrap();
        println!("combat contest_deaths={} non_contest_deaths={} stupid_deaths={} owner_kills={} attacker_kills={} cutoff_attempts={} cutoff_successes={} boost_assisted_kills={} power_assisted_kills={:?}",self.contest,self.non_contest,self.stupid,self.kills,self.attacker_kills,self.cutoffs,self.cutoff_kills,self.boost_kills,self.power_kills);
        println!("combat_recent_60 contest_deaths={} non_contest_deaths={} stupid_deaths={}",self.recent_contest,self.recent_non_contest,self.recent_stupid);
        let rates=self.deaths.iter().zip(self.live).map(|(counts,ticks)|counts.map(|n|n as f64*1800.0/ticks.max(1) as f64)).collect::<Vec<_>>();
        println!("leadups live_cohort_ticks={:?} category_cohorts={:?} deaths_per_live_snake_min={:?} max_length={} first_length_ticks={:?}",self.live,self.deaths,rates,self.max_length,self.first_lengths);
    }
}
fn cohort(n:usize)->usize {if n<40 {0} else if n<100 {1} else {2}}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn newborn_and_uncalled_deaths_cannot_inherit_a_contest_or_forecast() {
        let w=World::new(Config::default()).unwrap();
        let prefix=format!("/tmp/snakes-leadups-{}",std::process::id());
        let mut leadups=Leadups::new(&prefix);
        let mut obs=[DesktopObservation::default();MAX_SNAKES];
        let mut called=[false;MAX_SNAKES];
        obs[0]=DesktopObservation {generation:1,safe_ticks:72,contested:true,continuation:true,..Default::default()};
        for (generation,was_called) in [(2,false),(2,true),(1,false)] {
            leadups.ring[0].push_back(Decision {tick:10,generation:1,safe:72,continuation:true,unresolved:false,first:true,engaged:true,prey:0,prey_generation:0,cutoff:false,boosted:false,power:0});
            called[0]=was_called;
            leadups.record_death(&w,&CollisionEvent {tick:11,victim:0,generation,victim_length:20,reason:DeathReason::Body,..Default::default()},&obs,&called);
        }
        assert_eq!(leadups.deaths[0],[0,0,0,0,3]);
        assert_eq!((leadups.contest,leadups.non_contest,leadups.stupid),(0,3,0));
        leadups.out.flush().unwrap();drop(leadups);
        let output=std::fs::read_to_string(format!("{prefix}.deaths.csv")).unwrap();
        assert_eq!(output.matches("no_current_decision").count(),3);
        std::fs::remove_file(format!("{prefix}.deaths.csv")).unwrap();
    }
}
