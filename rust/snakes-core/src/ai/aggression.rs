// SPDX-License-Identifier: GPL-3.0-or-later
//! Utility/admission policy only. Every proposed move uses the same physical
//! rollout; risk tolerance never relaxes wall, self, expiry or first-step checks.
use super::*;
#[inline]
pub(super) fn level(w:&World)->f64 {
    if w.config().rules==crate::RuleSet::V2 {w.config().aggression as f64/100.0} else {0.5}
}
#[inline]
pub(super) fn bold(w:&World)->f64 {(2.0*level(w)-1.0).max(0.0)}
#[inline]
pub(super) fn planner_view<'a>(w:&World,mut s:SnakeView<'a>)->SnakeView<'a> {
    if w.config().rules==crate::RuleSet::V2 {
        let a=level(w);
        s.traits.aggression=if a<=0.5 {s.traits.aggression*(2.0*a)}
            else {s.traits.aggression+(1.0-s.traits.aggression)*(2.0*a-1.0)};
    }
    s
}
impl AiController {
    /// Existing-body barrier: propose closing the mouth of a small pocket.
    /// Two bounded 64-cell searches, only for a nearby giant encounter. Capped
    /// results mean unresolved. This is attack utility, never own safety.
    pub(super) fn barrier_goal(&mut self,w:&World,s:SnakeView<'_>,prey:usize)->Option<Point> {
        if bold(w)==0.0 || s.segments.len()<150 {return None;}
        let victim=w.snake(prey)?;
        if s.segments.len()<victim.segments.len()*3 {return None;}
        let head=s.segments[0].current;let center=victim.segments[0].current;
        if w.distance_squared(head,center)>(24.0*s.radius).powi(2) {return None;}
        // Look for an old arm beyond the rival, whose tail will outlast closure.
        let (speed,turn)=self.motion[s.id as usize].at(0);
        let mut endpoint=None;let mut best=f64::INFINITY;
        for k in 1..=24 {
            let index=10+k*s.segments.len().saturating_sub(30)/25;
            if index>=s.segments.len() {continue;}
            let p=s.segments[index].current;
            let d=w.displacement(head,p);let v=w.displacement(head,center);
            let length=(d.x*d.x+d.y*d.y).sqrt();
            if length<4.0*s.radius || length>speed*2.0 || v.x*d.x+v.y*d.y<=0.0 {continue;}
            if w.segment_distance_squared(center,head,p)>(6.0*s.radius).powi(2) {continue;}
            if normalize_angle(d.y.atan2(d.x)-s.angle).abs()>0.8 {continue;}
            if (s.segments.len()-index) as f64*s.radius*1.18<length+speed*0.6 {continue;}
            // Turn before the old arm, keeping an outward exit. The full
            // curved/deposited-neck rollout decides whether it can be driven.
            let gap=(speed/turn.max(0.01)*0.6).max(3.0*s.radius);
            let q=w.canonical_point(Point{x:p.x-d.x/length*gap,y:p.y-d.y/length*gap});
            if length<best {best=length;endpoint=Some(q);}
        }
        let goal=endpoint?;
        let (before,unresolved)=self.spatial.space(center,1<<s.id,64,0.0);
        // An already closed pocket has no mouth to close.
        if !unresolved && before<8 {return None;}
        let d=w.displacement(head,goal);
        let path:[Point;33]=std::array::from_fn(|k|w.canonical_point(Point{x:head.x+d.x*k as f64/32.0,y:head.y+d.y*k as f64/32.0}));
        let (after,capped)=self.spatial.trajectory_space(center,1<<s.id,64,0.0,&path,0,usize::MAX-1);
        if capped || after>=before || after>48 {return None;}
        Some(goal)
    }
    #[cfg(feature="desktop-diag")]
    pub(super) fn contested_target(&self,w:&World,s:SnakeView<'_>,state:State)->bool {
        let Some(f)=self.target_food(state).filter(|f|f.vacuum_owner<0) else {return false;};
        let mine=self.target_arrival(w,s,f);
        if mine>5.0 {return false;}
        self.rivals.iter().enumerate().any(|(id,r)|id!=s.id as usize && r.alive && {
            let other=w.snake(id).unwrap();
            if !other.alive || self.states[id].generation!=other.generation {return false;}
            let eta=self.target_arrival(w,other,f);
            eta<=5.0 && eta>=mine*0.5 && eta<=mine*2.0
                && (self.states[id].target==state.target || eta<2.0)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Config,RuleSet};
    fn crossing(rules:RuleSet,aggression:u8)->World {
        World::diagnostic_arena(Config {width:1600.0,height:1000.0,density:0.0,
            scale:70.0,speed:100.0,intelligence:100.0,self_collisions:true,
            rules,aggression,..Config::default()}, &[
            (Point{x:800.0,y:450.0},std::f64::consts::FRAC_PI_2,72,0.1),
            (Point{x:870.0,y:525.0},std::f64::consts::PI,24,0.6),
        ],&[]).unwrap()
    }
    #[test]
    fn aggression_changes_interest_without_mutating_traits_or_mechanics() {
        let mut w=crossing(RuleSet::V2,0);
        let original=w.snake(0).unwrap().traits.aggression;
        assert_eq!(planner_view(&w,w.snake(0).unwrap()).traits.aggression,0.0);
        assert_eq!(w.snake(0).unwrap().traits.aggression,original);
        let speed=w.motion_limits(0,0.0).unwrap();
        for value in [50,75,100] {
            w.reconfigure(Config {aggression:value,..w.config()}).unwrap();
            assert_eq!(w.motion_limits(0,0.0).unwrap(),speed);
        }
        assert_eq!(planner_view(&w,w.snake(0).unwrap()).traits.aggression,1.0);
    }
    #[test]
    fn live_aggression_change_cancels_old_attack_commitment() {
        let mut w=crossing(RuleSet::V2,100);let mut ai=AiController::new();
        ai.steer(&w,w.snake(0).unwrap());
        ai.states[0].prey=2;ai.states[0].attack.valid=true;
        w.reconfigure(Config {aggression:0,..w.config()}).unwrap();
        ai.steer(&w,w.snake(0).unwrap());
        assert_eq!(ai.competition_debug(0).unwrap().prey,None);
        assert_eq!(ai.competition_debug(0).unwrap().attack_stage,0);
    }
    #[test]
    fn classic_ignores_aggression_over_a_physical_replay() {
        let mut low=crossing(RuleSet::Classic,0);let mut high=crossing(RuleSet::Classic,100);
        let mut a=AiController::new();let mut b=AiController::new();
        for _ in 0..360 {
            low.step(&mut a);high.step(&mut b);
            for (x,y) in low.snakes().zip(high.snakes()) {
                assert_eq!((x.alive,x.generation,x.angle,x.desired_angle,x.segments.len()),
                    (y.alive,y.generation,y.angle,y.desired_angle,y.segments.len()));
                for (p,q) in x.segments.iter().zip(y.segments) {assert_eq!(p.current,q.current);}
            }
            assert_eq!(low.rng_state(),high.rng_state());
        }
    }
    #[test]
    fn bold_snake_executes_a_boosted_cutoff_and_survives_the_exit() {
        let mut w=World::diagnostic_arena(Config {width:1600.0,height:1000.0,
            density:0.0,scale:70.0,speed:100.0,intelligence:100.0,
            self_collisions:true,rules:RuleSet::V2,aggression:100,..Config::default()}, &[
            (Point{x:400.0,y:400.0},0.0,72,0.1),
            (Point{x:430.0,y:350.0},1.57,24,0.6),
        ],&[]).unwrap();
        let mut ai=AiController::new();let mut cutoffs=0;let mut bursts=0;
        for _ in 0..60 {
            if !w.snake(0).unwrap().alive || !w.snake(1).unwrap().alive {break;}
            let input=ai.steer(&w,w.snake(0).unwrap());
            cutoffs+=usize::from(ai.competition_debug(0).unwrap().attack_stage>0);
            bursts+=usize::from(input.rush>0.0);
            let mut limited=crate::controller::ScriptedController::new(|_,s:SnakeView<'_>| {
                if s.id==0 {input} else {Steering {desired_angle:s.angle,rush:0.0}}
            });
            w.step(&mut limited);
        }
        assert!(cutoffs>0 && bursts>0,"cutoffs={cutoffs} bursts={bursts}");
        assert!(w.snake(0).unwrap().alive);
        assert_eq!(w.stats().self_deaths,0);assert_eq!(w.stats().wall_deaths,0);
        assert_eq!(w.stats().deaths,1,"the committed crossing kills the limited rival");
    }

}
