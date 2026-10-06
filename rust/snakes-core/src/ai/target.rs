// SPDX-License-Identifier: GPL-3.0-or-later
//! Steering ends at World::feed_snakes' capture disk or pickup_items' contact
//! disk. Captured food is pulled/consumed by World without further pursuit;
//! Spark, Shard (including death fields), Pellet and Prism share that rule.
use super::aggression;
use crate::{effects, FoodView, Point, SnakeView, World};

pub(super) const ITEM_BIT: u64 = 1 << 63;

/// Hot target cache: render/future payloads belong to the world/ABI records.
/// Keep only contact and valuation inputs copied through AI rollouts.
#[derive(Clone,Copy,Debug)]
pub(super) struct TargetFood {
    pub id:u64,pub position:Point,pub value:f64,pub size:f64,
    pub vacuum_owner:i32,pub feast_id:u64,pub kind:crate::FoodKind,pub motion_ticks:u16,
}
impl From<FoodView> for TargetFood {
    fn from(f:FoodView)->Self {
        Self {id:f.id,position:f.position,value:f.value,size:f.size,vacuum_owner:f.vacuum_owner,
            feast_id:f.feast_id,kind:f.kind,motion_ticks:f.motion_ticks}
    }
}
const _:()=assert!(std::mem::size_of::<Option<TargetFood>>()<=64);


/// Cache outside rollout loops: no target searches or effect dispatch per step.
#[derive(Clone, Copy)]
pub(super) struct Contact {
    kind:crate::FoodKind,
    zone:bool,
    ordinary: f64,
    magnet: f64,
    effects: super::forecast::Track,
    claimed: bool,
    available: bool,
    ready_step:usize,
    item:bool,
    alive:bool,
    guarding:bool,
}
impl Contact {
    #[cfg(test)]
    #[inline]
    pub(super) fn new(w: &World, s: SnakeView<'_>, f: impl Into<TargetFood>) -> Self {
        let mut f:TargetFood=f.into();
        if f.kind==crate::FoodKind::PrismSeed {
            if let Some(raw)=w.food.iter().find(|raw|raw.id==f.id) {f.motion_ticks=raw.ripe_tick.saturating_sub(w.tick()).min(u16::MAX as u64) as u16;}
        }
        Self::forecast(s,f,super::forecast::Track::observed(w,s))
    }
    pub(super) fn forecast(s:SnakeView<'_>,f:impl Into<TargetFood>,track:super::forecast::Track)->Self {
        let f=f.into();
        let item = f.id & ITEM_BIT != 0;
        let ordinary = s.radius * if item { 1.3 } else { 3.0 } + f.size;
        let magnet = if !item {s.radius * effects::modifiers(effects::EffectKind::Magnet as u8,1).food_reach + f.size} else {ordinary};
        Self { kind:f.kind,zone:f.id==super::events::ID,ordinary, magnet, effects:track,item,alive:s.alive,guarding:s.face.guarding && !track.store,ready_step:if item || f.kind==crate::FoodKind::PrismSeed {f.motion_ticks as usize} else {0},
            claimed: !item && f.vacuum_owner == s.id as i32,
            available: f.kind!=crate::FoodKind::Meteor && (item || f.vacuum_owner < 0 || f.vacuum_owner == s.id as i32) }
    }
    pub(super) fn with_guard(mut self,guarding:bool)->Self {self.guarding=guarding && !self.effects.store;self}
    #[inline]
    pub(super) fn reach(self, step: usize) -> f64 {
        self.reach_with(self.effects.before(step))
    }
    #[inline]
    pub(super) fn reach_with(self,effect:super::forecast::Effect)->f64 {
        if effect.is(effects::EffectKind::Magnet) {self.magnet} else {self.ordinary}
    }
    pub(super) fn reached_with(self,distance_squared:f64,effect:super::forecast::Effect,step:usize)->bool {
        !self.zone && self.alive && crate::world::events::food_pickup_eligible(self.kind,step as u64,self.ready_step as u64,0) && self.available
            && (!self.item || crate::Item::pickup_allowed(self.alive,self.guarding,effect.ticks))
            && (self.claimed || distance_squared<=self.reach_with(effect).powi(2))
    }
    pub(super) fn distance_with(self,distance:f64,effect:super::forecast::Effect)->f64 {
        if !self.available {f64::INFINITY} else if self.claimed {0.0} else {(distance-self.reach_with(effect)).max(0.0)}
    }
    /// Only an existing vacuum claim proves collection before movement.
    #[inline]
    pub(super) fn collected(self) -> bool { self.claimed || !self.available }
    #[inline]
    pub(super) fn reached(self, distance_squared: f64, step: usize) -> bool {
        self.reached_with(distance_squared,self.effects.before(step),step)
    }
    /// A pending overlap can finish steering only when the proposed straight
    /// movement still reaches the disk. The current position alone cannot.
    pub(super) fn ahead(self,w:&World,head:Point,angle:f64,travel:f64,center:Point)->bool {
        let next=w.canonical_point(Point{x:head.x+angle.cos()*travel,y:head.y+angle.sin()*travel});
        self.reached(w.distance_squared(next,center),1)
    }
    #[inline]
    pub(super) fn distance(self, distance: f64, step: usize) -> f64 {
        if !self.available { f64::INFINITY }
        else if self.claimed { 0.0 }
        else { (distance - self.reach(step)).max(0.0) }
    }
    /// Straight routing needs only reach the capture disk, not its center.
    pub(super) fn approach(self, w: &World, head: Point, center: Point, step: usize) -> Point {
        let d = w.displacement(head, center);
        let distance = (d.x*d.x + d.y*d.y).sqrt();
        let fraction = self.distance(distance, step) / distance.max(1.0);
        w.canonical_point(Point { x: head.x + d.x*fraction, y: head.y + d.y*fraction })
    }
}

