// SPDX-License-Identifier: GPL-3.0-or-later
//! One effect clock for every forecast participant. Inputs are steering's
//! post-decrement observations. Movement uses before(); pickup runs in item
//! order with lowest live snake ID winning, then collisions use at().
//! Future spawns, food growth and unobserved rival controls are not predicted.
use super::*;
use crate::effects::EffectKind;
use std::cell::Cell;
/// A single target's discrete food transport; no heap or enlarged target cache.
#[derive(Clone,Copy,Default)]
pub(super) struct FoodMotion {pub position:Point,pub velocity:Point,captured_until:u16,absorbed:bool}
impl FoodMotion {
    pub fn new(w:&World,f:target::TargetFood)->Self {
        let raw=w.food.get(f.food_index as usize).filter(|raw|raw.id==f.id)
            .or_else(||w.food.iter().find(|raw|raw.id==f.id));
        Self {position:f.position,velocity:raw.map_or(Point::default(),|raw|raw.velocity),
            captured_until:if raw.is_some_and(|raw|raw.captured_by!=0) {w.vortex().map_or(0,|v|v.life_ticks+1)} else {0},absorbed:false}
    }
    pub fn advance(&mut self,w:&World) {
        if self.velocity.x.abs()+self.velocity.y.abs()<0.1 {return;}
        self.position.x+=self.velocity.x*STEP_SECONDS;self.position.y+=self.velocity.y*STEP_SECONDS;
        let damping=0.16_f64.powf(STEP_SECONDS);self.velocity.x*=damping;self.velocity.y*=damping;
        if !w.config().deadly_walls {self.position=w.canonical_point(self.position);} else {
            for (v,speed,n) in [(&mut self.position.x,&mut self.velocity.x,w.config().width),(&mut self.position.y,&mut self.velocity.y,w.config().height)] {
                if *v<2.0 || *v>n-2.0 {*v=v.clamp(2.0,n-2.0);*speed*= -0.45;}
            }
        }
    }
    pub fn unavailable(self)->bool {self.absorbed || self.captured_until!=0}
    /// Steering observes movement one's food update and pull already done.
    /// Later updates move free food, then pull/capture, then release at burst.
    /// Return new ownership so callers can revoke a provisional vacuum claim.
    pub fn advance_brew(&mut self,w:&World,brew:Brew,kind:crate::FoodKind,step:usize)->bool {
        if step<=1 || self.absorbed {return false;}
        if self.captured_until==0 {self.advance(w);}
        if self.captured_until!=0 && step>=self.captured_until as usize {self.captured_until=0;}
        if brew.start==0 || step<brew.start as usize || step>=brew.pull_end as usize
            || !crate::world::whirlpool::capturable(kind) {return false;}
        let captured=self.captured_until==0 && brew.inside(self.position);
        if captured {self.captured_until=brew.end;self.velocity=Point::default();}
        if self.captured_until!=0 {
            let cfg=w.config();let d=w.displacement(brew.center,self.position);
            let distance=(d.x*d.x+d.y*d.y).sqrt();let radius=brew.radius2.sqrt();
            let q=(1.0-distance/radius).clamp(0.0,1.0);
            let next=(distance-radius*0.25*(0.55+0.6*q)*STEP_SECONDS).max(0.0);
            if next<0.9*w.config().base_radius() {self.absorbed=true;return captured;}
            let angle=if w.reduced_motion {0.0} else {(0.5+1.9*q)*STEP_SECONDS};
            let (sin,cos)=angle.sin_cos();let scale=next/distance.max(1e-9);
            let p=Point{x:brew.center.x+(d.x*cos-d.y*sin)*scale,y:brew.center.y+(d.x*sin+d.y*cos)*scale};
            self.position=if cfg.deadly_walls {Point{x:p.x.clamp(2.0,cfg.width-2.0),y:p.y.clamp(2.0,cfg.height-2.0)}} else {w.canonical_point(p)};
        }
        captured
    }

}
#[derive(Clone,Copy,Default)]
pub(super) struct Brew {pub start:u16,pub end:u16,pub pull_end:u16,center:Point,radius2:f64,width:f64,height:f64,wrap:bool}
impl Brew {
    fn new(w:&World,center:Point,start:usize,end:usize,pull_end:usize)->Self {Self {start:start as u16,end:end as u16,pull_end:pull_end as u16,center,
        radius2:(w.config().base_radius()*crate::world::whirlpool::RADIUS).powi(2),width:w.config().width,height:w.config().height,wrap:!w.config().deadly_walls}}
    pub fn inside(self,p:Point)->bool {
        let mut d=Point{x:p.x-self.center.x,y:p.y-self.center.y};
        if self.wrap {for (v,n) in [(&mut d.x,self.width),(&mut d.y,self.height)] {if *v>n*0.5 {*v-=n;} else if *v< -n*0.5 {*v+=n;}}}
        d.x*d.x+d.y*d.y<=self.radius2
    }
    /// Absorbed nutrition becomes burst shards; only surviving particles
    /// regain their own identity at the deadline. Share the rollout transport.
    #[inline(never)]
    pub fn captures(self,w:&World,f:target::TargetFood,step:usize,previous:Self)->bool {
        if self.start==0 || f.id&target::ITEM_BIT!=0 || !crate::world::whirlpool::capturable(f.kind) {return false;}
        let mut motion=FoodMotion::new(w,f);
        // Ordinary stationary food outside the disk cannot enter this brew.
        if !motion.unavailable() && motion.velocity.x.abs()+motion.velocity.y.abs()<0.1 && !self.inside(motion.position)
            && (previous.start==0 || !previous.inside(motion.position)) {return false;}
        for j in 2..=step.min(self.end as usize) {
            let brew=if previous.start!=0 && j<self.start as usize {previous} else {self};
            motion.advance_brew(w,brew,f.kind,j);
            if motion.unavailable() && step<brew.end as usize {return true;}
            if motion.absorbed {return true;}
        }
        motion.unavailable()
    }
}
pub(super) const EFFECT_EVENTS:usize=crate::MAX_ITEMS+MAX_SNAKES;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Effect { pub kind: u8, pub ticks: u16 }
impl Effect {
    pub fn observed(w:&World,s:SnakeView<'_>)->Self {
        if w.config().rules==crate::RuleSet::V2 { Self {kind:s.effect_kind,ticks:s.effect_ticks} }
        else {Self::default()}
    }
    pub fn after(self,elapsed:usize)->Self {
        let ticks=self.ticks.saturating_sub(elapsed.min(u16::MAX as usize) as u16);
        Self {kind:if ticks==0 {0} else {self.kind},ticks}
    }
    pub fn is(self,kind:EffectKind)->bool {self.kind==kind as u8 && self.ticks>0}
}
/// A small copy for target scoring; no controller borrow in rollout loops.
#[derive(Clone, Copy, Default)]
pub(super) struct Track {pub store:bool,initial:Effect,pickups:[Effect;crate::MAX_ITEMS+1],steps:[u16;crate::MAX_ITEMS+1],count:u8,consumed_at:u16}
impl Track {
    #[cfg(test)]
    pub fn observed(w:&World,s:SnakeView<'_>)->Self {Self {store:w.config().store_power_ups,initial:Effect::observed(w,s),..Self::default()}}
    pub fn before(&self,step:usize)->Effect {
        if let Some(i)=(0..self.count as usize).filter(|&i|(self.steps[i] as usize)<step).max_by_key(|&i|self.steps[i]) {
            if self.consumed_at>=self.steps[i] && (self.consumed_at as usize)<step {return Effect::default();}
            return self.pickups[i].after(step-self.steps[i] as usize);
        }
        if self.consumed_at>0 && (self.consumed_at as usize)<step {return Effect::default();}
        self.initial.after(step.saturating_sub(1))
    }
}
#[derive(Clone, Copy, Default)]
struct Pickup {step:u16,id:u8,order:u8,effect:Effect}
#[derive(Clone,Copy,Default)]
struct Freeze {step:u16,mask:u16}
// Release deadlines use the observed conservative speed floor. A Nova can
// lower that floor: discount only its future movement interval, never its
// pickup movement. Keeping the original floor after thaw is conservative.
fn release_clock(novas:&[Freeze],id:usize,time:f64,slowdown:f64)->f64 {
    let mut clock=time;
    if slowdown<1.0 {for nova in novas.iter().filter(|n|n.mask&(1<<id)!=0) {
        let start=nova.step as f64*STEP_SECONDS;
        let duration=(crate::effects::frost::FROZEN_TICKS-1) as f64*STEP_SECONDS;
        clock-=(time-start).clamp(0.0,duration)*(1.0-slowdown);
    }}
    clock
}
/// Candidates retain events, not two full per-step mask tables. The hot
/// rollout owns those masks; endpoint queries reconstruct their fourteen bits.
#[derive(Clone, Copy, Default)]
pub(super) struct Snapshot {
    phase_possible:u16,
    flip_mask:u16,
    defeated:u16,
    defeat_steps:[u16;MAX_SNAKES],
    consumed_at:[u16;MAX_SNAKES],
    flip_steps:[u16;MAX_SNAKES],
    initial:[Effect;MAX_SNAKES],pickups:[Pickup;EFFECT_EVENTS],count:usize,
    novas:[Freeze;EFFECT_EVENTS],nova_count:usize,
}
impl Snapshot {
    pub fn body_time(&self,id:usize,time:f64,r:&Rival)->f64 {
        let duration=(crate::effects::frost::FROZEN_TICKS-1) as f64*STEP_SECONDS;
        time+self.novas[..self.nova_count].iter().filter(|n|n.mask&(1<<id)!=0).count() as f64
            *duration*(1.0-r.release_slowdown.min(1.0))
    }
    /// The pooled grid has one query clock. Use the slowest included owner's
    /// clock so it cannot clear any body earlier than its exact release clock.
    /// This preserves grid/proof caches without rebuilding cells per candidate.
    pub fn space_time(&self,mask:u16,time:f64,rivals:&[Rival;MAX_SNAKES])->f64 {
        if self.flip_mask & mask!=0 && self.flip_steps.iter().enumerate().any(|(id,&step)|mask&(1<<id)!=0 && step!=0 && time>=step as f64*STEP_SECONDS) {return 0.0;}
        let victims=self.novas[..self.nova_count].iter().fold(0,|bits,n|bits|n.mask)&mask;
        let mut bits=victims;let mut clock=time;
        while bits!=0 {let id=bits.trailing_zeros() as usize;bits&=bits-1;
            clock=clock.min(release_clock(&self.novas[..self.nova_count],id,time,rivals[id].release_slowdown));
        }
        clock
    }
    pub fn at(&self,id:usize,step:usize)->Effect {
        if step==0 {return self.initial[id];}
        if let Some(p)=self.pickups[..self.count].iter().filter(|p|p.id as usize==id && p.step as usize<=step).max_by_key(|p|p.step) {
            if self.consumed_at[id]>=p.step && self.consumed_at[id] as usize<=step {return Effect::default();}
            return p.effect.after(step-p.step as usize);
        }
        if self.consumed_at[id]>0 && self.consumed_at[id] as usize<=step {return Effect::default();}
        self.initial[id].after(step-1)
    }
    pub fn mask(&self,w:&World,id:usize,step:usize)->u16 {
        if self.phase_possible&(1<<id)!=0 && self.at(id,step).is(EffectKind::Phase) {return 0;}
        let mut mask=if w.config().self_collisions {u16::MAX} else {u16::MAX^(1<<id)};
        let mut dead=0;let mut bits=self.defeated;
        while bits!=0 {let other=bits.trailing_zeros() as usize;bits&=bits-1;
            if step>self.defeat_steps[other] as usize {dead|=1<<other;}
        }
        let mut bits=self.phase_possible & mask & !dead;
        while bits!=0 {let other=bits.trailing_zeros() as usize;bits&=bits-1;
            if self.at(other,step).is(EffectKind::Phase) {mask&=!(1<<other);}
        }
        mask & !dead
    }
}
#[derive(Clone, Copy)]
pub(super) struct Timeline {
    pub brew:Brew,previous_brew:Brew,windup_brew_mask:u16,
    scheduled:bool,
    pub flip_steps:[u16;MAX_SNAKES],
    pub flip_mask:u16,
    pub flip_conflict_at:u16,
    field_flips:u8,
    defeated:u16,
    defeat_steps:[u16;MAX_SNAKES],
    flip_slots:[u8;MAX_SNAKES],
    inventory_stashes:[Pickup;crate::MAX_ITEMS],stash_count:usize,inventory_counts:[u8;MAX_SNAKES],inventory_expiry:[[u16;3];MAX_SNAKES],inventory_expired:[u8;MAX_SNAKES],store:bool,inventory_release:[u16;MAX_SNAKES],windup_frost:[u16;MAX_SNAKES],windup_frost_mask:u16,
    initial:[Effect;MAX_SNAKES],
    pickups:[Pickup;EFFECT_EVENTS],
    count:usize,
    pickup_mask:u16,
    movement_mask:u16,
    venom_possible:u16,
    pub(super) frost_empty:u16,
    pub(super) frost_hits:[u8;MAX_SNAKES],
    frozen:[u16;MAX_SNAKES],
    immunity:[u16;MAX_SNAKES],
    novas:[Freeze;EFFECT_EVENTS],nova_count:usize,
    guards:u16,
    guard_targets:[u64;MAX_SNAKES],
    pub sever_cut:[usize;MAX_SNAKES],
    pub bite_step:[usize;MAX_SNAKES],
    pub consumed_at:[usize;MAX_SNAKES],
    phase:[u16;STEPS+1],
    surge:[u16;STEPS+1],
}
impl Default for Timeline {
    fn default()->Self {Self {brew:Brew::default(),previous_brew:Brew::default(),windup_brew_mask:0,flip_slots:[0;MAX_SNAKES],scheduled:false,defeated:0,defeat_steps:[0;MAX_SNAKES],field_flips:0,flip_conflict_at:0,flip_mask:0,flip_steps:[0;MAX_SNAKES],inventory_stashes:[Pickup::default();crate::MAX_ITEMS],stash_count:0,inventory_counts:[0;MAX_SNAKES],inventory_expiry:[[0;3];MAX_SNAKES],inventory_expired:[0;MAX_SNAKES],store:false,inventory_release:[0;MAX_SNAKES],windup_frost:[0;MAX_SNAKES],windup_frost_mask:0,initial:[Effect::default();MAX_SNAKES],pickups:[Pickup::default();EFFECT_EVENTS],count:0,pickup_mask:0,movement_mask:0,venom_possible:0,frost_empty:0,frost_hits:[0;MAX_SNAKES],frozen:[0;MAX_SNAKES],immunity:[0;MAX_SNAKES],novas:[Freeze::default();EFFECT_EVENTS],nova_count:0,guards:0,guard_targets:[0;MAX_SNAKES],sever_cut:[0;MAX_SNAKES],bite_step:[0;MAX_SNAKES],consumed_at:[0;MAX_SNAKES],phase:[0;STEPS+1],surge:[0;STEPS+1]}}
}
impl Timeline {
    pub fn new(w:&World)->Self {
        let mut result=Self::default();
        if let Some(v)=w.vortex() {
            result.brew=Brew::new(w,v.position,2,v.life_ticks as usize+1,crate::world::whirlpool::PULL_END.saturating_sub(v.age_ticks) as usize+1);
        }
        for s in w.snakes().filter(|s|s.alive) {
            let id=s.id as usize;let e=Effect::observed(w,s);result.initial[id]=e;
            result.frozen[id]=s.face.frozen_ticks;result.immunity[id]=s.face.thaw_immunity_ticks;
            if e.is(EffectKind::Venom) {result.venom_possible|=1<<id;}
            result.set_guard(id,s.face.guarding,s.face.target_id);
            let masks=if e.is(EffectKind::Phase) {result.phase[0]|=1<<id;&mut result.phase}
                else if e.is(EffectKind::Surge) {&mut result.surge} else {continue;};
            for mask in &mut masks[1..=e.ticks.min(STEPS as u16) as usize] {*mask|=1<<id;}
        }
        result.store=w.config().store_power_ups;
        result.field_flips=w.items().filter(|i|i.kind==EffectKind::Flip).count() as u8;
        for s in w.snakes().filter(|s|s.alive) {
            let id=s.id as usize;result.inventory_counts[id]=s.inventory.count;result.inventory_expiry[id]=s.inventory.life;
            result.flip_slots[id]=(0..s.inventory.count as usize).fold(0,|bits,slot|bits|if s.inventory.kinds[slot]==EffectKind::Flip as u8 {1<<slot} else {0});
            if s.inventory.windup!=0 && s.inventory.life[s.inventory.windup as usize-1]>s.inventory.windup_ticks as u16 {
                let slot=s.inventory.windup as usize-1;
                result.schedule_use(id,EffectKind::from_byte(s.inventory.kinds[slot]),s.inventory.windup_ticks as usize+1,slot);

            }
        }
        // phase[0] is the cached observed owner mask, before completions.
        result.surge[0]=result.surge[1];result
    }
    /// Strategy values observed effects and unavoidable movement-one pickups.
    /// Later predicted rival pickups are possible outcomes, not grounds to
    /// abandon an opportunity. Physical rollouts still use their exact timeline.
    pub fn opportunities(w:&World,initial:Self,motions:&[Motion;MAX_SNAKES],boosted:&[Motion;MAX_SNAKES])->Self {
        if w.config().rules!=crate::RuleSet::V2 || !w.config().power_ups {return initial;}
        let mut forecast=Forecast::empty(initial);
        // Two movement bounds retain the strategy's possible/certain contract.
        // Their endpoint updates run through the physical ordered pass below.
        let poses=Cell::new([[Point::default();2];MAX_SNAKES]);
        let mut errors=[[0.0;2];MAX_SNAKES];
        let mut positions=[Point::default();MAX_SNAKES];
        let mut bounds=poses.get();
        for snake in w.snakes().filter(|s|s.alive) {
            let id=snake.id as usize;forecast.alive|=1<<id;
            let head=snake.segments[0].current;
            for (side,motion) in [&motions[id],&boosted[id]].into_iter().enumerate() {
                let (speed,turn)=motion.at(0);let travel=speed*STEP_SECONDS;
                bounds[id][side]=w.canonical_point(Point{x:head.x+snake.angle.cos()*travel,y:head.y+snake.angle.sin()*travel});
                errors[id][side]=2.0*travel*(turn*STEP_SECONDS*0.5).min(std::f64::consts::FRAC_PI_2).sin();
            }
            positions[id]=bounds[id][0];forecast.radii[id]=motions[id].radii[0];
        }
        poses.set(bounds);
        for (i,&item) in w.items().enumerate() {
            forecast.items[i]=item;forecast.pending|=1<<i;forecast.owners[i]=forecast.alive;
        }
        forecast.advance_ordered_bounds(w,1,&mut positions,|f,id| {
            let snake=w.snake(id).unwrap();let head=snake.segments[0].current;
            let mut bounds=poses.get();
            for (side,motion) in [&motions[id],&boosted[id]].into_iter().enumerate() {
                let travel=motion.at(0).0*STEP_SECONDS;
                // Radius and paid length are the same reconstruction inputs
                // used by candidate rollouts, after ordered earlier effects.
                let rush=if side==1 && w.boost_ready(id) {0.6} else {0.0};
                let len=f.endpoint_len(w,id,rush,1);let radius=f.radius(w,id,rush,motion,1);
                bounds[id][side]=crate::world::flip::forecast_pose(w,snake,&[head,bounds[id][side]],&[0.0,travel],len,radius).0;
            }
            poses.set(bounds);bounds[id][0]
        },None,0.0,|_f,id,item,_position| {
            let mut possible=false;let mut certain=true;
            for (side,motion) in [&motions[id],&boosted[id]].into_iter().enumerate() {
                let distance=w.distance_squared(poses.get()[id][side],item.position).sqrt();
                let reach=1.3*motion.radii[0]+item.radius;
                possible|=distance<=reach+errors[id][side];
                certain&=distance+errors[id][side]<=reach;
            }
            (possible,certain)
        });
        forecast.effects
    }
    pub fn snapshot(&self)->Snapshot {Snapshot {phase_possible:self.pickups[..self.count].iter().filter(|p|p.effect.is(EffectKind::Phase)).fold(self.phase[0],|mask,p|mask|(1<<p.id)),defeated:self.defeated,defeat_steps:self.defeat_steps,consumed_at:self.consumed_at.map(|step|step as u16),flip_mask:self.flip_mask,flip_steps:self.flip_steps,initial:self.initial,pickups:self.pickups,count:self.count,novas:self.novas,nova_count:self.nova_count}}
    pub fn release_clock(&self,id:usize,time:f64,slowdown:f64)->f64 {if self.flip_steps[id]!=0 && time>=self.flip_steps[id] as f64*STEP_SECONDS {return 0.0;} release_clock(&self.novas[..self.nova_count],id,time,slowdown)}
    pub fn release_uncacheable(&self)->bool {self.nova_count>crate::MAX_ITEMS}
    pub fn release_key(&self)->[u32;crate::MAX_ITEMS] {
        if self.nova_count==0 {return [0;crate::MAX_ITEMS];}
        std::array::from_fn(|i|if i<self.nova_count {(self.novas[i].step as u32)<<16|self.novas[i].mask as u32} else {0})
    }
    fn open_brew(&mut self,w:&World,center:Point,step:usize) {
        // Activation follows feeding at age zero. Its first pull is the next
        // movement; age LIFE bursts before feeding at step + LIFE. Only an
        // already observed vortex needs the extra movement-one offset.
        if self.brew.start==0 || step>=self.brew.end as usize {
            // Within the bounded rollout at most one observed brew can be
            // followed by a new one (STEPS < LIFE). Keep that transport so
            // a replacement cannot resurrect an already absorbed identity.
            self.previous_brew=self.brew;
            self.brew=Brew::new(w,center,step+1,step+crate::world::whirlpool::LIFE as usize,step+crate::world::whirlpool::PULL_END as usize);
        }
    }
    /// Shared rollouts can already contain a later activation. Food must use
    /// its predecessor through the activation frame, before the first pull.
    pub fn feeding_brew(&self,step:usize)->Brew {
        if self.previous_brew.start!=0 && step<self.brew.start as usize {self.previous_brew} else {self.brew}
    }
    pub fn food_available(&self,w:&World,f:target::TargetFood,step:usize)->bool {!self.brew.captures(w,f,step,self.previous_brew)}
    pub fn track(&self,id:usize)->Track {
        let mut result=Track {store:self.store,initial:self.initial[id],consumed_at:self.consumed_at[id] as u16,..Track::default()};
        for pickup in &self.pickups[..self.count] {
            if pickup.id as usize==id {let i=result.count as usize;result.pickups[i]=pickup.effect;result.steps[i]=pickup.step as u16;result.count+=1;}
        }
        result
    }
    #[inline]
    pub fn venom_bits(&self)->u16 {self.venom_possible}
    #[inline]
    pub fn may_have(&self,id:usize,kind:EffectKind)->bool {
        if kind==EffectKind::Venom {return self.venom_possible&(1<<id)!=0;}
        self.initial[id].is(kind) || self.pickups[..self.count].iter().any(|p|p.id as usize==id && p.effect.is(kind))
    }
    pub fn has_scheduled(&self)->bool {self.scheduled}
    pub fn collision_effects(&self)->bool {
        self.flip_mask!=0 || self.windup_frost.iter().any(|&t|t>0) || self.venom_possible!=0
            || self.initial.iter().any(|e|e.is(EffectKind::Phase) || e.is(EffectKind::Surge))
            || self.pickups[..self.count].iter().any(|p|p.effect.is(EffectKind::Phase) || p.effect.is(EffectKind::Surge))
    }
    pub fn defeat_after(&mut self,id:usize,step:usize) {
        if self.defeated&(1<<id)!=0 {return;}
        self.defeated|=1<<id;self.defeat_steps[id]=step as u16;
        // Collision cache only: projected corpses are absent, while at()/before()
        // retain real effect clocks. No Phase effect is activated.
        for bits in &mut self.phase[step+1..] {*bits|=1<<id;}
    }
    pub fn movement_bits(&self)->u16 {self.movement_mask}
    pub fn movement_event(&self,step:usize)->bool {
        (self.flip_mask!=0 && self.flip_steps.iter().any(|&t|t as usize==step)) || self.pickups[..self.count].iter().any(|p|p.step as usize==step && self.movement_mask&(1<<p.id)!=0) || self.novas[..self.nova_count].iter().any(|n|n.step as usize==step && n.mask!=0)
    }
    pub fn has_pickup(&self,id:usize)->bool {self.pickup_mask&(1<<id)!=0}
    fn surge_pickup_before(&self,id:usize,step:usize)->Option<usize> {
        // The first Surge already forgives the entire outstanding payment;
        // later replacements/refreshes cannot restore that debt.
        self.pickups[..self.count].iter().filter(|p|p.id as usize==id && (p.step as usize)<step && p.effect.is(EffectKind::Surge)).min_by_key(|p|p.step).map(|p|p.step as usize)
    }
    pub fn same_movement(&self,other:&Self,id:usize,step:usize)->bool {
        let a=self.before(id,step);let b=other.before(id,step);
        self.flip_steps[id]==other.flip_steps[id] && crate::effects::modifiers(a.kind,a.ticks).speed==crate::effects::modifiers(b.kind,b.ticks).speed
            && self.surge_pickup_before(id,step)==other.surge_pickup_before(id,step)
            && self.freeze_step(id,step)==other.freeze_step(id,step)
            && self.frozen_at(id,step)==other.frozen_at(id,step)
    }
    #[inline]
    pub fn at(&self,id:usize,step:usize)->Effect {
        let step=step.max(1);
        if let Some(pickup)=self.pickups[..self.count].iter().filter(|p|p.id as usize==id && p.step as usize<=step).max_by_key(|p|p.step) {
            if self.consumed_at[id]>=pickup.step as usize && self.consumed_at[id]<=step {return Effect::default();}
            return pickup.effect.after(step-pickup.step as usize);
        }
        if self.consumed_at[id]>0 && self.consumed_at[id]<=step {return Effect::default();}
        self.initial[id].after(step-1)
    }
    /// Effect used for movement/feed, before this step's endpoint pickups.
    #[inline]
    pub fn before(&self,id:usize,step:usize)->Effect {
        if step<=1 {self.initial[id]} else {self.at(id,step-1).after(1)}
    }
    #[inline]
    pub fn phased(&self,id:usize,step:usize)->bool {
        if step==0 {self.initial[id].is(EffectKind::Phase)}
        else if step<=STEPS {self.phase[step]&(1<<id)!=0}
        else {self.defeated&(1<<id)!=0 && step>self.defeat_steps[id] as usize || self.at(id,step).is(EffectKind::Phase)}
    }
    #[inline]
    pub fn contact(&self,a:usize,b:usize,step:usize)->bool {!self.phased(a,step) && !self.phased(b,step)}
    #[inline]
    pub fn phase_bits(&self,step:usize)->u16 {
        if step==0 {self.initial.iter().enumerate().fold(0,|mask,(id,e)|mask|if e.is(EffectKind::Phase) {1<<id} else {0})}
        else if step<=STEPS {self.phase[step]} else {self.masked_defeats(step)|self.kind_bits(EffectKind::Phase,step)}
    }
    #[inline]
    pub fn reach_scale(&self,id:usize,step:usize)->f64 {
        let active=if step<=STEPS {self.surge[step.max(1)]&(1<<id)!=0} else {self.at(id,step).is(EffectKind::Surge)};
        if active {1.6} else {1.0}
    }
    pub fn surge_bits(&self,step:usize)->u16 {
        if step<=STEPS {self.surge[step.max(1)]} else {self.kind_bits(EffectKind::Surge,step)}
    }
    fn masked_defeats(&self,step:usize)->u16 {
        let mut dead=0;let mut bits=self.defeated;
        while bits!=0 {let id=bits.trailing_zeros() as usize;bits&=bits-1;if step>self.defeat_steps[id] as usize {dead|=1<<id;}}
        dead
    }
    fn kind_bits(&self,kind:EffectKind,step:usize)->u16 {
        let mut result=0;
        for id in 0..MAX_SNAKES {if self.at(id,step).is(kind) {result|=1<<id;}}
        result
    }
    pub fn max_reach_scale(&self,step:usize)->f64 {if self.surge_bits(step)!=0 {1.6} else {1.0}}
    pub fn set_guard(&mut self,id:usize,guarding:bool,target:u64) {
        if guarding {self.guards|=1<<id;} else {self.guards&=!(1<<id);}
        self.guard_targets[id]=target;
    }
    #[inline]
    fn pickup_eligible(&self,item:&crate::Item,tick:u64,step:usize,id:usize,alive:bool)->bool {
        let guarding=self.guards&(1<<id)!=0 && !self.store;
        item.pickup_eligible(tick+step as u64,step-1,alive,guarding,
            if guarding {self.at(id,step).ticks} else {0})
    }
    pub(super) fn pickup(&mut self,id:usize,item:crate::Item,step:usize,order:usize)->bool {
        // Mechanics clears every guard committed to a capsule when it is
        // consumed, before considering the next capsule in item order.
        let mut guards=self.guards;
        while guards!=0 {
            let other=guards.trailing_zeros() as usize;guards&=guards-1;
            if self.guard_targets[other]==item.id {self.guards&=!(1<<other);}
        }
        for slot in 0..3 {
            let life=self.inventory_expiry[id][slot];
            if life>0 && (life as usize)<step && self.inventory_expired[id]&(1<<slot)==0 {
                self.inventory_counts[id]=self.inventory_counts[id].saturating_sub(1);self.inventory_expired[id]|=1<<slot;
            }
        }
        if self.inventory_release[id]!=0 && self.inventory_release[id] as usize<=step && self.inventory_expired[id]&8==0 {
            self.inventory_counts[id]=self.inventory_counts[id].saturating_sub(1);
            // Bit 3 marks the completed use; retain its deadline for pure capacity queries.
            self.inventory_expired[id]|=8;
        }
        if item.kind==EffectKind::Flip && self.field_flips>0 {self.field_flips-=1;}
        if (self.store || item.kind==EffectKind::Flip) && self.inventory_counts[id]<3 {
            self.inventory_counts[id]+=1;
            self.inventory_stashes[self.stash_count]=Pickup {step:step as u16,id:id as u8,order:order as u8,effect:Effect {kind:item.kind as u8,ticks:1800}};self.stash_count+=1;
            return false;
        }
        if item.kind==EffectKind::Flip && self.field_flips>0 {
            // Pickup arbitration runs again from the swapped endpoint. Until
            // that chain is modeled, another Flip capsule makes it unresolved.
            self.flip_conflict_at=if self.flip_conflict_at==0 {step as u16} else {self.flip_conflict_at.min(step as u16)};
        }
        self.activation(id,item.kind,step);true
    }
    /// Capacity immediately before an arrival, including shelf expiry,
    /// pending use and earlier forecast acquisitions in (step, item-order) order.
    /// The queried capsule itself and later same-movement stashes are excluded.
    pub fn stores_at(&self,w:&World,id:usize,step:usize,order:usize)->bool {
        if !self.store && !w.items().nth(order).is_some_and(|i|i.kind==EffectKind::Flip) {return false;}
        let inv=w.snake(id).unwrap().inventory;
        let expired=self.inventory_expiry[id][..inv.count as usize].iter().filter(|&&life|(life as usize)<step).count();
        let used=usize::from(self.inventory_release[id]!=0 && self.inventory_release[id] as usize<=step
            || inv.windup!=0 && inv.life[inv.windup as usize-1]>inv.windup_ticks as u16 && inv.windup_ticks as usize+1<=step);
        let stashed=self.inventory_stashes[..self.stash_count].iter().filter(|p|p.id as usize==id && p.order as usize!=order && (p.step as usize,p.order as usize)<(step,order)).count();
        (inv.count as usize).saturating_sub(expired+used)+stashed<3
    }
    #[inline]
    pub fn flip_available(&self,id:usize,step:usize)->bool {
        let mut slots=self.flip_slots[id]&7;
        if self.inventory_release[id]!=0 && self.inventory_release[id] as usize<=step {slots&=!(self.flip_slots[id]>>3);}
        (0..3).any(|slot|slots&(1<<slot)!=0 && self.inventory_expiry[id][slot]>0 && step<=self.inventory_expiry[id][slot] as usize)
            || self.inventory_stashes[..self.stash_count].iter().any(|p|p.id as usize==id
                && p.effect.kind==EffectKind::Flip as u8 && p.step as usize<=step
                && step<(p.step as usize+p.effect.ticks as usize))
    }
    pub(super) fn schedule_use(&mut self,id:usize,kind:EffectKind,step:usize,slot:usize) {
        self.scheduled=true;
        // Upper slot bits identify the one Flip consumed at the release deadline.
        if kind==EffectKind::Flip {self.flip_slots[id]|=1<<(slot+3);}
        self.inventory_release[id]=step as u16;self.inventory_expiry[id][slot]=u16::MAX;
        if kind==EffectKind::Whirlpool {self.windup_brew_mask|=1<<id;}
        else if kind==EffectKind::Frost {self.windup_frost[id]=step as u16;self.windup_frost_mask|=1<<id;}
        else {self.activation(id,kind,step);}
    }
    pub(super) fn activation(&mut self,id:usize,kind:EffectKind,step:usize) {
        if kind==EffectKind::Flip {
            if self.flip_steps[id]!=0 {
                self.flip_conflict_at=if self.flip_conflict_at==0 {step as u16} else {self.flip_conflict_at.min(step as u16)};
                return;
            }
            self.flip_steps[id]=step as u16;self.flip_mask|=1<<id;self.movement_mask|=1<<id;return;
        }
        if matches!(kind,EffectKind::Frost|EffectKind::Whirlpool) {return;}

        if kind==EffectKind::Venom {self.venom_possible|=1<<id;}
        let effect=if kind==EffectKind::Frost {Effect::default()} else {Effect {kind:kind as u8,ticks:kind.duration()}};
        self.pickups[self.count]=Pickup {id:id as u8,effect,step:step as u16,..Default::default()};self.count+=1;self.pickup_mask|=1<<id;
        let initial=self.initial[id];
        if kind==EffectKind::Surge || crate::effects::modifiers(initial.kind,initial.ticks).speed!=1.0 {self.movement_mask|=1<<id;}
        for j in step..=STEPS {
            self.phase[j]&=!(1<<id);self.surge[j]&=!(1<<id);
            let active=self.at(id,j);
            if active.ticks>0 {
                if active.kind==EffectKind::Phase as u8 {self.phase[j]|=1<<id;}
                if active.kind==EffectKind::Surge as u8 {self.surge[j]|=1<<id;}
            }
        }
    }
    fn freeze_step(&self,id:usize,step:usize)->Option<usize> {
        self.novas[..self.nova_count].iter().find(|n|n.mask&(1<<id)!=0 && (n.step as usize)<step).map(|n|n.step as usize)
    }
    fn frozen_at(&self,id:usize,step:usize)->u16 {
        let at=self.novas[..self.nova_count].iter().rev().find(|n|n.mask&(1<<id)!=0 && n.step as usize<=step);
        if let Some(n)=at {crate::effects::frost::FROZEN_TICKS.saturating_sub((step-n.step as usize).min(u16::MAX as usize) as u16)}
        else {self.frozen[id].saturating_sub(step.saturating_sub(1).min(u16::MAX as usize) as u16)}
    }
    fn immunity_at(&self,id:usize,step:usize)->u16 {
        let at=self.novas[..self.nova_count].iter().rev().find(|n|n.mask&(1<<id)!=0 && n.step as usize<=step);
        let end=at.map(|n|n.step as usize+crate::effects::frost::FROZEN_TICKS as usize)
            .or_else(||(self.frozen[id]>0).then_some(self.frozen[id] as usize+1));
        if let Some(end)=end {if step<end {0} else {crate::effects::frost::THAW_IMMUNITY_TICKS.saturating_sub((step-end).min(u16::MAX as usize) as u16)}}
        else {self.immunity[id].saturating_sub(step.saturating_sub(1).min(u16::MAX as usize) as u16)}
    }
    pub(super) fn nova(&mut self,w:&World,owner:usize,center:Point,step:usize,alive:u16,positions:impl Fn(usize)->Point) {
        let mut mask=0u16;let mut heads=alive;
        while heads!=0 {
            let id=heads.trailing_zeros() as usize;heads&=heads-1;
            if crate::effects::frost::freeze_eligible(true,id==owner,self.frozen_at(id,step),self.immunity_at(id,step),
                w.distance_squared(center,positions(id)),w.config().base_radius()) {mask|=1<<id;}
        }
        if mask==0 {self.frost_empty|=1<<owner;}
        self.frost_hits[owner]+=mask.count_ones() as u8;
        self.novas[self.nova_count]=Freeze {step:step as u16,mask};self.nova_count+=1;
        self.pickup_mask|=mask;self.movement_mask|=mask;
    }
    pub fn mask(&self,w:&World,id:usize,step:usize)->u16 {
        if self.phased(id,step) {0} else {
            (if w.config().self_collisions {u16::MAX} else {u16::MAX^(1<<id)}) & !self.phase_bits(step)
        }
    }
}

/// Immutable per-observation capsule bounds and contests. Alternative controls
/// only recompute their own endpoints and rivals whose movement has changed.
#[derive(Clone, Copy)]
pub(super) struct Items {
    reach:[[[u16;MAX_SNAKES];crate::MAX_ITEMS];2],
    earliest:[[u16;MAX_SNAKES];2],
    owners:[[u16;crate::MAX_ITEMS];STEPS+1],
    hits:[[u16;crate::MAX_ITEMS];STEPS+1],
    near:[[u16;crate::MAX_ITEMS];STEPS+1],
    items:[crate::Item;crate::MAX_ITEMS],
    pending:u8,
    alive:u16,
    radii:[f64;MAX_SNAKES],
    pickup_steps:[u16;crate::MAX_ITEMS],
    pickup_owners:[u8;crate::MAX_ITEMS],
    movement_mask:u16,
    pickup_movement:[u16;crate::MAX_ITEMS],
}
impl Default for Items {
    fn default()->Self {Self {reach:[[[u16::MAX;MAX_SNAKES];crate::MAX_ITEMS];2],earliest:[[u16::MAX;MAX_SNAKES];2],owners:[[0;crate::MAX_ITEMS];STEPS+1],hits:[[0;crate::MAX_ITEMS];STEPS+1],near:[[0;crate::MAX_ITEMS];STEPS+1],items:[crate::Item::default();crate::MAX_ITEMS],pending:0,alive:0,radii:[0.0;MAX_SNAKES],pickup_steps:[u16::MAX;crate::MAX_ITEMS],pickup_owners:[MAX_SNAKES as u8;crate::MAX_ITEMS],movement_mask:0,pickup_movement:[0;crate::MAX_ITEMS]}}
}
impl Items {
    pub fn shared(&mut self,forecast:&Forecast) {
        self.pickup_steps=forecast.pickup_steps;self.pickup_owners=forecast.pickup_owners;
        self.movement_mask=forecast.effects.movement_bits();
        for i in 0..crate::MAX_ITEMS {
            let owner=self.pickup_owners[i] as usize;
            self.pickup_movement[i]=if owner<MAX_SNAKES {(1<<owner)&self.movement_mask} else {0};
            if self.items[i].kind==EffectKind::Frost {
                // A missed Nova restores its victims' full-speed paths. Several
                // Novas at one endpoint may share victims, so retain their union.
                for nova in &forecast.effects.novas[..forecast.effects.nova_count] {
                    if nova.step==self.pickup_steps[i] {self.pickup_movement[i]|=nova.mask;}
                }
            }
        }
    }
    pub fn reachable(&self,id:usize,horizon:usize,boosted:bool)->bool {self.earliest[usize::from(boosted)][id] as usize<=horizon}
    pub fn prepare(w:&World,motions:&[Motion;MAX_SNAKES],boosted:&[Motion;MAX_SNAKES])->Self {
        let mut result=Self::default();
        if w.config().rules!=crate::RuleSet::V2 || !w.config().power_ups || w.items().next().is_none() {return result;}
        let field_surge=w.items().any(|i|i.kind==EffectKind::Surge);
        let field_flip=w.items().any(|i|i.kind==EffectKind::Flip);
        let surge_scale=crate::effects::modifiers(EffectKind::Surge as u8,1).speed;
        for s in w.snakes().filter(|s|s.alive) {result.alive|=1<<s.id;result.radii[s.id as usize]=s.radius;}
        for (i,item) in w.items().enumerate() {
            result.items[i]=*item;result.pending|=1<<i;
            for s in w.snakes().filter(|s|s.alive) {
                let id=s.id as usize;
                // Include proposed uses as well as the already-pending wind-up.
                // These shared bounds serve every alternative control.
                let inventory_surge=w.config().store_power_ups && (0..s.inventory.count as usize)
                    .any(|slot|s.inventory.kinds[slot]==EffectKind::Surge as u8 && s.inventory.life[slot]>s.inventory.windup_ticks as u16);
                let scale=if field_surge || inventory_surge {surge_scale} else {1.0};
                let distance=Self::reversal_distance(w,s,item.position,field_flip).sqrt();
                let travel=(distance-1.3*s.radius-item.radius).max(0.0);
                for (mode,motion) in [&motions[id],&boosted[id]].into_iter().enumerate() {
                    // Round down: pruning may retain an extra tick, never drop a reachable pickup.
                    result.reach[mode][i][id]=(travel/(motion.max_speed*scale*STEP_SECONDS)).floor().clamp(1.0,u16::MAX as f64) as u16;
                    result.earliest[mode][id]=result.earliest[mode][id].min(result.reach[mode][i][id]);
                    if item.kind==EffectKind::Frost {
                        // Contact bounds prune capsule ownership, not Nova
                        // participation. Its centre is the collector's head.
                        let owner_radius=result.radii.iter().copied().fold(0.0,f64::max);
                        let nova_travel=(distance-16.0*w.config().base_radius()-1.3*owner_radius-item.radius).max(0.0);
                        let nova_step=(nova_travel/(motion.max_speed*scale*STEP_SECONDS)).floor().clamp(1.0,u16::MAX as f64) as u16;
                        result.earliest[mode][id]=result.earliest[mode][id].min(nova_step);
                    }
                    let step=result.reach[mode][i][id] as usize;
                    if mode==0 && step<=STEPS {result.owners[step][i]|=1<<id;}
                }
            }
        }
        for step in 2..=STEPS {
            for i in 0..crate::MAX_ITEMS {result.owners[step][i]|=result.owners[step-1][i];}
        }
        result
    }
    /// A reversal can start at any retained body index (payment or Venom
    /// can shorten it). Distance to the body bounds all those destinations.
    fn reversal_distance(w:&World,s:SnakeView<'_>,p:Point,field_flip:bool)->f64 {
        let can_flip=field_flip || s.inventory.kinds[..s.inventory.count as usize].contains(&(EffectKind::Flip as u8));
        if !can_flip {return w.distance_squared(s.segments[0].current,p);}
        let nearest=s.segments.iter().map(|segment|w.distance_squared(segment.current,p)).fold(f64::INFINITY,f64::min).sqrt();
        // A shortened/radius-adjusted tail may lie between indexed samples.
        // Retain one full spacing of slack rather than prune that destination.
        (nearest-s.radius*1.18).max(0.0).powi(2)
    }
    pub fn contests(&mut self,w:&World,rivals:&[Rival;MAX_SNAKES],motions:&[Motion;MAX_SNAKES],effects:&Timeline) {
        for (i,item) in w.items().enumerate() {
            for (id,r) in rivals.iter().enumerate().filter(|(_,r)|r.alive) {
                if self.reach[0][i][id] as usize>STEPS {continue;}
                let surge=effects.surge_pickup_before(id,STEPS+1);
                let pickup_radius=surge.map(|step|w.ai_forecast_radius(id,0.0,step,Some(step),effects.freeze_step(id,STEPS+1)));
                // Cache endpoint contact only: award-time eligibility depends
                // on earlier pickups in each candidate's own item pass.
                for step in 1..=STEPS {
                    if !item.pickup_eligible(w.tick()+step as u64,step-1,true,false,0) {continue;}
                    let radius=if surge.is_some_and(|s|s<step) {pickup_radius.unwrap()} else {motions[id].radii[effects.freeze_step(id,step).map_or(step-1,|f|(step-1).min(f-1)).min(24)]};
                    let reach=1.3*radius+item.radius;
                    let distance=w.distance_squared(r.path[step],item.position);
                    if distance<=reach*reach {self.hits[step][i]|=1<<id;}

                }
            }
            if item.kind==EffectKind::Phase {
                // Static rivals' exact future endpoints are already available.
                // A merely nearby path must not force one-tick sweeps forever.
                for step in 1..=STEPS {
                    for future in step..=(step+4).min(STEPS) {self.near[step][i]|=self.hits[future][i];}
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct MotionValue {key:u32,surge:usize,rush:f64,limits:(f64,f64)}
impl MotionValue {const EMPTY:Self=Self {key:u32::MAX,surge:usize::MAX,rush:0.0,limits:(0.0,0.0)};}

pub(super) struct Forecast {
    pub effects:Timeline,
    items:[crate::Item;crate::MAX_ITEMS],
    pending:u8,
    owners:[u16;crate::MAX_ITEMS],
    alive:u16,
    radii:[f64;MAX_SNAKES],
    pub sweep_needed:bool,
    pickup_steps:[u16;crate::MAX_ITEMS],
    pickup_owners:[u8;crate::MAX_ITEMS],
    movement_changed:u16,
    motion_cache:[Cell<MotionValue>;MAX_SNAKES],
    radius_cache:[Cell<(usize,f64)>;MAX_SNAKES],
}
impl Forecast {
    pub fn empty(initial:Timeline)->Self {
        Self {effects:initial,items:[crate::Item::default();crate::MAX_ITEMS],pending:0,owners:[0;crate::MAX_ITEMS],alive:0,radii:[0.0;MAX_SNAKES],sweep_needed:false,pickup_steps:[u16::MAX;crate::MAX_ITEMS],pickup_owners:[MAX_SNAKES as u8;crate::MAX_ITEMS],movement_changed:0,motion_cache:[const {Cell::new(MotionValue::EMPTY)};MAX_SNAKES],radius_cache:[const {Cell::new((usize::MAX,0.0))};MAX_SNAKES]}
    }
    #[cfg(test)]
    pub fn new(w:&World,initial:Timeline)->Self {
        let mut result=Self {effects:initial,items:[crate::Item::default();crate::MAX_ITEMS],pending:0,owners:[0;crate::MAX_ITEMS],alive:0,radii:[0.0;MAX_SNAKES],sweep_needed:false,pickup_steps:[u16::MAX;crate::MAX_ITEMS],pickup_owners:[MAX_SNAKES as u8;crate::MAX_ITEMS],movement_changed:0,motion_cache:[const {Cell::new(MotionValue::EMPTY)};MAX_SNAKES],radius_cache:[const {Cell::new((usize::MAX,0.0))};MAX_SNAKES]};
        if w.config().rules!=crate::RuleSet::V2 || !w.config().power_ups || w.items().next().is_none() {return result;}
        for s in w.snakes().filter(|s|s.alive) {result.alive|=1<<s.id;result.radii[s.id as usize]=s.radius;}
        if w.config().rules==crate::RuleSet::V2 && w.config().power_ups {
            for (i,&item) in w.items().enumerate() {result.items[i]=item;result.pending|=1<<i;result.owners[i]=result.alive;}
        }
        result
    }
    pub fn cached(_w:&World,initial:Timeline,items:&Items,horizon:usize,boosted:u16)->Self {
        let mut result=Self {effects:initial,items:items.items,pending:items.pending,owners:items.owners[horizon],alive:items.alive,radii:items.radii,sweep_needed:false,pickup_steps:[u16::MAX;crate::MAX_ITEMS],pickup_owners:[MAX_SNAKES as u8;crate::MAX_ITEMS],movement_changed:0,motion_cache:[const {Cell::new(MotionValue::EMPTY)};MAX_SNAKES],radius_cache:[const {Cell::new((usize::MAX,0.0))};MAX_SNAKES]};
        let mut pending=result.pending;
        while pending!=0 {
            let i=pending.trailing_zeros() as usize;pending&=pending-1;
            let mut changed=boosted & result.alive;
            while changed!=0 {
                let id=changed.trailing_zeros() as usize;changed&=changed-1;
                if items.reach[1][i][id] as usize<=horizon {result.owners[i]|=1<<id;}
            }
            if result.owners[i]==0 {result.pending&=!(1<<i);}
        }
        result
    }
    pub fn advance_windups(&mut self,w:&World,step:usize,positions:impl Fn(usize)->Point) {
        let mut brews=self.effects.windup_brew_mask;
        while brews!=0 {
            let id=brews.trailing_zeros() as usize;brews&=brews-1;
            if self.effects.inventory_release[id] as usize==step && self.effects.defeated&(1<<id)==0 {
                self.effects.open_brew(w,positions(id),step);
            }
        }
        let mut pending=self.effects.windup_frost_mask;
        while pending!=0 {
            let id=pending.trailing_zeros() as usize;pending&=pending-1;
            if self.effects.windup_frost[id] as usize==step && self.effects.defeated&(1<<id)==0 {
                let alive=w.snakes().filter(|s|s.alive).fold(0,|bits,s|bits|(1<<s.id)) & !self.effects.defeated;
                self.effects.nova(w,id,positions(id),step,alive,&positions);
                self.movement_changed|=self.effects.movement_bits();
            }
        }
    }
    /// Mirror activate_inventory(), then pickup_items(): every completion
    /// updates the endpoints seen by later IDs and by later field capsules.
    /// Used only when a reversal is possible; ordinary cached contests stay hot.
    pub fn advance_ordered(&mut self,w:&World,step:usize,positions:&mut [Point;MAX_SNAKES],
        reverse:impl FnMut(&Self,usize)->Point,cached:Option<&Items>,reserve:f64) {
        self.advance_ordered_bounds(w,step,positions,reverse,cached,reserve,|f,id,item,position| {
            let contact=w.distance_squared(position,item.position)<=(1.3*f.radii[id]+item.radius).powi(2);
            (contact,contact)
        });
    }
    // A possible lower ID blocks a later certain pickup. Both strategies and
    // physical rollouts share completions, swaps, guards and Nova ordering.
    fn advance_ordered_bounds(&mut self,w:&World,step:usize,positions:&mut [Point;MAX_SNAKES],
        mut reverse:impl FnMut(&Self,usize)->Point,cached:Option<&Items>,reserve:f64,
        mut contact:impl FnMut(&Self,usize,&crate::Item,Point)->(bool,bool)) {
        self.sweep_needed=false;
        for id in 0..MAX_SNAKES {
            if self.effects.defeated&(1<<id)!=0 {continue;}
            if self.effects.flip_steps[id] as usize==step {
                positions[id]=reverse(self,id);
                self.effects.set_guard(id,false,0);
            }
            if self.effects.inventory_release[id] as usize==step && self.effects.windup_brew_mask&(1<<id)!=0 {
                self.effects.open_brew(w,positions[id],step);
            }
            if self.effects.windup_frost[id] as usize==step && self.effects.windup_frost_mask&(1<<id)!=0 {
                let alive=w.snakes().filter(|s|s.alive).fold(0,|bits,s|bits|(1<<s.id)) & !self.effects.defeated;
                self.effects.nova(w,id,positions[id],step,alive,|other|positions[other]);
                self.movement_changed|=self.effects.movement_bits();
            }
        }
        let mut pending=self.pending;
        while pending!=0 {
            let i=pending.trailing_zeros() as usize;pending&=pending-1;
            let item=self.items[i];
            if step>item.life_ticks as usize {self.pending&=!(1<<i);continue;}
            if !item.pickup_eligible(w.tick()+step as u64,step-1,true,false,0) {continue;}
            let mut owners=self.alive & self.owners[i];let mut winner=MAX_SNAKES;
            while owners!=0 {
                let id=owners.trailing_zeros() as usize;owners&=owners-1;
                if !self.effects.pickup_eligible(&item,w.tick(),step,id,true) {continue;}
                let reach=1.3*self.radii[id]+item.radius;
                let distance=w.distance_squared(positions[id],item.position);
                if item.kind==EffectKind::Phase && reserve>0.0 && distance<=(reach+reserve).powi(2) {self.sweep_needed=true;}
                let (possible,certain)=contact(self,id,&item,positions[id]);
                if possible {if certain {winner=id;} break;}
            }
            if let Some(items)=cached {
                if items.pickup_steps[i] as usize==step && winner!=items.pickup_owners[i] as usize {
                    self.movement_changed|=items.pickup_movement[i];
                }
            }
            if winner==MAX_SNAKES {continue;}
            let active=self.effects.pickup(winner,item,step,i);
            if active && item.kind==EffectKind::Whirlpool {self.effects.open_brew(w,item.position,step);}
            if active && item.kind==EffectKind::Flip {
                positions[winner]=reverse(self,winner);
                self.effects.set_guard(winner,false,0);
            }
            if active && item.kind==EffectKind::Frost {
                self.effects.nova(w,winner,positions[winner],step,self.alive,|id|positions[id]);
            }
            self.movement_changed|=self.effects.movement_bits();
            self.pickup_steps[i]=step as u16;self.pickup_owners[i]=winner as u8;self.pending&=!(1<<i);
        }
    }
    pub fn movement_changed(&self)->u16 {self.movement_changed}
    pub fn reachable_surge(&self,id:usize)->bool {
        self.effects.pickups[..self.effects.count].iter().any(|p|p.id as usize==id && p.effect.is(EffectKind::Surge)) || self.items.iter().enumerate().any(|(i,item)|self.pending&(1<<i)!=0 && self.owners[i]&(1<<id)!=0 && item.kind==EffectKind::Surge)
    }
    /// Cached ordinary rival contests; changed participants remain exact.
    #[inline]
    pub fn advance_cached(&mut self,w:&World,step:usize,positions:impl Fn(usize)->Point,changed:u16,items:&Items,reserve:f64) {
        self.sweep_needed=false;
        self.advance_windups(w,step,&positions);
        if self.pending==0 {return;}
        self.advance_cached_items(w,step,positions,changed,items,reserve);
    }
    #[inline(never)]
    fn advance_cached_items(&mut self,w:&World,step:usize,positions:impl Fn(usize)->Point,changed:u16,items:&Items,reserve:f64) {
        let mut pending=self.pending;
        while pending!=0 {
            let i=pending.trailing_zeros() as usize;pending&=pending-1;
            let item=self.items[i];
            if step>item.life_ticks as usize {self.pending&=!(1<<i);continue;}
            if !item.pickup_eligible(w.tick()+step as u64,step-1,true,false,0) {continue;}
            let mut owners=self.alive & self.owners[i];
            let mut guards=owners & self.effects.guards;
            while guards!=0 {
                let id=guards.trailing_zeros() as usize;guards&=guards-1;
                if !self.effects.pickup_eligible(&item,w.tick(),step,id,true) {owners&=!(1<<id);}
            }
            let mut hits=items.hits[step][i] & owners & !changed;
            if reserve>0.0 && items.near[step][i] & owners & !changed!=0 {self.sweep_needed=true;}
            let mut dynamic=owners & changed;
            while dynamic!=0 {
                let id=dynamic.trailing_zeros() as usize;dynamic&=dynamic-1;
                let reach=1.3*self.radii[id]+item.radius;
                let distance=w.distance_squared(positions(id),item.position);
                if item.kind==EffectKind::Phase && reserve>0.0 && distance<=(reach+reserve).powi(2) {self.sweep_needed=true;}
                if distance<=reach*reach {hits|=1<<id;}
            }
            let winner=if hits==0 {MAX_SNAKES} else {hits.trailing_zeros() as usize};
            let expected=items.pickup_owners[i] as usize;
            let expected_step=items.pickup_steps[i] as usize;
            if expected_step==step && winner!=expected {
                self.movement_changed|=items.pickup_movement[i];
            }
            if winner!=MAX_SNAKES {
                let active=self.effects.pickup(winner,item,step,i);
            if active && item.kind==EffectKind::Whirlpool {self.effects.open_brew(w,item.position,step);}
                if active && item.kind==EffectKind::Frost {self.effects.nova(w,winner,positions(winner),step,self.alive,&positions);self.movement_changed|=self.effects.movement_bits()|items.pickup_movement[i];}
                self.pickup_steps[i]=step as u16;self.pickup_owners[i]=winner as u8;
                if winner!=expected || step!=expected_step {
                    let affected=(1<<winner) | if expected<MAX_SNAKES {1<<expected} else {0};
                    self.movement_changed|=items.pickup_movement[i]|affected&(items.movement_mask|self.effects.movement_bits());
                }
                self.pending&=!(1<<i);
            }
        }
    }
    /// Conservative travel bounds discard unreachable owners once, rather
    /// than testing fourteen endpoints per capsule on every rollout movement.
    #[inline]
    #[cfg(test)]
    pub fn bound(&mut self,w:&World,horizon:usize,max_speed:impl Fn(usize)->f64) {
        if self.pending==0 {return;}
        self.bound_items(w,horizon,max_speed);
    }
    #[inline(never)]
    #[cfg(test)]
    fn bound_items(&mut self,w:&World,horizon:usize,max_speed:impl Fn(usize)->f64) {
        let pickup_scale=if self.items.iter().any(|item|item.kind==EffectKind::Surge) {
            crate::effects::modifiers(EffectKind::Surge as u8,1).speed
        } else {1.0};
        for (i,item) in self.items.iter().enumerate() {
            if self.pending&(1<<i)==0 {continue;}
            let mut alive=self.alive;let mut owners=0;
            while alive!=0 {
                let id=alive.trailing_zeros() as usize;alive&=alive-1;
                let reach=max_speed(id)*pickup_scale*horizon as f64*STEP_SECONDS+1.3*self.radii[id]+item.radius;
                if Items::reversal_distance(w,w.snake(id).unwrap(),item.position,self.items.iter().any(|i|i.kind==EffectKind::Flip))<=reach*reach {owners|=1<<id;}
            }
            self.owners[i]=owners;
            if owners==0 {self.pending&=!(1<<i);}
        }
    }
    pub fn defeat_after(&mut self,id:usize,step:usize) {self.effects.defeat_after(id,step);self.alive&=!(1<<id);}
    pub fn endpoint_len(&self,w:&World,id:usize,rush:f64,step:usize)->usize {
        let len=w.ai_forecast_len(id,rush,step.saturating_sub(1),self.effects.surge_pickup_before(id,step+1),self.effects.freeze_step(id,step+1));
        if self.effects.sever_cut[id]>0 && self.effects.bite_step[id]<=step {len.min(self.effects.sever_cut[id])} else {len}
    }
    pub fn body_len(&self,w:&World,id:usize,rush:f64,step:usize)->usize {
        w.ai_forecast_len(id,rush,24,self.effects.surge_pickup_before(id,step+1),self.effects.freeze_step(id,step+1))
    }
    pub fn brewing(&self)->bool {self.effects.brew.start!=0 || self.effects.windup_brew_mask!=0
        || self.pending!=0 && self.items.iter().any(|item|item.kind==EffectKind::Whirlpool)}
    pub fn has_items(&self)->bool {self.pending!=0 || self.effects.has_scheduled()}
    pub fn set_radius(&mut self,id:usize,radius:f64) {self.radii[id]=radius;}
    pub fn set_radii(&mut self,radius:impl Fn(usize)->f64) {
        if self.pending==0 {return;}
        let mut alive=self.alive;
        while alive!=0 {let id=alive.trailing_zeros() as usize;alive&=alive-1;self.radii[id]=radius(id);}
    }
    /// All movement endpoints must be available before awarding any capsule.
    /// The reserve forces individual sweeps before Phase acquisition by either
    /// party, preserving the preceding corporeal movements.
    #[inline]
    pub fn advance(&mut self,w:&World,step:usize,positions:impl Fn(usize)->Point,reserve:f64) {
        self.sweep_needed=false;
        self.advance_windups(w,step,&positions);
        if self.pending==0 {return;}
        self.advance_items(w,step,positions,reserve);
    }
    #[inline(never)]
    fn advance_items(&mut self,w:&World,step:usize,positions:impl Fn(usize)->Point,reserve:f64) {
        let mut pending=self.pending;
        while pending!=0 {
            let i=pending.trailing_zeros() as usize;pending&=pending-1;
            let item=self.items[i];
            if step>item.life_ticks as usize {self.pending&=!(1<<i);continue;}
            if !item.pickup_eligible(w.tick()+step as u64,step-1,true,false,0) {continue;}
            let mut alive=self.alive & self.owners[i];
            while alive!=0 {
                let id=alive.trailing_zeros() as usize;alive&=alive-1;
                if !self.effects.pickup_eligible(&item,w.tick(),step,id,true) {continue;}
                let reach=1.3*self.radii[id]+item.radius;
                let distance=w.distance_squared(positions(id),item.position);
                if item.kind==EffectKind::Phase && reserve>0.0 && distance<=(reach+reserve).powi(2) {self.sweep_needed=true;}
                if distance>reach*reach {continue;}
                let active=self.effects.pickup(id,item,step,i);
                if active && item.kind==EffectKind::Whirlpool {self.effects.open_brew(w,item.position,step);}
                if active && item.kind==EffectKind::Frost {self.effects.nova(w,id,positions(id),step,self.alive,&positions);self.movement_changed|=self.effects.movement_bits();}
                self.pickup_steps[i]=step as u16;self.pickup_owners[i]=id as u8;self.pending&=!(1<<i);break;
            }
        }
    }
    #[inline]
    pub fn radius(&self,w:&World,id:usize,rush:f64,motion:&Motion,step:usize)->f64 {
        if !self.effects.has_pickup(id) {return motion.radii[(step-1).min(24)];}
        self.radius_after_pickup(w,id,rush,motion,step)
    }
    #[inline(never)]
    fn radius_after_pickup(&self,w:&World,id:usize,rush:f64,motion:&Motion,step:usize)->f64 {
        if let Some(surge)=self.effects.surge_pickup_before(id,step) {
            let cached=self.radius_cache[id].get();
            let freeze=self.effects.freeze_step(id,step);
            // Only the first payment cutoff matters; retain the original
            // cache footprint while also invalidating an earlier freeze.
            let cutoff=freeze.map_or(surge,|f|surge.min(f));
            if cached.0==cutoff {return cached.1;}
            let radius=w.ai_forecast_radius(id,rush,step-1,Some(surge),freeze);
            self.radius_cache[id].set((cutoff,radius));radius
        }
        else {motion.radii[self.effects.freeze_step(id,step).map_or(step-1,|f|(step-1).min(f-1)).min(24)]}
    }
    #[inline]
    pub fn motion(&self,w:&World,id:usize,rush:f64,motion:&Motion,step:usize)->(f64,f64) {
        if !self.effects.has_pickup(id) {return motion.at(step-1);}
        self.motion_after_pickup(w,id,rush,motion,step)
    }
    /// The endpoint needs only remaining movement stages. An expired burst's
    /// turning radius must not reject the ordinary-speed wall exit after it.
    #[inline]
    pub fn wall_limits(&self,w:&World,id:usize,rush:f64,motion:&Motion,step:usize)->(f64,f64) {
        let mut bound=motion.wall_limits_from(step.saturating_sub(1));
        if self.effects.has_pickup(id) {
            let thaw=step+self.effects.frozen_at(id,step) as usize;
            let expiry=step+self.effects.before(id,step).ticks as usize;
            // Motion is piecewise between payment, activation, expiry, thaw
            // and Nightfall boundaries. Include both ends of every future
            // stage: dawn may raise speed while a scheduled Surge is active.
            let mut sample=|future:usize| {
                if future<step {return;}
                let (speed,turn)=self.motion(w,id,rush,motion,future);
                bound.0=bound.0.max(speed/turn.max(0.01));bound.1=bound.1.max(speed);
            };
            for future in [step,thaw,expiry,25] {sample(future);}
            for p in self.effects.pickups[..self.effects.count].iter().filter(|p|p.id as usize==id) {
                let start=p.step as usize+1;
                let end=p.step as usize+p.effect.ticks as usize;
                for future in [start,end,end+1] {sample(future);}
            }
            for n in self.effects.novas[..self.effects.nova_count].iter().filter(|n|n.mask&(1<<id)!=0) {
                for future in [n.step as usize+1,n.step as usize+crate::effects::frost::FROZEN_TICKS as usize] {sample(future);}
            }
            // Ordinary daylight stages are already reserved by Motion.
            // Only sample Nightfall boundaries overlapping changed effects;
            // reconstructing motion for Nightfall thousands of ticks away
            // cannot improve this bound and evicts the useful motion cache.
            let changed_end=self.effects.pickups[..self.effects.count].iter().filter(|p|p.id as usize==id)
                .map(|p|p.step as usize+p.effect.ticks as usize+1).max().unwrap_or(step).max(thaw);
            if let Some(start)=w.forecast_night_start() {
                for tick in [start,start+90,start+crate::world::events::NIGHT_LIFE-90,start+crate::world::events::NIGHT_LIFE] {
                    let future=tick.saturating_sub(w.tick()) as usize;
                    if future<=changed_end {sample(future);}
                }
            }
        }
        bound
    }
    // Keep replacement's state reconstruction off the ordinary cached path.
    // A different snake's pickup cannot change this snake's motion or radius.
    #[inline(never)]
    fn motion_after_pickup(&self,w:&World,id:usize,rush:f64,motion:&Motion,step:usize)->(f64,f64) {
        let effect=self.effects.before(id,step);
        let surge=self.effects.surge_pickup_before(id,step);
        // Phase/Magnet replacement changes contact/feed state, not motion.
        // Only Surge changes the speed modifier or forgives burst debt in R3.
        let initial=self.effects.initial[id].after(step-1);
        let freeze=self.effects.freeze_step(id,step);
        let frozen=self.effects.frozen_at(id,step);
        let night_changes=w.forecast_night(step-1)!=w.world_event.night;
        if !night_changes && freeze.is_none() && surge.is_none() && crate::effects::modifiers(effect.kind,effect.ticks).speed==crate::effects::modifiers(initial.kind,initial.ticks).speed {motion.at(step-1)}
        else {
            // After the single burst ends, only effect speed transitions can
            // change motion. Counter decrements alone do not change limits.
            let key=(step-1).min(if night_changes {u16::MAX as usize} else {24}) as u32 | ((if effect.ticks>0 {effect.kind} else {0}) as u32)<<16 | if frozen>0 {0x8000_0000} else {0};
            let surge_step=surge.unwrap_or(usize::MAX);
            let cached=self.motion_cache[id].get();
            if cached.key==key && cached.surge==surge_step && cached.rush==rush {return cached.limits;}
            let limits=w.ai_forecast_frost_motion(id,rush,step-1,effect.kind,effect.ticks,surge,freeze,frozen);
            self.motion_cache[id].set(MotionValue {key,surge:surge_step,rush,limits});limits
        }
    }
}

#[cfg(test)]
mod review_tests {
    use super::*;
    #[test]
    fn sparse_snapshot_phase_mask_matches_full_scan_at_transitions() {
        let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,density:0.0,
            width:4000.0,height:2000.0,..Default::default()},
            &[(Point{x:1500.0,y:1000.0},0.0,40,0.0),(Point{x:800.0,y:1000.0},0.0,20,0.0)],&[]).unwrap();
        w.snakes[0].effect_kind=EffectKind::Phase as u8;w.snakes[0].effect_ticks=3;
        let mut timeline=Timeline::new(&w);
        // Replacement, new Phase, then expiry, with same-step projected deaths.
        timeline.pickups[0]=Pickup {step:3,id:1,effect:Effect {kind:3,ticks:8},..Default::default()};
        timeline.pickups[1]=Pickup {step:7,id:1,effect:Effect {kind:1,ticks:20},..Default::default()};
        timeline.pickups[2]=Pickup {step:40,id:0,effect:Effect {kind:3,ticks:30},..Default::default()};
        timeline.count=3;
        for self_collisions in [false,true] {for defeated in [0,1,2,3] {
            w.config.self_collisions=self_collisions;timeline.defeated=defeated;
            let snapshot=timeline.snapshot();
            for step in 0..=STEPS+20 {for id in 0..MAX_SNAKES {
                let mut expected=if self_collisions {u16::MAX} else {u16::MAX^(1<<id)};
                for other in 0..MAX_SNAKES {if snapshot.at(other,step).is(EffectKind::Phase) {expected&=!(1<<other);}}
                if step>0 {expected&=!defeated;}
                if snapshot.at(id,step).is(EffectKind::Phase) {expected=0;}
                assert_eq!(snapshot.mask(&w,id,step),expected,"self={self_collisions} defeated={defeated} id={id} step={step}");
            }}
        }}
    }
    #[test]
    fn snapshots_and_timeline_keep_separate_death_and_consumption_deadlines() {
        let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,density:0.0,
            width:4000.0,height:2000.0,self_collisions:false,..Default::default()},
            &[(Point{x:1500.0,y:1000.0},0.0,40,0.0),(Point{x:800.0,y:1000.0},0.0,20,0.0)],&[]).unwrap();
        let mut t=Timeline::new(&w);
        t.activation(0,EffectKind::Venom,1);t.consumed_at[0]=4;
        t.activation(0,EffectKind::Surge,6);
        t.activation(1,EffectKind::Phase,1);t.activation(1,EffectKind::Magnet,3);
        t.defeat_after(1,7);t.defeat_after(2,50);
        let snapshot=t.snapshot();
        for step in 0..=STEPS+20 {
            let expected=(if step==1 || step==2 || step>7 {0} else {2})|(if step>50 {0} else {4});
            assert_eq!(snapshot.mask(&w,0,step)&6,expected,"snapshot step={step}");
            assert_eq!(t.mask(&w,0,step)&6,expected,"timeline step={step}");
            if step>0 {assert_eq!(snapshot.at(0,step),t.at(0,step));}
            assert_eq!(t.track(0).before(step+1),t.before(0,step+1));
        }
        // The observed Phase must survive a movement-one replacement when a
        // candidate subsequently backs off to its original endpoint.
        w.snakes[1].effect_kind=3;w.snakes[1].effect_ticks=20;
        let mut t=Timeline::new(&w);
        t.activation(1,EffectKind::Magnet,1);
        assert_eq!(t.snapshot().mask(&w,0,0)&2,0);
        assert_eq!(t.snapshot().mask(&w,0,1)&2,2);
    }

    use crate::{Config,RuleSet,Item};
    use crate::controller::ScriptedController;

    #[test]
    fn remote_nova_victim_rollouts_match_world_step() {
        for burst in [false,true] {for kind in [EffectKind::None,EffectKind::Surge,EffectKind::Phase] {
            let mut w=World::diagnostic_arena(Config {store_power_ups:false,rules:RuleSet::V2,density:0.0,
                speed:0.0,scale:200.0,width:10000.0,height:4000.0,self_collisions:false,
                deadly_walls:false,..Default::default()},
                &[(Point{x:6000.0,y:2000.0},0.0,24,0.0),(Point{x:6000.0,y:2270.0},0.0,800,0.0)],&[]).unwrap();
            w.snakes[1].effect_kind=kind as u8;w.snakes[1].effect_ticks=if kind==EffectKind::None {0} else {8};
            w.items.push(Item {id:42,kind:EffectKind::Frost,position:w.segments[0].current,
                radius:2.0,life_ticks:750,..Default::default()});
            struct Observe {ai:AiController,c:Candidate,shared:Candidate,observed:Option<World>,burst:bool}
            impl Controller for Observe {
                fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
                    if self.observed.is_none() {
                        self.observed=Some(w.diagnostic_snapshot());self.ai.prepare(w);
                        let victim=w.snake(1).unwrap();
                        let motion=if self.burst {self.ai.boosted_motion[1]} else {self.ai.motion[1]};
                        assert!(w.distance_squared(victim.segments[0].current,w.items[0].position).sqrt()
                            >motion.max_speed*STEPS as f64*STEP_SECONDS+1.3*victim.radius+w.items[0].radius,
                            "fixture must exclude capsule contact");
                        assert!(self.ai.effects.freeze_step(1,2).is_some(),"shared Nova must hit victim");
                        self.ai.rollout_into(w,victim,State {desired:0.6,rush:if self.burst {0.6} else {0.0},
                            turn_until:u64::MAX,..Default::default()},1,STEPS,&mut self.c);
                        assert_eq!(self.c.steps,STEPS);
                        // Also exercise the branch that reuses a shared Nova
                        // timeline without handling any candidate pickups.
                        let mut rivals=self.ai.simulation_rivals.take().unwrap();
                        for r in rivals.iter_mut() {r.dynamic=false;}
                        self.ai.rollout_simulation::<false,true>(w,victim,State {desired:0.6,
                            rush:if self.burst {0.6} else {0.0},turn_until:u64::MAX,..Default::default()},
                            1,STEPS,&mut self.shared,&mut rivals);
                        self.ai.simulation_rivals=Some(rivals);
                        assert_eq!(self.shared.steps,STEPS);
                    }
                    Steering {desired_angle:if s.id==1 {0.6} else {s.angle},rush:if s.id==1 && w.tick()==0 && self.burst {0.6} else {0.0}}
                }
            }
            let mut c=Observe {ai:AiController::new(),c:Candidate::default(),shared:Candidate::default(),observed:None,burst};
            for step in 1..=STEPS {
                w.step(&mut c);
                assert!(w.snakes[1].alive);
                assert_eq!(c.c.body_len,w.snakes[1].len,"Nova must retain unpaid burst segments");
                assert_eq!(c.shared.body_len,w.snakes[1].len);
                if step==1 {assert_eq!(w.snakes[1].frozen_ticks,75);assert_eq!(w.snakes[1].boost_ticks,0);}
                assert!(w.distance_squared(c.c.path[step],w.segments[MAX_SEGMENTS].current)<1e-12,
                    "burst={burst} kind={kind:?} step={step} predicted={:?} actual={:?}",c.c.path[step],w.segments[MAX_SEGMENTS].current);
                assert!(w.distance_squared(c.shared.path[step],w.segments[MAX_SEGMENTS].current)<1e-12,
                    "shared branch burst={burst} kind={kind:?} step={step}");
            }
        }}
    }

