// SPDX-License-Identifier: GPL-3.0-or-later
//! Capsule races reuse ordinary safety rollouts and the staged cut-off planner.
use super::*;
impl AiController {
    pub(super) fn grudge_prey(&self,w:&World,s:SnakeView<'_>,other:usize)->bool {
        s.face.grudge_ticks>0 && s.face.grudge_id==other as u32
            && w.snake(other).is_some_and(|r|r.alive && r.generation==s.face.grudge_generation
                && r.segments.len() as f64<=1.2*s.segments.len() as f64
                && w.distance_squared(s.segments[0].current,r.segments[0].current)<(30.0*s.radius).powi(2))
    }
    pub(super) fn prepare_races(&mut self,w:&World) {
        self.capsule_etas.fill([f64::INFINITY;MAX_SNAKES]);
        for (slot,f) in self.food[MAX_FOOD..].iter().enumerate().filter_map(|(slot,f)|f.map(|f|(slot,f))) {
            for (id,r) in self.rivals.iter().enumerate().filter(|(_,r)|r.alive) {
                let d=w.displacement(r.path[0],f.position);
                self.capsule_etas[slot][id]=((d.x*d.x+d.y*d.y).sqrt()-if f.id==events::ID {f.size} else {0.0}).max(0.0)/r.speed.max(1.0)
                    +normalize_angle(d.y.atan2(d.x)-r.angle).abs()/r.turn.max(0.01);
            }
        }
    }
    pub(super) fn advance_race(&self,w:&World,s:SnakeView<'_>,state:&mut State) {
        if state.target & target::ITEM_BIT==0 {
            state.race_losing_ticks=0;state.guarding=false;self.vulture_seed(w,s,state);
            if self.target_food(*state).is_some_and(|f|f.kind==crate::FoodKind::Prism) {
                // The ambush phase ends at ripening, even between strategy
                // slots. Retained cutoff controls must not carry racers away
                // from the now-collectible prize for another attack window.
                state.prey=0;state.clear_attacks(s.angle);state.track_goal=true;
            }
            return;
        }
        let Some(f)=self.target_food(*state) else {state.set_target(0);state.waypoint=None;return;};
        if f.id==events::ID && w.starfall_landed()
            && w.distance_squared(s.segments[0].current,f.position)<=f.size*f.size {
            state.set_target(0);state.prey=0;state.clear_attacks(s.angle);state.track_goal=false;
            state.waypoint=None;return;
        }
        let etas=self.capsule_etas[state_item_slot(self,f)];let mine=etas[s.id as usize];
        let rival=etas.iter().enumerate().filter(|(id,_)|*id!=s.id as usize
            && self.states[*id].target==state.target && self.rivals[*id].alive
            && self.states[*id].generation==w.snake(*id).unwrap().generation)
            .map(|(_,&eta)|eta).fold(f64::INFINITY,f64::min);
        // A wave has many prizes: later arrivals still have food to race for.
        let wave_slack=if f.id==events::ID {2.0+4.0*aggression::bold(w)} else {0.0};
        if !state.guarding && rival+wave_slack<(0.8-0.25*aggression::bold(w))*mine {state.race_losing_ticks=state.race_losing_ticks.saturating_add(1);} else {state.race_losing_ticks=0;}
        if state.race_losing_ticks>=10+(20.0*aggression::bold(w)) as u8 {
            state.rejected=state.target;state.reject_until=w.tick()+60;
            #[cfg(feature="desktop-diag")] {state.abandon=super::AbandonReason::LostRace;}
            state.set_target(0);state.waypoint=None;state.prey=0;state.clear_attacks(s.angle);
            state.track_goal=false;state.race_losing_ticks=0;
        }
        if f.id!=events::ID {self.guard_capsule(w,s,state);}
    }
    pub(super) fn vulture_seed(&self,w:&World,s:SnakeView<'_>,state:&mut State) {
        let Some(f)=self.target_food(*state).filter(|f|f.kind==crate::FoodKind::PrismSeed) else {
            if state.vulturing {state.turn_accum=0.0;state.waypoint=None;state.best_distance=f64::MAX;state.last_progress=w.tick();
                state.commit_until=0;state.turn_sign=0;state.track_goal=true;}
            state.vulturing=false;return;
        };
        let (speed,turn)=self.motion[s.id as usize].at(0);
        // Leave curvature reserve for entering and correcting the orbit. Fast,
        // large snakes need the same physical room as their ordinary rollouts;
        // a fixed upper bound in body radii made reference settings impossible.
        let radius=(5.5*s.radius).max(speed/turn.max(0.01)*1.35).max(speed*0.7)
            .max((s.segments.len() as f64*s.radius*1.18).min(speed*3.0)/std::f64::consts::TAU*1.1);
        let head=s.segments[0].current;
        let radial=w.displacement(f.position,head);
        let distance=(radial.x*radial.x+radial.y*radial.y).sqrt();
        let was_vulturing=state.vulturing;
        // Once established, retain the wait until the final six ticks. ETA
        // includes a pessimistic inside-turn loop; using it to release an
        // orbit sent heads through the disk while still unripe, then away.
        let early=if was_vulturing {f.motion_ticks>6}
            else {self.target_arrival(w,s,f)+0.15<f.motion_ticks as f64*STEP_SECONDS};
        let nearby=if was_vulturing {w.distance_squared(head,state.vulture_center)<(radius*1.6+3.0*s.radius).powi(2)}
            else {distance<radius*1.6+3.0*s.radius};
        state.vulturing=early && nearby;
        if state.vulturing {
            // Follow the tangent closest to the actual approach, retaining its
            // handedness until release. Traits break only a radial-entry tie.
            if !was_vulturing {
                let cross=radial.x*s.angle.sin()-radial.y*s.angle.cos();
                state.coil_sign=if cross.abs()>s.radius {cross.signum()}
                    else if s.traits.turn_bias<0.0 {-1.0} else {1.0};
                // Enter a reachable circle tangent to the actual head. The
                // prize lies on its approaching side, rather than demanding
                // an immediate inward ninety-degree turn from a centered ring.
                state.vulture_center=w.canonical_point(Point {
                    x:head.x-s.angle.sin()*state.coil_sign*radius,
                    y:head.y+s.angle.cos()*state.coil_sign*radius});
            }
            state.vulture_until=w.tick()+f.motion_ticks.saturating_sub(6) as u64;
            state.guard_radius=radius;state.goal=state.vulture_center;state.track_goal=true;
            state.best_distance=f64::MAX;state.last_progress=w.tick();state.waypoint=None;
        } else if was_vulturing {
            // Leave early enough to reach the capture disk at ripening. The
            // shared Contact clock still forbids collecting an unripe seed.
            state.turn_accum=0.0;state.waypoint=None;state.commit_until=0;
            state.turn_sign=0;state.track_goal=true;state.goal=f.position;
            state.best_distance=f64::MAX;state.last_progress=w.tick();
        }
    }
    pub(super) fn guard_worth(&self,w:&World,s:SnakeView<'_>,f:target::TargetFood)->bool {
        f.value>=5.0 || w.items().find(|item|item.id==f.id & !target::ITEM_BIT).is_some_and(|item| match item.kind {
            crate::effects::EffectKind::Magnet=>self.spatial.cluster_weight(self.spatial.key(f.position))>=4.0,
            crate::effects::EffectKind::Phase=>s.flags & crate::flags::TRAPPED!=0,
            _=>false,
        })
    }
    pub(super) fn guard_capsule(&self,w:&World,s:SnakeView<'_>,state:&mut State) {
        let Some(f)=self.target_food(*state).filter(|f|f.id & target::ITEM_BIT!=0 && f.id!=events::ID) else {state.guarding=false;return;};
        let (speed,turn)=self.motion[s.id as usize].at(0);
        let radius=(6.0*s.radius).max(speed/turn*1.15);
        let near=w.distance_squared(s.segments[0].current,f.position)<(15.0*s.radius).powi(2);
        state.guarding=self.guard_worth(w,s,f) && near && radius<=7.0*s.radius && s.effect_ticks>if state.guarding {crate::Item::GUARD_RELEASE_TICKS} else {90};
        if state.guarding {
            state.guard_radius=radius;state.coil_sign=if s.traits.turn_bias<0.0 {-1.0} else {1.0};
            state.waypoint=None;state.goal=f.position;state.track_goal=true;state.best_distance=f64::MAX;state.last_progress=w.tick();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fast_large_seed_orbit_uses_turn_radius_and_approach_handedness() {
        for direction in [-1.0,1.0] {
            let mut w=World::diagnostic_arena(crate::Config {aggression:50,width:3440.0,height:1440.0,
                density:0.0,scale:185.0,speed:230.0,rules:crate::RuleSet::V2,
                intelligence:100.0,self_collisions:true,deadly_walls:true,
                ..crate::Config::default()},&[
                (Point{x:1700.0,y:720.0},direction*std::f64::consts::FRAC_PI_2,24,0.9)],&[]).unwrap();
            let (speed,turn)=w.motion_limits(0,0.0).unwrap();
            let radius=(5.5*w.snakes[0].radius).max(1.35*speed/turn);
            let center=Point{x:1700.0-radius,y:720.0};
            w.food.push(crate::world::Food {id:88,p:center,kind:crate::FoodKind::PrismSeed,
                value:5.0,size:8.0,ripe_tick:90,motion_ticks:90,life:33.0,owner:-1,..Default::default()});
            let mut ai=AiController::new();ai.prepare(&w);
            let s=w.snake(0).unwrap();
            let mut state=State {target:88,target_index:0,generation:s.generation,..Default::default()};
            ai.vulture_seed(&w,s,&mut state);
            assert!(state.vulturing);
            assert!(state.guard_radius>=1.35*speed/turn);
            assert_eq!(state.coil_sign,direction);
            let goal=AiController::trajectory_goal(&w,state,s.segments[0].current);
            let d=w.displacement(s.segments[0].current,goal);
            assert!(normalize_angle(d.y.atan2(d.x)-s.angle).abs()<0.01);
            // Depart before ripening when direct approach uses the remaining
            // time, but the shared capture eligibility remains unripe.
            ai.food[0].as_mut().unwrap().motion_ticks=1;
            state.turn_accum=8.0;state.commit_until=80;
            ai.vulture_seed(&w,s,&mut state);
            assert!(!state.vulturing);assert_eq!(state.turn_accum,0.0);
            assert_eq!(state.commit_until,0);
        }
    }
    fn arena()->(World,AiController,State) {
        let mut w=World::diagnostic_arena(crate::Config {aggression:50,width:1600.0,height:1000.0,density:0.0,speed:50.0,
            rules:crate::RuleSet::V2,self_collisions:true,intelligence:100.0,..crate::Config::default()},&[
            (Point{x:400.0,y:400.0},0.0,48,0.9),(Point{x:420.0,y:500.0},0.0,48,0.9)],&[]).unwrap();
        w.items.push(crate::Item {id:77,kind:crate::effects::EffectKind::Surge,position:Point{x:460.0,y:400.0},
            radius:12.0,life_ticks:750,..crate::Item::default()});
        let mut ai=AiController::new();ai.prepare(&w);
        let state=State {target:77|target::ITEM_BIT,target_index:MAX_FOOD,generation:w.snake(0).unwrap().generation,..State::default()};
        (w,ai,state)
    }
    #[test]
    fn ripe_prize_ends_retained_ambush_before_the_next_strategy_slot() {
        let (mut w,mut ai,mut state)=arena();
        w.food.push(crate::world::Food {id:88,kind:crate::FoodKind::Prism,
            p:Point{x:500.0,y:400.0},size:8.0,value:5.0,life:30.0,owner:-1,..Default::default()});
        ai.tick=u64::MAX;ai.prepare(&w);
        state.target=88;state.target_index=0;state.prey=2;
        state.prey_generation=w.snake(1).unwrap().generation;
        state.hunt_until=180;state.next_response=60;
        state.attack=Attack {valid:true,prey:2,prey_generation:state.prey_generation,..Default::default()};
        ai.advance_race(&w,w.snake(0).unwrap(),&mut state);
        assert_eq!(state.target,88);assert_eq!(state.prey,0);
        assert!(!state.attack.valid);assert!(state.track_goal);
        // A due tactical revision cannot reintroduce the seed's attack phase.
        state.prey=2;ai.tactics(&w,w.snake(0).unwrap(),&mut state,1.0);
        assert_eq!(state.target,88);assert_eq!(state.prey,0);
    }
    #[test]
    fn planned_pounce_allows_a_reversal_but_recovery_resumes_after_four_seconds() {
        let (mut w,mut ai,mut state)=arena();w.items.clear();w.tick=1;
        w.food.push(crate::world::Food {id:88,kind:crate::FoodKind::Prism,
            p:Point{x:500.0,y:400.0},size:8.0,value:5.0,ripe_tick:1,
            life:30.0,owner:-1,..Default::default()});
        state.target=88;state.target_index=0;state.turn_accum=3.3;
        state.last_angle=w.snake(0).unwrap().angle;state.best_distance=f64::MAX;
        ai.states[0]=state;
        let _=ai.steer(&w,w.snake(0).unwrap());
        assert_eq!(ai.states[0].target,88);assert_eq!(ai.states[0].orbit_until,0);
        assert_eq!(ai.states[0].turn_accum,0.0);
        w.tick=132;ai.states[0].turn_accum=3.3;
        let _=ai.steer(&w,w.snake(0).unwrap());
        assert_eq!(ai.states[0].target,0);
        assert!(ai.states[0].orbit_until>w.tick(),"ordinary recovery must resume");
    }
    #[test]
    fn commits_require_a_winning_eta_or_a_ready_close_race_boost() {
        let (mut w,mut ai,mut state)=arena();state.target=0;
        let f=ai.food[MAX_FOOD].unwrap();
        ai.capsule_etas[0]=[f64::INFINITY;MAX_SNAKES];ai.capsule_etas[0][1]=1.0;
        ai.capsule_etas[0][0]=1.1;
        assert!(ai.target_score(&w,w.snake(0).unwrap(),&state,f,1.1,0.0,110.0)>0.0);
        w.snakes[0].cooldown_ticks=20;
        assert_eq!(ai.target_score(&w,w.snake(0).unwrap(),&state,f,1.1,0.0,110.0),0.0);
        ai.capsule_etas[0][0]=0.8;
        assert!(ai.target_score(&w,w.snake(0).unwrap(),&state,f,0.8,0.0,110.0)>0.0);
        w.snakes[0].cooldown_ticks=0;ai.capsule_etas[0][0]=1.3;
        assert_eq!(ai.target_score(&w,w.snake(0).unwrap(),&state,f,1.3,0.0,110.0),0.0);
    }
    #[test]
    fn abort_needs_ten_consecutive_ticks_against_a_committed_live_leader() {
        let (w,mut ai,mut state)=arena();ai.capsule_etas[0][0]=2.0;ai.capsule_etas[0][1]=1.0;
        for _ in 0..20 {ai.advance_race(&w,w.snake(0).unwrap(),&mut state);}assert_ne!(state.target,0,"non-racers cannot cause an abort");
        ai.states[1]=State {target:state.target,generation:w.snake(1).unwrap().generation,..State::default()};
        for _ in 0..9 {ai.advance_race(&w,w.snake(0).unwrap(),&mut state);}assert_ne!(state.target,0);
        ai.capsule_etas[0][1]=1.8;ai.advance_race(&w,w.snake(0).unwrap(),&mut state);assert_eq!(state.race_losing_ticks,0);
        ai.capsule_etas[0][1]=1.0;
        for _ in 0..10 {ai.advance_race(&w,w.snake(0).unwrap(),&mut state);}assert_eq!(state.target,0);
        assert_eq!(state.rejected,77|target::ITEM_BIT);
    }
    #[test]
    fn switching_capsules_restarts_losing_observations_and_guard_entry() {
        let (mut w,mut ai,mut state)=arena();
        w.items.push(crate::Item {id:78,kind:crate::effects::EffectKind::Surge,
            position:Point{x:500.0,y:400.0},radius:12.0,life_ticks:750,..Default::default()});
        ai.tick=u64::MAX;ai.prepare(&w);
        state.race_losing_ticks=9;state.guarding=true;state.guard_radius=42.0;
        state.rejected=state.target;state.reject_until=60;state.next_response=60;
        state.best_distance=1.0;state.last_progress=0;
        w.tick=12;w.snakes[0].effect_kind=1;w.snakes[0].effect_ticks=60;
        // A is excluded; B is winnable at selection, then a leader commits.
        ai.capsule_etas[1][0]=1.0;ai.capsule_etas[1][1]=2.0;
        ai.strategy(&w,w.snake(0).unwrap(),&mut state,1.0);
        assert_eq!(state.target,78|target::ITEM_BIT);
        assert_eq!(state.race_losing_ticks,0);assert!(!state.guarding);
        assert_eq!(state.guard_radius,0.0);assert_eq!(state.best_distance,f64::MAX);
        assert_eq!(state.last_progress,12);
        ai.states[1]=State {target:state.target,generation:w.snake(1).unwrap().generation,..Default::default()};
        ai.capsule_etas[1][0]=2.0;ai.capsule_etas[1][1]=1.0;
        for expected in 1..10 {
            ai.advance_race(&w,w.snake(0).unwrap(),&mut state);
            assert_eq!(state.race_losing_ticks,expected);assert_eq!(state.target,78|target::ITEM_BIT);
        }
        ai.advance_race(&w,w.snake(0).unwrap(),&mut state);
        assert_eq!(state.target,0);assert_eq!(state.rejected,78|target::ITEM_BIT);
    }
    #[test]
    fn blocked_capsule_fallback_resets_race_progress_and_tactics() {
        let (mut w,mut ai,mut state)=arena();
        for j in 0..w.snakes[1].len {
            let p=Point{x:440.0,y:280.0+j as f64*8.0};
            w.segments[MAX_SEGMENTS+j]=crate::Segment {current:p,previous:p};
        }
        w.items.push(crate::Item {id:78,kind:crate::effects::EffectKind::Surge,
            position:Point{x:400.0,y:650.0},radius:12.0,life_ticks:750,..Default::default()});
        w.tick=12;ai.tick=u64::MAX;ai.prepare(&w);
        // The exact body blocks A; an occupied coarse grid rules out a waypoint.
        // B is lower-ranked and has a clear approach on the other side.
        ai.spatial.occupied.fill(u16::MAX);
        ai.capsule_etas[0][1]=f64::INFINITY;ai.capsule_etas[1][1]=f64::INFINITY;
        state.race_losing_ticks=9;state.best_distance=1.0;state.last_progress=0;
        state.next_response=60;state.prey=2;state.attack.valid=true;state.commit_until=60;
        ai.strategy(&w,w.snake(0).unwrap(),&mut state,1.0);
        assert_eq!(state.debug.target_food_ids[0],77|target::ITEM_BIT);
        assert_eq!(state.target,78|target::ITEM_BIT,"must exercise the alternative path");
        assert_eq!(state.race_losing_ticks,0);assert_eq!(state.best_distance,f64::MAX);
        assert_eq!(state.last_progress,12);assert_eq!(state.prey,0);
        assert!(!state.attack.valid);assert_eq!(state.commit_until,0);
    }
    #[test]
    fn expired_capsules_and_respawns_drop_race_state() {
        let (mut w,mut ai,mut state)=arena();
        state.race_losing_ticks=9;state.guarding=true;
        w.items.clear();ai.tick=u64::MAX;ai.prepare(&w);
        ai.advance_race(&w,w.snake(0).unwrap(),&mut state);
        assert_eq!(state.target,0);assert_eq!(state.race_losing_ticks,0);assert!(!state.guarding);
        ai.states[0]=State {generation:w.snakes[0].generation-1,target:77|target::ITEM_BIT,
            race_losing_ticks:9,guarding:true,..Default::default()};
        ai.steer(&w,w.snake(0).unwrap());
        assert_eq!(ai.states[0].race_losing_ticks,0);assert!(!ai.states[0].guarding);
    }
    #[test]
    fn guard_keeps_its_effect_until_the_last_thirty_ticks_and_grudge_is_bounded() {
        let (mut w,ai,mut state)=arena();w.snakes[0].effect_kind=1;w.snakes[0].effect_ticks=100;
        ai.guard_capsule(&w,w.snake(0).unwrap(),&mut state);assert!(state.guarding);
        w.snakes[0].effect_ticks=60;ai.guard_capsule(&w,w.snake(0).unwrap(),&mut state);assert!(state.guarding);
        w.snakes[0].effect_ticks=30;ai.guard_capsule(&w,w.snake(0).unwrap(),&mut state);assert!(!state.guarding);
        let generation=w.snakes[1].generation;w.faces[0].grudge_id=1;
        w.faces[0].grudge_generation=generation;w.faces[0].grudge_ticks=150;
        assert!(ai.grudge_prey(&w,w.snake(0).unwrap(),1));
        w.snakes[1].generation+=1;assert!(!ai.grudge_prey(&w,w.snake(0).unwrap(),1));
        w.snakes[1].generation=generation;w.snakes[1].len=60;assert!(!ai.grudge_prey(&w,w.snake(0).unwrap(),1));
    }
    #[test]
    fn early_escape_and_orbit_returns_clear_guard_state() {
        for escape in [false,true] {
            let (w,mut ai,mut state)=arena();state.guarding=true;state.race_losing_ticks=9;
            if escape {state.escape_until=w.tick()+30;} else {state.orbit_until=w.tick()+30;}
            ai.strategy(&w,w.snake(0).unwrap(),&mut state,1.0);
            assert_eq!(state.target,0);assert!(!state.guarding);assert_eq!(state.race_losing_ticks,0);
        }
    }

    #[test]
    fn controller_keeps_guard_orbit_until_release_without_circle_recovery() {
        let mut w=World::diagnostic_arena(crate::Config {aggression:50,width:1600.0,height:1000.0,density:0.0,
            speed:50.0,rules:crate::RuleSet::V2,self_collisions:true,intelligence:100.0,
            ..crate::Config::default()},&[(Point{x:460.0,y:400.0},std::f64::consts::FRAC_PI_2,8,0.9)],&[]).unwrap();
        w.items.push(crate::Item {id:77,kind:crate::effects::EffectKind::Surge,position:Point{x:400.0,y:400.0},
            radius:12.0,life_ticks:750,..Default::default()});
        w.snakes[0].effect_kind=crate::effects::EffectKind::Magnet as u8;w.snakes[0].effect_ticks=240;
        let mut ai=AiController::new();let mut rotation=0.0;let mut last=w.snakes[0].angle;
        let mut guard_ticks=0;let mut released=false;let mut picked=false;
        for _ in 0..280 {
            w.step(&mut ai);
            let s=w.snake(0).unwrap();assert!(s.alive,"reason={:?}",w.last_death_reason(0));
            let state=ai.states[0];
            if state.guarding {
                guard_ticks+=1;rotation+=normalize_angle(s.angle-last).abs();
                assert_eq!(state.target,77|target::ITEM_BIT);
                assert_eq!(state.orbit_until,0);assert_eq!(state.reject_until,0);
                assert_eq!(s.effect_kind,crate::effects::EffectKind::Magnet as u8);
            }
            if s.effect_ticks<=30 && s.effect_kind==crate::effects::EffectKind::Magnet as u8 {
                released=true;assert!(!state.guarding);assert_eq!(state.orbit_until,0);
            }
            last=s.angle;
            if w.frame_events().any(|e|e.kind==crate::EventKind::Pickup) {picked=true;break;}
        }
        assert!(guard_ticks>100 && rotation>2.8,"guard ticks={guard_ticks} rotation={rotation}");
        assert!(released && picked,"released={released} picked={picked}");
    }

    #[test]
    fn rollout_uses_current_guard_intent_before_world_exports_it() {
        for guarding in [false,true] {
            let (mut w,mut ai,mut state)=arena();
            w.snakes[1].alive=false;w.snakes[1].len=0;
            w.snakes[0].effect_kind=crate::effects::EffectKind::Magnet as u8;w.snakes[0].effect_ticks=100;
            let p=w.segments[0].current;
            w.items.push(crate::Item {id:78,kind:crate::effects::EffectKind::Phase,
                position:p,radius:30.0,life_ticks:750,..Default::default()});
            w.faces[0].guarding=!guarding;w.faces[0].target_id=77;
            ai.tick=u64::MAX;ai.prepare(&w);
            state.guarding=guarding;
            let c=ai.rollout(&w,w.snake(0).unwrap(),state,2,1);
            assert_eq!(c.steps,1);
            assert_eq!(c.effects.at(0,1).kind,if guarding {crate::effects::EffectKind::Magnet as u8} else {crate::effects::EffectKind::Phase as u8});
        }
    }

    #[test]
    fn overlapping_guarded_capsule_stays_shortlisted_until_release() {
        for ticks in [31,60,100] {
            let (mut w,mut ai,mut state)=arena();
            w.snakes[1].alive=false;w.snakes[1].len=0;
            w.snakes[0].effect_kind=2;w.snakes[0].effect_ticks=ticks;
            w.items[0].position=w.segments[0].current;
            w.faces[0].guarding=true;w.faces[0].target_id=77;
            state.guarding=true;state.next_response=60;
            ai.tick=u64::MAX;ai.prepare(&w);
            ai.strategy(&w,w.snake(0).unwrap(),&mut state,1.0);
            assert_eq!(state.debug.target_food_ids[0],77|target::ITEM_BIT,"ticks={ticks}");
            assert_eq!(state.target,77|target::ITEM_BIT);assert!(state.guarding);
            ai.states[0]=state;w.step(&mut ai);
            assert_eq!(w.snake(0).unwrap().effect_kind,if ticks==31 {1} else {2});
            assert_eq!(w.items().any(|i|i.id==77),ticks>31);
        }
    }

}

#[cfg(test)]
mod prism_tests {
    use super::*;
    #[test]
    fn controller_vultures_then_feasts_without_early_pickup_or_circle_recovery() {
        let mut w=World::diagnostic_arena(crate::Config {aggression:50,width:1600.0,height:1000.0,density:0.0,
            speed:50.0,rules:crate::RuleSet::V2,self_collisions:true,intelligence:100.0,
            ..crate::Config::default()},&[(Point{x:460.0,y:400.0},std::f64::consts::FRAC_PI_2,8,0.9)],&[]).unwrap();
        w.food.push(crate::world::Food {id:88,kind:crate::FoodKind::PrismSeed,
            p:Point{x:400.0,y:400.0},size:8.0,value:5.0,ripe_tick:90,motion_ticks:90,
            life:33.0,original_life:33.0,owner:-1,..Default::default()});
        let mut ai=AiController::new();let mut waiting=0;let mut feast=false;
        for _ in 0..180 {
            w.step(&mut ai);
            assert!(w.snake(0).unwrap().alive,"reason={:?}",w.last_death_reason(0));
            if ai.states[0].vulturing {
                waiting+=1;
                assert!(w.tick()<90);
                assert_eq!(ai.states[0].target,88);
                assert_eq!(ai.states[0].orbit_until,0);
                assert_eq!(ai.states[0].reject_until,0);
            }
            if w.frame_events().any(|e|e.kind==crate::EventKind::Feast) {
                assert!(w.tick()>=90);
                assert!(!ai.states[0].vulturing);
                feast=true;break;
            }
        }
        assert!(waiting>0,"the early arrival never entered its seed orbit");
        assert!(feast,"the vulture did not release and collect the ripe prize");
    }
    #[test]
    fn seed_ready_clock_matches_completed_frame_and_post_decrement_observations() {
        let mut w=World::diagnostic_arena(crate::Config {aggression:50,rules:crate::RuleSet::V2,density:0.0,..crate::Config::default()},
            &[(Point{x:400.0,y:300.0},0.0,24,0.9)],&[]).unwrap();
        w.tick=1;
        w.food.push(crate::world::Food {id:88,kind:crate::FoodKind::PrismSeed,p:Point{x:400.0,y:300.0},
            ripe_tick:2,motion_ticks:1,value:5.0,owner:-1,..Default::default()});
        let mut ai=AiController::new();ai.prepare(&w);
        let c=target::Contact::forecast(w.snake(0).unwrap(),ai.food[0].unwrap(),ai.opportunities.track(0));
        assert!(c.reached(0.0,1),"completed frame: next endpoint is ripe");
        w.tick=0;ai.tick=u64::MAX;ai.prepare(&w);
        let c=target::Contact::forecast(w.snake(0).unwrap(),ai.food[0].unwrap(),ai.opportunities.track(0));
        assert!(!c.reached(0.0,1));assert!(c.reached(0.0,2));
    }
    #[test]
    fn seed_contact_waits_for_ripe_across_forecasts_and_overlap_shortlisting() {
        let mut w=World::diagnostic_arena(crate::Config {aggression:50,rules:crate::RuleSet::V2,density:0.0,speed:50.0,self_collisions:false,..crate::Config::default()},
            &[(Point{x:400.0,y:300.0},0.0,24,0.9)],&[]).unwrap();
        let r=w.snakes[0].radius;
        w.food.push(crate::world::Food {id:88,p:Point{x:400.0+5.5*r,y:300.0},kind:crate::FoodKind::PrismSeed,
            value:5.0,size:r*0.62,ripe_tick:91,motion_ticks:90,life:33.0,owner:-1,..Default::default()});
        let mut ai=AiController::new();ai.prepare(&w);let f=ai.food[0].unwrap();
        assert!((f.value-90.0/(1.0+91.0/30.0*0.35)).abs()<1e-8);
        let s=w.snake(0).unwrap();let c=target::Contact::forecast(s,f,ai.opportunities.track(0));
        assert!(!c.reached(0.0,90));assert!(c.reached(0.0,91));
        let mut state=State {target:88,target_index:0,generation:s.generation,..Default::default()};
        ai.vulture_seed(&w,s,&mut state);assert!(state.vulturing);
        assert!(state.guard_radius>=5.5*r);assert!(!ai.food_race(&w,s,state));
        state.turn_accum=9.0;state.set_target(77);assert!(!state.vulturing);assert_eq!(state.guard_radius,0.0);
        w.food[0].p=w.segments[0].current;ai.tick=u64::MAX;ai.prepare(&w);
        ai.strategy(&w,w.snake(0).unwrap(),&mut state,1.0);assert_eq!(state.target,88,"overlapping unripe seed stays discoverable");
        w.food[0].kind=crate::FoodKind::Prism;w.food[0].motion_ticks=0;ai.tick=u64::MAX;ai.prepare(&w);
        ai.vulture_seed(&w,w.snake(0).unwrap(),&mut state);assert!(!state.vulturing);
        assert_eq!(ai.food[0].unwrap().value,90.0);
        assert!(target::Contact::forecast(w.snake(0).unwrap(),ai.food[0].unwrap(),ai.opportunities.track(0)).reached(0.0,1));
    }
}
