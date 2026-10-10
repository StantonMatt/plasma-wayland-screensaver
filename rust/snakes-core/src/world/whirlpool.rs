// SPDX-License-Identifier: GPL-3.0-or-later
//! One world-owned brew, independent of the holder's effect, death and reversal.
//! Captures are retired by absorption or released by reconfiguration/burst.
use super::*;
use effects::EffectKind;
#[derive(Clone,Copy,Debug,Default)]
pub struct WhirlpoolStats {pub opened:u64,pub bursts:u64,pub captured_value:f64,pub shards:u64,pub holder_eaten:u64,pub rival_eaten:u64}
/// Burst source tags reuse the existing cold feast/identity fields. Shards
/// remain ordinary Shard food; no extra bytes in every hot food record.
pub(crate) const ESSENCE_BIT:u64=1<<63;
pub const RADIUS:f64=22.0;
pub const LIFE:u16=150;
pub const PULL_END:u16=138;
#[inline]
pub(crate) fn capturable(kind:FoodKind)->bool {matches!(kind,FoodKind::Spark|FoodKind::Shard|FoodKind::Pellet)}
impl World {
    /// Capacity replacement must conserve nutrition already owned by a brew.
    pub(super) fn evict_food(&mut self,index:usize) {
        let f=self.food[index];
        if f.captured_by!=0 {
            if let Some(v)=self.items.get_mut(f.captured_by as usize-1).filter(|v|v.vortex) {v.captured_value+=f.value as f32;}
        }
        self.food.remove(index);
    }
    pub fn set_reduced_motion(&mut self,enabled:bool) {self.reduced_motion=enabled;}
    pub fn vortex(&self)->Option<&Item> {self.items.iter().find(|i|i.vortex)}
    /// Capsule score / placement density; excluded event/prism food never counts.
    pub fn whirlpool_food_count(&self,p:Point)->usize {
        let reach=(RADIUS*self.config.base_radius()).powi(2);
        self.food.iter().filter(|f|f.captured_by==0 && capturable(f.kind) && self.distance_squared(p,f.p)<=reach).count()
    }
    pub(super) fn refresh_capture_slot(&mut self) {
        let Some(slot)=self.items.iter().position(|i|i.vortex).map(|i|i as u16+1) else {return;};
        for f in &mut self.food {if f.captured_by!=0 {f.captured_by=slot;}}
    }
    pub(crate) fn open_whirlpool(&mut self,id:usize,p:Point) {
        if self.vortex().is_some() {return;}
        self.whirlpool_stats.opened+=1;self.emit_bubble(id,Glyph::Whirlpool);
        self.items.push(Item {id:self.next_item,kind:EffectKind::Whirlpool,vortex:true,
            position:self.canonical_point(p),radius:RADIUS*self.config.base_radius(),life_ticks:LIFE,
            owner_snake_id:id as u32,owner_generation:self.snakes[id].generation,
            leader_snake_id:u32::MAX,guard_snake_id:u32::MAX,contender_ids:[u32::MAX;2],
            contender_etas:[f32::INFINITY;2],leader_eta:f32::INFINITY,..Default::default()});
        self.next_item=self.next_item.wrapping_add(1);
    }
    pub(super) fn advance_whirlpool(&mut self) {
        let Some(slot)=self.items.iter().position(|i|i.vortex) else {return;};
        self.items[slot].age_ticks+=1;self.items[slot].charge_ticks+=1;self.items[slot].life_ticks-=1;
        let v=self.items[slot];let r=self.config.base_radius();let g=self.config.geometry();
        let mut value=0.0;
        if v.age_ticks<PULL_END {
            // Reverse live-count scan permits in-place removals without capacity work.
            for i in (0..self.food.len()).rev() {
                let f=&mut self.food[i];
                if !capturable(f.kind) {continue;}
                let d=g.delta(v.position,f.p);let dist2=d.x*d.x+d.y*d.y;
                if f.captured_by==0 && dist2>v.radius*v.radius {continue;}
                if f.captured_by==0 {
                    if f.owner>=0 {f.life=f.original_life;}
                    f.owner=-1;f.attraction=0.0;f.velocity=Point::default();
                }
                f.captured_by=slot as u16+1;
                let distance=dist2.sqrt();let q=(1.0-distance/v.radius).clamp(0.0,1.0);
                let next=(distance-v.radius*0.25*(0.55+0.6*q)*STEP_SECONDS).max(0.0);
                if next<0.9*r {value+=f.value;self.food.remove(i);continue;}
                let angle=if self.reduced_motion {0.0} else {(0.5+1.9*q)*STEP_SECONDS};let (sin,cos)=angle.sin_cos();let scale=next/distance.max(1e-9);
                let p=Point{x:v.position.x+(d.x*cos-d.y*sin)*scale,y:v.position.y+(d.x*sin+d.y*cos)*scale};
                f.p=if g.deadly {Point{x:p.x.clamp(2.0,g.width-2.0),y:p.y.clamp(2.0,g.height-2.0)}} else {g.wrap(p)};
            }
        }
        self.items[slot].captured_value+=value as f32;
        if v.age_ticks<LIFE {return;}
        let mut v=self.items.remove(slot);
        // Reserve the burst before releasing particles. Prefer ordinary free
        // food; if only captured particles remain, absorb replacements into
        // the burst and resize it before deciding each subsequent eviction.
        let mut n=((v.captured_value as f64/0.7).round() as usize+10).clamp(12,46);
        while self.food.len()+n>self.config.maximum_food() {
            let Some(i)=self.food.iter().position(|f|f.captured_by==0 && capturable(f.kind))
                .or_else(||self.food.iter().position(|f|capturable(f.kind))) else {break;};
            let f=self.food.remove(i);
            if f.captured_by!=0 {
                v.captured_value+=f.value as f32;
                n=((v.captured_value as f64/0.7).round() as usize+10).clamp(12,46);
            }
        }
        for f in &mut self.food {f.captured_by=0;}
        // Unabsorbed particles resume their original life and ordinary ownership.
        // Shards conserve captured value plus the six-unit item bonus.
        let value=(v.captured_value as f64+6.0)/n as f64;
        self.whirlpool_stats.bursts+=1;self.whirlpool_stats.captured_value+=v.captured_value as f64;
        for k in 0..n {
            if self.food.len()>=self.config.maximum_food() {break;}
            let a=k as f64*TAU/n as f64;let d=Point{x:a.cos(),y:a.sin()};
            let speed=(70.0+60.0*k as f64/(n-1) as f64)*r/6.0;
            self.whirlpool_stats.shards+=1;
            self.add_food(Food {feast:ESSENCE_BIT|v.id,trail_index:v.owner_snake_id,feast_len:v.owner_generation,p:Point{x:v.position.x+d.x*r,y:v.position.y+d.y*r},
                velocity:Point{x:d.x*speed,y:d.y*speed},value,life:14.0+6.0*k as f64/(n-1) as f64,
                color:self.snakes.get(v.owner_snake_id as usize).map_or(0,|s|s.color),kind:FoodKind::Shard,..Default::default()});
        }
        self.push_event(FrameEvent {tick:self.tick+1,position:v.position,snake_id:v.owner_snake_id,
            generation:v.owner_generation,other_snake_id:7,kind:EventKind::VortexBurst,
            duration_ticks:14,value:v.captured_value,release_tick:v.id,..Default::default()});
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::{Controller,ScriptedController,Steering};
    struct Use {action:u32}
    impl Controller for Use {
        fn steer(&mut self,_:&World,s:SnakeView<'_>)->Steering {Steering{desired_angle:s.angle,rush:0.0}}
        fn use_request(&self,_:u32)->u32 {self.action}
    }
    fn arena(storage:bool)->World {
        let mut w=World::diagnostic_arena(Config{rules:RuleSet::V2,width:4000.0,height:2000.0,
            deadly_walls:false,self_collisions:false,store_power_ups:storage,world_events:false,..Default::default()},
            &[(Point{x:1500.0,y:1000.0},0.0,24,0.0)],&[]).unwrap();
        w.item_timer=10000;w.prism_timer=10000;w
    }
    fn capsule(w:&mut World) {w.items.push(Item{id:88,kind:EffectKind::Whirlpool,position:w.segments[0].current,
        life_ticks:750,radius:20.0,..Default::default()});}
    fn food(w:&mut World,p:Point,kind:FoodKind) {w.food.push(Food{id:44+w.food.len() as u64,p,kind,value:1.0,
        life:20.0,original_life:20.0,owner:-1,size:2.0,..Default::default()});}
    #[test]
    fn whirlpool_storage_chooses_endpoint_and_preserves_effect_through_world_step() {
        let mut w=arena(true);capsule(&mut w);w.step(&mut Use{action:0});
        assert_eq!(w.snakes[0].inventory.kinds[0],7);assert!(w.vortex().is_none());
        let pickup=w.segments[0].current;w.snakes[0].effect_kind=3;w.snakes[0].effect_ticks=120;
        for _ in 0..5 {w.step(&mut Use{action:1});}
        let v=*w.vortex().unwrap();assert_eq!(v.position,w.segments[0].current);assert_ne!(v.position,pickup);
        assert_eq!(w.snakes[0].effect_kind,3);assert_eq!(w.snakes[0].inventory.count,0);
        assert!(w.frame_events().any(|e|e.kind==EventKind::Pickup && e.flags==event_flags::HELD_ACTIVATION));
    }
    #[test]
    fn whirlpool_capture_releases_magnet_claim_excludes_prizes_and_wraps() {
        let mut w=arena(false);let p=Point{x:1.0,y:1000.0};w.open_whirlpool(0,p);
        let positions=[Point{x:3990.0,y:1000.0},Point{x:20.0,y:1000.0}];
        for kind in [FoodKind::Spark,FoodKind::Shard,FoodKind::Pellet,FoodKind::Prism,FoodKind::PrismSeed,FoodKind::Star,FoodKind::Meteor] {food(&mut w,positions[0],kind);}
        w.food[0].owner=0;w.food[0].life=-1.0;w.snakes[0].effect_kind=2;w.snakes[0].effect_ticks=300;
        w.step(&mut Use{action:0});
        for f in w.food.iter().filter(|f|f.id>=44 && f.id<=50) {
            if capturable(f.kind) {assert_eq!(f.owner,-1);assert_ne!(f.captured_by,0);assert!(f.life>0.0);assert!(!f.pickup_eligible(u64::MAX));assert!(f.p.x>=0.0 && f.p.x<4000.0);}
            else {assert_eq!(f.captured_by,0);}
        }
        w.reconfigure(Config{power_ups:false,..w.config()}).unwrap();assert!(w.vortex().is_none());assert!(w.food.iter().all(|f|f.captured_by==0));
    }
    #[test]
    fn whirlpool_burst_has_exact_clock_and_conserves_food_value() {
        let mut w=arena(false);capsule(&mut w);w.step(&mut Use{action:0});
        let p=w.vortex().unwrap().position;food(&mut w,p,FoodKind::Shard);
        for _ in 0..137 {w.step(&mut Use{action:0});}
        assert_eq!(w.vortex().unwrap().charge_ticks,137);assert!(w.vortex().unwrap().captured_value>=1.0);
        let p=Point{x:p.x+100.0,y:p.y};food(&mut w,p,FoodKind::Shard);let id=w.food.last().unwrap().id;
        for _ in 0..12 {w.step(&mut Use{action:0});}
        assert_eq!(w.vortex().unwrap().charge_ticks,149);assert_eq!(w.food.iter().find(|f|f.id==id).unwrap().captured_by,0);
        let value=w.vortex().unwrap().captured_value;w.step(&mut Use{action:0});assert!(w.vortex().is_none());
        let e=w.frame_events().find(|e|e.kind==EventKind::VortexBurst).unwrap();assert_eq!(e.value,value);assert_eq!(e.duration_ticks,14);
        assert!(w.food.iter().all(|f|f.captured_by==0));
    }
    #[test]
    fn whirlpool_slot_survives_capsule_compaction_and_holder_death() {
        let mut w=arena(false);capsule(&mut w);w.open_whirlpool(0,Point{x:1800.0,y:1000.0});
        food(&mut w,Point{x:1900.0,y:1000.0},FoodKind::Shard);
        w.step(&mut Use{action:0});assert_eq!(w.items.len(),1);assert_eq!(w.food.iter().find(|f|f.id==44).unwrap().captured_by,1);
        w.snakes[0].dying=DeathReason::Body;w.explode_snake(0);assert!(w.vortex().is_some());
        w.step_n(&mut ScriptedController::new(|_,s:SnakeView<'_>|Steering{desired_angle:s.angle,rush:0.0}),150);
        assert!(w.vortex().is_none());
    }
    #[test]
    fn whirlpool_calm_transport_is_radial_and_burst_value_is_exact_at_the_cap() {
        let mut w=arena(false);w.set_reduced_motion(true);let p=Point{x:2000.0,y:1000.0};w.open_whirlpool(0,p);
        food(&mut w,Point{x:p.x+100.0,y:p.y},FoodKind::Shard);
        w.step(&mut Use{action:0});let f=w.food.iter().find(|f|f.id==44).unwrap();
        assert_eq!(f.p.y,p.y);assert!(f.p.x<p.x+100.0);assert_ne!(f.captured_by,0);
        let slot=w.items.iter().position(|i|i.vortex).unwrap();
        w.items[slot].charge_ticks=149;w.items[slot].age_ticks=149;w.items[slot].life_ticks=1;w.items[slot].captured_value=7.0;
        while w.food.len()<w.config.maximum_food() {food(&mut w,Point{x:3000.0,y:500.0},FoodKind::Spark);}
        let first=w.next_food;w.step(&mut Use{action:0});
        let shards:Vec<_>=w.food.iter().filter(|f|f.feast&ESSENCE_BIT!=0 && f.id>=first).collect();
        assert_eq!(shards.len(),20);assert!((shards.iter().map(|f|f.value).sum::<f64>()-13.0).abs()<1e-9);
        assert!(shards.iter().all(|f|f.kind==FoodKind::Shard && (14.0..=20.0).contains(&f.life) && f.captured_by==0));
        assert_eq!(w.food.len(),w.config.maximum_food());
    }

    #[test]
    fn whirlpool_capacity_evictions_credit_captured_nutrition() {
        for boost in [false,true] {
            let mut w=arena(false);let center=Point{x:2500.0,y:1000.0};w.open_whirlpool(0,center);
            food(&mut w,Point{x:center.x+100.0,y:center.y},FoodKind::Spark);
            w.step(&mut Use{action:0});assert_ne!(w.food[0].captured_by,0);
            // Death eviction scans backward; boost replacement scans forward.
            while w.food.len()<w.config.maximum_food() {food(&mut w,Point{x:3500.0,y:500.0},FoodKind::Spark);}
            if boost {w.snakes[0].boost_ticks=4;w.snakes[0].boost_cost=2;w.advance_boost(0,false);} else {
                let last=w.food.len()-1;w.food.swap(0,last);w.snakes[0].dying=DeathReason::Body;w.explode_snake(0);
            }
            assert!(!w.food.iter().any(|f|f.id==44));assert_eq!(w.vortex().unwrap().captured_value,1.0);
            w.items[0].age_ticks=149;w.items[0].charge_ticks=149;w.items[0].life_ticks=1;
            w.step(&mut Use{action:0});assert_eq!(w.whirlpool_stats.captured_value,1.0);
            let value=w.food.iter().filter(|f|f.feast&ESSENCE_BIT!=0).map(|f|f.value).sum::<f64>();
            assert!((value-7.0).abs()<1e-9);
        }
    }

    #[test]
    fn whirlpool_starfall_replacement_credits_capture_before_transport() {
        let mut w=arena(false);let center=Point{x:2500.0,y:1000.0};w.open_whirlpool(0,center);
        food(&mut w,Point{x:center.x+100.0,y:center.y},FoodKind::Spark);
        w.step(&mut Use{action:0});assert_ne!(w.food[0].captured_by,0);
        while w.food.len()<w.config.maximum_food() {food(&mut w,Point{x:3500.0,y:500.0},FoodKind::Spark);}
        w.config.world_events=true;w.tick=44;w.event_schedule.star_start=Some(0);
        w.event_schedule.star_due=u64::MAX;w.event_schedule.night_due=u64::MAX;
        w.step(&mut Use{action:0});
        assert!(!w.food.iter().any(|f|f.id==44));assert_eq!(w.vortex().unwrap().captured_value,1.0);
        assert!(w.food.iter().any(|f|f.kind==FoodKind::Meteor));
    }
    #[test]
    fn whirlpool_burst_at_all_captured_cap_conserves_evicted_nutrition() {
        let mut w=arena(false);let center=Point{x:2500.0,y:1000.0};w.open_whirlpool(0,center);
        while w.food.len()<w.config.maximum_food() {food(&mut w,Point{x:center.x+100.0,y:center.y},FoodKind::Spark);}
        w.step(&mut Use{action:0});assert!(w.food.iter().all(|f|f.captured_by!=0));
        let before=w.food.iter().map(|f|f.value).sum::<f64>();
        w.items[0].age_ticks=149;w.items[0].charge_ticks=149;w.items[0].life_ticks=1;
        w.step(&mut Use{action:0});assert_eq!(w.whirlpool_stats.captured_value,46.0);
        assert_eq!(w.food.len(),w.config.maximum_food());
        assert!((w.food.iter().map(|f|f.value).sum::<f64>()-before-6.0).abs()<1e-9);
    }

    #[test]
    fn whirlpool_timing_spawn_exclusion_includes_burst_frame() {
        // Spawning precedes the vortex update: the burst frame still excludes
        // Whirlpool capsules; the following frame admits them.
        let mut after=0;
        for age in [148_u16,149,150] {for seed in 1..=80 {
            let mut w=arena(false);w.rng=WorldRng::new(seed);
            w.last_item_kind=EffectKind::None;
            w.open_whirlpool(0,Point{x:2500.0,y:1000.0});
            w.items[0].age_ticks=age-1;w.items[0].life_ticks=LIFE-(age-1);
            for step in 0_u16..3 {
                w.item_timer=1;w.items.retain(|i|i.vortex);w.step(&mut Use{action:0});
                let count=w.items.iter().filter(|i|!i.vortex && i.kind==EffectKind::Whirlpool).count();
                if age+step<=LIFE {assert_eq!(count,0);} else {after+=count;}
            }
        }}
        assert!(after>0,"post-burst spawn distribution must exercise admission");
    }

}