impl super::AiController {
    /// A nearby rival must actually be approaching this capsule. Never invent
    /// a chase behind its tail or use body size to excuse a losing race.
    pub(super) fn capsule_contender(&self,w:&World,s:SnakeView<'_>,state:super::State)->Option<usize> {
        if s.traits.aggression<0.2 {return None;}
        let f=self.target_food(state)?;
        let eta=self.target_arrival(w,s,f);
        let mut best=f64::MAX;let mut contender=None;
        for (other,r) in self.rivals.iter().enumerate() {
            if other==s.id as usize || !r.alive || s.segments.len()<r.len+super::surge::advantage(w,s,4+2+s.segments.len()/100)
                || self.opportunities.phased(other,1)
                || w.distance_squared(s.segments[0].current,r.path[0])>480.0*480.0 {continue;}
            let d=w.displacement(r.path[0],f.position);
            let bearing=crate::normalize_angle(d.y.atan2(d.x)-r.angle).abs();
            let rival=w.snake(other).unwrap();
            let rival_eta=self.target_arrival(w,rival,f);
            let committed=self.states[other].generation==rival.generation && self.states[other].target==f.id;
            if !(committed || bearing<0.65) || rival_eta>=best {continue;}
            if eta>rival_eta {
                // Trailing denial needs a point at least three radii ahead of
                // the leader that we can reach before it. Cutoff rollouts then
                // certify the physical crossing, tail payments and safe exit.
                let path=w.displacement(r.path[0],f.position);
                let length=(path.x*path.x+path.y*path.y).sqrt();
                if ![0.35,0.6,0.85].into_iter().any(|q| {
                    let ahead=length*q;
                    let p=w.canonical_point(Point{x:r.path[0].x+path.x*q,y:r.path[0].y+path.y*q});
                    ahead>=3.0*r.radius && self.arrival(w,&self.rivals[s.id as usize],p)<ahead/r.speed
                }) {continue;}
            }
            best=rival_eta;contender=Some(other);
        }
        contender
    }
    /// Capsules are strategic objectives, valued separately from nutrition.
    /// Bounded inputs reuse the food grid and live rival rows; no new scans of
    /// bodies, scratch storage or RNG are needed.
    pub(super) fn capsule_value(&self,w:&World,s:SnakeView<'_>,item:&crate::Item,eta:f64)->f64 {
        use effects::EffectKind;
        if w.config().store_power_ups {
            if s.inventory.count>=3 {return 0.0;}
            // Storage preserves the active effect and makes every kind useful.
            let duplicates=s.inventory.kinds.iter().filter(|&&k|k==item.kind as u8).count();
            return (145.0-duplicates as f64*25.0)/(1.0+eta*0.03);
        }
        let id=s.id as usize;
        let density=self.spatial.cluster_weight(self.spatial.key(item.position)).min(12.0);
        let mut useful=0.0_f64;
        if item.kind==EffectKind::Venom {
            for r in w.snakes().filter(|r|r.alive && r.id!=s.id && r.segments.len()>7
                && r.face.bite_immunity_ticks==0 && r.flags&crate::flags::PHASED==0) {
                let len=r.segments.len();
                for q in [55,70,85] {
                    let index=(len*q/100).max(len/2+1).max(4).min(len-1);
                    let distance=w.distance_squared(item.position,r.segments[index].current).sqrt();
                    let travel=distance/self.motion[id].at(0).0;
                    if travel<5.0 {useful=useful.max(((len-index) as f64/(1.0+travel)).min(100.0));}
                }
            }
        }
        for (other,r) in self.rivals.iter().enumerate() {
            if other==id || !r.alive {continue;}
            let near=w.distance_squared(item.position,r.path[0])<(s.radius*55.0).powi(2);
            if !near {continue;}
            match item.kind {
                EffectKind::Surge if s.segments.len()>=r.len+4 => useful=useful.max(50.0),
                EffectKind::Phase if r.len>=s.segments.len()+6 => useful=useful.max(45.0),
                _=>{}
            }
        }
        if item.kind==EffectKind::Magnet {useful+=density*5.0;}
        if item.kind==EffectKind::Phase && s.flags & crate::flags::TRAPPED!=0 {useful+=100.0;}
        let life=item.life_ticks as f64*crate::STEP_SECONDS;
        let urgency=1.0+0.4*(eta/life.max(crate::STEP_SECONDS)).clamp(0.0,1.0);
        // Preserve a useful current effect until its window is nearly over.
        let held=super::forecast::Effect::observed(w,s);
        let replacement=if held.ticks as f64*crate::STEP_SECONDS>eta+1.0 {0.45} else {1.0};
        // An instant Nova has no inventory value. Race for it only when the
        // arrival forecast includes an eligible rival, and scale its offensive
        // value with the user's aggression setting.
        if item.kind==EffectKind::Frost {
            let victims=self.frost_value_at(w,s,item.position,eta)/8.0;
            return victims*(35.0+145.0*aggression::level(w))*urgency*replacement;
        }
        let base=if item.kind==EffectKind::Venom && useful==0.0 {25.0} else {110.0};
        (base+useful+effects::ai_bonus(item.kind,w,s,item.position))*urgency*replacement
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ai::{AiController,State}, controller::Controller, Config, Item, RuleSet};
    fn arena()->World {
        let mut w=World::diagnostic_arena(Config {store_power_ups:false,width:1600.0,height:1000.0,
            density:0.0,intelligence:100.0,rules:RuleSet::V2,self_collisions:true,
            deadly_walls:true,..Config::default()},
            &[(Point{x:400.0,y:400.0},0.0,48,0.9)],&[Point{x:550.0,y:400.0}]).unwrap();
        w.items.clear();
        w
    }
    fn capsule(w:&mut World,p:Point,life:u16) {
        w.items.push(Item {id:77,kind:effects::EffectKind::Phase,position:p,
            radius:12.0,life_ticks:life,..Item::default()});
    }
    #[test]
    fn capsule_is_a_prime_objective_and_can_require_a_deliberate_turn() {
        for p in [Point{x:1000.0,y:400.0},Point{x:250.0,y:650.0}] {
            let mut w=arena();capsule(&mut w,p,750);
            let mut ai=AiController::new();ai.prepare(&w);
            let mut state=State::default();ai.strategy(&w,w.snake(0).unwrap(),&mut state,1.0);
            assert_eq!(state.target,77|ITEM_BIT,"position={p:?}");
            assert_eq!(state.prey,0);
        }
    }
    #[test]
    fn pursuit_keeps_commitment_but_rejects_an_expiring_unreachable_capsule() {
        let mut w=arena();capsule(&mut w,Point{x:900.0,y:400.0},750);
        let mut ai=AiController::new();ai.prepare(&w);
        let mut state=State::default();ai.strategy(&w,w.snake(0).unwrap(),&mut state,1.0);
        assert_eq!(state.target,77|ITEM_BIT);
        // A new nutritional temptation must not erase the committed capsule.
        w.food[0].value=4.0;ai.tick=u64::MAX;ai.prepare(&w);
        ai.strategy(&w,w.snake(0).unwrap(),&mut state,1.0);
        assert_eq!(state.target,77|ITEM_BIT);
        w.items[0].life_ticks=1;ai.tick=u64::MAX;ai.prepare(&w);
        ai.strategy(&w,w.snake(0).unwrap(),&mut state,1.0);
        assert_ne!(state.target,77|ITEM_BIT);
    }
    #[test]
    fn capsules_reward_effect_context_and_preserve_a_current_window() {
        let mut w=arena();capsule(&mut w,Point{x:900.0,y:400.0},750);
        let mut ai=AiController::new();ai.prepare(&w);
        let item=w.items[0];let ordinary=ai.capsule_value(&w,w.snake(0).unwrap(),&item,2.0);
        w.snakes[0].intent_flags|=crate::flags::TRAPPED;
        assert!(ai.capsule_value(&w,w.snake(0).unwrap(),&item,2.0)>ordinary);
        w.snakes[0].intent_flags=0;
        w.snakes[0].effect_kind=effects::EffectKind::Magnet as u8;w.snakes[0].effect_ticks=300;
        assert!(ai.capsule_value(&w,w.snake(0).unwrap(),&item,2.0)<ordinary);
        w.snakes[0].effect_ticks=1;
        assert_eq!(ai.capsule_value(&w,w.snake(0).unwrap(),&item,2.0),ordinary);
    }
    #[test]
    fn capsule_race_compares_turn_aware_arrivals_and_rejects_a_losing_rush() {
        let mut w=World::diagnostic_arena(Config {store_power_ups:false,width:1600.0,height:1000.0,density:0.0,
            rules:RuleSet::V2,self_collisions:true,..Config::default()},&[
            (Point{x:710.0,y:400.0},0.0,24,0.9),
            (Point{x:710.0,y:425.0},0.0,24,0.9)],&[]).unwrap();
        capsule(&mut w,Point{x:800.0,y:400.0},750);
        let mut ai=AiController::new();ai.prepare(&w);
        let state=State {target:77|ITEM_BIT,target_index:crate::MAX_FOOD,..State::default()};
        assert!(ai.food_race(&w,w.snake(0).unwrap(),state));
        w.diagnostic_body(1,&[Point{x:780.0,y:400.0};24],0.0).unwrap();
        ai.tick=u64::MAX;ai.prepare(&w);
        assert!(!ai.food_race(&w,w.snake(0).unwrap(),state));
    }
    #[test]
    fn larger_capsule_contender_can_block_an_approaching_rival() {
        let mut w=World::diagnostic_arena(Config {store_power_ups:false,width:1600.0,height:1000.0,density:0.0,
            rules:RuleSet::V2,intelligence:100.0,self_collisions:true,..Config::default()},&[
            (Point{x:800.0,y:495.0},std::f64::consts::FRAC_PI_2,72,0.9),
            (Point{x:870.0,y:525.0},std::f64::consts::PI,24,0.9)],&[]).unwrap();
        capsule(&mut w,Point{x:800.0,y:525.0},750);
        let mut ai=AiController::new();ai.prepare(&w);
        let state=State {target:77|ITEM_BIT,target_index:crate::MAX_FOOD,..State::default()};
        assert_eq!(ai.capsule_contender(&w,w.snake(0).unwrap(),state),Some(1));
        // A nearby head facing away is not evidence of a capsule contest.
        w.snakes[1].angle=0.0;ai.tick=u64::MAX;ai.prepare(&w);
        assert_eq!(ai.capsule_contender(&w,w.snake(0).unwrap(),state),None);
        ai.states[1]=State {generation:w.snake(1).unwrap().generation+1,target:77|ITEM_BIT,..State::default()};
        assert_eq!(ai.capsule_contender(&w,w.snake(0).unwrap(),state),None,"a prior life is not a current commitment");
        w.snakes[1].angle=std::f64::consts::PI;w.snakes[0].len=20;
        ai.tick=u64::MAX;ai.prepare(&w);
        assert_eq!(ai.capsule_contender(&w,w.snake(0).unwrap(),state),None);
    }
    #[test]
    fn a_capsule_does_not_inherit_the_previous_hunts_steering_goal() {
        let mut w=World::diagnostic_arena(Config {store_power_ups:false,width:1600.0,height:1000.0,density:0.0,
            rules:RuleSet::V2,intelligence:100.0,self_collisions:true,..Config::default()},&[
            (Point{x:800.0,y:450.0},std::f64::consts::FRAC_PI_2,72,0.9),
            (Point{x:1070.0,y:525.0},std::f64::consts::PI,24,0.9)],&[]).unwrap();
        let position=Point{x:800.0,y:525.0};capsule(&mut w,position,750);
        w.items[0].kind=effects::EffectKind::Surge;
        let mut ai=AiController::new();ai.prepare(&w);
        let mut state=State {prey:2,prey_generation:w.snake(1).unwrap().generation,
            hunt_until:180,next_response:10,goal:Point{x:1400.0,y:800.0},..State::default()};
        ai.strategy(&w,w.snake(0).unwrap(),&mut state,1.0);
        assert_eq!(state.target,77|ITEM_BIT);
        assert_eq!(state.goal,position);
        assert_eq!(state.prey,0,"stale tactics are cleared even before the next response");
    }
    #[test]
    fn capsule_collection_in_open_space_keeps_self_and_wall_safety() {
        let mut w=arena();capsule(&mut w,Point{x:600.0,y:400.0},750);
        let mut ai=AiController::new();
        for _ in 0..180 {w.step(&mut ai);}
        assert_eq!(w.stats().self_deaths,0);assert_eq!(w.stats().wall_deaths,0);
        assert!(!w.items().any(|i|i.id==77));
        // Deliberate unsafe expiry geometry remains covered by Phase's
        // World-backed first-tangible-sweep regressions.
        let _=ai.steer(&w,w.snake(0).unwrap());
    }
}
