// SPDX-License-Identifier: GPL-3.0-or-later
//! Frost-specific valuation, evacuation and prey choice. Safety and staged
//! cutoffs remain owned by the ordinary rollout; no shared targeting rewrite.
use super::*;
use crate::effects::frost::{freeze_eligible,FROZEN_TICKS,THAW_IMMUNITY_TICKS};
pub(crate) fn item_bonus(w:&World,s:SnakeView<'_>,p:Point)->f64 {
    let head=s.segments[0].current;let d=w.displacement(head,p);
    let (speed,turn)=w.motion_limits(s.id as usize,0.0).unwrap();
    let eta=(d.x.hypot(d.y)/speed.max(1.0)+normalize_angle(d.y.atan2(d.x)-s.angle).abs()/turn.max(0.01)).max(0.0);
    value_at(w,s,p,eta)
}
// Arrival is supplied by capsule scoring, so steering and paid bursts use
// the same contact ETA as ordinary races.
pub(super) fn value_at(w:&World,s:SnakeView<'_>,p:Point,eta:f64)->f64 {
    // Pickup is at the first contact endpoint, before reaching the capsule
    // centre. Value Nova around that approach-side head, with one movement
    // of reserve for steering/contact discretisation.
    let approach=w.displacement(s.segments[0].current,p);let distance=approach.x.hypot(approach.y);
    let reach=1.3*s.radius+2.1*w.config().base_radius();
    let center=if distance>reach {w.canonical_point(Point{x:p.x-approach.x/distance*reach,y:p.y-approach.y/distance*reach})} else {s.segments[0].current};
    let steps=crate::effects::forecast_step(eta);
    let elapsed=steps.saturating_sub(1);
    let mut bonus=0.0;
    for r in w.snakes().filter(|r|r.alive && r.id!=s.id) {
        let h=r.segments[0].current;
        let mut boundaries=[eta,r.boost_ticks as f64*STEP_SECONDS,r.effect_ticks as f64*STEP_SECONDS,r.face.frozen_ticks as f64*STEP_SECONDS];
        boundaries.sort_unstable_by(f64::total_cmp);
        let mut time=0.0;let mut travel=0.0;
        for end in boundaries {
            let end=end.min(eta);if end<=time {continue;}
            let offset=(time/STEP_SECONDS+1e-7).floor() as usize;
            let (speed,_)=crate::effects::forecast_motion(w,r.id as usize,0.0,offset);
            travel+=speed*(end-time);time=end;
        }
        let point=w.canonical_point(Point{x:h.x+r.angle.cos()*travel,y:h.y+r.angle.sin()*travel});
        let frozen=r.face.frozen_ticks.saturating_sub(elapsed.min(u16::MAX as usize) as u16);
        let immunity=if r.face.frozen_ticks>0 {if elapsed<r.face.frozen_ticks as usize {0} else {
            THAW_IMMUNITY_TICKS.saturating_sub((elapsed-r.face.frozen_ticks as usize).min(u16::MAX as usize) as u16)}}
            else {r.face.thaw_immunity_ticks.saturating_sub(elapsed.min(u16::MAX as usize) as u16)};
        if freeze_eligible(true,false,frozen,immunity,w.distance_squared(point,center),w.config().base_radius()) {bonus+=8.0;}
    }
    bonus
}
pub(super) fn extended_cutoff_window(w:&World,prey:usize)->bool {
    w.snake(prey).is_some_and(|p|p.alive && p.face.frozen_ticks>24)
}
// The exact pickup rollout, not a goal heuristic, determines Nova utility.
// Wasting a capsule must lose to an equally safe bypass; safety admission
// and unavoidable emergency choices retain their ordinary ordering.
pub(super) fn rollout_value(w:&World,effects:&forecast::Timeline,id:usize)->f64 {
    let level=aggression::level(w);
    effects.frost_hits[id] as f64*(320.0+480.0*level)
        -if effects.frost_empty&(1<<id)!=0 {700.0+400.0*level} else {0.0}
}
impl AiController {
    // Capsule scoring reuses the already-integrated rival rows. Do not build
    // another motion forecast for every valuation or extrapolate offensive
    // certainty beyond the shared safety horizon.
    pub(super) fn frost_value_at(&self,w:&World,s:SnakeView<'_>,p:Point,eta:f64)->f64 {
        let step=crate::effects::forecast_step(eta);
        if step>STEPS {return 0.0;}
        let approach=w.displacement(s.segments[0].current,p);let distance=approach.x.hypot(approach.y);
        let reach=1.3*s.radius+2.1*w.config().base_radius();
        let center=if distance>reach {w.canonical_point(Point{x:p.x-approach.x/distance*reach,y:p.y-approach.y/distance*reach})} else {
            let speed=self.motion[s.id as usize].at(0).0;
            w.canonical_point(Point{x:s.segments[0].current.x+s.angle.cos()*speed*STEP_SECONDS,y:s.segments[0].current.y+s.angle.sin()*speed*STEP_SECONDS})
        };
        let elapsed=step-1;let mut value=0.0;
        for r in w.snakes().filter(|r|r.alive && r.id!=s.id) {
            let frozen=r.face.frozen_ticks.saturating_sub(elapsed.min(u16::MAX as usize) as u16);
            let immunity=if r.face.frozen_ticks>0 {
                if elapsed<r.face.frozen_ticks as usize {0} else {
                    THAW_IMMUNITY_TICKS.saturating_sub((elapsed-r.face.frozen_ticks as usize).min(u16::MAX as usize) as u16)
                }
            } else {r.face.thaw_immunity_ticks.saturating_sub(elapsed.min(u16::MAX as usize) as u16)};
            // Eligibility belongs to the arrival endpoint. A narrower
            // current-head gate drops both the outer Nova ring and rivals
            // approaching it during the already-integrated forecast.
            if freeze_eligible(true,false,frozen,immunity,
                w.distance_squared(self.opportunity_rival(r.id as usize).path[step],center),w.config().base_radius()) {value+=8.0;}
        }
        value
    }
    pub(super) fn frost_tactics(&self,w:&World,s:SnakeView<'_>,state:&mut State)->bool {
        if w.config().rules!=crate::RuleSet::V2 || !w.config().power_ups {return false;}
        let head=s.segments[0].current;let r=w.config().base_radius();
        // Move beyond the Nova radius before a closer rival reaches the capsule.
        for (slot,item) in w.items().enumerate().filter(|(_,i)|i.kind==crate::effects::EffectKind::Frost) {
            let distance=w.distance_squared(head,item.position).sqrt();
            if distance>=18.0*r {continue;}
            let food=self.food[MAX_FOOD+slot].unwrap();
            let own=self.target_arrival(w,s,food);
            let own_step=crate::effects::forecast_step(own);
            let (speed,turn)=self.motion[s.id as usize].at(0);
            let gap=(speed/turn.max(0.01)+1.3*s.radius+2.1*r+2.0*r).max(8.0*r);
            let stores=w.config().store_power_ups && self.opportunities.stores_at(w,s.id as usize,own_step,slot);
            let useful=stores || self.frost_value_at(w,s,item.position,own)>0.0;
            // Do not spend an instant offensive capsule on an empty Nova.
            // Keep a small approach gap until a susceptible rival arrives;
            // its next observation can immediately reopen the race.
            if distance<gap && item.pickup_eligible(w.tick()+1,0,true,s.face.guarding && !w.config().store_power_ups,s.effect_ticks) && !useful {
                let d=w.displacement(item.position,head);let a=if distance>0.01 {d.y.atan2(d.x)} else {s.angle};
                state.suspend_objective();state.set_target(0);state.prey=0;
                state.clear_coil(s.angle);state.clear_attacks(s.angle);
                state.goal=w.canonical_point(Point{x:item.position.x+a.cos()*(gap+2.0*r),y:item.position.y+a.sin()*(gap+2.0*r)});
                state.waypoint=None;state.track_goal=true;state.dodge_until=w.tick()+6;
                state.commit_until=0;state.turn_until=u64::MAX;state.desired=a;state.exit_angle=a;
                return true;
            }
            let danger=s.face.frozen_ticks==0 && s.face.thaw_immunity_ticks==0 && w.snakes().any(|other| {
                if !other.alive || other.id==s.id
                    || w.distance_squared(other.segments[0].current,item.position)>=distance*distance {return false;}
                let eta=self.target_arrival(w,other,food);
                if !eta.is_finite() || eta>=own {return false;}
                let step=crate::effects::forecast_step(eta);
                ! (w.config().store_power_ups && self.opportunities.stores_at(w,other.id as usize,step,slot))
                    && item.pickup_eligible(w.tick()+step as u64,step-1,true,other.face.guarding && !w.config().store_power_ups,
                    other.effect_ticks.saturating_sub((step-1).min(u16::MAX as usize) as u16))
            });
            if !danger {
                // Near a useful Nova, acquire the actual capsule objective.
                // This is an offensive approach, evaluated by ordinary safety
                // rollouts, rather than an incidental overlap while hunting.
                if distance<gap && useful && item.pickup_eligible(w.tick()+own_step as u64,own_step-1,true,s.face.guarding && !w.config().store_power_ups,
                    s.effect_ticks.saturating_sub((own_step-1).min(u16::MAX as usize) as u16)) {
                    let target=item.id|target::ITEM_BIT;
                    if state.target!=target {state.suspend_objective();}
                    state.prey=0;state.clear_coil(s.angle);state.clear_attacks(s.angle);
                    // A per-movement acquisition bypasses strategy's progress
                    // reset. Its stall clock and distance belong to this capsule,
                    // while a refresh must retain progress and its checked route.
                    if state.target!=target {state.best_distance=f64::MAX;state.last_progress=w.tick();}
                    state.set_target(target);state.target_index=MAX_FOOD+slot;state.goal=item.position;
                    state.track_goal=true;state.dodge_until=0;
                    return true;
                }
                continue;
            }
            let d=w.displacement(item.position,head);let a=if distance>0.01 {d.y.atan2(d.x)} else {s.angle};
            state.goal=w.canonical_point(Point{x:item.position.x+a.cos()*19.0*r,y:item.position.y+a.sin()*19.0*r});
            state.suspend_objective();state.set_target(0);state.prey=0;state.guarding=false;state.vulturing=false;
            state.clear_coil(s.angle);state.clear_attacks(s.angle);state.waypoint=None;state.track_goal=true;state.dodge_until=w.tick()+12;
            state.commit_until=0;state.turn_until=u64::MAX;state.desired=a;state.exit_angle=a;
            return true;
        }
        if s.traits.aggression<0.2 || s.face.frozen_ticks>0 || s.effect_kind==4 {return false;} // Venom keeps its rear-quarter plan.
        let mut best=None;let mut value=f64::MAX;
        for prey in w.snakes().filter(|p|p.alive && p.id!=s.id && p.face.frozen_ticks>6 && p.flags&crate::flags::PHASED==0) {
            let rival=self.opportunity_rival(prey.id as usize);
            let eta=self.arrival(w,&self.rivals[s.id as usize],rival.path[36]);
            if eta>prey.face.frozen_ticks as f64*STEP_SECONDS || eta>FROZEN_TICKS as f64*STEP_SECONDS {continue;}
            let score=eta*if state.prey==prey.id as usize+1 && state.prey_generation==prey.generation {0.8} else {1.0};
            if score<value {value=score;best=Some(prey);}
        }
        let Some(prey)=best else {return false;};
        if state.prey!=prey.id as usize+1 || state.prey_generation!=prey.generation {
            state.clear_attacks(s.angle);state.clear_coil(s.angle);
        }
        state.prey=prey.id as usize+1;state.prey_generation=prey.generation;
        state.hunt_until=w.tick()+prey.face.frozen_ticks as u64;
        state.goal=self.opportunity_rival(prey.id as usize).path[36];state.suspend_objective();state.set_target(0);
        state.waypoint=None;state.track_goal=true;state.guarding=false;state.vulturing=false;state.debug.flags|=8;
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Config,RuleSet,Item,effects::EffectKind};
    fn arena()->World {World::diagnostic_arena(Config {store_power_ups:false,width:1600.0,height:1000.0,density:0.0,
        speed:300.0,scale:200.0,intelligence:100.0,rules:RuleSet::V2,self_collisions:false,..Default::default()},
        &[(Point{x:450.0,y:400.0},0.0,64,1.0),(Point{x:600.0,y:480.0},-std::f64::consts::FRAC_PI_2,24,0.0)],&[]).unwrap()}
    fn refresh_arena()->World {
        let config=Config {store_power_ups:false,density:27.0,..arena().config()};
        let mut w=World::diagnostic_arena(config,&[
            (Point{x:450.0,y:400.0},0.0,64,1.0),(Point{x:600.0,y:480.0},-std::f64::consts::FRAC_PI_2,24,0.0),
            (Point{x:1000.0,y:800.0},0.0,24,0.0),(Point{x:1100.0,y:800.0},0.0,24,0.0),
            (Point{x:1200.0,y:800.0},0.0,24,0.0),(Point{x:1300.0,y:800.0},0.0,24,0.0)],&[]).unwrap();
        for id in 2..w.snake_count() {w.snakes[id].alive=false;w.snakes[id].respawn=100.0;}
        w
    }
    #[test]
    fn frost_refresh_discards_old_route_through_world_step() {
        let mut w=refresh_arena();w.faces[1].thaw_immunity_ticks=45;
        let p=Point{x:500.0,y:400.0};
        w.items.push(Item {id:42,kind:EffectKind::Frost,position:p,
            life_ticks:750,radius:37.8,..Default::default()});
        let mut ai=AiController::new();ai.prepare(&w);
        // Six live slots put snake 0 outside the strategy quota on tick 1.
        for id in 2..w.snake_count() {w.snakes[id].alive=false;w.snakes[id].respawn=100.0;}
        w.tick=1;
        ai.states[0]=State {generation:w.snakes[0].generation,waypoint:Some(Point{x:1200.0,y:400.0}),
            turn_until:u64::MAX,best_distance:f64::MAX,..Default::default()};
        struct CheckRefresh {ai:AiController}
        impl Controller for CheckRefresh {
            fn delegate(&self,_id:u32)->Option<&dyn Controller> {Some(&self.ai)}
            fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
                if s.id==0 {
                    self.ai.prepare(w);
                    let mut state=self.ai.states[0];
                    assert!(self.ai.frost_tactics(w,s,&mut state));
                    assert_eq!(state.waypoint,None,"refresh must clear the route before candidate selection");
                    assert_eq!(AiController::trajectory_goal(w,state,s.segments[0].current),state.goal);
                    self.ai.states[0]=state;
                }
                self.ai.steer(w,s)
            }
        }
        let mut check=CheckRefresh {ai};
        w.step(&mut check);let ai=check.ai;
        assert!(ai.states[0].dodge_until>w.tick());
        assert_eq!(ai.states[0].waypoint,None);
        assert_eq!(AiController::trajectory_goal(&w,ai.states[0],w.segments[0].current),ai.states[0].goal);
    }
    #[test]
    fn active_capsules_and_prizes_defer_resume_through_world_step() {
        for kind in [EffectKind::Frost,EffectKind::Surge,EffectKind::Phase,EffectKind::Magnet,EffectKind::Venom,EffectKind::None] {
            let mut w=refresh_arena();
            for id in 1..w.snake_count() {w.snakes[id].alive=false;w.snakes[id].respawn=100.0;}
            let p=Point{x:1250.0,y:400.0}; // Beyond the per-tick Frost refresh radius.
            w.food.push(crate::world::Food {id:99,p:Point{x:450.0,y:700.0},life:100.0,value:1.0,owner:-1,..Default::default()});
            let (target,index)=if kind==EffectKind::None {
                w.food.push(crate::world::Food {id:42,p,life:100.0,value:9.0,owner:-1,kind:crate::FoodKind::Prism,..Default::default()});(42,1)
            } else {
                w.items.push(Item {id:42,kind,position:p,life_ticks:750,radius:37.8,..Default::default()});(42|target::ITEM_BIT,MAX_FOOD)
            };
            let mut ai=AiController::new();ai.prepare(&w);w.tick=1;ai.states[0]=State {generation:w.snakes[0].generation,
                target,target_index:index,suspended_target:99,suspended_index:0,goal:p,
                track_goal:true,best_distance:f64::MAX,turn_until:u64::MAX,..Default::default()};
            w.step(&mut ai);
            assert_eq!(ai.states[0].target,target,"kind={kind:?}");
            assert_eq!(ai.states[0].suspended_target,99,"kind={kind:?}");
        }
    }
    #[test]
    fn offensive_capsule_value_scales_with_aggression_and_requires_a_victim() {
        let mut w=arena();let mut ai=AiController::new();ai.prepare(&w);
        let item=Item {kind:EffectKind::Frost,position:w.segments[0].current,
            life_ticks:750,radius:40.0,..Default::default()};
        let mut values=[0.0;3];
        for (i,level) in [0,50,100].into_iter().enumerate() {
            w.config.aggression=level;values[i]=ai.capsule_value(&w,w.snake(0).unwrap(),&item,0.0);
        }
        assert!(values[0]<values[1] && values[1]<values[2]);
        w.faces[1].thaw_immunity_ticks=45;
        assert_eq!(ai.capsule_value(&w,w.snake(0).unwrap(),&item,0.0),0.0);
        w.faces[1].thaw_immunity_ticks=0;w.snakes[1].alive=false;
        assert_eq!(ai.capsule_value(&w,w.snake(0).unwrap(),&item,0.0),0.0);
    }
    #[test]
    fn frost_values_outer_ring_and_arriving_victims_through_world_step() {
        for (steps,gap) in [(1usize,15.0),(5,17.0)] {
            let mut w=arena();let head=w.segments[0].current;
            let travel=w.motion_limits(0,0.0).unwrap().0*STEP_SECONDS;
            let reach=1.3*w.snakes[0].radius+2.1*w.config.base_radius();
            let center=Point{x:head.x+travel*steps as f64,y:head.y};
            let prey=Point{x:center.x+gap*w.config.base_radius(),y:center.y};
            w.diagnostic_body(1,&[prey;24],std::f64::consts::PI).unwrap();
            let item=Item {id:42,kind:EffectKind::Frost,
                position:Point{x:center.x+reach-0.01,y:center.y},
                life_ticks:750,radius:2.1*w.config.base_radius(),..Default::default()};
            w.items.push(item);let mut ai=AiController::new();ai.prepare(&w);
            let eta=steps as f64*STEP_SECONDS;
            assert_eq!(ai.frost_value_at(&w,w.snake(0).unwrap(),item.position,eta),8.0,
                "an eligible arrival must survive the current-head prefilter: steps={steps}");
            w.step_n(&mut crate::controller::ScriptedController::new(|_,s:SnakeView<'_>|
                Steering {desired_angle:s.angle,rush:0.0}),steps as u32);
            assert_eq!(w.faces[1].frozen_ticks,FROZEN_TICKS,"steps={steps}");
            assert!(w.items.is_empty());
        }
    }
    #[test]
    fn empty_nova_is_avoided_even_by_a_thaw_immune_holder() {
        let mut w=arena();w.faces[1].thaw_immunity_ticks=45;
        let p=Point{x:500.0,y:400.0};
        w.items.push(Item {id:42,kind:EffectKind::Frost,position:p,
            life_ticks:750,radius:37.8,..Default::default()});
        w.faces[0].thaw_immunity_ticks=45;
        let mut ai=AiController::new();ai.prepare(&w);
        let mut state=State {target:99,target_index:3,..Default::default()};
        assert!(ai.frost_tactics(&w,w.snake(0).unwrap(),&mut state));
        assert_eq!((state.target,state.suspended_target),(0,99));
        assert!(w.distance_squared(state.goal,p)>(8.0*w.config.base_radius()).powi(2));
    }
    #[test]
    fn frost_uses_contact_arrival_and_explicitly_acquires_a_useful_nova() {
        let mut w=arena();let p=Point{x:500.0,y:400.0};
        w.items.push(Item {id:42,kind:EffectKind::Frost,position:p,
            life_ticks:750,radius:37.8,..Default::default()});
        let mut ai=AiController::new();ai.prepare(&w);
        // Centre ETA is deliberately misleading once the head reaches contact.
        ai.capsule_etas[0][0]=10.0;
        let mut state=State {target:99,target_index:3,..Default::default()};
        assert!(ai.frost_tactics(&w,w.snake(0).unwrap(),&mut state));
        assert_eq!(state.target,42|target::ITEM_BIT);
        assert_eq!((state.suspended_target,state.suspended_index),(99,3));
        assert_eq!(state.goal,p);assert!(state.track_goal);
        let candidate=Candidate {area:0,uncertain:false,..Default::default()};
        state.set_target(0);
        State::commit_candidate(&mut state,&candidate,0,&w,w.snake(0).unwrap(),1.0,24);
        assert_eq!((state.suspended_target,state.suspended_index),(99,3),"nested escape must preserve Frost's suspended objective");
    }
    #[test]
    fn frost_acquisition_owns_its_progress_and_retains_only_its_route() {
        let mut w=refresh_arena();let p=Point{x:500.0,y:400.0};w.tick=100;
        w.items.push(Item {id:42,kind:EffectKind::Frost,position:p,
            life_ticks:750,radius:37.8,..Default::default()});
        let mut ai=AiController::new();ai.prepare(&w);
        let route=Some(Point{x:500.0,y:470.0});let target=42|target::ITEM_BIT;
        for old in [target,99] {
            let mut state=State {target:old,target_index:MAX_FOOD,waypoint:route,
                track_goal:true,goal:p,last_progress:1,best_distance:0.0,..Default::default()};
            assert!(ai.frost_tactics(&w,w.snake(0).unwrap(),&mut state));
            assert_eq!(state.target,target);
            if old==target {
                assert_eq!((state.last_progress,state.best_distance),(1,0.0),
                    "refresh must not disguise a genuinely stalled capsule");
                assert_eq!(state.waypoint,route);
            } else {
                assert_eq!((state.last_progress,state.best_distance),(100,f64::MAX),
                    "new capsule must not inherit an already expired food stall clock");
                assert_eq!(state.waypoint,None);
            }
        }
    }
    #[test]
    fn exact_nova_rollout_rewards_hits_and_rejects_empty_consumption() {
        let mut w=arena();let mut effects=forecast::Timeline::new(&w);
        let center=w.segments[0].current;
        effects.nova(&w,0,center,1,3,|id|w.segments[id*MAX_SEGMENTS].current);
        assert_eq!(effects.frost_hits[0],1);assert_eq!(effects.frost_empty,0);
        assert!(rollout_value(&w,&effects,0)>0.0);
        w.faces[1].thaw_immunity_ticks=45;
        let mut empty=forecast::Timeline::new(&w);
        empty.nova(&w,0,center,1,3,|id|w.segments[id*MAX_SEGMENTS].current);
        assert_eq!(empty.frost_hits[0],0);assert_eq!(empty.frost_empty,1);
        assert!(rollout_value(&w,&empty,0)<0.0);
        let high=rollout_value(&w,&effects,0);w.config.aggression=0;
        assert!(rollout_value(&w,&effects,0)<high);
    }
    #[test]
    fn nova_forecast_preserves_effect_and_stops_burst_at_pickup_endpoint() {
        for kind in [EffectKind::None,EffectKind::Surge,EffectKind::Magnet,EffectKind::Phase,EffectKind::Venom] {
            let mut w=arena();w.config.deadly_walls=false;
            w.snakes[1].effect_kind=kind as u8;w.snakes[1].effect_ticks=kind.duration();w.snakes[1].boost_ticks=24;w.snakes[1].rush=0.6;
            let item=Item {kind:EffectKind::Frost,position:w.segments[0].current,life_ticks:750,radius:40.0,..Default::default()};
            let mut effects=forecast::Timeline::new(&w);
            effects.pickup(0,item,1,0);effects.nova(&w,0,w.segments[0].current,1,3,|id|w.segments[id*MAX_SEGMENTS].current);
            let motion=Motion::forecast(&w,1,0.0);let f=forecast::Forecast::empty(effects);
            let before=f.motion(&w,1,0.0,&motion,1);
            let source=w.diagnostic_snapshot();
            crate::effects::activate(EffectKind::Frost,&mut w,0);
            // The isolated motion oracle has no future collisions or nutrition.
            // Remove the Nova owner after activation so its advancing trail
            // cannot kill the perpendicular victim during the real steps.
            w.snakes[0].alive=false;w.snakes[0].len=0;w.snakes[0].respawn=1e9;
            w.food.resize(w.config().food_count(),crate::world::Food {p:Point{x:1400.0,y:900.0},
                life:1000.0,owner:-1,value:0.0,..Default::default()});
            assert_eq!(f.effects.at(0,1).ticks,0);assert_eq!(f.effects.at(1,1).kind,kind as u8);
            assert_ne!(before,w.motion_limits(1,0.0).unwrap());
            for step in 2..=95 {
                w.test_frost_tick();
                assert_eq!(f.motion(&source,1,0.0,&motion,step),w.motion_limits(1,0.0).unwrap(),"kind={kind:?} step={step}");
            }
        }
    }
    #[test]
    fn frost_value_observes_endpoint_immunity_and_independent_thaw_limits() {
        let mut w=arena();let p=w.segments[0].current;
        assert_eq!(item_bonus(&w,w.snake(0).unwrap(),p),8.0);
        w.faces[1].thaw_immunity_ticks=1;
        assert_eq!(item_bonus(&w,w.snake(0).unwrap(),p),0.0,"movement-one immunity has already decremented");
        w.faces[1].thaw_immunity_ticks=0;w.faces[1].frozen_ticks=75;w.snakes[1].frozen_ticks=75;
        w.snakes[1].effect_kind=1;w.snakes[1].effect_ticks=120;
        let motion=Motion::forecast(&w,1,0.0);
        for offset in 0..138 {assert_eq!(motion.at(offset),crate::effects::forecast_motion(&w,1,0.0,offset),"offset={offset}");}
    }
    #[test]
    fn frost_avoidance_honors_guards_immunity_and_target_identity() {
        let mut w=arena();let p=w.segments[MAX_SEGMENTS].current;
        w.items.push(Item {id:1,kind:EffectKind::Frost,position:p,life_ticks:750,radius:31.5,..Default::default()});
        let mut ai=AiController::new();ai.prepare(&w);let mut state=State {target:123,target_index:7,..Default::default()};
        ai.capsule_etas[0][0]=1.0;ai.capsule_etas[0][1]=0.0;
        assert!(ai.frost_tactics(&w,w.snake(0).unwrap(),&mut state));
        assert_eq!((state.suspended_target,state.suspended_index),(123,7));
        assert!(w.distance_squared(state.goal,p)>(16.0*w.config.base_radius()).powi(2));
        w.faces[1].guarding=true;w.snakes[1].effect_kind=4;w.snakes[1].effect_ticks=240;
        assert!(!ai.frost_tactics(&w,w.snake(0).unwrap(),&mut state));
        w.faces[1].guarding=false;w.faces[0].thaw_immunity_ticks=45;
        assert!(!ai.frost_tactics(&w,w.snake(0).unwrap(),&mut state));
        w.items.clear();w.faces[0].thaw_immunity_ticks=0;
        w.snakes[1].frozen_ticks=75;w.faces[1].frozen_ticks=75;w.tick+=1;ai.prepare(&w);
        state.prey=2;state.prey_generation=0;state.attack.valid=true;state.coil_radius=10.0;
        assert!(ai.frost_tactics(&w,w.snake(0).unwrap(),&mut state));
        assert_eq!(state.prey_generation,w.snakes[1].generation);
        assert!(!state.attack.valid);assert_eq!(state.coil_radius,0.0);
    }
    #[test]
    fn frozen_prey_duels_report_cutoff_success() {
        let mut kills=[0;2];let mut cuts=[0;2];let mut plans=[0;2];let runs=24;
        for frozen in [false,true] {for n in 0..runs {
            let mut w=World::diagnostic_arena(Config {store_power_ups:false,width:1600.0,height:1000.0,density:0.0,
                scale:70.0,speed:100.0,intelligence:100.0,self_collisions:true,
                rules:RuleSet::V2,aggression:100,..Config::default()}, &[
                (Point{x:400.0,y:400.0},0.0,72,0.1),
                (Point{x:550.0+6.0*n as f64,y:350.0},std::f64::consts::FRAC_PI_2,24,0.6),
            ],&[]).unwrap();
            let p=w.snakes().nth(1).unwrap().segments[0].current;
            let body:Vec<_>=(0..24).map(|j|Point{x:p.x,y:p.y-j as f64*w.snakes[1].radius*1.18}).collect();
            w.diagnostic_body(1,&body,std::f64::consts::FRAC_PI_2).unwrap();
            if frozen {w.snakes[1].frozen_ticks=75;w.faces[1].frozen_ticks=75;}
            // Frost policy is independent of spawning; the fixture grants the
            // state directly while keeping the normal power-up policy enabled.
            w.config.power_ups=true;
            let mut ai=AiController::new();
            let mut had_plan=false;let mut cut=false;
            for _ in 0..75 {
                let input=ai.steer(&w,w.snake(0).unwrap());
                let mut controlled=crate::controller::ScriptedController::new(|_,s:SnakeView<'_>| {
                    if s.id==0 {input} else {crate::controller::Steering {desired_angle:s.angle,rush:0.0}}
                });
                w.step(&mut controlled);
                had_plan|=ai.states[0].attack.valid || ai.states[0].attack_options.iter().any(|a|a.valid);
                cut|=w.collision_events().any(|e|e.victim==1 && e.reason==crate::DeathReason::Body && e.owner_mask&1!=0);
                if !w.snakes[1].alive || !w.snakes[0].alive {break;}
            }
            plans[usize::from(frozen)]+=usize::from(had_plan);
            cuts[usize::from(frozen)]+=usize::from(cut && w.snakes[0].alive);
            kills[usize::from(frozen)]+=usize::from(!w.snakes[1].alive && w.snakes[0].alive);
        }}
        println!("Frost duel suite: runs={runs} normal kills={} ({:.2}%) frozen-prey kills={} ({:.2}%) staged plans normal={} frozen={}",kills[0],100.0*kills[0] as f64/runs as f64,kills[1],100.0*kills[1] as f64/runs as f64,plans[0],plans[1]);
        println!("frozen-prey body cut-off success={}/{} ({:.2}%) vs unfrozen={}/{} ({:.2}%)",cuts[1],runs,100.0*cuts[1] as f64/runs as f64,cuts[0],runs,100.0*cuts[0] as f64/runs as f64);
        assert!(plans[1]>0,"frozen prey must reach the staged cutoff planner");
        assert!(cuts[1]>cuts[0],"freezing must create a successful body cutoff in this duel set");
    }
    #[test]
    fn frost_storage_tactics_match_world_acquisition() {
        for (rival_full,earlier_capsule) in [(false,false),(true,false),(true,true),(false,true)] {
            let mut w=arena();w.config.store_power_ups=true;
            w.segments[crate::MAX_SEGMENTS].current=Point{x:500.0,y:500.0};
            if rival_full || earlier_capsule {w.snakes[1].inventory=crate::Inventory {count:if earlier_capsule {2} else {3},kinds:[3;3],life:[1800;3],..Default::default()};}
            let other_capsule=Item {id:41,kind:EffectKind::Magnet,position:Point{x:500.0,y:500.0},radius:37.8,life_ticks:750,..Default::default()};
            if earlier_capsule && rival_full {w.items.push(other_capsule);}
            w.items.push(Item {id:42,kind:EffectKind::Frost,position:Point{x:500.0,y:500.0},radius:37.8,life_ticks:750,..Default::default()});
            if earlier_capsule && !rival_full {w.items.push(other_capsule);}
            struct Observe {ai:AiController,full:bool,checked:bool}
            impl Controller for Observe {
                fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
                    if s.id==0 {
                        self.ai.prepare(w);let mut state=State::default();
                        assert!(self.ai.frost_tactics(w,s,&mut state));
                        if self.full {assert!(state.dodge_until>w.tick());}
                        else {assert_eq!(state.dodge_until,0);assert_eq!(state.target,42|target::ITEM_BIT);}
                        self.checked=true;
                    }
                    Steering {desired_angle:s.angle,rush:0.0}
                }
            }
            let mut c=Observe {ai:AiController::new(),full:rival_full,checked:false};w.step(&mut c);
            assert!(c.checked);assert_eq!(w.snakes[0].frozen_ticks>0,rival_full);
            assert_eq!(w.snakes[1].inventory.count,if rival_full || earlier_capsule {3} else {1});
        }
        // Empty offensive Nova is still a useful stored acquisition with no rivals.
        let mut w=arena();w.config.store_power_ups=true;w.snakes[1].alive=false;
        w.items.push(Item {id:43,kind:EffectKind::Frost,position:Point{x:490.0,y:400.0},radius:37.8,life_ticks:750,..Default::default()});
        struct Acquire {ai:AiController}
        impl Controller for Acquire {
            fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
                self.ai.prepare(w);let mut state=State::default();
                assert!(self.ai.frost_tactics(w,s,&mut state));assert_eq!(state.target,43|target::ITEM_BIT);assert_eq!(state.dodge_until,0);
                Steering {desired_angle:0.0,rush:0.0}
            }
        }
        w.step(&mut Acquire {ai:AiController::new()});assert_eq!(w.snakes[0].inventory.kinds[0],5);
        assert_eq!(w.snakes[0].effect_ticks,0);
    }

}
