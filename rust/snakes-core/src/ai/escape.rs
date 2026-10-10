// SPDX-License-Identifier: GPL-3.0-or-later
//! Held-item escape proofs use the ordinary effect-aware rollout and wall gate.
use super::*;
use crate::effects::EffectKind;

#[cfg(feature="desktop-diag")]
#[derive(Clone,Copy)]
pub(super) struct EscapeDecision {tick:u64,generation:u32,state:State,horizon:usize,eligible:bool}


impl AiController {
    pub(super) fn choose_escape_item(&mut self,w:&World,s:SnakeView<'_>,state:&mut State,baseline:&Candidate,horizon:usize)->bool {
        let need=(s.segments.len() as f64*s.radius*s.radius*8.0/(self.spatial.dx*self.spatial.dy)).ceil().max(24.0) as usize;
        // A surviving ordinary continuation needs no rescue, even if an old
        // escape commitment or presentation flag has not expired yet.
        if baseline.wall_safe && baseline.steps>=horizon && baseline.area>=need {return false;}
        let mut trial=Candidate::default();
        // Use the same horizon that admitted the ordinary candidates. Phase
        // still proves tangible emergence below; enclosure risk already
        // extends the ordinary horizon. A blanket extension rejected timely
        // rescues without changing the endpoint room/tail-corridor check.
        let proof_horizon=horizon;
        #[cfg(test)] let proof_horizon=if self.legacy_escape_proof && w.config().self_collisions && s.segments.len()>=40 {STEPS} else {proof_horizon};
        // Instant effects and speed are cheaper than spending intangibility
        // or the one bite. Preserve Venom for offence only after proving an
        // equally surviving alternative, independent of aggression.
        for kind in [EffectKind::Frost,EffectKind::Flip,EffectKind::Surge,EffectKind::Phase,EffectKind::Venom] {
            for slot in 0..s.inventory.count as usize {
                if s.inventory.kinds[slot]!=kind as u8 {continue;}
                let Some(mut proposal)=self.held_escape(w,s,*state,slot,proof_horizon,&mut trial) else {continue;};
                if proposal.use_slot==0 {
                    State::commit_candidate(&mut proposal,&trial,trial.kind,w,s,self.rivals[s.id as usize].turn,need);
                    *state=proposal;
                    self.record_escape(w,s,state,&trial,proof_horizon);
                    #[cfg(feature="desktop-diag")] self.observe_escape(w,s,*state,&trial,proof_horizon,need);
                    return true;
                }
                // Danger can expose a maneuver the ordinary search did not
                // sample. If its item-free version also survives, take that
                // path and keep the item rather than count a needless rescue.
                let mut ordinary=Candidate::default();let mut free=proposal;free.use_slot=0;
                self.rollout_into(w,s,free,trial.kind,proof_horizon,&mut ordinary);
                if self.escape_room(w,s,&mut ordinary,proof_horizon) {
                    State::commit_candidate(&mut free,&ordinary,ordinary.kind,w,s,self.rivals[s.id as usize].turn,need);
                    *state=free;
                    self.record_escape(w,s,state,&ordinary,proof_horizon);
                    #[cfg(feature="desktop-diag")] self.observe_escape(w,s,*state,&ordinary,proof_horizon,need);
                    return true;
                }
                State::commit_candidate(&mut proposal,&trial,trial.kind,w,s,self.rivals[s.id as usize].turn,need);
                proposal.flip_situation=if kind==EffectKind::Flip {1} else {0};
                proposal.flip_prey=0;proposal.flip_prey_generation=0;
                proposal.flip_request_tick=w.tick();
                *state=proposal;
                self.record_escape(w,s,state,&trial,if kind==EffectKind::Phase {STEPS} else {proof_horizon});
                #[cfg(feature="desktop-diag")] self.observe_escape(w,s,*state,&trial,if kind==EffectKind::Phase {STEPS} else {proof_horizon},need);
                #[cfg(feature="desktop-diag")] {
                    let d=&mut self.desktop[s.id as usize];
                    d.escape_item=kind as u8;d.escape_item_safe=trial.steps;d.escape_item_bite=trial.venom_bite;
                    d.escape_item_danger=baseline.steps<horizon || baseline.area<need;
                }
                return true;
            }
        }
        false
    }

    fn escape_room(&mut self,w:&World,s:SnakeView<'_>,c:&mut Candidate,horizon:usize)->bool {
        if !c.wall_safe || c.capped || c.steps<horizon
            || c.venom_bite>0 && c.steps<c.venom_bite+VENOM_EXIT_STEPS {return false;}
        let id=s.id as usize;
        let need=(s.segments.len() as f64*s.radius*s.radius*8.0/(self.spatial.dx*self.spatial.dy)).ceil().max(24.0) as usize;
        let mask=self.candidate_mask(w,id,c);
        let release=c.effects.space_time(mask,c.steps as f64*STEP_SECONDS,&self.rivals);
        let (area,capped)=if w.config().self_collisions && s.segments.len()>=40 {
            let rate=s.radius*1.18/(w.motion_limits(id,c.rush).unwrap().0*STEP_SECONDS).max(0.1);
            self.spatial.trajectory_space(c.path[c.steps],mask,(need*2).max(64),release,
                &c.path[..=c.steps],(rate*10.0).ceil() as usize,
                (c.effects.body_time(id,s.segments.len() as f64*self.rivals[id].release_rate+self.rivals[id].growth_delay,&self.rivals[id])/STEP_SECONDS).ceil() as usize)
        } else {self.spatial.space(c.path[c.steps],mask,(need*2).max(64),release)};
        c.area=area;c.uncertain=capped;
        area>=need
    }

    // Spending Venom as an escape requires opening a corridor by cutting.
    // Try ordinary controls for a no-cut forecast before searching another
    // maneuver. Endpoint room and the post-cut continuation remain required.
    fn escape_effect(&mut self,w:&World,s:SnakeView<'_>,proposal:&mut State,kind:EffectKind,horizon:usize,out:&mut Candidate)->bool {
        if !self.escape_room(w,s,out,horizon) {return false;}
        #[cfg(test)] if self.legacy_escape_proof {return true;}
        if kind!=EffectKind::Venom || out.venom_bite!=0 {return true;}
        let mut free=*proposal;free.use_slot=0;
        let mut ordinary=Candidate::default();
        self.rollout_into(w,s,free,out.kind,horizon,&mut ordinary);
        if !self.escape_room(w,s,&mut ordinary,horizon) {return false;}
        *proposal=free;*out=ordinary;true
    }

    fn held_escape(&mut self,w:&World,s:SnakeView<'_>,state:State,slot:usize,horizon:usize,out:&mut Candidate)->Option<State> {
        let kind=EffectKind::from_byte(s.inventory.kinds[slot]);
        if !matches!(kind,EffectKind::Venom|EffectKind::Phase|EffectKind::Frost|EffectKind::Surge|EffectKind::Flip)
            || s.inventory.life[slot]<=4 || s.inventory.windup!=0 || s.inventory.cooldown!=0 {return None;}
        let horizon=if kind==EffectKind::Phase {STEPS} else {horizon};
        let mut proposal=state;proposal.use_slot=slot as u8+1;proposal.revise_opponents=true;
        self.rollout_into(w,s,proposal,1,horizon,out);
        if self.escape_effect(w,s,&mut proposal,kind,horizon,out) {return Some(proposal);}
        // Reservations, offence and food pursuit cannot constrain an escape.
        proposal.clear_coil(s.angle);proposal.clear_attacks(s.angle);
        proposal.set_target(0);proposal.waypoint=None;proposal.track_goal=false;
        proposal.venom_target=0;proposal.venom_alternative=None;proposal.prey=0;
        proposal.rush=0.0;proposal.escape_boost=false;
        for maneuver in [2,3,4,5,6,7,8,9,10] {
            self.rollout_into(w,s,proposal,maneuver,horizon,out);
            if self.escape_effect(w,s,&mut proposal,kind,horizon,out) {return Some(proposal);}
        }
        None
    }

    #[cfg(feature="desktop-diag")]
    pub(super) fn remember_escape_decision(&mut self,w:&World,s:SnakeView<'_>,mut state:State,baseline:&Candidate,horizon:usize) {
        state.use_slot=0;
        let need=(s.segments.len() as f64*s.radius*s.radius*8.0/(self.spatial.dx*self.spatial.dy)).ceil().max(24.0) as usize;
        let danger=baseline.steps<horizon || state.debug.flags&32!=0
            || s.flags&crate::flags::TRAPPED!=0 || w.tick()<state.escape_until;
        let eligible=w.config().rules==crate::RuleSet::V2 && w.config().power_ups
            && s.inventory.count!=0 && s.inventory.windup==0 && s.inventory.cooldown==0
            && danger && !(baseline.wall_safe && baseline.steps>=horizon && baseline.area>=need);
        self.escape_decisions[s.id as usize]=Some(EscapeDecision {tick:w.tick(),generation:s.generation,state,horizon,eligible});
    }

    // Ordinary danger and planning statistics stay separate from the path
    // actually committed, including free continuations and cleared objectives.
    #[cfg(feature="desktop-diag")]
    fn observe_escape(&mut self,w:&World,s:SnakeView<'_>,state:State,c:&Candidate,horizon:usize,need:usize) {
        let id=s.id as usize;let mut d=self.desktop[id];
        d.selected=c.kind as u8;d.reused_plan=false;d.selected_score=c.score;
        d.safe_ticks=c.steps;d.horizon=horizon;d.next_head=c.path[1];
        d.continuation=c.wall_safe && c.steps>=horizon && c.area>=need && !c.uncertain && !c.capped;
        d.continuation_unresolved=c.uncertain;d.any_first_step=c.checked && c.steps>0;
        d.abandon=state.abandon;
        d.escape_started|=w.tick()<state.escape_until && d.mode!=2;
        let delta=w.displacement(s.segments[0].current,state.goal);
        d.retained_drift=normalize_angle(delta.y.atan2(delta.x)-state.desired).abs();
        d.reach=self.target_food(state).map_or(0.0,|f|target::Contact::forecast(s,f,self.opportunities.track(id)).reach(1));
        self.observe_objective(w,s,state,&mut d);
        self.desktop[id]=d;
    }

    /// Escape commits replace the ordinary winner's controls, so public
    /// debug and decision diagnostics must describe the executed escape.
    /// Danger flags stay from the ordinary search: they gate item use.
    fn record_escape(&mut self,w:&World,s:SnakeView<'_>,state:&mut State,c:&Candidate,horizon:usize) {
        state.record_path(c);
        let id=s.id as usize;
        if self.diagnostic_enabled {
            let decision=&mut self.decisions[id];decision.selected=c.kind;
            decision.horizon=horizon;decision.goal=state.waypoint.unwrap_or(state.goal);decision.reused_plan=false;
            decision.candidates[c.kind]=CandidateDiagnostic {
                checked:c.checked,wall_safe:c.wall_safe,wall_first_safe:c.wall_first_safe,desired:c.desired,
                safe_ticks:c.steps,area:c.area,area_capped:c.uncertain,
                turn_ticks:if c.turn_until!=u64::MAX {c.turn_until.saturating_sub(w.tick()) as usize} else {0},
                track_goal:c.tracks_goal,exit_angle:c.exit_angle,capped:c.capped,score:c.score,rush:c.rush,
                coil_center:state.coil_center,coil_radius:state.coil_radius,coil_sign:state.coil_sign,
                coil_pitch:state.coil_pitch,coil_progress:state.coil_progress,attack_replies:c.replies,
                attack_error:c.attack.error,attack_valid:c.attack.valid,
                attack_turn_ticks:if c.attack.valid {c.attack.turn_at.saturating_sub(w.tick()) as usize} else {0},
                attack_crossing:if c.attack.valid {c.attack.crossing} else {c.desired},
                attack_crossing_rush:if c.attack.valid {c.attack.crossing_rush} else {c.rush}};
        }
    }

    #[cfg(feature="desktop-diag")]
    pub(super) fn observe_objective(&self,w:&World,s:SnakeView<'_>,state:State,d:&mut DesktopObservation) {
            d.contested=self.contested_target(w,s,state);
            d.fleeing=w.tick()<state.dodge_until;
            d.cutoff=state.attack.valid || (state.barrier && state.track_goal && state.prey!=0 && w.tick()>=state.escape_until && w.tick()>=state.orbit_until);d.cutoff_start=state.attack.start;
            d.boosted=s.boost_ticks>0 || state.rush>0.0;
            d.power_up=if s.effect_ticks>0 {s.effect_kind} else {0};
            d.target=state.target;d.goal=state.goal;
            d.prey=state.prey as u32;d.prey_generation=state.prey_generation;d.venom_target=state.venom_target as u32;
            let objective=self.target_food(state);
            d.target_kind=objective.map_or(0,|f|if f.kind==crate::FoodKind::Meteor {3} else if f.id & target::ITEM_BIT!=0 {1} else if matches!(f.kind,crate::FoodKind::Prism|crate::FoodKind::PrismSeed) {2} else {0});
            d.distance=w.distance_squared(s.segments[0].current,objective.map_or(state.goal,|f|f.position)).sqrt();
            d.flip_situation=state.flip_situation;d.turn_accum=state.turn_accum;d.tracking=state.track_goal;
            let bearing=w.displacement(s.segments[0].current,state.goal);
            d.target_bearing=normalize_angle(bearing.y.atan2(bearing.x)-s.angle);
            d.mode=if w.tick()<state.orbit_until {1} else if w.tick()<state.escape_until {2}
                else if state.vulturing && objective.is_some_and(|f|f.kind==crate::FoodKind::Meteor) {if w.vortex().is_some_and(|v|v.owner_snake_id==s.id && v.owner_generation==s.generation) {12} else {13}} else if state.coil_radius>0.0 {3} else if state.guarding {4} else if state.vulturing {5}
                else if state.venom_standoff {6} else if state.venom_target!=0 {7}
                else if state.prey!=0 {8} else if state.target & target::ITEM_BIT!=0 {9}
                else if self.target_food(state).is_some_and(|f|matches!(f.kind,crate::FoodKind::Prism|crate::FoodKind::PrismSeed)) {10}
                else if state.target!=0 {11} else {0};
    }

    /// Counterfactual at the same pre-inventory decision, horizon and admission
    /// gates. Replay calls this after steer; generation/tick reject stale data.
    #[cfg(feature="desktop-diag")]
    pub fn usable_escape_items(&mut self,w:&World,s:SnakeView<'_>,wanted:u8)->u8 {
        let s=surge::planner_view(w,aggression::planner_view(w,s));
        let Some(decision)=self.escape_decisions[s.id as usize].filter(|d|d.tick==w.tick() && d.generation==s.generation && d.eligible) else {return 0;};
        let mut result=0;let mut trial=Candidate::default();
        for slot in 0..s.inventory.count as usize {
            let bit=1u8.checked_shl(s.inventory.kinds[slot] as u32).unwrap_or(0);
            if wanted&bit==0 || result&bit!=0 {continue;}
            let Some(proposal)=self.held_escape(w,s,decision.state,slot,decision.horizon,&mut trial).filter(|p|p.use_slot==slot as u8+1) else {continue;};
            // Real admission also keeps an equally safe ordinary maneuver.
            let mut free=proposal;free.use_slot=0;let mut ordinary=Candidate::default();
            self.rollout_into(w,s,free,trial.kind,decision.horizon,&mut ordinary);
            if !self.escape_room(w,s,&mut ordinary,decision.horizon) {result|=bit;}
        }
        result
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Config,RuleSet,Inventory};
    fn arena(kind:EffectKind,gap:f64)->World {
        let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,width:8000.0,height:4000.0,
            density:0.0,trails:0.0,scale:200.0,speed:300.0,aggression:0,intelligence:100.0,
            deadly_walls:true,self_collisions:false,world_events:false,..Default::default()},&[
            (Point{x:3000.0,y:2000.0},0.0,20,0.0),
            (Point{x:3000.0+gap,y:2800.0},std::f64::consts::FRAC_PI_2,140,0.0),
            (Point{x:2800.0,y:2800.0},std::f64::consts::FRAC_PI_2,140,0.0)],&[]).unwrap();
        w.snakes[0].inventory=Inventory {kinds:[kind as u8,0,0],life:[1800,0,0],count:1,..Default::default()};
        w
    }
    fn boxed(kind:EffectKind)->World {
        let mut w=arena(kind,120.0);
        let own:Vec<_>=(0..60).map(|i|Point{x:3000.0-i as f64*7.08,y:2000.0}).collect();
        w.diagnostic_body(0,&own,0.0).unwrap();
        fn path(vertices:&[Point])->Vec<Point> {
            let mut out=vec![vertices[0]];
            for pair in vertices.windows(2) {let d=Point{x:pair[1].x-pair[0].x,y:pair[1].y-pair[0].y};
                let n=((d.x*d.x+d.y*d.y).sqrt()/7.08).ceil() as usize;
                for i in 1..=n {let t=i as f64/n as f64;out.push(Point{x:pair[0].x+d.x*t,y:pair[0].y+d.y*t});}
            }out
        }
        let top=path(&[Point{x:4000.0,y:2800.0},Point{x:2700.0,y:2800.0},Point{x:2700.0,y:1980.0},Point{x:3120.0,y:1980.0},Point{x:3120.0,y:2020.0}]);
        let bottom=path(&[Point{x:4000.0,y:1200.0},Point{x:2700.0,y:1200.0},Point{x:2700.0,y:2020.0},Point{x:3120.0,y:2020.0}]);
        w.diagnostic_body(1,&top,0.0).unwrap();w.diagnostic_body(2,&bottom,0.0).unwrap();
        w.snakes[1].traits.speed_bias=0.1;w.snakes[2].traits.speed_bias=0.1;
        w
    }
    struct Driver {ai:AiController}
    impl Controller for Driver {
        fn delegate(&self,id:u32)->Option<&dyn Controller> {if id==0 {Some(&self.ai)} else {None}}
        fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
            if s.id==0 {self.ai.steer(w,s)} else {Steering {desired_angle:s.angle,rush:0.0}}
        }
    }
    fn closing(kind:EffectKind,gap:f64,distance:f64)->World {
        let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,width:8000.0,height:4000.0,
            density:0.0,trails:0.0,scale:200.0,speed:300.0,aggression:0,intelligence:100.0,
            deadly_walls:true,self_collisions:false,world_events:false,..Default::default()},&[
            (Point{x:3000.0,y:2000.0},0.0,20,0.0),
            (Point{x:3000.0+distance,y:2000.0+gap},-std::f64::consts::FRAC_PI_2,60,0.0),
            (Point{x:3000.0+distance,y:2000.0-gap},std::f64::consts::FRAC_PI_2,60,0.0)],&[]).unwrap();
        w.snakes[0].inventory=Inventory {kinds:[kind as u8,0,0],life:[1800,0,0],count:1,..Default::default()};w
    }
    #[test]
    fn boxed_by_two_rivals_uses_venom_phase_or_flip_and_survives() {
        for kind in [EffectKind::Venom,EffectKind::Phase,EffectKind::Flip] {
            let mut w=boxed(kind);let mut ai=Driver {ai:AiController::new()};
            let mut cut=false;let mut activation=0;let mut flip=false;
            for step in 1..=STEPS {
                w.step(&mut ai);
                if step==1 {assert_eq!(w.snakes[0].inventory.windup,1,"{kind:?} must request before contact");}
                assert!(w.snakes[0].alive,"{kind:?} died at {step}");
                for e in w.frame_events() {
                    cut|=e.kind==crate::EventKind::Sever && e.other_snake_id==0;
                    flip|=e.kind==crate::EventKind::Flip && e.snake_id==0;
                    if e.kind==crate::EventKind::Pickup && e.snake_id==0 && e.flags&crate::event_flags::HELD_ACTIVATION!=0 {activation=step;}
                }
                if step<5 {assert_eq!(w.snakes[0].effect_ticks,0,"four corporeal wind-up movements");}
            }
            assert_eq!(activation,5,"completion is after the fourth wind-up update");
            assert_eq!(w.snakes[0].inventory.count,0);
            if kind==EffectKind::Venom {assert!(cut,"Venom must actually sever the blocking body");}
            if kind==EffectKind::Flip {assert!(flip,"Flip must reverse to the open tail side");}
            if kind==EffectKind::Phase {assert_eq!(w.snakes[0].effect_ticks,0,"survives after tangible emergence");}
        }
    }
    #[test]
    fn closing_gap_uses_frost_or_surge_and_survives() {
        for (kind,gap,bias) in [(EffectKind::Frost,60.0,0.5),(EffectKind::Surge,200.0,1.0)] {
            let mut w=closing(kind,gap,80.0);w.snakes[1].traits.speed_bias=bias;w.snakes[2].traits.speed_bias=bias;
            let mut ai=Driver {ai:AiController::new()};let mut activation=0;let mut frozen=false;
            for step in 1..=NORMAL_STEPS {
                w.step(&mut ai);assert!(w.snakes[0].alive,"{kind:?} died at {step}");
                if step==1 {assert_eq!(w.snakes[0].inventory.windup,1);}
                frozen|=w.snakes[1].frozen_ticks>0 && w.snakes[2].frozen_ticks>0;
                if w.frame_events().any(|e|e.kind==crate::EventKind::Pickup && e.snake_id==0 && e.flags&crate::event_flags::HELD_ACTIVATION!=0) {activation=step;}
            }
            assert_eq!(activation,5);
            if kind==EffectKind::Frost {assert!(frozen,"both trapping heads must be frozen");}
            if kind==EffectKind::Surge {assert_eq!(w.snakes[0].effect_kind,kind as u8);}
        }
    }
    #[test]
    fn escape_prefers_a_surviving_phase_to_venom_and_skips_an_unusable_first_item() {
        let mut w=boxed(EffectKind::Venom);
        w.snakes[0].inventory=Inventory {kinds:[5,4,3],life:[1800;3],count:3,..Default::default()};
        let mut ai=Driver {ai:AiController::new()};w.step(&mut ai);
        assert_eq!(w.snakes[0].inventory.windup,3,"Frost cannot open this box; keep Venom when Phase survives");
        for _ in 1..STEPS {w.step(&mut ai);assert!(w.snakes[0].alive);}
        assert!(w.snakes[0].inventory.kinds.contains(&(EffectKind::Venom as u8)));
    }
    #[test]
    fn open_snake_keeps_every_escape_kind() {
        for kind in [EffectKind::Venom,EffectKind::Phase,EffectKind::Flip,EffectKind::Frost,EffectKind::Surge] {
            let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,width:8000.0,height:4000.0,
                density:0.0,aggression:0,world_events:false,deadly_walls:true,..Default::default()},
                &[(Point{x:4000.0,y:2000.0},0.0,20,0.0)],&[]).unwrap();
            w.snakes[0].inventory=Inventory {kinds:[kind as u8,0,0],life:[1800,0,0],count:1,..Default::default()};
            let mut ai=AiController::new();
            for _ in 0..30 {w.step(&mut ai);assert!(w.snakes[0].alive);assert_eq!(w.snakes[0].inventory.windup,0);}
            assert_eq!(w.snakes[0].inventory.count,1,"save {kind:?} without danger");
        }
    }
    #[test]
    fn windup_too_late_does_not_waste_an_escape() {
        for kind in [EffectKind::Venom,EffectKind::Phase] {
            let mut w=boxed(kind);
            let points:Vec<_>=w.snake(0).unwrap().segments.iter().map(|s|Point{x:s.current.x+95.0,y:s.current.y}).collect();
            w.diagnostic_body(0,&points,0.0).unwrap();
            let mut ai=Driver {ai:AiController::new()};let mut used=false;
            for _ in 0..5 {w.step(&mut ai);used|=w.frame_events().any(|e|e.kind==crate::EventKind::Use && e.snake_id==0);if !w.snakes[0].alive {break;}}
            assert!(!used,"cannot finish {kind:?} before unavoidable contact");
            assert!(!w.snakes[0].alive,"fixture must be too late for the wind-up");
        }
    }
    #[test]
    fn desktop_no_cut_venom_forecast_keeps_the_item() {
        // Reproduce the observed seed-404 request at tick 9154 under the old
        // proof, then compare the same World/controller snapshot. No fixture
        // serialization or production legacy-policy switch is needed.
        let mut w=World::new(Config {width:7920.0,height:1440.0,density:80.0,
            trails:100.0,scale:200.0,speed:300.0,intelligence:100.0,
            self_collisions:true,deadly_walls:true,palette_size:6,rules:RuleSet::V2,
            seed:404,..Default::default()}).unwrap();
        let mut legacy=AiController::new();legacy.legacy_escape_proof=true;
        for _ in 0..9153 {w.step(&mut legacy);}
        let s=w.snake(7).unwrap();assert_eq!(s.generation,2);
        assert!(s.inventory.kinds.contains(&(EffectKind::Venom as u8)));
        let mut fixed=legacy.clone();fixed.legacy_escape_proof=false;
        let mut fixed_world=w.diagnostic_snapshot();
        w.step(&mut legacy);fixed_world.step(&mut fixed);
        assert_eq!(w.snake(7).unwrap().inventory.windup,1,"observed legacy request");
        assert_eq!(fixed_world.snake(7).unwrap().inventory.windup,0,"no-cut path keeps Venom");
        let mut legacy_cut=false;let mut legacy_head_death=false;
        for _ in 9154..9196 {
            w.step(&mut legacy);fixed_world.step(&mut fixed);
            legacy_cut|=w.frame_events().any(|e|e.kind==crate::EventKind::Sever && e.other_snake_id==7 && e.other_generation==2);
            legacy_head_death|=w.collision_events().any(|e|e.victim==7 && e.generation==2 && e.reason==crate::DeathReason::Head);
        }
        assert!(!legacy_cut,"legacy activation never opened a corridor");
        assert!(legacy_head_death,"observed head death at tick 9196");
        assert_eq!(fixed_world.snake(7).unwrap().generation,2);
        assert!(fixed_world.snake(7).unwrap().alive,"ordinary path survives the observed failure");
    }

    #[test]
    fn public_debug_and_decision_follow_the_committed_escape() {
        for kind in [EffectKind::Venom,EffectKind::Phase,EffectKind::Flip] {
            let mut w=boxed(kind);let mut ai=AiController::new();ai.enable_diagnostics();
            w.step(&mut ai);
            let state=ai.states[0];let d=ai.decision(0);let selected=d.candidates[d.selected];
            assert!(w.snake(0).unwrap().alive);
            assert!((selected.desired-state.desired).abs()<1e-12,"{kind:?}: decision names the executed control");
            assert!((state.debug.safe_seconds-selected.safe_ticks as f64*STEP_SECONDS).abs()<1e-12,"{kind:?}");
            assert_eq!(state.debug.reachable_cells,selected.area as u32,"{kind:?}");
            assert!(state.debug.path_count>1);
            assert_ne!(state.use_slot,0,"{kind:?}: the boxed snake spends its item");
            // The ordinary winner dies early; the committed escape is certified.
            assert!(selected.safe_ticks>=d.horizon,"{kind:?}: safe={} horizon={}",selected.safe_ticks,d.horizon);
        }
    }

    #[cfg(feature="desktop-diag")]
    #[test]
    fn executed_observation_follows_item_and_free_escape_commits() {
        for kind in [EffectKind::Venom,EffectKind::Phase,EffectKind::Flip] {
            let mut w=boxed(kind);let mut ai=AiController::new();ai.enable_diagnostics();
            w.step(&mut ai);let d=ai.desktop_observation(0);
            assert_eq!(d.escape_item,kind as u8);
            assert!(w.distance_squared(d.next_head,w.snake(0).unwrap().segments[0].current)<1e-12);
            assert_eq!(d.target,ai.states[0].target);assert_eq!(d.goal,ai.states[0].goal);
            assert_eq!(d.tracking,ai.states[0].track_goal);
            assert_eq!(d.selected as usize,ai.decision(0).selected);
            assert_eq!(d.safe_ticks,ai.decision(0).candidates[ai.decision(0).selected].safe_ticks);
            assert!(d.ordinary_danger,"successful escape does not erase the ordinary danger");
        }
        let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,density:0.0,world_events:false,
            aggression:0,width:8000.0,height:4000.0,..Default::default()},
            &[(Point{x:4000.0,y:2000.0},0.0,20,0.0)],&[]).unwrap();
        w.snakes[0].inventory=Inventory {kinds:[4,0,0],life:[1800,0,0],count:1,..Default::default()};
        let mut ai=AiController::new();ai.steer(&w,w.snake(0).unwrap());
        let mut state=ai.states[0];state.set_target(123);state.goal=Point{x:4500.0,y:2000.0};
        let mut c=Candidate::default();ai.rollout_into(&w,w.snake(0).unwrap(),state,2,NORMAL_STEPS,&mut c);
        state.set_target(0);state.goal=Point{x:4100.0,y:2100.0};
        ai.desktop[0].target=123;ai.desktop[0].next_head=Point{x:-1.0,y:-1.0};ai.desktop[0].ordinary_danger=true;
        assert!(ai.choose_escape_item(&w,w.snake(0).unwrap(),&mut state,&Candidate::default(),NORMAL_STEPS));
        assert_eq!(state.use_slot,0,"alternate item-free continuation is committed");
        let d=ai.desktop_observation(0);assert_eq!(d.next_head,c.path[1]);
        assert_eq!(d.target,0);assert_eq!(d.goal,state.goal);assert!(d.ordinary_danger);
        assert_eq!(d.escape_item,0,"item-free continuation is not an activation");
    }

    #[cfg(feature="desktop-diag")]
    #[test]
    fn usable_items_excludes_no_cut_venom_and_equally_safe_free_paths() {
        let mut w=arena(EffectKind::Venom,1000.0);
        let mut ai=AiController::new();ai.steer(&w,w.snake(0).unwrap());
        let s=w.snake(0).unwrap();let state=ai.states[0];
        ai.remember_escape_decision(&w,s,state,&Candidate::default(),NORMAL_STEPS);
        let mut trial=Candidate::default();
        let proposal=ai.held_escape(&w,s,state,0,NORMAL_STEPS,&mut trial).unwrap();
        assert_eq!(proposal.use_slot,0,"fixture has a no-cut ordinary continuation");
        assert_eq!(ai.usable_escape_items(&w,s,1<<4),0);
        w.snakes[0].inventory.kinds[0]=EffectKind::Frost as u8;
        let s=w.snake(0).unwrap();ai.remember_escape_decision(&w,s,state,&Candidate::default(),NORMAL_STEPS);
        assert_eq!(ai.usable_escape_items(&w,s,1<<5),0,"actual admission keeps an equally safe free path");
    }

    #[cfg(feature="desktop-diag")]
    #[test]
    fn counterfactual_uses_recorded_horizon_and_pre_inventory_identity() {
        let mut w=boxed(EffectKind::Frost);w.config.self_collisions=true;
        let mut ai=AiController::new();ai.steer(&w,w.snake(0).unwrap());
        ai.states[0].turn_accum=1.0;ai.states[0].desired=3.0;
        ai.steer(&w,w.snake(0).unwrap());
        let s=w.snake(0).unwrap();
        assert_eq!(ai.desktop_observation(0).ordinary_horizon,STEPS,"scheduled enclosure-risk decision extends to 138");
        let state=ai.escape_decisions[0].unwrap().state;
        ai.remember_escape_decision(&w,s,state,&Candidate::default(),STEPS);
        assert_eq!(ai.escape_decisions[0].unwrap().horizon,STEPS);
        let mut trial=Candidate::default();
        let proof=ai.held_escape(&w,s,state,0,STEPS,&mut trial);
        let mut expected=0;
        if let Some(p)=proof.filter(|p|p.use_slot==1) {
            let mut free=p;free.use_slot=0;let mut ordinary=Candidate::default();
            ai.rollout_into(&w,s,free,trial.kind,STEPS,&mut ordinary);
            if !ai.escape_room(&w,s,&mut ordinary,STEPS) {expected=1<<5;}
        }
        // Mutating the post-inventory state cannot change the captured proof.
        ai.states[0].use_slot=3;ai.states[0].goal=Point{x:-100.0,y:-100.0};
        assert_eq!(ai.usable_escape_items(&w,s,1<<5),expected);
        ai.escape_decisions[0].as_mut().unwrap().generation+=1;
        assert_eq!(ai.usable_escape_items(&w,s,1<<5),0);
        ai.remember_escape_decision(&w,s,state,&Candidate::default(),STEPS);
        ai.escape_decisions[0].as_mut().unwrap().tick+=1;
        assert_eq!(ai.usable_escape_items(&w,s,1<<5),0);
    }

}
