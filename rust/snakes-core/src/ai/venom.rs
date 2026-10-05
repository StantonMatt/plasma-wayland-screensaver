// SPDX-License-Identifier: GPL-3.0-or-later
//! Value-weighted rear-body pursuit and head-facing counter-play, checked by the ordinary
//! wall, head, self-deposition and one-bite body rollouts. No unchecked steering.
use super::*;
impl AiController {
    // Certify later bites on the predicted physical trail, never on delayed
    // avoidance occupancy. The grid sample identifies a trail location; motion
    // changes which physical segment occupies it (and therefore the cut index).
    pub(super) fn venom_physical_bite(&self,w:&World,s:SnakeView<'_>,other:usize,
        sample:usize,a:Point,b:Point,start:usize,end:usize,effects:&forecast::Timeline,r:&Rival)->Option<usize> {
        let spacing=r.radius*1.18;
        // The release delay includes slowing/growth reserves. It is not a
        // physical prediction. During outstanding growth, certify on the exact
        // first-tick sweep only, rather than awarding an uncertain future cut.
        if w.forecast_growth_active(other) {return None;}
        let position=|index:usize,step:usize| {
            let behind=index as f64*spacing-r.distance[step];
            if behind>=0.0 {return w.forecast_trail_point(other,behind);}
            let ahead=-behind;let k=r.distance[..=step].partition_point(|&d|d<ahead).clamp(1,step);
            let t=((ahead-r.distance[k-1])/(r.distance[k]-r.distance[k-1]).max(0.0001)).clamp(0.0,1.0);
            let d=w.displacement(r.path[k-1],r.path[k]);
            w.canonical_point(Point{x:r.path[k-1].x+d.x*t,y:r.path[k-1].y+d.y*t})
        };
        let first=(sample as f64+r.distance[start]/spacing).floor() as usize;
        let last=((sample as f64+r.distance[end]/spacing).ceil() as usize).min(r.len-1);
        let effect=effects.at(s.id as usize,end);
        let immunity=w.faces[other].bite_immunity_ticks.saturating_sub(end.saturating_sub(1) as u16);
        for index in first..=last {
            if !crate::world::venom::bite_eligible(w.config().rules,effect.kind,effect.ticks,s.id as usize,other,
                index,r.len,immunity,effects.phased(other,end)) {continue;}
            let body=crate::world::taper::body_radius(r.radius,index as f64,r.len);
            let reach=crate::world::taper::contact_radius(w.config().rules,s.radius,body,false);
            if w.segments_distance_squared(a,b,position(index,start),position(index,end))<reach*reach {return Some(index);}
        }
        None
    }
    pub(super) fn venom_strike_goal(w:&World,s:SnakeView<'_>,r:SnakeView<'_>,index:usize,standoff:bool)->Point {
        let bite=r.segments[index].current;
        let trail=w.displacement(r.segments[index-1].current,bite);
        let norm=(trail.x*trail.x+trail.y*trail.y).sqrt().max(0.001);
        let behind=w.displacement(bite,s.segments[0].current);
        let side=if trail.x*behind.y-trail.y*behind.x>=0.0 {1.0} else {-1.0};
        // Under confrontation, first flank on the current side. Once a
        // forecast consumes the bite, it continues straight out of the trail.
        if !standoff {return bite;}
        let offset=5.0*s.radius;
        w.canonical_point(Point{x:bite.x-trail.y/norm*side*offset,
            y:bite.y+trail.x/norm*side*offset})
    }
    fn venom_local_plan(&self,w:&World,s:SnakeView<'_>)->Option<(VenomPlan,f64)> {
        let head=s.segments[0].current;let key=self.spatial.key(head);let reach=10.0*s.radius;
        let dx=(reach/self.spatial.dx).ceil() as isize;let dy=(reach/self.spatial.dy).ceil() as isize;
        let (speed,turn)=self.motion[s.id as usize].at(0);let mut best=None;let mut eta_best=f64::INFINITY;let mut visited=0;
        // Inspect the centre and adjacent cells first. A dense titan in a
        // far corner must not exhaust the budget before the nearby bite.
        for band in 0..=dx.max(dy) {for (x,y) in self.spatial.ring_offsets(band) {
            if x.abs()>dx || y.abs()>dy {continue;}
            let Some(cell)=self.spatial.offset(key,x,y) else {continue;};
            if self.spatial.occupied[cell]&!(1<<s.id)==0 {continue;}
            let mut at=self.spatial.heads[cell];
            while at>=0 {
                let encoded=at as usize;at=self.spatial.next[encoded];visited+=1;
                if visited>256 {return best;}
                let other=encoded/MAX_SEGMENTS;let index=encoded%MAX_SEGMENTS;
                if other==s.id as usize {continue;}
                let r=w.snake(other).unwrap();
                if !r.alive || !crate::world::venom::bite_eligible(w.config().rules,s.effect_kind,s.effect_ticks,s.id as usize,other,
                    index,r.segments.len(),r.face.bite_immunity_ticks,r.flags&crate::flags::PHASED!=0) {continue;}
                let d=w.displacement(head,r.segments[index].current);let distance=(d.x*d.x+d.y*d.y).sqrt();
                if distance>reach {continue;}
                let eta=distance/speed+normalize_angle(d.y.atan2(d.x)-s.angle).abs()/turn.max(0.01);
                if eta>=eta_best || eta>(s.effect_ticks-1) as f64*STEP_SECONDS*0.9 {continue;}
                let facing=w.displacement(r.segments[0].current,head);
                let standoff=w.distance_squared(head,r.segments[0].current)<(8.0*s.radius).powi(2)
                    && normalize_angle(facing.y.atan2(facing.x)-r.angle).abs()<std::f64::consts::FRAC_PI_4;
                eta_best=eta;best=Some((VenomPlan {goal:Self::venom_strike_goal(w,s,r,index,standoff),
                    target:other+1,generation:r.generation,index,standoff},eta));
            }
        }}
        best
    }
    #[cfg(test)]
    pub(super) fn venom_tactics(&self,w:&World,s:SnakeView<'_>,state:&mut State) {
        self.venom_tactics_cached(w,s,state,&mut None);
    }
    pub(super) fn venom_tactics_cached(&self,w:&World,s:SnakeView<'_>,state:&mut State,cache:&mut Option<VenomHunt>) {
        if w.config().rules!=crate::RuleSet::V2 {return;}
        let head=s.segments[0].current;
        state.venom_standoff=false;state.venom_alternative=None;
        if s.effect_kind==4 && s.effect_ticks>1 && aggression::level(w)>0.0 {
            // A large cut is valuable, but a smaller nearby rival is preferable
            // to letting the entire effect expire. Sample the severable body,
            // rather than using distance to its head or one rear-quarter point.
            let hunt=cache.get_or_insert_with(|| {
            #[cfg(test)]
            self.venom_searches.set(self.venom_searches.get()+1);
            let (speed,turn)=self.motion[s.id as usize].at(0);
            let remaining=(s.effect_ticks-1) as f64*STEP_SECONDS;
            let mut choices=[None;4];
            // Uniform samples are too coarse on a 1600-point titan. The
            // existing body grid supplies an exact nearby strike at bounded cost.
            let local=self.venom_local_plan(w,s);
            let mut nearest=local.map(|(plan,_)|plan);let mut nearest_eta=local.map_or(f64::INFINITY,|(_,eta)|eta);
            for r in w.snakes().filter(|r|r.alive && r.id!=s.id) {
                let len=r.segments.len();
                if !crate::world::venom::bite_eligible(w.config().rules,s.effect_kind,s.effect_ticks,
                    s.id as usize,r.id as usize,len.saturating_sub(1),len,
                    r.face.bite_immunity_ticks,r.flags&crate::flags::PHASED!=0) {continue;}
                let first=(len/2+1).max(4);
                for sample in 0..8 {
                    let index=first+(len-1-first)*sample/8;
                    let bite=r.segments[index].current;
                    let d=w.displacement(head,bite);
                    let distance=(d.x*d.x+d.y*d.y).sqrt();
                    let angle=normalize_angle(d.y.atan2(d.x)-s.angle).abs();
                    let eta=distance/speed+angle/turn.max(0.01);
                    // Leave time to strike. A single burst can close a marginal
                    // gap, but never select a target beyond the effect's clock.
                    if eta>remaining*0.9 {continue;}
                    let rh=r.segments[0].current;
                    let facing=w.displacement(rh,head);
                    let standoff=w.distance_squared(head,rh)<(8.0*s.radius).powi(2)
                        && normalize_angle(facing.y.atan2(facing.x)-r.angle).abs()<std::f64::consts::FRAC_PI_4;
                    // The nearest feasible body also gets a physical rollout.
                    // It can win when a valuable giant is inaccessible, rather
                    // than letting safety choose endless unrelated wandering.
                    if eta<nearest_eta {nearest_eta=eta;nearest=Some(VenomPlan {goal:Self::venom_strike_goal(w,s,r,index,standoff),
                        target:r.id as usize+1,generation:r.generation,index,standoff});}
                    let value=(len-index) as f64/(0.6+eta)
                        *if r.face.frozen_ticks as f64*STEP_SECONDS>eta {1.0+aggression::level(w)} else {1.0}
                        *(1.0+(len as f64/s.segments.len() as f64).min(3.0)*0.15)
                        *if standoff {0.3+0.3*aggression::bold(w)} else {1.0}
                        *if state.venom_target==r.id as usize+1 && state.venom_generation==r.generation {1.15} else {1.0};
                    for slot in 0..choices.len() {
                        if value>choices[slot].map_or(0.0,|(_,_,_,v)|v) {
                            for next in (slot+1..choices.len()).rev() {choices[next]=choices[next-1];}
                            choices[slot]=Some((r,index,standoff,value));break;
                        }
                    }
                }
            }
            let mut best=None;let mut best_value=0.0;
            for (r,index,standoff,value) in choices.into_iter().flatten() {
                let mut tests=0;
                let (blocked,_,capped)=self.body_blocked(w,s,head,r.segments[index].current,0.0,0.0,&mut tests);
                // A clear rear-body corridor beats a large cut hidden behind
                // a neck or our own coil. This is only a preference; moving
                // geometry and every published control retain full rollouts.
                let value=value*if capped {0.7} else if blocked {0.2} else {1.0};
                if value>best_value {best_value=value;best=Some((r,index,standoff));}
            }
            let best=best.map(|(r,index,standoff)|VenomPlan {
                goal:Self::venom_strike_goal(w,s,r,index,standoff),target:r.id as usize+1,
                generation:r.generation,index,standoff}).or(nearest);
            VenomHunt {best,nearest}
            });
            if let Some(plan)=hunt.best {
                state.clear_attacks(s.angle);state.clear_coil(s.angle);
                if state.venom_target!=plan.target || state.venom_generation!=plan.generation {
                    state.turn_accum=0.0;state.commit_until=0;state.turn_until=u64::MAX;
                    state.desired=s.angle;state.exit_angle=s.angle;
                    state.venom_target=plan.target;state.venom_generation=plan.generation;
                }
                state.venom_alternative=hunt.nearest;state.venom_index=plan.index;
                state.set_target(0);state.guarding=false;state.vulturing=false;state.waypoint=None;
                state.prey=plan.target;state.prey_generation=plan.generation;state.hunt_until=w.tick()+12;
                state.goal=plan.goal;
                state.venom_standoff=plan.standoff;
                state.track_goal=true;state.rush=0.0;
                state.debug.flags|=8;
                return;
            }
        }
        if state.venom_target!=0 {
            state.prey=0;state.venom_target=0;state.venom_generation=0;state.venom_index=0;state.clear_attacks(s.angle);
            state.track_goal=false;state.commit_until=0;state.turn_until=u64::MAX;
            state.desired=s.angle;state.exit_angle=s.angle;state.set_target(0);state.waypoint=None;
            state.goal=w.canonical_point(Point{x:head.x+s.angle.cos()*300.0,y:head.y+s.angle.sin()*300.0});
        }
        if s.face.bite_immunity_ticks>0 || s.flags&crate::flags::PHASED!=0 {return;}
        // Preparation already records active holders. Ordinary ticks should
        // not construct every rival's exported view just to reject its kind.
        let mut holders=self.initial_effects.venom_bits();
        while holders!=0 {
            let id=holders.trailing_zeros() as usize;holders&=holders-1;
            let Some(r)=w.snake(id) else {continue;};
            if !r.alive || !crate::world::venom::bite_eligible(w.config().rules,
                r.effect_kind,r.effect_ticks,r.id as usize,s.id as usize,s.segments.len()*78/100,
                s.segments.len(),s.face.bite_immunity_ticks,s.flags&crate::flags::PHASED!=0) {continue;}
            let rh=r.segments[0].current;let d=w.displacement(head,rh);
            let targets_me=r.face.prey==s.id;
            let tail=s.segments[s.segments.len()*78/100].current;
            let threat=w.displacement(rh,tail);let dist=(threat.x*threat.x+threat.y*threat.y).sqrt();
            let cone=dist<6.0*r.radius && normalize_angle(threat.y.atan2(threat.x)-r.angle).abs()<std::f64::consts::FRAC_PI_4;
            if (targets_me && d.x*d.x+d.y*d.y<(16.0*s.radius).powi(2)) || cone {
                state.clear_attacks(s.angle);state.set_target(0);state.waypoint=None;state.guarding=false;state.vulturing=false;
                // Show the head. Larger defenders can win the head-on; smaller
                // ones still run ordinary head safety and choose an escape.
                state.goal=rh;state.track_goal=true;state.rush=0.0;state.venom_standoff=true;
                if state.coil_radius>0.0 {state.coil_radius=(state.coil_radius*0.9).max(self.motion[s.id as usize].at(0).0/self.motion[s.id as usize].at(0).1);}
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn venom_growth_reserves_do_not_substitute_for_physical_bite_certification() {
        for growing in [false,true] {
            let mut w=crate::world::venom::tests::fixture(80,62);
            w.snakes[1].growth=if growing {100.0} else {0.01};
            let mut ai=AiController::new();ai.prepare(&w);ai.rivals[1].distance.fill(0.0);
            assert!(ai.rivals[1].growth_delay>0.0);
            assert_eq!(w.forecast_growth_active(1),growing);
            let s=w.snake(0).unwrap();let p=s.segments[0].current;
            let mut effects=ai.effects;let mut visits=0;
            let result=ai.body_blocked_effects::<true>(&w,s,p,p,2.0*STEP_SECONDS,0.0,&mut visits,false,&mut effects);
            assert_eq!(result.0,growing,"a future cut is unresolved only during actual stretching");
            assert_eq!(effects.consumed_at[0],if growing {0} else {2});
        }
    }
    #[test]
    fn checked_sever_exit_releases_moved_stump_but_keeps_a_stationary_stump() {
        let w=crate::world::venom::tests::fixture(80,62);let mut ai=AiController::new();ai.prepare(&w);
        let s=w.snake(0).unwrap();let stump=w.snake(1).unwrap().segments[61].current;
        for moved in [false,true] {
            ai.rivals[1].distance.fill(if moved {40.0} else {0.0});
            let mut effects=ai.effects;effects.sever_cut[1]=62;effects.bite_step[1]=1;effects.consumed_at[0]=1;
            let mut visits=0;
            assert!(2.0*STEP_SECONDS<STEP_SECONDS+ai.rivals[1].release_rate+0.15,
                "the former conservative stump reserve has not expired");
            let result=ai.body_blocked_effects::<true>(&w,s,stump,stump,2.0*STEP_SECONDS,0.0,&mut visits,false,&mut effects);
            assert_eq!(result.0,!moved,"only physical tail motion can release the retained stump");
        }
    }
    #[test]
    fn shared_bite_forecast_consumes_once_and_respects_immunity_phase_and_front_half() {
        for (cut,immunity,phase,expected) in [(62,0,false,false),(40,0,false,true),(62,1,false,true),(62,0,true,false)] {
            let mut w=crate::world::venom::tests::fixture(80,cut);w.faces[1].bite_immunity_ticks=immunity;
            if phase {w.snakes[1].effect_kind=3;w.snakes[1].effect_ticks=120;}
            let mut ai=AiController::new();ai.prepare(&w);let s=w.snake(0).unwrap();let p=s.segments[0].current;
            let mut timeline=ai.effects;let mut tests=0;
            let result=ai.body_blocked_effects::<true>(&w,s,p,p,STEP_SECONDS,0.0,&mut tests,false,&mut timeline);
            assert_eq!(result.0,expected,"cut={cut} immunity={immunity} phase={phase}");
            if !expected && !phase {
                assert_eq!(timeline.at(0,1).kind,0);assert!(timeline.sever_cut[1]>40);
                let stump=w.segments[MAX_SEGMENTS+timeline.sever_cut[1]-1].current;
                assert!(ai.body_blocked_effects::<true>(&w,s,stump,stump,2.0*STEP_SECONDS,0.0,&mut tests,false,&mut timeline).0);
            }
        }
    }
    #[test]
    fn surge_reserve_never_certifies_or_deepens_a_bite_outside_physical_contact() {
        for surge in [false,true] {for time in [STEP_SECONDS,2.0*STEP_SECONDS] {for contact in [false,true] {
            let mut w=crate::world::venom::tests::fixture(80,62);
            if surge {w.snakes[1].effect_kind=1;w.snakes[1].effect_ticks=180;}
            let p=w.segments[MAX_SEGMENTS+62].current;
            // A contiguous physical trail: sample 61 is inside the safety
            // reserve but outside its physical disk. Only 62 can touch.
            let head=Point{x:p.x,y:p.y+if contact {6.0} else {10.0}};
            w.segments[0]=crate::Segment {current:head,previous:head};
            let mut ai=AiController::new();ai.prepare(&w);
            // Freeze the physical forecast for the second-step reserve test;
            // moving endpoints are covered separately below.
            if time>STEP_SECONDS {ai.rivals[1].distance.fill(0.0);}
            let mut timeline=ai.effects;let mut visits=0;
            let result=ai.body_blocked_effects::<true>(&w,w.snake(0).unwrap(),head,head,time,0.0,&mut visits,false,&mut timeline);
            assert!(!result.0,"eligible rear reserve alone is not a lethal body");
            assert!(visits>0,"the reserve must still query nearby rear geometry");
            assert_eq!(timeline.sever_cut[1],if contact {62} else {0},"surge={surge} time={time} contact={contact}");
            assert_eq!(timeline.consumed_at[0],if contact {phase::step(time)} else {0});
            // Mechanics has no Surge radius multiplier and certifies exactly the
            // same cut, or preserves the charge when neither point touches.
            crate::world::venom::tests::resolve_contacts(&mut w);
            assert_eq!(w.snakes[1].len,if contact {62} else {80});
            assert_eq!(w.snakes[0].effect_kind,if contact {0} else {4});
        }}}
    }
    #[test]
    fn narrow_local_hunt_visits_wrapped_cells_once_before_its_record_cap() {
        for one_row in [false,true] {
            let mut w=crate::world::venom::tests::fixture(300,200);
            w.reconfigure(crate::Config {width:480.0,height:80.0,deadly_walls:false,..w.config()}).unwrap();
            let mut ai=AiController::new();ai.prepare(&w);
            // Also exercise the supported one-row grid directly. Valid Config
            // geometry gives three rows, where band three repeats the centre.
            if one_row {ai.spatial.rows=1;ai.spatial.dy=80.0;} else {assert_eq!(ai.spatial.rows,3);}
            let dx=ai.spatial.dx;let head=Point{x:8.0*dx-0.5,y:40.0};
            let target=Point{x:if one_row {8.0*dx+0.5} else {10.0*dx+0.5},y:40.0};
            w.segments[0]=crate::Segment {current:head,previous:head};w.snakes[0].angle=0.0;
            for j in 0..300 {
                let p=if (1..=150).contains(&j) {Point{x:7.5*dx,y:40.0}}
                    else if j==155 {target} else {Point{x:dx*0.5,y:40.0}};
                w.segments[MAX_SEGMENTS+j]=crate::Segment {current:p,previous:p};
            }
            ai.spatial.heads.fill(-1);ai.spatial.occupied.fill(0);
            for j in 1..300 {
                let key=ai.spatial.key(w.segments[MAX_SEGMENTS+j].current);let encoded=MAX_SEGMENTS+j;
                ai.spatial.next[encoded]=ai.spatial.heads[key];ai.spatial.heads[key]=encoded as i32;ai.spatial.occupied[key]|=2;
            }
            let (plan,_)=ai.venom_local_plan(&w,w.snake(0).unwrap()).expect("151 unique local records fit the 256-record budget");
            assert_eq!(plan.index,155,"one_row={one_row}");
        }
    }
    #[test]
    fn later_bites_use_moving_physical_trails_instead_of_conservative_release() {
        let mut w=crate::world::venom::tests::fixture(80,79);
        let old=w.segments[MAX_SEGMENTS+79].current;
        let mut ai=AiController::new();ai.prepare(&w);
        ai.rivals[1].speed=300.0;ai.rivals[1].distance[1]=10.0;ai.rivals[1].distance[2]=20.0;
        let s=w.snake(0).unwrap();let mut effects=ai.effects;let mut visits=0;
        assert!(2.0*STEP_SECONDS<ai.rivals[1].release_rate+0.15,"avoidance still reserves the old endpoint");
        assert!(!ai.body_blocked_effects::<true>(&w,s,old,old,2.0*STEP_SECONDS,0.0,&mut visits,false,&mut effects).0);
        assert_eq!(effects.sever_cut[1],0);assert_eq!(effects.consumed_at[0],0);
        // Candidate pickups can change a rival's speed independently of the
        // shared occupancy forecast. Certification must use that scenario.
        let mut scratch=ai.rivals;scratch[1].dynamic=true;scratch[1].distance.fill(0.0);
        let mut scenario=ai.effects;visits=0;
        assert!(!ai.body_blocked_effects_from::<true>(&w,s,old,old,2.0*STEP_SECONDS,0.0,&mut visits,false,&mut scenario,1,Some(&scratch)).0);
        assert_eq!(scenario.sever_cut[1],79);
        let contact=Point{x:old.x+15.0,y:old.y};let mut effects=ai.effects;visits=0;
        assert!(!ai.body_blocked_effects::<true>(&w,s,contact,contact,2.0*STEP_SECONDS,0.0,&mut visits,false,&mut effects).0);
        assert!(effects.sever_cut[1]>40,"a contact on the moving tail is still certified");
        for point in &mut w.segments[MAX_SEGMENTS..MAX_SEGMENTS+80] {
            point.previous=Point{x:point.current.x+10.0,y:point.current.y};point.current.x+=20.0;
        }
        w.segments[0]=crate::Segment {current:old,previous:old};
        crate::world::venom::tests::resolve_contacts(&mut w);
        assert_eq!(w.snakes[1].len,80);assert_eq!(w.snakes[0].effect_kind,4);
        w.segments[0]=crate::Segment {current:contact,previous:contact};
        crate::world::venom::tests::resolve_contacts(&mut w);
        assert_eq!(w.snakes[1].len,effects.sever_cut[1]);
    }
    #[test]
    fn forecast_keeps_same_step_collision_dimensions_then_retapers_next_step() {
        for bite_first in [false,true] {
            let w=crate::world::venom::tests::boundary_contact_fixture(bite_first);
            let mut ai=AiController::new();ai.prepare(&w);let s=w.snake(0).unwrap();let head=s.segments[0].current;
            // Explicit already-consumed strike also exercises shared contact
            // dimensions independently of which grid enumerates the bite first.
            let mut timeline=ai.effects;timeline.sever_cut[1]=62;timeline.bite_step[1]=1;timeline.consumed_at[0]=1;
            let mut tests=0;
            let result=ai.body_blocked_effects::<true>(&w,s,head,head,STEP_SECONDS,0.0,&mut tests,false,&mut timeline);
            assert!(result.0);assert!(result.1<0.0,"same-step contact retains the original physical width");
        }
    }
    #[test]
    fn venom_hunt_refreshes_once_per_decision_and_on_unscheduled_holder_ticks() {
        let w=crate::world::venom::tests::hunt_fixture();let mut ai=AiController::new();ai.prepare(&w);
        let s=w.snake(0).unwrap();let mut state=State::default();let mut cache=None;
        ai.strategy_with_venom(&w,s,&mut state,1.0,&mut cache);
        ai.venom_tactics_cached(&w,s,&mut state,&mut cache);
        ai.strategy_with_venom(&w,s,&mut state,1.0,&mut cache);
        assert_eq!(ai.venom_searches.get(),1,"scheduled and urgent strategies reapply a cached hunt");
        ai.venom_tactics_cached(&w,s,&mut state,&mut None);
        assert_eq!(ai.venom_searches.get(),2,"the next decision refreshes immediately");
        // A scheduled strategy can return early for an escape. Its post-strategy
        // refresh must still search once, instead of trusting the schedule bit.
        state.escape_until=45;let mut cache=None;
        ai.strategy_with_venom(&w,s,&mut state,1.0,&mut cache);
        ai.venom_tactics_cached(&w,s,&mut state,&mut cache);
        assert_eq!(ai.venom_searches.get(),3);
    }
    #[test]
    #[ignore = "pinned microbenchmark; run explicitly with --ignored --nocapture"]
    fn benchmark_venom_decision_hunt_refresh() {
        let w=crate::world::venom::tests::fixture(1600,1248);let mut ai=AiController::new();ai.prepare(&w);
        let s=w.snake(0).unwrap();let iterations=20000;let mut uncached=Vec::new();let mut cached=Vec::new();
        for pair in 0..7 {for mode in [pair%2,(pair+1)%2] {
            let start=std::time::Instant::now();
            for _ in 0..iterations {
                let mut state=State::default();let mut cache=None;
                ai.venom_tactics_cached(&w,s,&mut state,&mut cache);
                if mode==0 {cache=None;}
                ai.venom_tactics_cached(&w,s,&mut state,&mut cache);
                std::hint::black_box(state);
            }
            let us=start.elapsed().as_secs_f64()*1e6/iterations as f64;
            if mode==0 {uncached.push(us);} else {cached.push(us);}
        }}
        uncached.sort_by(f64::total_cmp);cached.sort_by(f64::total_cmp);
        println!("hunt_us uncached={:.3} cached={:.3} saving={:.3}",uncached[3],cached[3],uncached[3]-cached[3]);
    }
    #[test]
    fn local_grid_finds_precise_titan_body_points_between_uniform_samples() {
        let mut w=crate::world::venom::tests::fixture(1600,1248);let mut ai=AiController::new();ai.prepare(&w);
        let (plan,_)=ai.venom_local_plan(&w,w.snake(0).unwrap()).expect("a close titan body is a strike opportunity");
        assert_eq!(plan.target,2);assert!(plan.index.abs_diff(1248)<9);
        w.faces[1].bite_immunity_ticks=60;
        assert!(ai.venom_local_plan(&w,w.snake(0).unwrap()).is_none());
    }
    #[test]
    fn reachable_unsampled_local_strike_becomes_the_primary_hunt() {
        let mut w=crate::world::venom::tests::fixture(1600,1248);
        w.snakes[0].effect_ticks=3;w.snakes[0].angle=-std::f64::consts::FRAC_PI_2;
        let mut ai=AiController::new();ai.prepare(&w);let s=w.snake(0).unwrap();
        let (speed,turn)=ai.motion[0].at(0);let remaining=(s.effect_ticks-1) as f64*STEP_SECONDS*0.9;
        for sample in 0..8 {
            let index=801+(1599-801)*sample/8;
            let d=w.displacement(s.segments[0].current,w.snake(1).unwrap().segments[index].current);
            assert!((d.x*d.x+d.y*d.y).sqrt()/speed+normalize_angle(d.y.atan2(d.x)-s.angle).abs()/turn>remaining);
        }
        let (plan,eta)=ai.venom_local_plan(&w,s).expect("unsampled strike is reachable");
        assert!(eta<=remaining);
        let mut state=State::default();ai.venom_tactics(&w,s,&mut state);
        assert_eq!((state.venom_target,state.venom_generation,state.venom_index),(plan.target,plan.generation,plan.index));
        assert_eq!(state.goal,plan.goal);assert_eq!(state.prey,plan.target);assert!(state.track_goal);
        let mut candidate=Candidate::default();ai.rollout_into(&w,s,state,0,12,&mut candidate);
        assert!(candidate.tracks_goal,"local fallback enters the ordinary physical rollout");
    }
    #[test]
    fn forecast_checks_self_front_half_and_second_victim_after_a_bite() {
        for owner in 0..3 {for bite_first in [true,false] {
            let w=crate::world::venom::tests::multi_contact_fixture(owner,bite_first);
            let mut ai=AiController::new();ai.prepare(&w);let s=w.snake(0).unwrap();let p=s.segments[0].current;
            let mut timeline=ai.effects;let mut tests=0;
            assert!(ai.body_blocked_effects::<true>(&w,s,p,p,STEP_SECONDS,0.0,&mut tests,false,&mut timeline).0,
                "owner={owner} bite_first={bite_first}");
        }}
    }
    #[test]
    fn forecast_multi_point_bite_chooses_the_same_deepest_contact_as_mechanics() {
        let w=crate::world::venom::tests::overlap_fixture();let mut ai=AiController::new();ai.prepare(&w);
        let s=w.snake(0).unwrap();let p=s.segments[0].current;let mut timeline=ai.effects;let mut tests=0;
        assert!(!ai.body_blocked_effects::<true>(&w,s,p,p,STEP_SECONDS,0.0,&mut tests,false,&mut timeline).0);
        assert_eq!(timeline.sever_cut[1],61);
    }
    #[test]
    fn forecast_sever_retapers_the_retained_stump_and_expires_old_samples() {
        let w=crate::world::venom::tests::fixture(80,62);
        let mut ai=AiController::new();ai.prepare(&w);let s=w.snake(0).unwrap();
        let stump=w.snake(1).unwrap().segments[61].current;
        let outside=Point{x:stump.x,y:stump.y+9.0};let mut tests=0;
        let mut timeline=ai.effects;timeline.sever_cut[1]=62;timeline.bite_step[1]=1;timeline.consumed_at[0]=1;
        assert!(!ai.body_blocked_effects::<true>(&w,s,outside,outside,2.0*STEP_SECONDS,0.0,&mut tests,false,&mut timeline).0,
            "the severed endpoint has tail taper, not its former mid-body width");
        tests=0;
        assert!(ai.body_blocked_effects::<true>(&w,s,stump,stump,2.0*STEP_SECONDS,0.0,&mut tests,false,&mut timeline).0,
            "the physical stump still blocks a direct contact");
        tests=0;
        let expiry=STEP_SECONDS+3.0*ai.rivals[1].release_rate+0.16;
        assert!(!ai.body_blocked_effects::<true>(&w,s,stump,stump,expiry,0.0,&mut tests,false,&mut timeline).0,
            "old stump samples leave after the shorter retained trail moves on");
    }
    #[test]
    fn venom_target_and_standoff_reset_on_identity_change() {
        let mut w=crate::world::venom::tests::fixture(80,62);let mut ai=AiController::new();ai.prepare(&w);
        let mut state=State::default();ai.venom_tactics(&w,w.snake(0).unwrap(),&mut state);
        assert_eq!(state.venom_target,2);assert_eq!(state.prey,2);assert!(state.track_goal);
        state.venom_standoff=true;w.faces[1].bite_immunity_ticks=60;
        ai.venom_tactics(&w,w.snake(0).unwrap(),&mut state);
        assert_eq!(state.venom_target,0);assert!(!state.venom_standoff);
        w.faces[1].bite_immunity_ticks=0;w.snakes[1].generation+=1;
        ai.venom_tactics(&w,w.snake(0).unwrap(),&mut state);assert_eq!(state.venom_generation,w.snakes[1].generation);
    }
    #[test]
    fn hunts_severable_smaller_rivals_and_drops_stale_controls_on_expiry() {
        let mut w=crate::world::venom::tests::fixture(16,12);
        let mut ai=AiController::new();ai.prepare(&w);
        let mut state=State {commit_until:1000,turn_until:1000,desired:2.0,..State::default()};
        ai.venom_tactics(&w,w.snake(0).unwrap(),&mut state);
        assert_eq!(state.venom_target,2);assert_eq!(state.commit_until,0);assert_eq!(state.turn_until,u64::MAX);
        w.snakes[0].effect_ticks=0;
        ai.venom_tactics(&w,w.snake(0).unwrap(),&mut state);
        assert_eq!(state.venom_target,0);assert_eq!(state.prey,0);assert!(!state.track_goal);
    }
    #[test]
    fn aligned_venom_pursuit_requests_checked_boost_and_rejects_unreachable_targets() {
        let mut w=crate::world::venom::tests::fixture(160,124);
        let bite=w.segments[MAX_SEGMENTS+100].current;
        w.segments[0]=crate::Segment {current:Point{x:bite.x,y:bite.y+150.0},previous:Point{x:bite.x,y:bite.y+150.0}};
        w.snakes[0].angle=-std::f64::consts::FRAC_PI_2;
        let mut ai=AiController::new();ai.prepare(&w);let mut state=State::default();let s=w.snake(0).unwrap();
        ai.venom_tactics(&w,s,&mut state);assert_eq!(state.venom_target,2);
        assert!(state.goal.x>=w.segments[MAX_SEGMENTS+124].current.x,"prefer the valuable middle over the tip");
        state.goal=bite;assert_eq!(ai.rush_for(&w,s,state),0.6);
        state.venom_standoff=true;assert_eq!(ai.rush_for(&w,s,state),0.0);
        w.snakes[0].effect_ticks=2;
        ai.venom_tactics(&w,w.snake(0).unwrap(),&mut state);assert_eq!(state.venom_target,0);
    }
    #[test]
    fn venom_holder_severs_a_moving_body_and_survives_its_exit() {
        for speed in [100.0,230.0] {
        let mut w=crate::world::venom::tests::hunt_fixture();
        // This duel asserts the original personality policy. Full-aggression
        // defenders can counter-hunt instead of exposing the same rear body.
        w.reconfigure(crate::Config {speed,aggression:50,..w.config()}).unwrap();
        let mut ai=AiController::new();let mut bitten=false;let mut exit_until=0;
        for _ in 0..240 {
            w.step(&mut ai);
            if w.frame_events().any(|e|e.kind==crate::EventKind::Sever && e.other_snake_id==0) {bitten=true;exit_until=w.tick()+12;}
            if exit_until>0 && w.tick()>=exit_until {break;}
        }
        assert!(bitten,"held Venom should close and sever the accessible moving rear body");
        assert!(w.snakes[0].alive,"strike exit must retain ordinary safety checks");
        }
    }
    #[test]
    fn aggressive_venom_close_intercept_keeps_an_exit_at_real_speed() {
        struct Attacker {ai:AiController}
        impl Controller for Attacker {
            fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
                if s.id==0 {self.ai.steer(w,s)} else {Steering {desired_angle:s.angle,rush:0.0}}
            }
            fn face_intent(&self,id:u32)->crate::controller::FaceIntent {
                if id==0 {self.ai.face_intent(id)} else {crate::controller::FaceIntent::default()}
            }
        }
        for speed in [100.0,230.0] {
            let mut w=crate::world::venom::tests::hunt_fixture();
            w.reconfigure(crate::Config {speed,aggression:100,..w.config()}).unwrap();
            // Place the holder within a short checked intercept of the rear
            // half, still outside contact. This tests the offensive strike
            // and exit rather than whether a distant chase wins admission.
            let body:[Point;20]=std::array::from_fn(|j|Point {
                x:w.segments[j].current.x-100.0,y:w.segments[j].current.y-70.0});
            w.diagnostic_body(0,&body,w.snakes[0].angle).unwrap();
            let mut ai=Attacker {ai:AiController::new()};ai.ai.enable_diagnostics();let mut bitten=false;let mut exit_until=0;
            for _ in 0..240 {
                w.step(&mut ai);
                if w.frame_events().any(|e|e.kind==crate::EventKind::Sever && e.other_snake_id==0) {
                    let decision=ai.ai.decision(0);let selected=&ai.ai.candidates.as_ref().unwrap()[decision.selected];
                    assert!(selected.venom_bite>0 && selected.steps>=selected.venom_bite+VENOM_EXIT_STEPS && !selected.capped,
                        "speed{speed}: strike must have a checked exit");
                    bitten=true;exit_until=w.tick()+VENOM_EXIT_STEPS as u64;
                }
                if exit_until>0 && w.tick()>=exit_until {break;}
            }
            assert!(bitten,"aggression100 must use held Venom against an accessible moving body at speed{speed}");
            assert!(w.snakes[0].alive,"the aggressive bite must retain an exit at speed{speed}");
        }
    }
    #[test]
    fn alternative_venom_candidate_commits_its_target_identity_and_strike_point() {
        let w=crate::world::venom::tests::fixture(80,62);let s=w.snake(0).unwrap();
        let plan=VenomPlan {goal:Point{x:50.0,y:60.0},target:3,generation:9,index:32,standoff:false};
        let mut state=State {venom_target:2,venom_generation:7,prey:2,prey_generation:7,turn_accum:2.0,..Default::default()};
        let c=Candidate {venom_goal:Some(plan),tracks_goal:true,desired:0.2,area:64,..Default::default()};
        State::commit_candidate(&mut state,&c,8,&w,s,2.0,24);
        assert_eq!((state.venom_target,state.prey,state.venom_generation,state.prey_generation),(3,3,9,9));
        assert_eq!(state.venom_index,32);assert_eq!(state.goal,plan.goal);assert_eq!(state.turn_accum,0.0);
        assert!(state.track_goal);
    }
    #[test]
    fn venom_capsules_are_worth_pursuing_when_a_severable_body_is_nearby() {
        let w=crate::world::venom::tests::fixture(80,62);let mut ai=AiController::new();ai.prepare(&w);
        let s=w.snake(0).unwrap();
        let near=crate::Item {kind:crate::effects::EffectKind::Venom,position:w.snake(1).unwrap().segments[62].current,life_ticks:750,..Default::default()};
        let far=crate::Item {position:Point{x:2000.0,y:3000.0},..near};
        assert!(ai.capsule_value(&w,s,&near,1.0)>2.0*ai.capsule_value(&w,s,&far,1.0));
    }
    #[test]
    fn confronted_holder_flanks_and_defender_faces_the_biter() {
        let mut w=crate::world::venom::tests::fixture(80,62);
        let h=w.segments[0].current;
        let rh=Point{x:h.x+10.0,y:h.y};
        w.segments[MAX_SEGMENTS]=crate::Segment {current:rh,previous:rh};w.snakes[1].angle=std::f64::consts::PI;
        let mut ai=AiController::new();ai.prepare(&w);let mut state=State {coil_sign:1.0,..State::default()};
        ai.venom_tactics(&w,w.snake(0).unwrap(),&mut state);
        assert!(state.venom_standoff);assert!(state.track_goal);
        w.faces[0].prey=1;let mut defender=State::default();
        ai.venom_tactics(&w,w.snake(1).unwrap(),&mut defender);
        assert!(defender.venom_standoff);assert_eq!(defender.goal,h);
    }

}