    #[test]
    fn nova_keeps_existing_tail_and_spatial_occupancy_through_world_step() {
        let mut w=World::diagnostic_arena(Config {store_power_ups:false,rules:RuleSet::V2,density:0.0,
            speed:100.0,scale:200.0,width:4000.0,height:2000.0,self_collisions:false,
            deadly_walls:false,..Default::default()},
            &[(Point{x:2000.0,y:1100.0},0.0,1,0.0),(Point{x:2000.0,y:1000.0},0.0,64,0.0)],&[]).unwrap();
        w.snakes[1].traits.speed_bias=1.16;
        let crossing=w.segments[MAX_SEGMENTS+43].current;
        w.items.push(Item {id:42,kind:EffectKind::Frost,position:w.segments[0].current,
            radius:2.0,life_ticks:750,..Default::default()});
        struct Observe {ai:AiController,source:Option<World>}
        impl Controller for Observe {
            fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
                if self.source.is_none() {self.source=Some(w.diagnostic_snapshot());self.ai.prepare(w);}
                Steering {desired_angle:s.angle,rush:0.0}
            }
        }
        let mut c=Observe {ai:AiController::new(),source:None};
        for _ in 0..70 {w.step(&mut c);}
        assert_eq!(w.snakes[1].frozen_ticks,6);
        assert!(w.segments[MAX_SEGMENTS+63].current.x<crossing.x,"real tail has not cleared crossing: tail={:?} crossing={:?} head={:?} len={} radius={}",w.segments[MAX_SEGMENTS+63].current,crossing,w.segments[MAX_SEGMENTS].current,w.snakes[1].len,w.snakes[1].radius);
        let source=c.source.as_ref().unwrap();let time=70.0*STEP_SECONDS;let mut tests=0;
        let ordinary=c.ai.initial_effects;let frozen=c.ai.effects;
        assert!(!c.ai.cached_body_blocked_phase(source,source.snake(0).unwrap(),crossing,crossing,time,0.0,&mut tests,false,&ordinary).0);
        assert!(c.ai.body_blocked(source,source.snake(0).unwrap(),crossing,crossing,time,0.0,&mut tests).0,
            "predicted freeze must retain the old trail");
        assert!(c.ai.cached_body_blocked_phase(source,source.snake(0).unwrap(),crossing,crossing,time,0.0,&mut tests,false,&frozen).0,
            "cached ordinary release must not certify a frozen crossing");
        assert!(!c.ai.cached_body_blocked_phase(source,source.snake(0).unwrap(),crossing,crossing,time,0.0,&mut tests,false,&ordinary).0);
        let mask=1<<1;
        assert!(!c.ai.spatial.physical_blocked(c.ai.spatial.key(crossing),mask,time));
        let clock=c.ai.effects.snapshot().space_time(mask,time,&c.ai.rivals);
        assert!(c.ai.spatial.physical_blocked(c.ai.spatial.key(crossing),mask,clock),
            "spatial estimates must retain the same trail");
    }

