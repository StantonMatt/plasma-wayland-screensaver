// SPDX-License-Identifier: GPL-3.0-or-later
//! Rare held U-turn decisions. Ordinary candidates only scan live holders.
use super::*;
#[derive(Clone,Copy,Default)]
pub(super) struct AreaCache {pub epoch:u64,pub root:usize,pub mask:u16,pub limit:usize,pub result:(usize,bool)}
#[derive(Clone,Copy,Default)]
pub(super) struct Threat {pub id:usize,pub tail:Point,pub out:Point,pub range2:f64,pub inner2:f64,pub observers:u16}
impl AiController {
    #[inline]
    pub(super) fn flip_note(&mut self,index:usize) {
        #[cfg(feature="desktop-diag")] {self.flip_counts[index]+=1;}
        #[cfg(not(feature="desktop-diag"))] let _=index;
    }
    /// Cumulative decision counts; compiled out of ordinary simulation builds.
    #[cfg(feature="desktop-diag")]
    pub fn flip_decision_counts(&self)->[u64;24] {self.flip_counts}
    #[cfg(feature="desktop-diag")]
    pub fn reset_flip_decision_counts(&mut self) {self.flip_counts=[0;24];}

    /// Potential head at the tail: reserve the outward six-radius cone even
    /// before a rival announces a wind-up. A four-segment winner may ambush it.
    #[cfg(test)]
    pub(super) fn flip_tail_threat(&self,w:&World,s:SnakeView<'_>,p:Point,defeated:u16)->bool {self.flip_tail_sweep(w,s,p,p,defeated,0,&self.initial_effects)}
    #[inline]
    pub(super) fn flip_tail_sweep(&self,w:&World,s:SnakeView<'_>,a:Point,b:Point,defeated:u16,step:usize,effects:&forecast::Timeline)->bool {
        let mut threats=self.flip_reserves[s.id as usize];
        while threats!=0 {
            let index=threats.trailing_zeros() as usize;threats&=threats-1;
            let o=&self.flip_threats[index];
            if defeated&(1<<o.id)!=0 || !effects.contact(s.id as usize,o.id,step) || !effects.flip_available(o.id,step) {continue;}
            let q=w.displacement(o.tail,a);let v=w.displacement(a,b);let length2=v.x*v.x+v.y*v.y;
            let nearest=|lo:f64,hi:f64| {
                let t=if length2>1e-12 {(-(q.x*v.x+q.y*v.y)/length2).clamp(lo,hi)} else {lo};
                (q.x+v.x*t).powi(2)+(q.y+v.y*t).powi(2)
            };
            let distance2=nearest(0.0,1.0);
            if distance2>=o.range2 {continue;}
            if distance2<o.inner2 {return true;}
            // Clip the swept interval to the two 45-degree cone half-planes,
            // then test its closest point against the outer radial bound.
            let forward=q.x*o.out.x+q.y*o.out.y;let side=q.x*o.out.y-q.y*o.out.x;
            let df=v.x*o.out.x+v.y*o.out.y;let ds=v.x*o.out.y-v.y*o.out.x;
            let mut lo=0.0_f64;let mut hi=1.0_f64;
            for (base,change) in [(forward-side,df-ds),(forward+side,df+ds)] {
                if change.abs()<1e-12 {if base<0.0 {hi=-1.0;break;}}
                else if change>0.0 {lo=lo.max(-base/change);} else {hi=hi.min(-base/change);}
            }
            if lo<=hi && nearest(lo,hi)<o.range2 {return true;}
        }
        false
    }
    /// False eyes inform rivals who can currently see the tail. Lower IQ
    /// notices at a shorter range; announced wind-up is always public.
    pub(super) fn flip_observers(w:&World,s:SnakeView<'_>,tail:Point)->u16 {
        let sight=s.radius*(4.0+8.0*(w.config().intelligence/100.0).clamp(0.0,1.0));
        w.snakes().filter(|o|o.alive && o.id!=s.id).fold(0,|mask,o| {
            let d=w.displacement(o.segments[0].current,tail);
            if s.inventory.windup!=0 || d.x*d.x+d.y*d.y<=sight*sight
                && d.x*o.angle.cos()+d.y*o.angle.sin()>0.0 {mask|(1<<o.id)} else {mask}
        })
    }
    /// Follow the newly admitted opportunity for at most one second. This
    /// survives the endpoint reset; ordinary safety candidates still arbitrate.
    pub(super) fn flip_followup(&self,w:&World,s:SnakeView<'_>,state:&mut State) {
        if state.flip_until==0 {return;}
        if w.tick()>=state.flip_until || w.tick()<state.escape_until {
            state.flip_until=0;return;
        }
        if state.flip_situation==2 {
            let Some(prey)=w.snake(state.flip_prey as usize-1).filter(|p|p.alive
                && p.generation==state.flip_prey_generation && p.flags&crate::flags::PHASED==0
                && s.segments.len()>=p.segments.len()+4) else {state.flip_until=0;return;};
            state.prey=prey.id as usize+1;state.prey_generation=prey.generation;
            state.goal=prey.segments[0].current;state.hunt_until=state.flip_until;
        } else if state.flip_situation==3 {state.goal=state.flip_goal;}
        else {state.flip_until=0;return;}
        if w.distance_squared(s.segments[0].current,state.goal)<(s.radius*3.0).powi(2) && state.flip_situation==3 {
            state.flip_until=0;return;
        }
        state.target=0;state.waypoint=None;state.track_goal=true;state.guarding=false;
        state.vulturing=false;state.venom_target=0;state.venom_standoff=false;
        state.coil_radius=0.0;state.attack=Attack::default();state.attack_options=[Attack::default();2];
        let d=w.displacement(s.segments[0].current,state.goal);state.desired=d.y.atan2(d.x);
        state.turn_until=u64::MAX;state.exit_angle=state.desired;state.revise_opponents=true;
    }
    /// Only ordinary time-zero admission queries are reusable here. Candidate
    /// trajectory marks and release clocks never enter this cache.
    fn flip_area(&mut self,start:Point,mask:u16,limit:usize)->(usize,bool) {
        let root=self.spatial.key(start);
        let index=(root ^ mask as usize ^ limit)%self.flip_areas.len();
        let old=self.flip_areas[index];
        if old.epoch==self.cache_epoch && old.root==root && old.mask==mask && old.limit==limit {return old.result;}
        let clock=self.profile_enabled.then(std::time::Instant::now);
        let result=self.spatial.space(start,mask,limit,0.0);
        if let Some(c)=clock {self.flip_profile[3]+=c.elapsed().as_nanos();}
        self.flip_areas[index]=AreaCache {epoch:self.cache_epoch,root,mask,limit,result};
        result
    }
    fn flip_ambush_window(&self,w:&World,s:SnakeView<'_>,other:usize)->f64 {
        let aggression=aggression::level(w);
        let (speed,turn)=self.motion[s.id as usize].at(0);
        let (other_speed,_)=self.motion[other].at(0);
        6.0*s.radius+(speed+other_speed)*STEP_SECONDS*(4.0+12.0*aggression)
            +(speed/turn.max(0.01)).min(8.0*s.radius)*aggression
    }
    /// Off-cadence work requires an observed prize/rival within the full
    /// admission range. This is only a broad phase, never a safety proof.
    pub(super) fn flip_nearby(&self,w:&World,s:SnakeView<'_>)->bool {
        if s.flags&crate::flags::TRAPPED!=0 {return true;}
        let n=s.segments.len();if n<2 {return false;}
        let tail=s.segments[n-1].current;
        if aggression::level(w)>0.0 && w.snakes().any(|o|o.alive && o.id!=s.id
            && n>=o.segments.len()+4 && o.flags&crate::flags::PHASED==0
            && w.distance_squared(tail,o.segments[0].current)<self.flip_ambush_window(w,s,o.id as usize).powi(2)) {return true;}
        let reach2=(15.0*s.radius).powi(2);
        let expiring=s.inventory.kinds.iter().zip(s.inventory.life).any(|(&kind,life)|kind==6 && life>4 && life<=150);
        w.foods().any(|f|f.pickup_eligible(u64::MAX,0) && (f.vacuum_owner<0 || f.vacuum_owner==s.id as i32)
            && (matches!(f.kind,crate::FoodKind::Shard|crate::FoodKind::Prism|crate::FoodKind::Star)
                || expiring && matches!(f.kind,crate::FoodKind::Spark|crate::FoodKind::Pellet))
            && w.distance_squared(tail,f.position)<=reach2)
            || w.items().any(|i|i.pickable_from_tick<=w.tick()+4 && w.distance_squared(tail,i.position)<=reach2)
            || w.starfall_target().is_some_and(|(p,_,end)|end>w.tick()+30 && w.distance_squared(tail,p)<=reach2)
    }
    /// Returns the signature situation (1 escape, 2 ambush, 3 loot).
    #[cfg(test)]
    pub(super) fn flip_situation(&mut self,w:&World,s:SnakeView<'_>,baseline:&Candidate)->u8 {
        self.flip_opportunity(w,s,baseline).0
    }
    pub(super) fn flip_opportunity(&mut self,w:&World,s:SnakeView<'_>,baseline:&Candidate)->(u8,u16,Option<Point>) {
        let clock=self.profile_enabled.then(std::time::Instant::now);
        let result=self.flip_opportunity_impl(w,s,baseline);
        if let Some(c)=clock {self.flip_profile[1]+=c.elapsed().as_nanos();}
        result
    }
    fn flip_opportunity_impl(&mut self,w:&World,s:SnakeView<'_>,baseline:&Candidate)->(u8,u16,Option<Point>) {
        self.flip_note(0);
        let n=s.segments.len();
        if n<2 || w.forecast_growth_active(s.id as usize) {self.flip_note(1);return (0,0,None);}
        let head=s.segments[0].current;let tail=s.segments[n-1].current;
        let d=w.displacement(s.segments[n-2].current,tail);let angle=d.y.atan2(d.x);
        let out=Point{x:angle.cos(),y:angle.sin()};
        let (speed,turn)=self.motion[s.id as usize].at(0);
        if !wall::reachable_circle(w,tail,out,
            wall::Circle::new(speed/turn.max(0.01)*1.25,speed*STEP_SECONDS),s.radius) {
            self.flip_note(2);return (0,0,None);
        }
        let aggression=aggression::level(w);
        // React before a wary rival reaches the false-eyes cone. The window
        // covers wind-up and an intercept, and scales with both motion limits.
        // This is admission only: the rollout must actually certify the kill.
        let mut ambush=0u16;
        if aggression>0.0 {for o in w.snakes().filter(|o|o.alive && o.id!=s.id) {
            let id=o.id as usize;
            let window=self.flip_ambush_window(w,s,id);
            let delta=w.displacement(o.segments[0].current,tail);
            let distance2=delta.x*delta.x+delta.y*delta.y;
            if distance2>=window*window {continue;}
            let distance=distance2.sqrt();
            self.flip_note(3);
            // Same four-segment V2 rule as mechanics. The safety forecast
            // separately reserves paid boost length and rejects phased contact.
            if n<o.segments.len()+4 || o.flags&crate::flags::PHASED!=0 {self.flip_note(4);continue;}
            let approaching=delta.x*o.angle.cos()+delta.y*o.angle.sin()>distance*(0.85-0.35*aggression);
            // Observed curved/intercept motion counts even if the current
            // heading is outside the old 45-degree gate; no private AI intent.
            let rival=&self.rivals[id];
            let intercept=!approaching && (4..=24).step_by(4).any(|step| {
                let p=w.canonical_point(Point{x:tail.x+out.x*speed*STEP_SECONDS*(step-4) as f64,
                    y:tail.y+out.y*speed*STEP_SECONDS*(step-4) as f64});
                w.distance_squared(p,rival.path[step])<(s.radius+o.radius+2.0).powi(2)
            });
            if !approaching && !intercept {self.flip_note(5);continue;}
            ambush|=1<<id;self.flip_note(6);
        }}
        let trapped=s.flags&crate::flags::TRAPPED!=0;
        let expiring=s.inventory.kinds.iter().zip(s.inventory.life).any(|(&kind,life)|kind==6 && life>4 && life<=150);
        let (head_sin,head_cos)=s.angle.sin_cos();
        let mut behind=0.0;let mut ahead=0.0;let mut goal=None;let mut goal_score=0.0;
        if ambush==0 && !trapped {
            // A long snake's rear objective can be far from its current head.
            // Require real distance saved and an outward approach from the tail,
            // instead of intersecting two tiny head/tail neighbourhoods.
            let reach=15.0*s.radius;
            let mut prize=|position:Point,value:f64| {
                let from_head=w.displacement(head,position);let from_tail=w.displacement(tail,position);
                let head2=from_head.x*from_head.x+from_head.y*from_head.y;
                let tail2=from_tail.x*from_tail.x+from_tail.y*from_tail.y;
                if head2<reach*reach && from_head.x*head_cos+from_head.y*head_sin>0.0 {ahead+=value;}
                if tail2>reach*reach {return;}
                self.flip_note(7);
                if from_head.x*head_cos+from_head.y*head_sin>=0.0 {return;}
                self.flip_note(8);
                let tail_distance=tail2.sqrt();
                if from_tail.x*out.x+from_tail.y*out.y>tail_distance*0.5
                    && head2.sqrt()>tail_distance+s.radius*if expiring {2.0} else {4.0} {
                    behind+=value;self.flip_note(9);
                    let score=value/(tail_distance+s.radius);
                    if score>goal_score {goal_score=score;goal=Some(position);}
                }
            };
            for f in w.foods() {
                if !f.pickup_eligible(u64::MAX,0) || f.vacuum_owner>=0 && f.vacuum_owner!=s.id as i32 {continue;}
                if matches!(f.kind,crate::FoodKind::Shard|crate::FoodKind::Prism|crate::FoodKind::Star)
                    || expiring && matches!(f.kind,crate::FoodKind::Spark|crate::FoodKind::Pellet) {
                    prize(f.position,if f.kind==crate::FoodKind::Prism {9.0} else {f.value});
                }
            }
            for item in w.items().filter(|i|i.pickable_from_tick<=w.tick()+4) {
                prize(item.position,item.kind.base_value());
            }
            if let Some((position,_,end))=w.starfall_target() {
                if end>w.tick()+30 {prize(position,7.5);}
            }
        }
        let loot=behind>=if expiring {1.0} else {6.0} && behind>ahead*if expiring {1.0} else {1.2};
        if !trapped && ambush==0 && !loot {self.flip_note(10);return (0,0,None);}
        // The winner's head/body cells cannot veto admission of the very
        // head-on being tested. This provisional area is conditional on that
        // kill; the full rollout and final area must certify it before use.
        let mask=self.effects.mask(w,s.id as usize,1)&!ambush;
        let need=(n as f64*s.radius*s.radius*8.0/(self.spatial.dx*self.spatial.dy)).ceil().max(24.0) as usize;
        let start=w.canonical_point(Point{x:tail.x+out.x*s.radius*3.0,y:tail.y+out.y*s.radius*3.0});
        let (area,_)=self.flip_area(start,mask,(need*4).max(128));
        if area<need {self.flip_note(11);return (0,0,None);}
        if trapped {
            let (current,_)=self.flip_area(baseline.path[baseline.steps],mask,(need*4).max(128));
            if area>=current.max(1)*2 {return (1,0,None);}
        }
        if ambush!=0 {(2,ambush,None)} else if loot {(3,0,goal)} else {(0,0,None)}
    }

}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Config,Inventory,RuleSet,controller::Controller};
    struct Predict {
        ai:AiController,own:usize,use_slot:u32,trial:Candidate,paths:[[Point;8];2],prepared:bool,
    }
    impl Predict {
        fn new(own:usize)->Self {Self {ai:AiController::new(),own,use_slot:0,trial:Candidate::default(),paths:[[Point::default();8];2],prepared:false}}
    }
    impl Controller for Predict {
        fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
            if !self.prepared {
                self.ai.prepare(w);
                let own=w.snake(self.own).unwrap();
                self.ai.rollout_into(w,own,State {desired:own.angle,use_slot:self.use_slot as u8,turn_until:u64::MAX,revise_opponents:true,..Default::default()},2,8,&mut self.trial);
                self.paths[self.own].copy_from_slice(&self.trial.path[..8]);
                let other=1-self.own;
                self.paths[other].copy_from_slice(&AiController::forecast_rival(self.ai.simulation_rivals.as_ref().unwrap(),&self.ai.rivals,other).path[..8]);
                self.prepared=true;
            }
            Steering {desired_angle:s.angle,rush:0.0}
        }
        fn use_request(&self,id:u32)->u32 {if id as usize==self.own {self.use_slot} else {0}}
    }
    fn order_arena()->World {
        World::diagnostic_arena(Config {rules:RuleSet::V2,width:6000.0,height:3000.0,density:0.0,
            deadly_walls:true,self_collisions:false,world_events:false,store_power_ups:false,..Default::default()},
            &[(Point{x:3000.0,y:1500.0},0.0,40,0.0),(Point{x:4500.0,y:2300.0},0.0,20,0.0)],&[]).unwrap()
    }
    struct RegressionDriver {
        ai:AiController,trial:Candidate,source:Option<World>,use_slot:u32,horizon:usize,
    }
    impl RegressionDriver {
        fn new(use_slot:u32,horizon:usize)->Self {Self {ai:AiController::new(),trial:Candidate::default(),source:None,use_slot,horizon}}
    }
    impl Controller for RegressionDriver {
        fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
            if self.source.is_none() {
                self.source=Some(w.diagnostic_snapshot());self.ai.prepare(w);
                self.trial=self.ai.rollout(w,w.snake(0).unwrap(),State {desired:w.snakes[0].angle,
                    use_slot:self.use_slot as u8,turn_until:u64::MAX,revise_opponents:true,..Default::default()},2,self.horizon);
            }
            Steering {desired_angle:s.angle,rush:0.0}
        }
        fn use_request(&self,id:u32)->u32 {if id==0 && self.source.as_ref().is_some_and(|w|w.tick()==0) {self.use_slot} else {0}}
    }
    #[test]
    fn candidate_flip_keeps_each_nonflipping_rivals_completion_sweep() {
        for winning in [false,true] {
            let mut w=order_arena();w.snakes[1].traits.speed_bias=30.0;
            let len=if winning {1} else {60};
            w.snakes[0].inventory=Inventory {kinds:[6,0,0],life:[1800,0,0],count:1,windup:1,windup_ticks:1,..Default::default()};
            let travel=w.motion_limits(0,0.0).unwrap().0*STEP_SECONDS;
            let tail=w.segments[39].current;let destination=Point{x:tail.x+travel,y:tail.y};
            let rival_travel=w.motion_limits(1,0.0).unwrap().0*STEP_SECONDS;
            let points:Vec<_>=(0..len).map(|i|Point{x:destination.x,y:destination.y-rival_travel*0.5-i as f64*7.08}).collect();
            w.diagnostic_body(1,&points,std::f64::consts::FRAC_PI_2).unwrap();
            let old=w.segments[MAX_SEGMENTS].current;
            let mut c=RegressionDriver::new(0,1);w.step(&mut c);
            let end=w.segments[MAX_SEGMENTS].current;let reach=(w.snakes[0].radius+w.snakes[1].radius)*0.82;
            assert!(w.distance_squared(destination,old)>reach*reach && w.distance_squared(destination,end)>reach*reach);
            assert!(w.segments_distance_squared(destination,destination,old,end)<reach*reach,"geometry crosses only between endpoints");
            assert!(w.distance_squared(destination,c.trial.path[1])<1e-10);
            if winning {
                assert!(w.snakes[0].alive && !w.snakes[1].alive);
                assert_eq!(c.trial.flip_head_wins&2,2);
                assert_eq!(c.trial.steps,1);
            } else {
                assert!(!w.snakes[0].alive);
                assert_eq!(w.last_death_reason(0),Some(crate::DeathReason::Head));
                assert_eq!(c.trial.steps,0);
            }
        }
    }
    #[test]
    fn held_reserve_contact_tracks_observed_acquired_replaced_and_expired_phase() {
        for actor in [0,1] {for transition in 0..4 {
            let mut w=order_arena();w.config.aggression=0;
            w.snakes[0].inventory=Inventory {kinds:[6,0,0],life:[1800,0,0],count:1,..Default::default()};
            let tail=w.segments[39].current;let point=Point{x:tail.x-30.0,y:tail.y};
            let pts:Vec<_>=(0..20).map(|i|Point{x:tail.x-60.0-i as f64*7.08,y:point.y}).collect();
            w.diagnostic_body(1,&pts,0.0).unwrap();
            if transition!=1 {w.snakes[actor].effect_kind=3;w.snakes[actor].effect_ticks=if transition==3 {3} else {100};}
            if transition==1 {let head=w.segments[actor*MAX_SEGMENTS].current;capsule(&mut w,42,crate::effects::EffectKind::Phase,head);}
            if transition==2 {
                let inv=&mut w.snakes[actor].inventory;let slot=inv.count as usize;
                inv.kinds[slot]=2;inv.life[slot]=1800;inv.count+=1;inv.windup=(slot+1) as u8;inv.windup_ticks=1;
            }
            let mut c=RegressionDriver::new(0,4);w.step(&mut c);
            let source=c.source.as_ref().unwrap().diagnostic_snapshot();let effects=c.ai.effects;
            for step in 1..=4 {
                if step>1 {w.step(&mut c);}
                let phased=w.snakes[actor].effect_kind==3 && w.snakes[actor].effect_ticks>0;
                assert_eq!(effects.phased(actor,step),phased,"actor={actor} transition={transition} step={step}");
                assert_eq!(c.ai.flip_tail_sweep(&source,source.snake(1).unwrap(),point,point,0,step,&effects),!phased,
                    "reserve eligibility actor={actor} transition={transition} step={step}");
                assert!(w.snakes.iter().take(2).all(|s|s.alive));
                assert!(w.distance_squared(point,w.segments[0].current)>(w.snakes[0].radius*2.0).powi(2));
            }
        }}
    }
    #[test]
    fn sparse_snapshot_constructor_retains_observed_phase_before_pending_replacement() {
        let mut w=order_arena();w.snakes[0].effect_kind=3;w.snakes[0].effect_ticks=100;
        w.snakes[0].inventory=Inventory {kinds:[2,0,0],life:[1800,0,0],count:1,windup:1,windup_ticks:1,..Default::default()};
        let contact=w.segments[20].current;
        let points:Vec<_>=(0..20).map(|i|Point{x:contact.x,y:contact.y-10.0-i as f64*7.08}).collect();
        w.diagnostic_body(1,&points,std::f64::consts::FRAC_PI_2).unwrap();
        let mut c=RegressionDriver::new(0,1);w.step(&mut c);
        let source=c.source.as_ref().unwrap();let snapshot=c.ai.initial_effects.snapshot();
        assert_eq!(snapshot.at(0,0).kind,3);assert_eq!(snapshot.mask(source,1,0)&1,0);
        assert_eq!(snapshot.mask(source,1,1)&1,1);assert_eq!(w.snakes[0].effect_kind,2);
        assert_eq!(w.last_death_reason(1),Some(crate::DeathReason::Body));
        assert!(source.distance_squared(contact,w.segments[MAX_SEGMENTS].current)<100.0);
    }
    // Isolate one real observed record so neighbouring conservative widths
    // cannot mask an erroneous index exemption in the narrow-phase regression.
    fn only_record(ai:&mut AiController,w:&World,encoded:usize) {
        ai.spatial.heads.fill(-1);ai.spatial.next[encoded]=-1;
        let key=ai.spatial.key(w.segments[encoded].current);ai.spatial.heads[key]=encoded as i32;
    }
    #[test]
    fn missing_reconstruction_never_exempts_the_old_head_now_at_the_tail() {
        let mut w=order_arena();w.snakes[0].inventory=Inventory {kinds:[6,0,0],life:[1800,0,0],count:1,windup:1,windup_ticks:1,..Default::default()};
        let mut c=RegressionDriver::new(0,1);w.step(&mut c);
        let mut source=c.source.take().unwrap();source.config.self_collisions=true;
        let p=source.segments[0].current;
        assert!(w.distance_squared(p,w.segments[w.snakes[0].len-1].current)<7.08_f64.powi(2));
        c.ai.rivals[0].reversal=Reversal::default();only_record(&mut c.ai,&source,0);
        let mut tests=0;let mut bite=None;
        assert!(c.ai.body_query::<false,true>(&source,source.snake(0).unwrap(),p,p,STEP_SECONDS,0.0,&mut tests,false,&c.ai.initial_effects,&mut bite,0,None).0);
    }
    #[test]
    fn reversed_self_neck_uses_post_swap_distance_after_thaw_and_surge() {
        for surge in [false,true] {
            let mut w=order_arena();w.snakes[0].frozen_ticks=6;w.faces[0].frozen_ticks=6;
            w.snakes[0].inventory=Inventory {kinds:[6,0,0],life:[1800,0,0],count:1,windup:1,windup_ticks:5,..Default::default()};
            if surge {let p=w.segments[39].current;capsule(&mut w,42,crate::effects::EffectKind::Surge,p);}
            let mut c=RegressionDriver::new(0,10);w.step(&mut c);
            for _ in 2..=10 {w.step(&mut c);}
            let mut source=c.source.take().unwrap();let reversal=c.ai.rollout_reversal;
            assert_eq!(reversal.step,5);assert_eq!(w.faces[0].flip_tick,5);
            assert!(w.distance_squared(c.trial.path[10],w.segments[0].current)<1e-10);
            let advance=(c.ai.rollout_distance[10]-reversal.distance)/(reversal.radius*1.18);
            let old_advance=c.ai.rollout_distance[10]*0.5/(source.snakes[0].radius*1.18);
            let index=(0..40).find(|&j|reversal.index(j,source.snakes[0].radius)+advance>=8.0
                && reversal.index(j,source.snakes[0].radius)+old_advance<8.0).expect("speed transition must straddle neck exemption");
            let p=source.segments[index].current;
            let corporeal=reversal.index(index,source.snakes[0].radius)+advance;
            let sample=corporeal.round() as usize;
            assert!(sample>=8 && sample<w.snakes[0].len);
            assert!(w.distance_squared(p,w.segments[sample].current)<(reversal.radius*1.18).powi(2),"real corporeal geometry surge={surge} index={index}");
            source.config.self_collisions=true;only_record(&mut c.ai,&source,index);
            let mut effects=c.ai.initial_effects;
            if surge {effects.activation(0,crate::effects::EffectKind::Surge,5);}
            let mut tests=0;let mut bite=None;
            assert!(c.ai.body_query::<false,true>(&source,source.snake(0).unwrap(),p,p,10.0*STEP_SECONDS,0.0,&mut tests,false,&effects,&mut bite,9,Some(c.ai.simulation_rivals.as_ref().unwrap())).0);
        }
    }
    #[test]
    fn reversed_footprint_keeps_candidate_paid_dimensions_and_real_body() {
        for boost in [false,true] {for frost in [false,true] {
            let mut w=order_arena();w.snakes[0].inventory=Inventory {kinds:[6,0,0],life:[1800,0,0],count:1,windup:1,windup_ticks:18,..Default::default()};
            if frost {let p=w.segments[MAX_SEGMENTS].current;capsule(&mut w,42,crate::effects::EffectKind::Frost,p);}
            struct Paid {driver:RegressionDriver,boost:bool}
            impl Controller for Paid {
                fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
                    if self.driver.source.is_none() {
                        self.driver.source=Some(w.diagnostic_snapshot());self.driver.ai.prepare(w);
                        self.driver.trial=self.driver.ai.rollout(w,w.snake(0).unwrap(),State {desired:0.0,rush:if self.boost {0.6} else {0.0},turn_until:u64::MAX,revise_opponents:true,..Default::default()},if self.boost {1} else {2},18);
                    }
                    Steering {desired_angle:s.angle,rush:if self.boost && s.id==0 && w.tick()==0 {0.6} else {0.0}}
                }
            }
            let mut c=Paid {driver:RegressionDriver::new(0,18),boost};
            for _ in 0..18 {w.step(&mut c);}
            let d=&mut c.driver;let source=d.source.as_ref().unwrap();let r=d.ai.rollout_reversal;
            assert_eq!(r.step,18);assert_eq!(r.len,w.snakes[0].len);assert_eq!(r.radius,w.snakes[0].radius);
            assert!(w.distance_squared(w.segments[0].current,d.trial.path[18])<1e-10,"boost={boost} frost={frost}");
            if boost {
                assert!(r.len<40,"proposed boost must pay before reversal");
                let index=(0..40).rfind(|&j|r.index(j,source.snakes[0].radius)<0.0
                    && (39.0-j as f64-r.distance/(source.snakes[0].radius*1.18))>=0.0).expect("baseline must retain a discarded sample");
                let sample=source.segments[index].current;let p=Point{x:sample.x,y:sample.y+10.5};
                assert!(w.distance_squared(p,w.segments[0].current)>((w.snakes[0].radius+source.snakes[1].radius)*0.82).powi(2),"geometry cleared index={index}");
                only_record(&mut d.ai,source,index);let mut tests=0;let mut bite=None;
                let mut rivals=Box::new([Rival::default();MAX_SNAKES]);rivals[0]=d.ai.rivals[0];rivals[0].dynamic=true;rivals[0].reversal=r;
                assert!(!d.ai.body_query::<false,true>(source,source.snake(1).unwrap(),p,p,18.0*STEP_SECONDS,0.0,&mut tests,false,&d.ai.initial_effects,&mut bite,17,Some(&rivals)).0);
            }
        }}
    }
    #[test]
    fn held_reserve_expires_or_is_consumed_but_another_flip_remains() {
        for (life,pending,count) in [(2,0,1),(1800,1,1),(1800,1,2)] {
            let mut w=order_arena();w.config.aggression=0;
            w.snakes[0].inventory=Inventory {kinds:[6,6,0],life:[life,1800,0],count,windup:pending,windup_ticks:pending,..Default::default()};
            let tail=w.segments[39].current;let point=Point{x:tail.x-30.0,y:tail.y};
            let pts:Vec<_>=(0..20).map(|i|Point{x:tail.x-60.0-i as f64*7.08,y:point.y}).collect();
            w.diagnostic_body(1,&pts,0.0).unwrap();
            let mut c=RegressionDriver::new(0,4);w.step(&mut c);let source=c.source.as_ref().unwrap().diagnostic_snapshot();
            assert!(c.ai.flip_tail_sweep(&source,source.snake(1).unwrap(),point,point,0,0,&c.ai.initial_effects));
            for step in 1..=4 {
                if step>1 {w.step(&mut c);}
                let available=w.snakes[0].inventory.kinds[..w.snakes[0].inventory.count as usize].contains(&6);
                assert_eq!(c.ai.initial_effects.flip_available(0,step),available,"life={life} pending={pending} count={count} step={step}");
                assert_eq!(c.ai.flip_tail_sweep(&source,source.snake(1).unwrap(),point,point,0,step,&c.ai.initial_effects),available);
                assert!(w.snakes.iter().take(2).all(|s|s.alive));
            }
        }
    }
    #[test]
    fn reversal_keeps_real_deposited_neck_edges_in_the_flip_group() {
        for flip in 1..=3 {
            let mut w=order_arena();w.snakes[0].len=100;w.snakes[1].traits.speed_bias=5.0;
            let points:Vec<_>=(0..40).map(|i|Point{x:3500.0-i as f64*7.08,y:1500.0}).collect();
            w.diagnostic_body(1,&points,0.0).unwrap();
            w.snakes[1].inventory=Inventory {kinds:[6,0,0],life:[1800,0,0],count:1,windup:1,windup_ticks:flip,..Default::default()};
            let tail=w.snake(1).unwrap().segments[39].current;
            let travel=w.motion_limits(1,0.0).unwrap().0*STEP_SECONDS;
            let gap=if flip==1 {20.0} else {40.0};
            let points:Vec<_>=(0..100).map(|i|Point{x:tail.x+(flip-1) as f64*travel,y:tail.y-gap-i as f64*7.08}).collect();
            w.diagnostic_body(0,&points,std::f64::consts::FRAC_PI_2).unwrap();
            let mut c=RegressionDriver::new(0,20);let mut death=0;
            for j in 1..=20 {w.step(&mut c);if !w.snakes[0].alive {death=j;break;}}
            assert_eq!(w.last_death_reason(0),Some(crate::DeathReason::Body),"flip={flip}");
            assert!(death>flip as usize,"the head has moved away from the deposited neck");
            assert!(c.trial.steps<death,"flip={flip}: retained {} movements through body death at {death}",c.trial.steps);
        }
    }
    #[test]
    fn curved_reversal_does_not_deposit_a_body_across_the_swap() {
        let mut w=order_arena();w.snakes[0].len=100;w.snakes[1].len=40;
        let radius=39.0*7.08/std::f64::consts::PI;
        let points:Vec<_>=(0..40).map(|i| {let angle=i as f64*std::f64::consts::PI/39.0;
            Point{x:3500.0-radius+radius*angle.cos(),y:1500.0+radius*angle.sin()}}).collect();
        w.diagnostic_body(1,&points,std::f64::consts::FRAC_PI_2).unwrap();
        w.snakes[1].inventory=Inventory {kinds:[6,0,0],life:[1800,0,0],count:1,windup:1,windup_ticks:1,..Default::default()};
        let points:Vec<_>=(0..100).map(|i|Point{x:3500.0-radius,y:1495.0-i as f64*7.08}).collect();
        w.diagnostic_body(0,&points,std::f64::consts::FRAC_PI_2).unwrap();
        let mut c=RegressionDriver::new(0,8);
        for _ in 0..8 {w.step(&mut c);assert!(w.snakes[0].alive && w.snakes[1].alive);}
        assert_eq!(w.faces[1].flip_tick,1);
        assert_eq!(c.trial.steps,8,"the old-head/new-head chord crosses a clear interior");
    }
    #[test]
    fn projected_head_death_only_changes_masks_after_its_step() {
        let mut w=order_arena();
        w.snakes[0].inventory=Inventory {kinds:[6,0,0],life:[1800,0,0],count:1,windup:1,windup_ticks:1,..Default::default()};
        let tail=w.snake(0).unwrap().segments[39].current;
        let points:Vec<_>=(0..20).map(|i|Point{x:tail.x-18.0-i as f64*7.08,y:tail.y}).collect();
        w.diagnostic_body(1,&points,0.0).unwrap();
        let mut c=RegressionDriver::new(0,20);let mut death=0;
        for j in 1..=20 {w.step(&mut c);if !w.snakes[1].alive {death=j;break;}}
        assert_eq!(w.last_death_reason(1),Some(crate::DeathReason::Head));
        assert_eq!(c.trial.flip_head_wins&2,2);
        let source=c.source.as_ref().unwrap();
        for step in 0..=death+1 {
            c.trial.steps=step;
            assert_eq!(c.ai.candidate_mask(source,0,&c.trial)&2,if step<=death {2} else {0},"retained endpoint={step} death={death}");
        }
    }
    #[test]
    fn frost_before_flip_uses_candidate_travel_for_live_original_body() {
        for (surge,night) in [(0,false),(20,false),(3,true),(20,true)] {
        let mut w=order_arena();w.config.store_power_ups=true;w.snakes[1].traits.speed_bias=5.0;
        if surge>0 {w.snakes[1].effect_kind=1;w.snakes[1].effect_ticks=surge;}
        if night {w.config.world_events=true;w.diagnostic_event_schedule(10000,1);}
        let points:Vec<_>=(0..40).map(|i|Point{x:3500.0-i as f64*7.08,y:1500.0}).collect();
        w.diagnostic_body(1,&points,0.0).unwrap();
        let offset=5.0*(w.motion_limits(1,0.0).unwrap().0-w.motion_limits(0,0.0).unwrap().0)*STEP_SECONDS;
        let points:Vec<_>=(0..40).map(|i|Point{x:3500.0+offset-i as f64*7.08,y:1440.0}).collect();
        w.diagnostic_body(0,&points,0.0).unwrap();
        w.snakes[0].inventory=Inventory {kinds:[5,0,0],life:[1800,0,0],count:1,..Default::default()};
        w.snakes[1].inventory=Inventory {kinds:[6,0,0],life:[1800,0,0],count:1,windup:1,windup_ticks:8,..Default::default()};
        let mut c=RegressionDriver::new(1,16);w.step(&mut c);
        let source=c.source.as_ref().unwrap();
        let mut effects=c.ai.initial_effects;effects.schedule_use(0,crate::effects::EffectKind::Frost,5,0);
        let mut f=forecast::Forecast::empty(effects);
        let mut rivals=Box::new([Rival::default();MAX_SNAKES]);
        for j in 1..=8 {
            AiController::advance_rivals(source,&mut rivals,&c.ai.motion,&f,j,1,Some((&c.ai.effects,&c.ai.rivals)),None);
            let mut positions=std::array::from_fn(|id|if id==0 {c.trial.path[j]} else {AiController::forecast_rival(&rivals,&c.ai.rivals,id).path[j]});
            f.advance_ordered(source,j,&mut positions,|f,id|AiController::reverse_rival(source,&mut rivals,&c.ai.rivals,&c.ai.motion,f,id,j),None,0.0);
            if j>1 {w.step(&mut crate::controller::ScriptedController::new(|_,s:SnakeView<'_>|Steering {desired_angle:s.angle,rush:0.0}));}
            assert!(w.snakes[1].alive);
            assert!(w.distance_squared(w.snake(1).unwrap().segments[0].current,positions[1])<1e-10,"movement={j} actual={:?} forecast={:?} frozen={} hits={}",w.snake(1).unwrap().segments[0].current,positions[1],w.snakes[1].frozen_ticks,f.effects.frost_hits[0]);
        }
        assert!(w.snakes[1].frozen_ticks>0);assert_eq!(w.faces[1].flip_tick,8);
        let baseline=&c.ai.rivals[1];let physical=&rivals[1];
        assert!(physical.distance[8]<baseline.distance[8]);
        let index=(39.0-physical.distance[8]/7.08).floor() as usize;
        assert!(39.0-index as f64-baseline.distance[8]/7.08<0.0,"baseline wrongly clears this sample");
        let p=source.snake(1).unwrap().segments[index].current;
        assert!(w.snake(1).unwrap().segments.iter().any(|seg|w.distance_squared(p,seg.current)<7.08_f64.powi(2)),"the slowed reversed body still occupies the sample");
        let mut tests=0;let mut bite=None;
        let blocked=c.ai.body_query::<false,true>(source,source.snake(0).unwrap(),p,p,8.0*STEP_SECONDS,0.0,&mut tests,false,&f.effects,&mut bite,7,Some(&rivals));
        assert!(blocked.0,"clearance must use the same trajectory that reconstructed the reversal: surge={surge} night={night}");
        }
    }
    #[test]
    fn projected_venom_consumption_and_sever_keep_their_step_in_queries() {
        let mut w=crate::world::venom::tests::fixture(40,25);w.config.world_events=false;
        let mut c=RegressionDriver::new(0,8);w.step(&mut c);
        assert_eq!(w.snakes[0].effect_kind,0);assert!(w.snakes[1].len<40);
        assert_eq!(c.trial.venom_bite,1);
        assert_eq!(c.trial.effects.at(0,1).kind,0,"snapshot must retain the actual consumption");
        let mut timeline=c.ai.initial_effects;timeline.sever_cut[1]=w.snakes[1].len;timeline.bite_step[1]=4;timeline.consumed_at[0]=4;
        let f=forecast::Forecast::empty(timeline);
        assert_eq!(f.endpoint_len(c.source.as_ref().unwrap(),1,0.0,3),40);
        assert_eq!(f.endpoint_len(c.source.as_ref().unwrap(),1,0.0,4),w.snakes[1].len);
        assert_eq!(timeline.snapshot().at(0,3).kind,4);
        assert_eq!(timeline.snapshot().at(0,4).kind,0);
        assert_eq!(timeline.track(0).before(4).kind,4);
        assert_eq!(timeline.track(0).before(5).kind,0);
    }
    fn capsule(w:&mut World,id:u64,kind:crate::effects::EffectKind,p:Point) {
        w.items.push(crate::Item {id,kind,position:p,radius:1.0,life_ticks:750,..Default::default()});
    }
    #[test]
    fn full_inventory_field_flip_updates_rival_and_candidate_on_contact_and_continuation() {
        for own in [0,1] {
            let mut w=order_arena();
            w.snakes[0].inventory=Inventory {kinds:[1;3],life:[1800;3],count:3,..Default::default()};
            let head=w.segments[0].current;
            capsule(&mut w,42,crate::effects::EffectKind::Flip,head);
            let mut c=Predict::new(own);
            for j in 1..=7 {
                w.step(&mut c);
                assert!(c.trial.steps>=7,"own={own} steps={} cap={}",c.trial.steps,c.trial.capped);
                for id in 0..2 {assert!(w.distance_squared(w.snake(id).unwrap().segments[0].current,c.paths[id][j])<1e-10,"own={own} id={id} step={j}");}
                assert_eq!(c.ai.rivals[0].path[j],c.paths[0][j],"prepare must use the same swap");
            }
            assert_eq!(w.faces[0].flip_tick,1);
            assert_eq!(c.trial.effects.space_time(1,STEP_SECONDS,&c.ai.rivals),0.0,"own={own}: the physical timeline must contain the pickup-time swap");
        }
    }
    #[test]
    fn held_flip_completes_before_phase_arbitration_at_old_or_new_endpoint() {
        for own in [0,1] {for at_tail in [false,true] {
            let mut w=order_arena();
            w.snakes[0].inventory=Inventory {kinds:[6,0,0],life:[1800,0,0],count:1,windup:1,windup_ticks:1,..Default::default()};
            let point=w.segments[if at_tail {39} else {0}].current;
            capsule(&mut w,42,crate::effects::EffectKind::Phase,point);
            let mut c=Predict::new(own);w.step(&mut c);
            assert_eq!(w.snakes[0].effect_kind,if at_tail {3} else {0});
            assert_eq!(c.trial.effects.at(0,1).kind,w.snakes[0].effect_kind,"own={own} at_tail={at_tail}");
            assert_eq!(c.ai.effects.at(0,1).kind,w.snakes[0].effect_kind);
            assert_eq!(c.ai.opportunities.at(0,1).kind,w.snakes[0].effect_kind);
            assert_eq!(c.trial.effects.mask(&w,1,0)&1,1,"endpoint zero precedes the pickup");
            assert_eq!(c.trial.effects.mask(&w,1,1)&1,if at_tail {0} else {1});
            assert!(w.distance_squared(w.segments[0].current,c.paths[0][1])<1e-10);
        }}
    }
    #[test]
    fn simultaneous_flip_and_frost_complete_in_snake_id_order() {
        for flip_id in [0,1] {for own in [0,1] {
            let mut w=order_arena();let frost_id=1-flip_id;
            // Old head is outside the Nova, swapped tail is inside. With the
            // lower Flip ID Frost sees the new head; with higher ID the old.
            let len=w.snakes[flip_id].len;let tail=w.segments[flip_id*crate::MAX_SEGMENTS+len-1].current;
            let radius=16.0*w.config().base_radius();
            let center=Point{x:tail.x-radius+12.0,y:tail.y-20.0};
            let points:Vec<_>=(0..w.snakes[frost_id].len).map(|i|Point{x:center.x-i as f64*w.snakes[frost_id].radius*1.18,y:center.y}).collect();
            w.diagnostic_body(frost_id,&points,0.0).unwrap();
            for (id,kind) in [(flip_id,6),(frost_id,5)] {w.snakes[id].inventory=Inventory {kinds:[kind,0,0],life:[1800,0,0],count:1,windup:1,windup_ticks:1,..Default::default()};}
            let mut c=Predict::new(own);w.step(&mut c);
            let expected=if flip_id==0 {1} else {0};
            assert_eq!(c.ai.effects.frost_hits[frost_id],expected,"flip={flip_id} own={own}");
            assert_eq!(c.ai.opportunities.frost_hits[frost_id],expected,"opportunity completion order");
            assert_eq!(w.snakes[flip_id].frozen_ticks>0,expected!=0);
            assert_eq!(c.ai.effects.frost_hits[frost_id],c.ai.initial_effects.frost_hits[frost_id]+expected);
            for id in 0..2 {assert!(w.distance_squared(w.snake(id).unwrap().segments[0].current,c.paths[id][1])<1e-10);}
            // The frozen continuation must be rebuilt after that same step.
            for j in 2..=7 {
                w.step(&mut c);
                for id in 0..2 {assert!(w.distance_squared(w.snake(id).unwrap().segments[0].current,c.paths[id][j])<1e-10,"flip={flip_id} own={own} id={id} j={j}");}
            }
        }}
    }
    #[test]
    fn long_holder_capsule_bounds_include_reversed_retained_tail() {
        for proposed in [false,true] {for kind in [crate::effects::EffectKind::Phase,crate::effects::EffectKind::Surge,crate::effects::EffectKind::Frost] {
            let mut w=order_arena();w.snakes[0].len=200;
            let points:Vec<_>=(0..200).map(|i|Point{x:3000.0-i as f64*w.snakes[0].radius*1.18,y:1500.0}).collect();
            w.diagnostic_body(0,&points,0.0).unwrap();
            w.snakes[0].inventory=Inventory {kinds:[6,0,0],life:[1800,0,0],count:1,windup:1,windup_ticks:1,..Default::default()};
            let tail=w.segments[199].current;capsule(&mut w,42,kind,tail);
            if proposed {w.snakes[0].inventory.windup=0;}
            let mut c=Predict::new(0);c.use_slot=if proposed {1} else {0};
            for j in 1..=7 {w.step(&mut c);assert_eq!(c.trial.effects.at(0,j).kind,w.snakes[0].effect_kind,"kind={kind:?} proposed={proposed} j={j}");}
            assert!(c.ai.item_forecast.reachable(0,8,false));
            assert_eq!(w.items().count(),0,"kind={kind:?} proposed={proposed}");
        }}
    }
    #[test]
    fn field_flip_changes_arbitration_for_later_capsules_in_item_order() {
        for own in [0,1] {for at_tail in [false,true] {
            let mut w=order_arena();
            w.snakes[0].inventory=Inventory {kinds:[1;3],life:[1800;3],count:3,..Default::default()};
            let head=w.segments[0].current;let tail=w.segments[39].current;
            capsule(&mut w,41,crate::effects::EffectKind::Flip,head);
            capsule(&mut w,42,crate::effects::EffectKind::Phase,if at_tail {tail} else {head});
            let mut c=Predict::new(own);w.step(&mut c);
            assert_eq!(w.snakes[0].effect_kind,if at_tail {3} else {0});
            assert_eq!(c.trial.effects.at(0,1).kind,w.snakes[0].effect_kind);
            assert_eq!(c.ai.effects.at(0,1).kind,w.snakes[0].effect_kind);
            assert_eq!(c.ai.opportunities.at(0,1).kind,w.snakes[0].effect_kind);
        }}
    }
    #[test]
    fn opportunity_field_flip_moves_later_frost_center_to_the_swapped_head() {
        let mut w=order_arena();w.snakes[0].inventory=Inventory {kinds:[1;3],life:[1800;3],count:3,..Default::default()};
        let head=w.segments[0].current;let tail=w.segments[39].current;
        let pts:Vec<_>=(0..20).map(|i|Point{x:tail.x-40.0-i as f64*7.08,y:tail.y+60.0}).collect();
        w.diagnostic_body(1,&pts,0.0).unwrap();
        capsule(&mut w,41,crate::effects::EffectKind::Flip,head);
        capsule(&mut w,42,crate::effects::EffectKind::Frost,tail);
        let mut c=Predict::new(0);w.step(&mut c);
        assert_eq!(w.snakes[1].frozen_ticks,75);
        assert_eq!(c.ai.opportunities.frost_hits[0],1);
        assert_eq!(c.ai.effects.frost_hits[0],1);
        assert!(w.distance_squared(head,w.segments[MAX_SEGMENTS].current)>(16.0*w.config.base_radius()).powi(2));
        assert!(w.distance_squared(w.segments[0].current,w.segments[MAX_SEGMENTS].current)<(16.0*w.config.base_radius()).powi(2));
        assert!(w.distance_squared(w.segments[0].current,c.paths[0][1])<1e-10);
    }
    #[test]
    fn reversal_clears_guard_intent_before_later_field_capsules() {
        let mut w=order_arena();w.snakes[0].effect_kind=2;w.snakes[0].effect_ticks=100;
        w.faces[0].guarding=true;w.faces[0].target_id=42;
        w.snakes[0].inventory=Inventory {kinds:[1;3],life:[1800;3],count:3,..Default::default()};
        let head=w.segments[0].current;let tail=w.segments[39].current;
        capsule(&mut w,41,crate::effects::EffectKind::Flip,head);
        capsule(&mut w,42,crate::effects::EffectKind::Phase,tail);
        struct Guard(Predict);
        impl Controller for Guard {
            fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {self.0.steer(w,s)}
            fn face_intent(&self,id:u32)->crate::controller::FaceIntent {
                crate::controller::FaceIntent {guarding:id==0,target_id:42,..Default::default()}
            }
        }
        let mut c=Guard(Predict::new(1));w.step(&mut c);
        assert_eq!(w.snakes[0].effect_kind,3);assert!(!w.faces[0].guarding);
        assert_eq!(c.0.trial.effects.at(0,1).kind,3);
        assert_eq!(c.0.ai.effects.at(0,1).kind,3);
    }
    #[test]
    fn field_flip_reserves_a_long_rivals_new_head_beyond_ordinary_head_reach() {
        let mut w=order_arena();w.snakes[1].len=200;
        let points:Vec<_>=(0..200).map(|i|Point{x:4500.0-i as f64*7.08,y:2300.0}).collect();
        w.diagnostic_body(1,&points,0.0).unwrap();
        let tail=w.segments[crate::MAX_SEGMENTS+199].current;
        let points:Vec<_>=(0..40).map(|i|Point{x:tail.x-i as f64*7.08,y:tail.y}).collect();
        w.diagnostic_body(0,&points,0.0).unwrap();
        w.snakes[1].inventory=Inventory {kinds:[1;3],life:[1800;3],count:3,..Default::default()};
        let head=w.segments[crate::MAX_SEGMENTS].current;capsule(&mut w,42,crate::effects::EffectKind::Flip,head);
        let mut c=Predict::new(0);w.step(&mut c);
        assert_eq!(w.last_death_reason(0),Some(crate::DeathReason::Head));
        assert_eq!(c.trial.steps,0,"a distant original head cannot hide its pickup-time destination");
    }
    #[test]
    fn growing_field_flip_caps_still_supply_a_checked_emergency_heading() {
        let mut w=order_arena();w.snakes[0].growth=500.0;
        w.snakes[0].inventory=Inventory {kinds:[1;3],life:[1800;3],count:3,..Default::default()};
        let head=w.segments[0].current;capsule(&mut w,42,crate::effects::EffectKind::Flip,head);
        let mut actual=w.diagnostic_snapshot();
        let mut c=Predict::new(0);w.step(&mut c);
        assert!(c.trial.capped && c.trial.checked && c.trial.steps==0);
        // Production choose() requires even a zero-length emergency prefix to
        // have constructed its heading; this must not panic under a growth cap.
        actual.step(&mut AiController::new());
        assert!(actual.frame_events().any(|e|e.kind==crate::EventKind::Flip));
    }
    #[test]
    fn future_rival_flip_does_not_hide_a_fast_head_crossing() {
        let mut w=order_arena();w.snakes[0].len=20;w.snakes[1].len=40;
        w.snakes[1].traits.speed_bias=100.0;
        let travel=w.motion_limits(1,0.0).unwrap().0*STEP_SECONDS;
        let head=w.segments[0].current;
        let points:Vec<_>=(0..40).map(|i|Point{x:head.x+2.0,y:head.y-travel*0.5-i as f64*7.08}).collect();
        w.diagnostic_body(1,&points,std::f64::consts::FRAC_PI_2).unwrap();
        w.snakes[1].inventory=Inventory {kinds:[6,0,0],life:[1800,0,0],count:1,windup:1,windup_ticks:5,..Default::default()};
        let mut c=Predict::new(0);w.step(&mut c);
        assert_eq!(c.ai.initial_effects.flip_steps[1],5);
        assert!(w.distance_squared(c.paths[0][1],c.paths[1][1])>100.0,"endpoints are clear");
        assert_eq!(c.trial.steps,0,"the pre-activation head sweep must reject movement one");
        assert_eq!(w.last_death_reason(0),Some(crate::DeathReason::Head));
    }
    #[test]
    fn projected_sever_then_flip_is_capped_and_mechanics_reverses_the_retained_body() {
        let mut w=crate::world::venom::tests::fixture(40,25);
        w.config.world_events=false;
        w.snakes[1].inventory=Inventory {kinds:[6,0,0],life:[1800,0,0],count:1,windup:1,windup_ticks:5,..Default::default()};
        let mut c=Predict::new(0);w.step(&mut c);
        assert!(c.trial.venom_bite>0,"the forecast must project the earlier sever");
        assert!(c.trial.capped && c.trial.steps<5,"a severed indexed trail cannot certify its later reversal");
        let retained=w.snakes[1].len;assert!(retained<40 && retained>=20,"actual cut={retained}");
        let mut f=forecast::Forecast::empty(c.ai.initial_effects);f.effects.sever_cut[1]=retained;
        assert_eq!(f.endpoint_len(&w,1,0.0,5),retained);
        w.snakes[0].alive=false;
        let mut straight=crate::controller::ScriptedController::new(|_,s:SnakeView<'_>|Steering {desired_angle:s.angle,rush:0.0});
        for _ in 0..3 {w.step(&mut straight);}
        let mut after=Predict::new(1);w.step(&mut after);
        assert_eq!(w.snakes[1].len,retained);assert_eq!(w.faces[1].flip_tick,5);
        assert!(w.distance_squared(w.snake(1).unwrap().segments[0].current,after.paths[1][1])<1e-10);
    }
    #[test]
    fn flip_forecast_and_world_step_share_endpoint_and_next_heading() {
        for wrap in [false,true] {for boosted in [false,true] {for curved in [false,true] {
            let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,width:4000.0,height:2000.0,density:0.0,
                deadly_walls:!wrap,self_collisions:true,world_events:false,..Default::default()},
                &[(Point{x:1500.0,y:1000.0},0.0,40,0.0)],&[]).unwrap();
            if curved {
                let spacing=w.snakes[0].radius*1.18;
                let points:Vec<_>=(0..40).map(|i| {let t=i as f64*0.05;w.canonical_point(Point{x:if wrap {20.0} else {1500.0}-spacing/0.05*t.sin(),y:1000.0+spacing/0.05*(1.0-t.cos())})}).collect();
                w.diagnostic_body(0,&points,0.0).unwrap();
            }
            w.snakes[0].inventory=Inventory {kinds:[6,0,0],life:[1800,0,0],count:1,..Default::default()};
            struct Observe {ai:AiController,boost:bool,path:[Point;7],count:usize}
            impl Controller for Observe {
                fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
                    if self.count==0 {
                        self.ai.prepare(w);let mut trial=Candidate::default();
                        let state=State {desired:s.angle,rush:if self.boost {0.6} else {0.0},use_slot:1,turn_until:u64::MAX,..Default::default()};
                        self.ai.rollout_into(w,s,state,1,30,&mut trial);
                        assert!(trial.steps>=6,"flip admission rejected at {} wall={} cap={}",trial.steps,trial.wall_safe,trial.capped);
                        self.path.copy_from_slice(&trial.path[..7]);
                    } else if self.count<=6 {
                        assert!(w.distance_squared(s.segments[0].current,self.path[self.count])<1e-12,
                            "endpoint {} actual {:?} forecast {:?}",self.count,s.segments[0].current,self.path[self.count]);
                    }
                    self.count+=1;Steering {desired_angle:s.angle,rush:if self.count==1 && self.boost {0.6} else {0.0}}
                }
                fn use_request(&self,_:u32)->u32 {if self.count==1 {1} else {0}}
            }
            let mut c=Observe {ai:AiController::new(),boost:boosted,path:[Point::default();7],count:0};
            for _ in 0..7 {w.step(&mut c);assert!(w.snakes[0].alive);}
        }}}
    }
    #[test]
    fn held_tail_reserves_cone_for_smaller_rivals_only() {
        let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,width:4000.0,height:2000.0,density:0.0,
            deadly_walls:false,world_events:false,aggression:0,..Default::default()},
            &[(Point{x:1500.0,y:1000.0},0.0,40,0.0),(Point{x:800.0,y:1000.0},0.0,20,0.0)],&[]).unwrap();
        w.snakes[0].inventory=Inventory {kinds:[6,0,0],life:[100,0,0],count:1,..Default::default()};
        let tail=w.snake(0).unwrap().segments[39].current;
        let points:Vec<_>=(0..20).map(|i|Point{x:tail.x-18.0-i as f64*7.08,y:tail.y}).collect();
        w.diagnostic_body(1,&points,0.0).unwrap();
        let mut ai=AiController::new();ai.prepare(&w);
        assert!(ai.flip_tail_threat(&w,w.snake(1).unwrap(),Point{x:tail.x-10.0,y:tail.y},0));
        assert!(!ai.flip_tail_threat(&w,w.snake(1).unwrap(),Point{x:tail.x-200.0,y:tail.y},0));
        assert!(!ai.flip_tail_threat(&w,w.snake(1).unwrap(),Point{x:tail.x-10.0,y:tail.y},1));
    }
    #[test]
    fn flip_signature_escape_ambush_loot_and_aggression() {
        for situation in [1,2,3] {
            let len=if situation==3 {8} else {40};
            let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,width:4000.0,height:2000.0,density:0.0,
                deadly_walls:false,self_collisions:true,world_events:false,aggression:100,..Default::default()},
                &[(Point{x:1500.0,y:1000.0},0.0,len,0.0),(Point{x:800.0,y:1400.0},0.0,20,0.0)],&[]).unwrap();
            w.snakes[0].inventory=Inventory {kinds:[6,0,0],life:[100,0,0],count:1,..Default::default()};
            if situation==1 {
                w.set_intent_flags(0,crate::flags::TRAPPED);
                let radius=8.0*w.snakes[0].radius;
                let points:Vec<_>=(0..40).map(|i| {let angle=i as f64*std::f64::consts::TAU/40.0;Point{x:1500.0+radius*angle.cos(),y:1000.0+radius*angle.sin()}}).collect();
                w.diagnostic_body(1,&points,std::f64::consts::FRAC_PI_2).unwrap();
            }
            if situation==2 {
                let tail=w.segments[len-1].current;
                let spacing=w.snakes[1].radius*1.18;
                let points:Vec<_>=(0..20).map(|i|Point{x:tail.x-3.0*w.snakes[0].radius-i as f64*spacing,y:tail.y}).collect();
                w.diagnostic_body(1,&points,0.0).unwrap();
            }
            if situation==3 {w.food.push(crate::world::Food {id:42,p:Point{x:1500.0-10.0*w.snakes[0].radius,y:1000.0},
                kind:crate::FoodKind::Prism,value:9.0,size:4.0,..Default::default()});}
            let mut ai=AiController::new();ai.prepare(&w);
            let mut baseline=Candidate::default();baseline.path[0]=w.segments[0].current;
            assert!(ai.flip_nearby(&w,w.snake(0).unwrap()),"signature {situation} must pass broad phase");
            assert_eq!(ai.flip_situation(&w,w.snake(0).unwrap(),&baseline),situation);
            w.config.aggression=0;
            assert_eq!(ai.flip_situation(&w,w.snake(0).unwrap(),&baseline),if situation==1 {1} else if situation==3 {3} else {0});
        }
    }
    #[test]
    fn flip_cone_sweep_catches_between_safe_endpoints_and_across_seam() {
        let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,width:4000.0,height:2000.0,density:0.0,
            deadly_walls:false,world_events:false,aggression:0,..Default::default()},
            &[(Point{x:1500.0,y:1000.0},0.0,40,0.0),(Point{x:800.0,y:1400.0},0.0,20,0.0)],&[]).unwrap();
        w.snakes[0].inventory=Inventory {kinds:[6,0,0],life:[100,0,0],count:1,..Default::default()};
        for seam in [false,true] {
            if seam {let dx=20.0-w.segments[39].current.x;let pts:Vec<_>=w.segments[..40].iter().map(|p|w.canonical_point(Point{x:p.current.x+dx,y:p.current.y})).collect();w.diagnostic_body(0,&pts,0.0).unwrap();}
            let tail=w.segments[39].current;
            let points:Vec<_>=(0..20).map(|i|w.canonical_point(Point{x:tail.x-18.0-i as f64*7.08,y:tail.y})).collect();
            w.diagnostic_body(1,&points,0.0).unwrap();
            let mut ai=AiController::new();ai.prepare(&w);let r=w.snakes[0].radius;
            let a=w.canonical_point(Point{x:tail.x-4.0*r,y:tail.y-7.0*r});
            let b=w.canonical_point(Point{x:tail.x-4.0*r,y:tail.y+7.0*r});
            assert!(!ai.flip_tail_threat(&w,w.snake(1).unwrap(),a,0));assert!(!ai.flip_tail_threat(&w,w.snake(1).unwrap(),b,0));
            assert!(ai.flip_tail_sweep(&w,w.snake(1).unwrap(),a,b,0,0,&ai.initial_effects));
        }
    }

    #[test]
    fn flip_target_contact_and_timeline_allow_guarded_storage_off_pickup() {
        let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,width:4000.0,height:2000.0,density:0.0,
            store_power_ups:false,deadly_walls:false,world_events:false,..Default::default()},
            &[(Point{x:1500.0,y:1000.0},0.0,40,0.0)],&[]).unwrap();
        w.snakes[0].effect_kind=1;w.snakes[0].effect_ticks=100;w.faces[0].guarding=true;
        let item=crate::Item {id:42,kind:crate::effects::EffectKind::Flip,position:w.segments[0].current,radius:40.0,life_ticks:750,..Default::default()};
        w.items.push(item);let mut ai=AiController::new();ai.prepare(&w);
        let contact=target::Contact::forecast(w.snake(0).unwrap(),ai.food[MAX_FOOD].unwrap(),ai.effects.track(0)).with_guard(true);
        assert!(contact.reached(0.0,1));
        assert!(ai.effects.stores_at(&w,0,1,0));
        assert_eq!(ai.effects.at(0,1).kind,1);
        assert!(item.pickup_eligible(w.tick()+1,0,true,true,100));
    }

    #[test]
    fn multiple_flip_contacts_never_certify_a_single_reversal_path() {
        let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,width:4000.0,height:2000.0,density:0.0,
            deadly_walls:false,self_collisions:false,world_events:false,..Default::default()},
            &[(Point{x:1500.0,y:1000.0},0.0,40,0.0)],&[]).unwrap();
        w.snakes[0].inventory=Inventory {kinds:[6;3],life:[1800;3],count:3,..Default::default()};
        let head=w.segments[0].current;let tail=w.segments[39].current;
        for (id,position) in [(41,head),(42,tail)] {w.items.push(crate::Item {id,kind:crate::effects::EffectKind::Flip,position,radius:40.0,life_ticks:750,..Default::default()});}
        let mut ai=AiController::new();ai.prepare(&w);
        let c=ai.rollout(&w,w.snake(0).unwrap(),State {desired:0.0,turn_until:u64::MAX,..Default::default()},2,30);
        assert!(c.capped && c.steps<30);
        w.step(&mut crate::controller::ScriptedController::new(|_,s:SnakeView<'_>|Steering {desired_angle:s.angle,rush:0.0}));
        assert_eq!(w.frame_events().filter(|e|e.kind==crate::EventKind::Flip).count(),2);
        assert!(w.distance_squared(w.segments[0].current,head)<100.0);
    }

    #[test]
    fn ambush_flip_is_requested_and_wins_through_world_step() {
        let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,width:4000.0,height:2000.0,density:0.0,
            deadly_walls:false,self_collisions:true,world_events:false,aggression:100,..Default::default()},
            &[(Point{x:1500.0,y:1000.0},0.0,40,0.0),(Point{x:800.0,y:1400.0},0.0,20,0.0)],&[]).unwrap();
        let generation=w.snakes[0].generation;let tail=w.segments[39].current;let r=w.snakes[0].radius;
        let spacing=w.snakes[1].radius*1.18;
        let pts:Vec<_>=(0..20).map(|i|Point{x:tail.x-3.0*r-i as f64*spacing,y:tail.y}).collect();w.diagnostic_body(1,&pts,0.0).unwrap();
        w.snakes[0].inventory=Inventory {kinds:[6,0,0],life:[100,0,0],count:1,..Default::default()};
        struct Duel(AiController);
        impl Controller for Duel {
            fn delegate(&self,id:u32)->Option<&dyn Controller> {if id==0 {Some(&self.0)} else {None}}
            fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {if s.id==0 {self.0.steer(w,s)} else {Steering {desired_angle:0.0,rush:0.0}}}
        }
        let mut c=Duel(AiController::new());let mut flipped=false;let mut won=false;
        for _ in 0..30 {
            w.step(&mut c);flipped|=w.frame_events().any(|e|e.kind==crate::EventKind::Flip && e.snake_id==0);
            won|=w.collision_events().any(|e|e.reason==crate::DeathReason::Head && e.victim==1 && e.owner_mask&1!=0 && e.owner_generations[0]==generation);
            if flipped && !won && w.tick()>w.faces[0].flip_tick {
                assert!(c.0.states[0].flip_until>w.tick(),"ambush objective must survive the endpoint reset");
                assert_eq!(c.0.states[0].prey,2);
            }
            if won {break;}
        }
        assert!(flipped,"the AI must request the ambush");assert!(won,"the ambush must win head-on");assert!(w.snakes[0].alive && w.snakes[0].generation==generation);
    }

    fn loot_arena(life:u16)->World {
        let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,width:6000.0,height:3000.0,density:0.0,
            deadly_walls:true,self_collisions:true,world_events:false,aggression:100,..Default::default()},
            &[(Point{x:3000.0,y:1500.0},0.0,200,0.0)],&[]).unwrap();
        w.snakes[0].inventory=Inventory {kinds:[6,0,0],life:[life,0,0],count:1,..Default::default()};w
    }
    #[test]
    fn long_snake_loot_uses_tail_neighbourhood_and_expiry_still_needs_a_prize() {
        for (kind,value,life,expected) in [(crate::FoodKind::Prism,9.0,1800,3),
            (crate::FoodKind::Shard,6.0,1800,3),(crate::FoodKind::Star,6.0,1800,3),
            (crate::FoodKind::Pellet,1.0,1800,0),(crate::FoodKind::Pellet,1.0,100,3)] {
            let mut w=loot_arena(life);let tail=w.segments[199].current;
            w.food.push(crate::world::Food {id:42,p:Point{x:tail.x-36.0,y:tail.y},kind,value,size:4.0,..Default::default()});
            let mut ai=AiController::new();ai.prepare(&w);
            let baseline=Candidate {steps:30,..Default::default()};
            assert_eq!(ai.flip_situation(&w,w.snake(0).unwrap(),&baseline),expected,"kind={kind:?} life={life}");
            w.food.clear();ai.prepare(&w);
            assert_eq!(ai.flip_situation(&w,w.snake(0).unwrap(),&baseline),0,"expiry alone is never useful");
            w.food.push(crate::world::Food {id:43,p:Point{x:tail.x+36.0,y:tail.y},kind:crate::FoodKind::Prism,value:9.0,size:4.0,..Default::default()});
            assert_eq!(ai.flip_situation(&w,w.snake(0).unwrap(),&baseline),0,"the new head must approach the prize");
        }
    }
    #[test]
    fn flip_loot_includes_capsules_and_starfall_zone() {
        for zone in [false,true] {
            let mut w=loot_arena(1800);let tail=w.segments[199].current;
            if zone {
                w.config.world_events=true;w.snakes[1]=w.snakes[0];
                let rival:Vec<_>=(0..20).map(|i|Point{x:4500.0-i as f64*7.08,y:2000.0}).collect();
                w.diagnostic_body(1,&rival,0.0).unwrap();
                w.diagnostic_event_schedule(1,10000);w.step(&mut crate::controller::BaselineController);
                let position=w.starfall_target().unwrap().0;
                let points:Vec<_>=(0..200).map(|i|Point{x:position.x+36.0+(199-i) as f64*7.08,y:position.y}).collect();
                w.diagnostic_body(0,&points,0.0).unwrap();w.snakes[1].alive=false;
            }
            else {w.items.push(crate::Item {id:42,kind:crate::effects::EffectKind::Surge,
                position:Point{x:tail.x-36.0,y:tail.y},radius:12.0,life_ticks:750,..Default::default()});}
            let mut ai=AiController::new();ai.prepare(&w);
            assert_eq!(ai.flip_situation(&w,w.snake(0).unwrap(),&Candidate::default()),3,"zone={zone}");
        }
    }
    #[test]
    fn ambush_admission_reaches_beyond_false_eyes_cone_and_preserves_size_rule() {
        let mut w=loot_arena(1800);let tail=w.segments[199].current;
        // A winning hunter is admitted before the rival's six-radius caution.
        // The rollout still has to prove an actual head contact.
        w.snakes[1]=w.snakes[0];w.snakes[1].len=190;w.snakes[1].inventory=Inventory::default();
        let points:Vec<_>=(0..190).map(|i|Point{x:tail.x-54.0-i as f64*7.08,y:tail.y}).collect();
        w.diagnostic_body(1,&points,0.0).unwrap();
        let mut ai=AiController::new();ai.prepare(&w);
        assert_eq!(ai.flip_situation(&w,w.snake(0).unwrap(),&Candidate::default()),2);
        w.snakes[1].len=197;ai.prepare(&w);
        assert_eq!(ai.flip_situation(&w,w.snake(0).unwrap(),&Candidate::default()),0,"three extra segments tie and kill both");
    }
    #[test]
    fn false_eyes_caution_requires_sight_and_scales_with_iq() {
        let mut w=loot_arena(1800);let tail=w.segments[199].current;
        // diagnostic_arena has fixed snake slots, initially dead.
        w.snakes[1]=w.snakes[0];w.snakes[1].len=20;w.snakes[1].inventory=Inventory::default();
        let points:Vec<_>=(0..20).map(|i|Point{x:tail.x-48.0-i as f64*7.08,y:tail.y}).collect();
        w.diagnostic_body(1,&points,0.0).unwrap();
        assert_ne!(AiController::flip_observers(&w,w.snake(0).unwrap(),tail)&2,0);
        w.config.intelligence=0.0;
        assert_eq!(AiController::flip_observers(&w,w.snake(0).unwrap(),tail)&2,0);
        w.config.intelligence=100.0;w.snakes[1].angle=std::f64::consts::PI;
        assert_eq!(AiController::flip_observers(&w,w.snake(0).unwrap(),tail)&2,0);
        w.snakes[0].inventory.windup=1;
        assert_ne!(AiController::flip_observers(&w,w.snake(0).unwrap(),tail)&2,0);
    }

    #[test]
    fn ambush_followup_cancels_on_identity_phase_size_and_escape() {
        for reason in 0..5 {
            let mut w=loot_arena(1800);let tail=w.segments[199].current;
            w.snakes[1]=w.snakes[0];w.snakes[1].len=190;
            let points:Vec<_>=(0..190).map(|i|Point{x:tail.x-54.0-i as f64*7.08,y:tail.y}).collect();
            w.diagnostic_body(1,&points,0.0).unwrap();
            let mut state=State {flip_until:30,flip_situation:2,flip_prey:2,
                flip_prey_generation:w.snakes[1].generation,..Default::default()};
            match reason {0=>w.snakes[1].generation+=1,1=>{w.snakes[1].effect_kind=3;w.snakes[1].effect_ticks=30;},
                2=>w.snakes[1].len=197,3=>state.escape_until=30,_=>w.tick=30}
            AiController::new().flip_followup(&w,w.snake(0).unwrap(),&mut state);
            assert_eq!(state.flip_until,0,"reason={reason}");
        }
    }
    #[test]
    fn tail_reserve_shrinks_for_bold_rivals_but_pending_flip_is_always_reserved() {
        let mut w=loot_arena(1800);let tail=w.segments[199].current;
        w.snakes[1]=w.snakes[0];w.snakes[1].len=20;w.snakes[1].inventory=Inventory::default();
        let points:Vec<_>=(0..20).map(|i|Point{x:tail.x-18.0-i as f64*7.08,y:tail.y}).collect();
        w.diagnostic_body(1,&points,0.0).unwrap();
        for (aggression,pending,expected) in [(0,0,true),(100,0,false),(100,1,true)] {
            w.config.aggression=aggression;w.snakes[0].inventory.windup=pending;
            let mut ai=AiController::new();ai.prepare(&w);
            assert_eq!(ai.flip_tail_threat(&w,w.snake(1).unwrap(),Point{x:tail.x-24.0,y:tail.y},0),expected);
            assert!(ai.flip_tail_threat(&w,w.snake(1).unwrap(),Point{x:tail.x-6.0,y:tail.y},0));
        }
    }

    #[test]
    fn flip_area_cache_is_exact_and_expires_with_tick_and_geometry() {
        let mut w=loot_arena(1800);let mut ai=AiController::new();ai.enable_profile();ai.prepare(&w);
        let start=Point{x:6000.0,y:1000.0};let mask=ai.effects.mask(&w,0,1);
        let first=ai.flip_area(start,mask,128);let count=ai.spatial.counts[2];
        assert_eq!(ai.flip_area(start,mask,128),first);
        assert_eq!(ai.spatial.counts[2],count,"second admission shares exact time-zero result");
        let changed=ai.flip_area(start,mask^2,128);
        assert_eq!(changed,ai.spatial.space(start,mask^2,128,0.0));
        w.tick+=1;ai.prepare(&w);let count=ai.spatial.counts[2];
        assert_eq!(ai.flip_area(start,mask,128),ai.spatial.space(start,mask,128,0.0));
        assert_eq!(ai.spatial.counts[2],count+2,"new observation cannot retain last tick's area");
        w.resize(8000.0,2000.0).unwrap();ai.prepare(&w);let count=ai.spatial.counts[2];
        assert_eq!(ai.flip_area(start,mask,128),ai.spatial.space(start,mask,128,0.0));
        assert_eq!(ai.spatial.counts[2],count+2,"same-tick resize also expires cache");
    }

    #[test]
    fn flip_prefilter_rejects_far_prizes_and_keeps_expiring_food() {
        let mut w=loot_arena(1800);let mut ai=AiController::new();ai.prepare(&w);
        assert!(!ai.flip_nearby(&w,w.snake(0).unwrap()));
        w.food.push(crate::world::Food {id:42,p:Point{x:5000.0,y:1000.0},
            kind:crate::FoodKind::Prism,value:9.0,..Default::default()});
        assert!(!ai.flip_nearby(&w,w.snake(0).unwrap()));
        w.food[0].p=w.segments[199].current;assert!(ai.flip_nearby(&w,w.snake(0).unwrap()));
        w.food[0].kind=crate::FoodKind::Pellet;assert!(!ai.flip_nearby(&w,w.snake(0).unwrap()));
        w.snakes[0].inventory.life[0]=100;assert!(ai.flip_nearby(&w,w.snake(0).unwrap()));
    }

}
