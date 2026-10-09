// SPDX-License-Identifier: GPL-3.0-or-later
//! Starfall is a strategic race objective in one extra fixed target slot.
//! It reuses capsule commitments, ETA/boost rules and certified crowd cutoffs.
use super::*;
pub(super) const SLOT:usize=MAX_FOOD+crate::MAX_ITEMS;
pub(super) const ID:u64=target::ITEM_BIT|crate::world::events::ZONE_ID;
impl AiController {
    pub(super) fn prepare_event_target(&mut self,w:&World) {
        let Some((position,radius,_))=w.starfall_target() else {return;};
        self.food[SLOT]=Some(target::TargetFood {id:ID,position,
            value:7.5,size:radius,vacuum_owner:-1,feast_id:0,kind:crate::FoodKind::Star,motion_ticks:0,item_kind:0});
    }
    pub(super) fn event_shortlist(&self,w:&World,s:SnakeView<'_>,state:&State,shortlist:&mut [(f64,usize);5]) {
        let Some(f)=self.food[SLOT] else {return;};
        if f.id==state.rejected && w.tick()<state.reject_until {return;}
        if w.starfall_landed()
            && w.distance_squared(s.segments[0].current,f.position)<=f.size*f.size {return;}
        let eta=self.target_arrival(w,s,f);
        if eta>(w.starfall_target().unwrap().2.saturating_sub(w.tick())) as f64*STEP_SECONDS {return;}
        let d=w.displacement(s.segments[0].current,f.position);
        let bearing=normalize_angle(d.y.atan2(d.x)-s.angle).abs();
        // Strategic units follow the existing prism's 5 => 90 normalization.
        Self::shortlist(shortlist,self.target_score(w,s,state,f,eta,bearing,7.5*18.0),SLOT);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zone_discovery_uses_race_commitment_and_meteors_are_never_forecast_pickups() {
        let mut w=World::diagnostic_arena(crate::Config {width:2000.0,height:1000.0,rules:crate::RuleSet::V2,
            intelligence:100.0,world_events:true,..crate::Config::default()},
            &[(Point{x:700.0,y:500.0},0.0,48,0.0),(Point{x:1300.0,y:500.0},std::f64::consts::PI,24,0.0)],&[]).unwrap();
        w.diagnostic_event_schedule(1,10000);
        w.step(&mut crate::controller::BaselineController);
        w.items.clear();w.food.clear();let position=w.starfall_target().unwrap().0;
        w.food.push(crate::world::Food {id:88,kind:crate::FoodKind::Meteor,p:Point{x:710.0,y:500.0},value:1.0,owner:-1,..Default::default()});
        let mut ai=AiController::new();ai.prepare(&w);
        assert!(ai.food[0].is_none());
        let mut state=State::default();ai.strategy(&w,w.snake(0).unwrap(),&mut state,1.0);
        assert_eq!(state.target,ID);assert_eq!(state.goal,position);
        ai.guard_capsule(&w,w.snake(0).unwrap(),&mut state);
        assert!(!state.guarding,"a feeding disk is never a held capsule orbit");
        ai.capsule_etas[crate::MAX_ITEMS]=[1.0;MAX_SNAKES];
        ai.capsule_etas[crate::MAX_ITEMS][0]=4.0;
        let empty=State::default();
        assert!(ai.target_score(&w,w.snake(0).unwrap(),&empty,ai.food[SLOT].unwrap(),4.0,0.0,126.0)>0.0,
            "a trailing head can still race for one of 24 stars");
        assert!(!target::Contact::forecast(w.snake(0).unwrap(),ai.food[SLOT].unwrap(),ai.opportunities.track(0)).reached(0.0,24));
        w.reconfigure(crate::Config {world_events:false,..w.config()}).unwrap();ai.tick=u64::MAX;ai.prepare(&w);ai.advance_race(&w,w.snake(0).unwrap(),&mut state);
        assert_eq!(state.target,0);
        state.generation=w.snake(0).unwrap().generation;
        state.suspended_target=ID;state.suspended_index=SLOT;
        ai.states[0]=state;
        let _=ai.steer(&w,w.snake(0).unwrap());
        assert_ne!(ai.states[0].target,ID,"expired suspended zones cannot resume");
        assert_eq!(ai.states[0].suspended_target,0);
    }
    #[test]
    fn night_invalidates_cached_physical_motion_and_hunters_keep_their_intent() {
        let mut w=World::new(crate::Config {rules:crate::RuleSet::V2,..Default::default()}).unwrap();
        let key=MotionKey::observed(&w,0);let speed=w.motion_limits(0,0.0).unwrap().0;
        w.world_event.night=1.0;
        assert!(key!=MotionKey::observed(&w,0));
        assert!((w.motion_limits(0,0.0).unwrap().0-speed*0.9).abs()<1e-9);

    }
    #[test]
    fn approaching_night_refreshes_cached_motion_before_the_fade_starts() {
        let mut w=World::new(crate::Config {rules:crate::RuleSet::V2,..Default::default()}).unwrap();
        w.diagnostic_event_schedule(10000,100);
        let mut ai=AiController::new();ai.prepare(&w);
        let initial=ai.motion_keys[0];let speed=ai.motion[0].at(24).0;
        w.tick=90;ai.prepare(&w);
        assert_eq!(w.world_event.night,0.0);
        assert!(initial!=ai.motion_keys[0],"scheduled onset enters the cached horizon before night starts");
        assert!(ai.motion[0].at(24).0<speed);
        assert_eq!(ai.motion[0].at(24),crate::effects::forecast_motion(&w,0,0.0,24));
    }
}