    #[test]
    fn skipped_pickups_invalidate_shared_motion_through_world_step() {
        for initial in [EffectKind::None,EffectKind::Surge] {
            for kind in [EffectKind::Frost,EffectKind::Surge,EffectKind::Phase,EffectKind::Magnet,EffectKind::Venom] {
                let mut w=World::diagnostic_arena(Config {store_power_ups:false,rules:RuleSet::V2,density:0.0,
                    width:4000.0,height:2000.0,self_collisions:false,..Default::default()},
                    &[(Point{x:1500.0,y:1000.0},0.0,24,0.0),(Point{x:1500.0,y:1050.0},0.0,24,0.0)],&[]).unwrap();
                w.snakes[0].effect_kind=initial as u8;w.snakes[0].effect_ticks=initial.duration();
                let travel=w.motion_limits(0,0.0).unwrap().0*STEP_SECONDS;
                let item=Item {id:42,kind,position:Point{x:1500.0+travel+1.3*w.snakes[0].radius+2.0-0.001,y:1000.0},
                    radius:2.0,life_ticks:750,..Default::default()};
                w.items.push(item);
                // Shared straight endpoint really picks up in production order.
                let mut shared=World::diagnostic_arena(w.config(),
                    &[(Point{x:1500.0,y:1000.0},0.0,24,0.0),(Point{x:1500.0,y:1050.0},0.0,24,0.0)],&[]).unwrap();
                shared.snakes[0].effect_kind=initial as u8;shared.snakes[0].effect_ticks=initial.duration();shared.items.push(item);
                shared.step(&mut ScriptedController::new(|_,s:SnakeView<'_>|Steering {desired_angle:s.angle,rush:0.0}));
                assert!(shared.items.is_empty(),"kind={kind:?}");
                let initial_timeline=Timeline::new(&w);
                let mut forecast=Forecast::new(&w,initial_timeline);
                forecast.advance(&w,1,|id|shared.segments[id*MAX_SEGMENTS].current,0.0);
                let mut items=Items::default();items.items[0]=item;items.pending=1;items.alive=3;
                items.radii[0]=w.snakes[0].radius;items.radii[1]=w.snakes[1].radius;
                items.owners.fill([3,0,0,0]);items.hits[1][0]=1;
                items.shared(&forecast);
                let affected=forecast.effects.movement_bits();
                let mut candidate=Forecast::cached(&w,initial_timeline,&items,4,0);
                w.step(&mut ScriptedController::new(|_,s:SnakeView<'_>|Steering {
                    desired_angle:if s.id==0 {std::f64::consts::PI} else {s.angle},rush:0.0}));
                assert_eq!(w.items.len(),1,"candidate must skip kind={kind:?}");
                candidate.advance_cached(&w,1,|id|w.segments[id*MAX_SEGMENTS].current,1,&items,0.0);
                assert_eq!(candidate.movement_changed()&affected,affected,"initial={initial:?} kind={kind:?}");
                // The same capsule is collected one tick later, after the
                // shared Nova's half-speed row has already become invalid.
                w.step(&mut ScriptedController::new(|_,s:SnakeView<'_>|Steering {desired_angle:s.angle,rush:0.0}));
                assert!(w.items.is_empty(),"delayed kind={kind:?}");
                candidate.advance_cached(&w,2,|id|w.segments[id*MAX_SEGMENTS].current,3,&items,0.0);
                assert_eq!(candidate.pickup_steps[0],2);
                assert_eq!(candidate.pickup_owners[0],0);
                if kind==EffectKind::Frost {assert_eq!(candidate.effects.frozen_at(1,2),w.snakes[1].frozen_ticks);}
            }
        }
    }

    #[test]
    fn thaw_immunity_matches_forecast_and_real_pickups_through_world_step() {
        struct CheckThaw {initial:Option<Timeline>,step:usize}
        impl Controller for CheckThaw {
            fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
                if s.id==1 {
                    let initial=self.initial.get_or_insert_with(||Timeline::new(w));
                    self.step+=1;
                    assert_eq!(initial.frozen_at(1,self.step),s.face.frozen_ticks);
                    assert_eq!(initial.immunity_at(1,self.step),s.face.thaw_immunity_ticks);
                    if self.step==2 {assert_eq!(s.face.scared_ticks,36);}
                }
                Steering {desired_angle:s.angle,rush:0.0}
            }
        }
        let mut w=World::diagnostic_arena(Config {store_power_ups:false,rules:RuleSet::V2,density:0.0,
            width:4000.0,height:2000.0,self_collisions:false,..Default::default()},
            &[(Point{x:1500.0,y:1000.0},0.0,24,0.0),(Point{x:1500.0,y:1050.0},0.0,24,0.0)],&[]).unwrap();
        w.snakes[1].frozen_ticks=2;w.faces[1].frozen_ticks=2;
        let mut check=CheckThaw {initial:None,step:0};let mut thaws=0;
        for step in 1..=47 {
            w.items.push(Item {id:42+step,kind:EffectKind::Frost,position:w.segments[0].current,
                radius:2.1*w.config.base_radius(),life_ticks:750,..Default::default()});
            w.step(&mut check);
            assert!(w.items.is_empty());
            thaws+=w.frame_events().filter(|e|e.kind==crate::EventKind::EffectExpiry && e.other_snake_id==5).count();
            assert_eq!(w.snakes[1].frozen_ticks,if step==1 {1} else if step==47 {75} else {0},"step={step}");
        }
        assert_eq!(thaws,1);
    }
    #[test]
    fn later_pickups_preserve_freeze_cutoff_and_radius_through_world_step() {
        for (freeze,surge) in [(1,10),(10,1),(5,5)] {
            for kind in [EffectKind::Surge,EffectKind::Phase,EffectKind::Magnet,EffectKind::Venom] {
                let mut w=World::diagnostic_arena(Config {store_power_ups:false,rules:RuleSet::V2,density:0.0,
                    width:8000.0,height:4000.0,self_collisions:false,deadly_walls:false,..Default::default()},
                    &[(Point{x:3000.0,y:2000.0},0.0,24,0.0),(Point{x:3000.0,y:2050.0},0.0,400,0.0)],&[]).unwrap();
                w.snakes[1].birth_len=390;w.snakes[1].radius=w.snakes[1].base_radius*1.025;
                let original=w.diagnostic_snapshot();
                let motion=Motion::forecast(&original,1,0.6);
                let mut forecast=Forecast::new(&original,Timeline::new(&original));
                for step in 1..=25 {
                    if step==freeze {w.items.push(Item {id:42,kind:EffectKind::Frost,
                        position:w.segments[0].current,radius:2.1*w.config.base_radius(),life_ticks:750,..Default::default()});}
                    if step==surge {w.items.push(Item {id:43,kind,position:w.segments[MAX_SEGMENTS].current,
                        radius:2.0,life_ticks:750,..Default::default()});}
                    w.step(&mut ScriptedController::new(|tick,s:SnakeView<'_>|Steering {desired_angle:s.angle,rush:if s.id==1 && tick==0 {0.6} else {0.0}}));
                    let positions=|id:usize|w.segments[id*MAX_SEGMENTS].current;
                    if step==freeze {
                        assert_eq!(w.snakes[1].frozen_ticks,75);
                        forecast.effects.pickup(0,Item {id:42,kind:EffectKind::Frost,..Default::default()},step,0);
                        forecast.effects.nova(&original,0,positions(0),step,3,positions);
                    }
                    if step==surge {
                        assert_eq!(w.snakes[1].effect_kind,kind as u8);
                        forecast.effects.pickup(1,Item {id:43,kind,..Default::default()},step,0);
                    }
                    if step!=freeze && step!=surge {
                        assert_eq!(forecast.motion(&original,1,0.6,&motion,step),w.motion_limits(1,0.0).unwrap(),
                            "motion freeze={freeze} pickup={surge} kind={kind:?} step={step}");
                    }
                    assert_eq!(forecast.radius(&original,1,0.6,&motion,step),w.snakes[1].radius,
                        "freeze={freeze} pickup={surge} kind={kind:?} step={step} len={}",w.snakes[1].len);
                }
            }
        }
    }
}

#[cfg(test)]
mod inventory_order_tests {
    use super::*;
    use crate::Config;
    #[test]
    fn capacity_queries_match_pickup_order_expiry_and_completed_use() {
        let mut w=World::diagnostic_arena(Config {density:0.0,store_power_ups:true,rules:crate::RuleSet::V2,..Default::default()},
            &[(Point{x:800.0,y:500.0},0.0,24,0.0)],&[]).unwrap();
        w.snakes[0].inventory=crate::Inventory {count:2,kinds:[1,2,0],life:[1800,1800,0],..Default::default()};
        let first=crate::Item {id:41,kind:EffectKind::Magnet,..Default::default()};
        let second=crate::Item {id:42,kind:EffectKind::Frost,..Default::default()};
        for step in [1,5] {
            let mut timeline=Timeline::new(&w);
            assert!(!timeline.pickup(0,first,step,0));
            assert!(timeline.stores_at(&w,0,step,0),"target must not count its own stash");
            assert!(timeline.stores_at(&w,0,step+1,0),"target identity survives an arrival estimate change");
            assert!(!timeline.stores_at(&w,0,step,1),"earlier same-step stash fills the last slot");
            assert!(timeline.pickup(0,second,step,1));
        }
        w.snakes[0].inventory.count=3;w.snakes[0].inventory.kinds[2]=3;w.snakes[0].inventory.life[2]=4;
        let mut timeline=Timeline::new(&w);
        assert!(!timeline.stores_at(&w,0,4,0));
        assert!(timeline.stores_at(&w,0,5,0));
        assert!(!timeline.pickup(0,first,5,0));
        assert!(!timeline.stores_at(&w,0,5,1));
        w.snakes[0].inventory.life[2]=1800;
        let mut timeline=Timeline::new(&w);timeline.schedule_use(0,EffectKind::Surge,5,0);
        assert!(!timeline.stores_at(&w,0,4,0));assert!(timeline.stores_at(&w,0,5,0));
        assert!(!timeline.pickup(0,first,5,0));
        assert!(!timeline.stores_at(&w,0,5,1));
        assert!(timeline.stores_at(&w,0,5,0),"completed proposed use must stay in query history");
        assert!(timeline.pickup(0,second,5,1),"use releases one slot, never one per capsule");
    }
}

#[cfg(test)]
#[path="whirlpool_timing_tests.rs"]
mod whirlpool_timing_tests;
