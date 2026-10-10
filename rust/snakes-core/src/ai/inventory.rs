// SPDX-License-Identifier: GPL-3.0-or-later
//! Inventory decisions reuse the current safety plan, rival rows and food grid.
use super::*;
use crate::effects::EffectKind;
impl AiController {
    pub(super) fn choose_inventory(&mut self,w:&World,s:SnakeView<'_>,state:&mut State,baseline:&Candidate,horizon:usize) {
        state.use_slot=0;
        let inv=s.inventory;
        if w.config().rules!=crate::RuleSet::V2 || !w.config().power_ups
            || inv.count==0 || inv.windup!=0 || inv.cooldown!=0 {return;}
        let danger=baseline.steps<horizon || state.debug.flags&32!=0
            || s.flags&crate::flags::TRAPPED!=0 || w.tick()<state.escape_until;
        if danger {
            // Search every held escape before applying offence/expiry utility.
            // A failed first item must not hide another certified escape.
            if self.choose_escape_item(w,s,state,baseline,horizon) {return;}
            // No proof survived the wind-up. Spending a rescue speculatively
            // cannot improve the chosen emergency prefix.
            return;
        }
        if !danger && w.tick()%3!=s.id as u64%3 {return;}
        let offensive=aggression::level(w);
        let mut kinds=0u8;
        for slot in 0..inv.count as usize {
            if inv.life[slot]>4 && (1..=7).contains(&inv.kinds[slot]) {kinds|=1<<inv.kinds[slot];}
        }
        if kinds==0 {return;}
        let flip=kinds&(1<<EffectKind::Flip as u8)!=0;
        let evaluate_flip=flip && (danger || state.last_strategy==w.tick() || self.flip_nearby(w,s));
        let (flip_situation,ambush_mask,flip_goal)=if evaluate_flip {self.flip_opportunity(w,s,baseline)} else {(0,0,None)};
        let phase=kinds&(1<<EffectKind::Phase as u8)!=0;
        let surge=kinds&(1<<EffectKind::Surge as u8)!=0;
        let frost=kinds&(1<<EffectKind::Frost as u8)!=0;
        let whirlpool=kinds&(1<<EffectKind::Whirlpool as u8)!=0;
        let brew=if whirlpool && w.vortex().is_none() && !danger && baseline.steps>=5 {
            self.whirlpool_nutrition(w,baseline.path[5])
        } else {0.0};
        let magnet=kinds&(1<<EffectKind::Magnet as u8)!=0;
        let evaluate_venom=kinds&(1<<EffectKind::Venom as u8)!=0 && offensive>0.0;
        // A saved Phase in open space has zero utility. Its inventory cannot
        // use rival/food evidence, so don't prepare those queries for it.
        if !whirlpool && !flip && phase && !surge && !frost && !magnet && !evaluate_venom
            && !(danger && baseline.steps>=4 || offensive>0.5 && state.track_goal && state.prey!=0 && baseline.steps<horizon) {return;}
        let head=s.segments[0].current;let r=s.radius;
        let mut close=0;let mut hunter=false;let mut venom=false;let mut venom_near=false;
        if surge || frost || evaluate_venom {for rival in w.snakes().filter(|o|o.alive && o.id!=s.id) {
            let d=w.displacement(rival.segments[0].current,head);
            let distance=(d.x*d.x+d.y*d.y).sqrt();
            if frost && crate::effects::frost::freeze_eligible(true,false,rival.face.frozen_ticks,rival.face.thaw_immunity_ticks,
                distance*distance,w.config().base_radius()) {close+=1;}
            hunter|=(surge || frost) && distance<8.0*r && d.x*rival.angle.cos()+d.y*rival.angle.sin()>distance*0.55
                && rival.segments.len()>s.segments.len();
            if evaluate_venom && rival.segments.len()>7 && rival.face.bite_immunity_ticks==0 && rival.flags&crate::flags::PHASED==0 {
                let idx=(rival.segments.len()*3/4).max(4);
                let p=rival.segments[idx].current;let d=w.displacement(head,p);
                let distance=(d.x*d.x+d.y*d.y).sqrt();
                venom_near|=distance<40.0*r;
                let tangent=w.displacement(rival.segments[(idx+1).min(rival.segments.len()-1)].current,p);
                // Approach the rear from its trailing side, never bite head-on.
                venom|=distance<10.0*r && rival.segments.len()*10>=s.segments.len()*13
                    && d.x*tangent.x+d.y*tangent.y>0.0
                    && normalize_angle(d.y.atan2(d.x)-s.angle).abs()<0.9;
            }
        }}
        let density=if magnet {self.spatial.cluster_weight(self.spatial.key(head))} else {0.0};
        let eta=if state.target!=0 && (surge || magnet && density>=3.0 && density<6.0) {
            w.distance_squared(head,state.goal).sqrt()/self.motion[s.id as usize].at(0).0.max(1.0)
        } else {f64::INFINITY};
        let contested=eta.is_finite() && w.snakes().filter(|o|o.alive && o.id!=s.id).any(|o| {
            let rival_eta=w.distance_squared(o.segments[0].current,state.goal).sqrt()/self.motion[o.id as usize].at(0).0.max(1.0);
            eta.max(rival_eta)<=eta.min(rival_eta)*1.25 && eta<3.0
        });
        let cutoff=state.attack.valid || (state.prey!=0 && state.track_goal && w.tick()>=state.escape_until);
        let mut chosen=None;let mut value=0.0;
        for slot in 0..inv.count as usize {
            if inv.life[slot]<=4 {continue;}
            let kind=EffectKind::from_byte(inv.kinds[slot]);let expiring=inv.life[slot]<=150;
            let score=match kind {
                EffectKind::Whirlpool if brew>=Self::whirlpool_minimum(inv.life[slot])
                    && (inv.life[slot]<=30 || self.essence.is_none() && self.whirlpool_audience(w,s,baseline.path[5]))=>100.0+brew*0.12,
                EffectKind::Flip if flip_situation==1=>900.0,
                EffectKind::Flip if flip_situation==2=>180.0*offensive,
                EffectKind::Flip if flip_situation==3=>100.0*(0.3+offensive),
                EffectKind::Phase if danger && baseline.steps>=4=>1000.0,
                EffectKind::Phase if offensive>0.5 && state.track_goal && state.prey!=0 && baseline.steps<horizon=>120.0*offensive,
                EffectKind::Surge if hunter || danger=>200.0,
                EffectKind::Surge if contested || cutoff=>150.0*(0.3+offensive),
                EffectKind::Surge if expiring && state.track_goal=>50.0,
                EffectKind::Frost if hunter || close>=2=>180.0*(0.4+offensive),
                EffectKind::Frost if close>0 && (expiring || cutoff)=>80.0*(0.2+offensive),
                EffectKind::Venom if venom && offensive>0.0=>140.0*offensive,
                EffectKind::Venom if expiring && venom_near && offensive>0.0=>40.0*offensive,
                EffectKind::Magnet if density>=6.0 || (density>=3.0 && contested)=>100.0,
                EffectKind::Magnet if expiring && density>0.0=>40.0,
                _=>0.0,
            };
            // Avoid replacing a still useful effect merely to spend another.
            let score=if !matches!(kind,EffectKind::Frost|EffectKind::Flip|EffectKind::Whirlpool) && s.effect_ticks>30 && !danger {score*0.25} else {score};
            let score=score+if score>0.0 && expiring {25.0} else {0.0};
            if score>value {value=score;chosen=Some(slot);}
        }
        let Some(slot)=chosen else {return;};
        let kind=EffectKind::from_byte(inv.kinds[slot]);
        if kind==EffectKind::Flip && flip_situation>0 {self.flip_note(20+flip_situation as usize);}
        let mut proposal=*state;proposal.use_slot=slot as u8+1;
        if kind==EffectKind::Flip {proposal.revise_opponents=true;}
        // Never assume Phase during the four corporeal wind-up movements.
        // Require a checked continuation, including Surge's new turn radius.
        let mut trial=Candidate::default();
        let proof_horizon=if kind==EffectKind::Phase {STEPS} else {horizon};
        let flip_clock=(self.profile_enabled && kind==EffectKind::Flip).then(std::time::Instant::now);
        self.rollout_into(w,s,proposal,1,proof_horizon,&mut trial);
        if let Some(c)=flip_clock {self.flip_profile[2]+=c.elapsed().as_nanos();}
        if !trial.wall_safe || trial.capped || trial.steps<proof_horizon || (trial.venom_bite>0 && trial.steps<trial.venom_bite+VENOM_EXIT_STEPS) {
            if kind==EffectKind::Flip {self.flip_note(if !trial.wall_safe {12} else if trial.capped {13} else {14});}
            return;
        }
        // Use the checked completion endpoint, not the current head or a
        // straight projection that ignores the selected turn and boost.
        if kind==EffectKind::Whirlpool && self.whirlpool_nutrition(w,trial.path[5])
            <Self::whirlpool_minimum(inv.life[slot]) {return;}
        if kind==EffectKind::Flip && flip_situation==2 && trial.flip_head_wins&ambush_mask==0 {
            self.flip_note(20);return;
        }
        // A speed/effect change needs a fresh continuation utility. Do not
        // borrow the ordinary candidate's space hint for the changed endpoint.
        let id=s.id as usize;
        let need=(s.segments.len() as f64*r*r*8.0/(self.spatial.dx*self.spatial.dy)).ceil().max(24.0) as usize;
        let mask=self.candidate_mask(w,id,&trial);
        let release=trial.effects.space_time(mask,trial.steps as f64*STEP_SECONDS,&self.rivals);
        let (area,_)=if w.config().self_collisions && s.segments.len()>=40 {
            let rate=r*1.18/(w.motion_limits(id,state.rush).unwrap().0*STEP_SECONDS).max(0.1);
            self.spatial.trajectory_space(trial.path[trial.steps],mask,(need*2).max(64),release,
                &trial.path[..=trial.steps],(rate*10.0).ceil() as usize,
                (trial.effects.body_time(id,s.segments.len() as f64*self.rivals[id].release_rate+self.rivals[id].growth_delay,&self.rivals[id])/STEP_SECONDS).ceil() as usize)
        } else {self.spatial.space(trial.path[trial.steps],mask,(need*2).max(64),release)};
        if area<need {if kind==EffectKind::Flip {self.flip_note(15);}return;}
        if kind==EffectKind::Phase && danger && baseline.steps>=horizon && s.flags&crate::flags::TRAPPED==0 && state.debug.flags&32==0 {return;}
        if kind==EffectKind::Flip {self.flip_note(16+flip_situation as usize);}
        state.use_slot=slot as u8+1;
        state.flip_situation=if kind==EffectKind::Flip {flip_situation} else {0};
        if kind==EffectKind::Flip {
            state.flip_prey=if flip_situation==2 {(trial.flip_head_wins&ambush_mask).trailing_zeros() as u8+1} else {0};
            state.flip_prey_generation=if state.flip_prey!=0 {w.snake(state.flip_prey as usize-1).unwrap().generation} else {0};
            state.flip_goal=flip_goal.unwrap_or_default();state.flip_request_tick=w.tick();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;
    #[test]
    fn inventory_phase_is_saved_in_the_open_and_surge_expiry_can_be_used() {
        let mut w=World::diagnostic_arena(Config {rules:crate::RuleSet::V2,width:4000.0,height:2000.0,
            density:0.0,deadly_walls:false,..Default::default()},&[(Point{x:1500.0,y:1000.0},0.0,30,0.0)],&[]).unwrap();
        w.snakes[0].inventory=crate::Inventory {kinds:[3,1,0],life:[100,100,0],count:2,..Default::default()};
        let mut ai=AiController::new();ai.prepare(&w);
        let s=w.snake(0).unwrap();let baseline=Candidate {steps:30,..Default::default()};
        let mut state=State {desired:0.0,goal:Point{x:2000.0,y:1000.0},track_goal:true,turn_until:u64::MAX,..Default::default()};
        ai.choose_inventory(&w,s,&mut state,&baseline,30);assert_eq!(state.use_slot,2);
        w.snakes[0].inventory.kinds[1]=0;w.snakes[0].inventory.count=1;
        ai.choose_inventory(&w,w.snake(0).unwrap(),&mut state,&baseline,30);assert_eq!(state.use_slot,0);
    }
    #[test]
    fn inventory_phase_admission_checks_the_first_tangible_exit() {
        let config=Config {rules:crate::RuleSet::V2,width:4000.0,height:4000.0,
            density:0.0,scale:200.0,speed:300.0,deadly_walls:true,self_collisions:false,..Default::default()};
        let occupant=[(Point{x:500.0,y:2000.0},0.0,30,0.0)];
        let probe=World::diagnostic_arena(config,&occupant,&[]).unwrap();
        let travel=probe.motion_limits(0,0.0).unwrap().0*STEP_SECONDS;
        let mut w=World::diagnostic_arena(Config {width:500.0+travel*140.0,..config},&occupant,&[]).unwrap();
        w.snakes[0].inventory=crate::Inventory {kinds:[3,0,0],life:[1800,0,0],count:1,..Default::default()};
        let mut ai=AiController::new();ai.prepare(&w);
        let mut state=State {desired:0.0,turn_until:u64::MAX,..Default::default()};
        let mut proposal=state;proposal.use_slot=1;
        let mut trial=Candidate::default();
        ai.rollout_into(&w,w.snake(0).unwrap(),proposal,1,NORMAL_STEPS,&mut trial);
        assert_eq!(trial.steps,NORMAL_STEPS,"short proof ends while still intangible");
        ai.rollout_into(&w,w.snake(0).unwrap(),proposal,1,STEPS,&mut trial);
        assert!(trial.steps>=125 && trial.steps<STEPS,"tangible exit hits the wall: steps={} capped={} width={} travel={}",trial.steps,trial.capped,w.config().width,travel);
        ai.choose_inventory(&w,w.snake(0).unwrap(),&mut state,&Candidate {steps:4,..Default::default()},NORMAL_STEPS);
        assert_eq!(state.use_slot,0,"keep Phase when a checked item-free turn escapes the unsafe straight exit");
        w.snakes[0].inventory.windup=1;w.snakes[0].inventory.windup_ticks=4;
        assert_eq!(phase::horizon(&w,w.snake(0).unwrap(),NORMAL_STEPS),STEPS);
    }
    #[test]
    fn inventory_pending_phase_and_surge_rollouts_match_real_endpoints() {
        for kind in [EffectKind::Phase,EffectKind::Surge,EffectKind::Magnet,EffectKind::Venom,EffectKind::Frost] {
            let mut w=World::diagnostic_arena(Config {rules:crate::RuleSet::V2,width:8000.0,height:4000.0,
                density:0.0,deadly_walls:false,..Default::default()},&[(Point{x:3000.0,y:2000.0},0.0,30,0.0)],&[]).unwrap();
            w.snakes[0].inventory=crate::Inventory {kinds:[kind as u8,0,0],life:[1800,0,0],count:1,
                windup:1,windup_ticks:4,cooldown:30};
            struct Observe {ai:AiController,path:Candidate,first:bool}
            impl Controller for Observe {
                fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
                    if self.first {
                        self.ai.prepare(w);self.ai.rollout_into(w,s,State {desired:0.0,turn_until:u64::MAX,..Default::default()},1,30,&mut self.path);self.first=false;
                        if s.inventory.kinds[0]==3 {assert!(!self.path.effects.at(0,3).is(EffectKind::Phase));assert!(self.path.effects.at(0,4).is(EffectKind::Phase));}
                    }
                    Steering {desired_angle:0.0,rush:0.0}
                }
            }
            let mut c=Observe {ai:AiController::new(),path:Candidate::default(),first:true};
            for step in 1..=30 {w.step(&mut c);assert!(w.snakes[0].alive);
                assert!(w.distance_squared(c.path.path[step],w.segments[0].current)<1e-12,"kind={kind:?} step={step}");}
        }
    }
    #[test]
    fn inventory_pending_frost_forecasts_victim_motion_and_preserves_holder_surge() {
        let mut w=World::diagnostic_arena(Config {rules:crate::RuleSet::V2,width:8000.0,height:4000.0,
            density:0.0,deadly_walls:false,..Default::default()},&[
            (Point{x:3000.0,y:2000.0},0.0,30,0.0),(Point{x:3000.0,y:2250.0},0.0,80,0.0)],&[]).unwrap();
        w.snakes[0].effect_kind=1;w.snakes[0].effect_ticks=90;
        w.snakes[0].inventory=crate::Inventory {kinds:[5,0,0],life:[1800,0,0],count:1,
            windup:1,windup_ticks:4,cooldown:30};
        struct Observe {ai:AiController,path:[Candidate;2],first:bool}
        impl Controller for Observe {
            fn steer(&mut self,w:&World,_s:SnakeView<'_>)->Steering {
                if self.first {
                    self.ai.prepare(w);
                    for id in 0..2 {self.ai.rollout_into(w,w.snake(id).unwrap(),State {desired:0.0,turn_until:u64::MAX,..Default::default()},1,30,&mut self.path[id]);}
                    self.first=false;
                }
                Steering {desired_angle:0.0,rush:0.0}
            }
        }
        let mut c=Observe {ai:AiController::new(),path:std::array::from_fn(|_|Candidate::default()),first:true};
        for step in 1..=30 {
            w.step(&mut c);
            assert_eq!(w.snakes[0].effect_kind,1);
            assert_eq!(w.snakes[1].frozen_ticks,if step<4 {0} else {75-(step-4) as u16});
            for id in 0..2 {assert!(w.distance_squared(c.path[id].path[step],w.segments[id*MAX_SEGMENTS].current)<1e-12,"id={id} step={step}");}
        }
    }

    #[test]
    fn inventory_surge_contact_bound_checks_own_and_rival_activations() {
        for holder in [0,1] {
            let mut w=World::diagnostic_arena(Config {rules:crate::RuleSet::V2,width:12000.0,height:2000.0,
                density:0.0,scale:40.0,speed:1000.0,intelligence:100.0,deadly_walls:false,self_collisions:false,..Default::default()},
                &[(Point{x:4000.0,y:1000.0},0.0,30,0.0),(Point{x:7600.0,y:1000.0},std::f64::consts::PI,40,0.0)],&[]).unwrap();
            w.snakes[holder].inventory=crate::Inventory {kinds:[1,0,0],life:[1800,0,0],count:1,
                windup:1,windup_ticks:5,cooldown:30};
            struct Observe {ai:AiController,path:Candidate,first:bool}
            impl Controller for Observe {
                fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
                    if self.first {
                        self.ai.prepare(w);
                        self.ai.rollout_into(w,w.snake(0).unwrap(),State {desired:0.0,revise_opponents:true,turn_until:u64::MAX,..Default::default()},1,72,&mut self.path);
                        self.first=false;
                    }
                    Steering {desired_angle:s.angle,rush:0.0}
                }
            }
            let mut c=Observe {ai:AiController::new(),path:Candidate::default(),first:true};
            for _ in 0..72 {w.step(&mut c);if !w.snakes[0].alive {break;}}
            assert!(!w.snakes[0].alive,"production collision, holder={holder}");
            assert!(c.path.steps<72,"rollout must reject contact, holder={holder}, steps={}",c.path.steps);
        }
    }
    #[test]
    fn inventory_surge_reach_keeps_later_full_inventory_replacement() {
        let mut w=World::diagnostic_arena(Config {rules:crate::RuleSet::V2,width:16000.0,height:2000.0,
            density:0.0,scale:40.0,speed:1000.0,intelligence:100.0,deadly_walls:false,self_collisions:false,..Default::default()},
            &[(Point{x:4000.0,y:1000.0},0.0,30,0.0)],&[]).unwrap();
        w.snakes[0].inventory=crate::Inventory {kinds:[1,2,4],life:[1800;3],count:3,..Default::default()};
        let ordinary=w.motion_limits(0,0.0).unwrap().0*STEP_SECONDS;
        for (id,kind,moves) in [(41,EffectKind::Magnet,10.0),(42,EffectKind::Phase,81.0)] {
            w.items.push(crate::Item {id,kind,position:Point{x:4000.0+ordinary*moves,y:1000.0},radius:12.6,life_ticks:750,..Default::default()});
        }
        struct Observe {ai:AiController,path:Candidate,first:bool,request:bool}
        impl Controller for Observe {
            fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
                self.request=w.tick()==0;
                if self.first {
                    self.ai.prepare(w);let state=State {desired:0.0,use_slot:1,turn_until:u64::MAX,..Default::default()};
                    self.ai.rollout_into(w,s,state,1,72,&mut self.path);self.first=false;
                }
                Steering {desired_angle:0.0,rush:0.0}
            }
            fn use_request(&self,_:u32)->u32 {u32::from(self.request)}
        }
        let mut c=Observe {ai:AiController::new(),path:Candidate::default(),first:true,request:false};
        for step in 1..=72 {
            w.step(&mut c);assert!(w.snakes[0].alive);
            assert!(w.distance_squared(c.path.path[step],w.segments[0].current)<1e-10,"step={step}");
            assert_eq!(c.path.effects.at(0,step).kind,w.snakes[0].effect_kind,"step={step}");
        }
        assert_eq!(w.snakes[0].inventory.count,3);
        assert!(c.path.effects.at(0,72).is(EffectKind::Phase));
        assert_eq!(w.items.len(),0);
    }
    #[test]
    fn inventory_surge_wall_bound_includes_activation_and_dawn() {
        for dawn in [false,true] {
            let mut w=World::diagnostic_arena(Config {rules:crate::RuleSet::V2,width:12000.0,height:2000.0,
                density:0.0,scale:40.0,speed:1000.0,intelligence:100.0,deadly_walls:true,self_collisions:false,..Default::default()},
                &[(Point{x:4000.0,y:1000.0},0.0,30,0.0)],&[]).unwrap();
            w.snakes[0].inventory=crate::Inventory {kinds:[1,0,0],life:[1800,0,0],count:1,..Default::default()};
            if dawn {
                w.diagnostic_event_schedule(10000,1);
                w.step(&mut crate::controller::BaselineController);
                w.tick=661;w.world_event.night=1.0;
            }
            struct Observe {ai:AiController,bound:(f64,f64),first:bool}
            impl Controller for Observe {
                fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
                    if self.first {
                        self.ai.prepare(w);let f=self.ai.intent_forecast::<true>(w,s,State {use_slot:1,..Default::default()},72,0);
                        self.bound=f.wall_limits(w,0,0.0,&self.ai.motion[0],2);self.first=false;
                    }
                    let (speed,turn)=w.motion_limits(0,0.0).unwrap();
                    assert!(self.bound.0+1e-8>=speed/turn && self.bound.1+1e-8>=speed,"bound={:?} actual={:?}",self.bound,(speed,turn));
                    Steering {desired_angle:0.0,rush:0.0}
                }
                fn use_request(&self,_:u32)->u32 {1}
            }
            let mut c=Observe {ai:AiController::new(),bound:(0.0,0.0),first:true};
            for _ in 0..100 {w.step(&mut c);assert!(w.snakes[0].alive);}
        }
    }

    #[test]
    fn inventory_full_field_surge_keeps_distinct_late_dawn_motion_queries() {
        let mut w=World::diagnostic_arena(Config {rules:crate::RuleSet::V2,width:16000.0,height:2000.0,
            density:0.0,scale:40.0,speed:1000.0,intelligence:100.0,deadly_walls:false,self_collisions:false,..Default::default()},
            &[(Point{x:4000.0,y:1000.0},0.0,30,0.0)],&[]).unwrap();
        w.diagnostic_event_schedule(10000,1);
        w.step(&mut crate::controller::ScriptedController::new(|_,s:SnakeView<'_>|Steering {desired_angle:s.angle,rush:0.0}));
        w.tick=481;w.world_event.night=1.0;w.items.clear();
        w.snakes[0].inventory=crate::Inventory {kinds:[3;3],life:[1800;3],count:3,..Default::default()};
        let h=w.segments[0].current;let ordinary=w.motion_limits(0,0.0).unwrap().0*STEP_SECONDS;
        w.items.push(crate::Item {id:42,kind:EffectKind::Surge,position:Point{x:h.x+ordinary*120.0,y:h.y},radius:12.6,life_ticks:750,..Default::default()});
        struct Observe {ai:AiController,step:usize,limits:[(f64,f64);2]}
        impl Controller for Observe {
            fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
                self.step+=1;
                if self.step==1 {
                    self.ai.prepare(w);let f=forecast::Forecast::empty(self.ai.effects);
                    assert!(f.effects.before(0,268).is(EffectKind::Surge));
                    self.limits=[f.motion(w,0,0.0,&self.ai.motion[0],260),f.motion(w,0,0.0,&self.ai.motion[0],268)];
                    assert!(self.limits[1].0>self.limits[0].0,"dawn queries beyond offset 255 must not alias");
                }
                if let Some(i)=[260,268].iter().position(|&step|step==self.step) {
                    let actual=w.motion_limits(0,0.0).unwrap();
                    assert!((actual.0-self.limits[i].0).abs()<1e-10 && (actual.1-self.limits[i].1).abs()<1e-10);
                }
                Steering {desired_angle:s.angle,rush:0.0}
            }
        }
        let mut c=Observe {ai:AiController::new(),step:0,limits:[(0.0,0.0);2]};
        for _ in 0..268 {w.step(&mut c);assert!(w.snakes[0].alive);}
    }

}
