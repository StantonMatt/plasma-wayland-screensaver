// SPDX-License-Identifier: GPL-3.0-or-later
//! One effect clock for every forecast participant. Inputs are steering's
//! post-decrement observations. Movement uses before(); pickup runs in item
//! order with lowest live snake ID winning, then collisions use at().
//! Future spawns, food growth and unobserved rival controls are not predicted.
use super::*;
use crate::effects::EffectKind;
use std::cell::Cell;

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
pub(super) struct Track {initial:Effect,pickups:[Effect;crate::MAX_ITEMS],steps:[u16;crate::MAX_ITEMS],count:u8}
impl Track {
    #[cfg(test)]
    pub fn observed(w:&World,s:SnakeView<'_>)->Self {Self {initial:Effect::observed(w,s),..Self::default()}}
    pub fn before(&self,step:usize)->Effect {
        for i in (0..self.count as usize).rev() {
            let at=self.steps[i] as usize;
            if at<step {return self.pickups[i].after(step-at);}
        }
        self.initial.after(step.saturating_sub(1))
    }
}
#[derive(Clone, Copy, Default)]
struct Pickup {step:u16,id:u8,effect:Effect}
/// Candidates retain events, not two full per-step mask tables. The hot
/// rollout owns those masks; endpoint queries reconstruct their fourteen bits.
#[derive(Clone, Copy, Default)]
pub(super) struct Snapshot {
    initial:[Effect;MAX_SNAKES],pickups:[Pickup;crate::MAX_ITEMS],count:usize,
}
impl Snapshot {
    pub fn at(&self,id:usize,step:usize)->Effect {
        let step=step.max(1);
        for p in self.pickups[..self.count].iter().rev() {
            if p.id as usize==id && (p.step as usize)<=step {return p.effect.after(step-p.step as usize);}
        }
        self.initial[id].after(step-1)
    }
    pub fn mask(&self,w:&World,id:usize,step:usize)->u16 {
        if self.at(id,step).is(EffectKind::Phase) {return 0;}
        let mut mask=if w.config().self_collisions {u16::MAX} else {u16::MAX^(1<<id)};
        for other in 0..MAX_SNAKES {if self.at(other,step).is(EffectKind::Phase) {mask&=!(1<<other);}}
        mask
    }
}
#[derive(Clone, Copy)]
pub(super) struct Timeline {
    initial:[Effect;MAX_SNAKES],
    pickups:[Pickup;crate::MAX_ITEMS],
    count:usize,
    pickup_mask:u16,
    movement_mask:u16,
    phase:[u16;STEPS+1],
    surge:[u16;STEPS+1],
}
impl Default for Timeline {
    fn default()->Self {Self {initial:[Effect::default();MAX_SNAKES],pickups:[Pickup::default();crate::MAX_ITEMS],count:0,pickup_mask:0,movement_mask:0,phase:[0;STEPS+1],surge:[0;STEPS+1]}}
}
impl Timeline {
    pub fn new(w:&World)->Self {
        let mut result=Self::default();
        for s in w.snakes().filter(|s|s.alive) {
            let id=s.id as usize;let e=Effect::observed(w,s);result.initial[id]=e;
            let masks=if e.is(EffectKind::Phase) {&mut result.phase}
                else if e.is(EffectKind::Surge) {&mut result.surge} else {continue;};
            for mask in &mut masks[1..=e.ticks.min(STEPS as u16) as usize] {*mask|=1<<id;}
        }
        result.phase[0]=result.phase[1];result.surge[0]=result.surge[1];result
    }
    /// Strategy values observed effects and unavoidable movement-one pickups.
    /// Later predicted rival pickups are possible outcomes, not grounds to
    /// abandon an opportunity. Physical rollouts still use their exact timeline.
    pub fn opportunities(w:&World,initial:Self,motions:&[Motion;MAX_SNAKES],boosted:&[Motion;MAX_SNAKES])->Self {
        let mut result=initial;
        if w.config().rules!=crate::RuleSet::V2 || !w.config().power_ups {return result;}
        for item in w.items().filter(|item|item.life_ticks>=1) {
            for s in w.snakes().filter(|s|s.alive) {
                let id=s.id as usize;let head=s.segments[0].current;
                let mut possible=false;let mut certain=true;
                for motion in [&motions[id],&boosted[id]] {
                    let (speed,turn)=motion.at(0);let travel=speed*STEP_SECONDS;
                    let endpoint=w.canonical_point(Point{x:head.x+s.angle.cos()*travel,y:head.y+s.angle.sin()*travel});
                    let distance=w.distance_squared(endpoint,item.position).sqrt();
                    let error=2.0*travel*(turn*STEP_SECONDS*0.5).min(std::f64::consts::FRAC_PI_2).sin();
                    let reach=1.3*motion.radii[0]+item.radius;
                    possible|=distance<=reach+error;
                    certain&=distance+error<=reach;
                }
                if possible {
                    if certain {result.pickup(id,item.kind,1);}
                    // A lower live ID that might win prevents certainty for
                    // every later ID, even if that later endpoint is inside.
                    break;
                }
            }
        }
        result
    }
    pub fn observed_before(&self,id:usize,step:usize)->Effect {self.initial[id].after(step.saturating_sub(1))}
    pub fn snapshot(&self)->Snapshot {Snapshot {initial:self.initial,pickups:self.pickups,count:self.count}}
    pub fn track(&self,id:usize)->Track {
        let mut result=Track {initial:self.initial[id],..Track::default()};
        for pickup in &self.pickups[..self.count] {
            if pickup.id as usize==id {let i=result.count as usize;result.pickups[i]=pickup.effect;result.steps[i]=pickup.step as u16;result.count+=1;}
        }
        result
    }
    pub fn may_have(&self,id:usize,kind:EffectKind)->bool {
        self.initial[id].is(kind) || self.pickups[..self.count].iter().any(|p|p.id as usize==id && p.effect.is(kind))
    }
    pub fn movement_bits(&self)->u16 {self.movement_mask}
    pub fn movement_event(&self,step:usize)->bool {
        self.pickups[..self.count].iter().any(|p|p.step as usize==step && self.movement_mask&(1<<p.id)!=0)
    }
    pub fn has_pickup(&self,id:usize)->bool {self.pickup_mask&(1<<id)!=0}
    fn surge_pickup_before(&self,id:usize,step:usize)->Option<usize> {
        // The first Surge already forgives the entire outstanding payment;
        // later replacements/refreshes cannot restore that debt.
        self.pickups[..self.count].iter().find(|p|p.id as usize==id && (p.step as usize)<step && p.effect.is(EffectKind::Surge)).map(|p|p.step as usize)
    }
    pub fn same_movement(&self,other:&Self,id:usize,step:usize)->bool {
        let a=self.before(id,step);let b=other.before(id,step);
        crate::effects::modifiers(a.kind,a.ticks).speed==crate::effects::modifiers(b.kind,b.ticks).speed
            && self.surge_pickup_before(id,step)==other.surge_pickup_before(id,step)
    }
    #[inline]
    pub fn at(&self,id:usize,step:usize)->Effect {
        let step=step.max(1);
        for pickup in self.pickups[..self.count].iter().rev() {
            if pickup.id as usize==id && (pickup.step as usize)<=step {return pickup.effect.after(step-pickup.step as usize);}
        }
        self.initial[id].after(step-1)
    }
    /// Effect used for movement/feed, before this step's endpoint pickups.
    #[inline]
    pub fn before(&self,id:usize,step:usize)->Effect {
        if step<=1 {self.initial[id]} else {self.at(id,step-1).after(1)}
    }
    #[inline]
    pub fn phased(&self,id:usize,step:usize)->bool {
        if step<=STEPS {self.phase[step.max(1)]&(1<<id)!=0}
        else {self.at(id,step).is(EffectKind::Phase)}
    }
    #[inline]
    pub fn contact(&self,a:usize,b:usize,step:usize)->bool {!self.phased(a,step) && !self.phased(b,step)}
    #[inline]
    pub fn phase_bits(&self,step:usize)->u16 {
        if step<=STEPS {self.phase[step.max(1)]} else {self.kind_bits(EffectKind::Phase,step)}
    }
    #[inline]
    pub fn reach_scale(&self,id:usize,step:usize)->f64 {
        let active=if step<=STEPS {self.surge[step.max(1)]&(1<<id)!=0} else {self.at(id,step).is(EffectKind::Surge)};
        if active {1.6} else {1.0}
    }
    pub fn surge_bits(&self,step:usize)->u16 {
        if step<=STEPS {self.surge[step.max(1)]} else {self.kind_bits(EffectKind::Surge,step)}
    }
    fn kind_bits(&self,kind:EffectKind,step:usize)->u16 {
        let mut result=0;
        for id in 0..MAX_SNAKES {if self.at(id,step).is(kind) {result|=1<<id;}}
        result
    }
    pub fn max_reach_scale(&self,step:usize)->f64 {if self.surge_bits(step)!=0 {1.6} else {1.0}}
    fn pickup(&mut self,id:usize,kind:EffectKind,step:usize) {
        let effect=Effect {kind:kind as u8,ticks:kind.duration()};
        self.pickups[self.count]=Pickup {id:id as u8,effect,step:step as u16};self.count+=1;self.pickup_mask|=1<<id;
        let initial=self.initial[id];
        if kind==EffectKind::Surge || crate::effects::modifiers(initial.kind,initial.ticks).speed!=1.0 {self.movement_mask|=1<<id;}
        for j in step..=STEPS {
            self.phase[j]&=!(1<<id);self.surge[j]&=!(1<<id);
            if j-step<effect.ticks as usize {
                if kind==EffectKind::Phase {self.phase[j]|=1<<id;}
                if kind==EffectKind::Surge {self.surge[j]|=1<<id;}
            }
        }
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
}
impl Default for Items {
    fn default()->Self {Self {reach:[[[u16::MAX;MAX_SNAKES];crate::MAX_ITEMS];2],earliest:[[u16::MAX;MAX_SNAKES];2],owners:[[0;crate::MAX_ITEMS];STEPS+1],hits:[[0;crate::MAX_ITEMS];STEPS+1],near:[[0;crate::MAX_ITEMS];STEPS+1],items:[crate::Item::default();crate::MAX_ITEMS],pending:0,alive:0,radii:[0.0;MAX_SNAKES],pickup_steps:[u16::MAX;crate::MAX_ITEMS],pickup_owners:[MAX_SNAKES as u8;crate::MAX_ITEMS],movement_mask:0}}
}
impl Items {
    pub fn shared(&mut self,forecast:&Forecast) {
        self.pickup_steps=forecast.pickup_steps;self.pickup_owners=forecast.pickup_owners;
        self.movement_mask=forecast.effects.movement_bits();
    }
    pub fn reachable(&self,id:usize,horizon:usize,boosted:bool)->bool {self.earliest[usize::from(boosted)][id] as usize<=horizon}
    pub fn prepare(w:&World,motions:&[Motion;MAX_SNAKES],boosted:&[Motion;MAX_SNAKES])->Self {
        let mut result=Self::default();
        if w.config().rules!=crate::RuleSet::V2 || !w.config().power_ups || w.items().next().is_none() {return result;}
        let scale=if w.items().any(|i|i.kind==EffectKind::Surge) {
            crate::effects::modifiers(EffectKind::Surge as u8,1).speed
        } else {1.0};
        for s in w.snakes().filter(|s|s.alive) {result.alive|=1<<s.id;result.radii[s.id as usize]=s.radius;}
        for (i,item) in w.items().enumerate() {
            result.items[i]=*item;result.pending|=1<<i;
            for s in w.snakes().filter(|s|s.alive) {
                let id=s.id as usize;
                let distance=w.distance_squared(s.segments[0].current,item.position).sqrt();
                let travel=(distance-1.3*s.radius-item.radius).max(0.0);
                for (mode,motion) in [&motions[id],&boosted[id]].into_iter().enumerate() {
                    // Round down: pruning may retain an extra tick, never drop a reachable pickup.
                    result.reach[mode][i][id]=(travel/(motion.max_speed*scale*STEP_SECONDS)).floor().clamp(1.0,u16::MAX as f64) as u16;
                    result.earliest[mode][id]=result.earliest[mode][id].min(result.reach[mode][i][id]);
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
    pub fn contests(&mut self,w:&World,rivals:&[Rival;MAX_SNAKES],motions:&[Motion;MAX_SNAKES],effects:&Timeline) {
        for (i,item) in w.items().enumerate() {
            for (id,r) in rivals.iter().enumerate().filter(|(_,r)|r.alive) {
                if self.reach[0][i][id] as usize>STEPS {continue;}
                let surge=effects.surge_pickup_before(id,STEPS+1);
                let pickup_radius=surge.map(|step|w.ai_forecast_radius(id,0.0,step,Some(step)));
                for step in 1..=STEPS.min(item.life_ticks as usize) {
                    let radius=if surge.is_some_and(|s|s<step) {pickup_radius.unwrap()} else {motions[id].radii[(step-1).min(24)]};
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
struct MotionValue {key:u16,surge:usize,rush:f64,limits:(f64,f64)}
impl MotionValue {const EMPTY:Self=Self {key:u16::MAX,surge:usize::MAX,rush:0.0,limits:(0.0,0.0)};}

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
    pub fn movement_changed(&self)->u16 {self.movement_changed}
    pub fn reachable_surge(&self,id:usize)->bool {
        self.items.iter().enumerate().any(|(i,item)|self.pending&(1<<i)!=0 && self.owners[i]&(1<<id)!=0 && item.kind==EffectKind::Surge)
    }
    /// Cached ordinary rival contests; changed participants remain exact.
    #[inline]
    pub fn advance_cached(&mut self,w:&World,step:usize,positions:impl Fn(usize)->Point,changed:u16,items:&Items,reserve:f64) {
        self.sweep_needed=false;
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
            let owners=self.alive & self.owners[i];
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
                self.movement_changed|=(1<<expected)&items.movement_mask;
            }
            if winner!=MAX_SNAKES {
                self.effects.pickup(winner,item.kind,step);
                self.pickup_steps[i]=step as u16;self.pickup_owners[i]=winner as u8;
                if winner!=expected || step!=expected_step {
                    let affected=(1<<winner) | if expected<MAX_SNAKES {1<<expected} else {0};
                    self.movement_changed|=affected&(items.movement_mask|self.effects.movement_bits());
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
                if w.distance_squared(w.snake(id).unwrap().segments[0].current,item.position)<=reach*reach {owners|=1<<id;}
            }
            self.owners[i]=owners;
            if owners==0 {self.pending&=!(1<<i);}
        }
    }
    pub fn has_items(&self)->bool {self.pending!=0}
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
            let mut alive=self.alive & self.owners[i];
            while alive!=0 {
                let id=alive.trailing_zeros() as usize;alive&=alive-1;
                let reach=1.3*self.radii[id]+item.radius;
                let distance=w.distance_squared(positions(id),item.position);
                if item.kind==EffectKind::Phase && reserve>0.0 && distance<=(reach+reserve).powi(2) {self.sweep_needed=true;}
                if distance>reach*reach {continue;}
                self.effects.pickup(id,item.kind,step);self.pickup_steps[i]=step as u16;self.pickup_owners[i]=id as u8;self.pending&=!(1<<i);break;
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
            if cached.0==surge {return cached.1;}
            let radius=w.ai_forecast_radius(id,rush,step-1,Some(surge));
            self.radius_cache[id].set((surge,radius));radius
        }
        else {motion.radii[(step-1).min(24)]}
    }
    #[inline]
    pub fn motion(&self,w:&World,id:usize,rush:f64,motion:&Motion,step:usize)->(f64,f64) {
        if !self.effects.has_pickup(id) {return motion.at(step-1);}
        self.motion_after_pickup(w,id,rush,motion,step)
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
        if surge.is_none() && crate::effects::modifiers(effect.kind,effect.ticks).speed==crate::effects::modifiers(initial.kind,initial.ticks).speed {motion.at(step-1)}
        else {
            // After the single burst ends, only effect speed transitions can
            // change motion. Counter decrements alone do not change limits.
            let key=(step-1).min(24) as u16 | ((if effect.ticks>0 {effect.kind} else {0}) as u16)<<8;
            let surge_step=surge.unwrap_or(usize::MAX);
            let cached=self.motion_cache[id].get();
            if cached.key==key && cached.surge==surge_step && cached.rush==rush {return cached.limits;}
            let limits=w.ai_forecast_motion(id,rush,step-1,effect.kind,effect.ticks,surge);
            self.motion_cache[id].set(MotionValue {key,surge:surge_step,rush,limits});limits
        }
    }
}
