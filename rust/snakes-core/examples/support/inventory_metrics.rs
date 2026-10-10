// SPDX-License-Identifier: GPL-3.0-or-later
//! Diagnostic-only inventory accounting, outside the timed simulation step.
use snakes_core::{*,ai::DesktopObservation};
use std::{fs::File,io::{BufWriter,Write}};
use super::item_lifecycle::{lifecycle,ItemLifecycle,StepInventory};
/// Inventory activation also emits Pickup. Only a field record consumed once
/// contributes to diagnostic acquisition/targeting counters.
pub fn field_acquisition<'a>(w:&World,e:&FrameEvent,items:&'a [Item],captured:&[(u64,usize)])->Option<&'a Item> {
    const ITEM_BIT:u64=1<<63;
    if !lifecycle(e).acquisition() {return None;}
    items.iter().find(|i|i.kind as u32==e.other_snake_id && w.distance_squared(i.position,e.position)<0.01
        && !captured.iter().any(|&(key,_)|key==(i.id|ITEM_BIT))
        && !w.items().any(|live|live.id==i.id))
}
#[derive(Clone,Copy,Default)]
struct Before {generation:u32,kinds:[u8;3],life:[u16;3],flags:u32}
#[derive(Clone,Copy,Default)]
struct Use {situation:u8,generation:u32,kind:u8,trap:bool,expiry:bool,at:u64}
pub struct InventoryMetrics {
    before:[Before;MAX_SNAKES],pending:[Use;MAX_SNAKES],instant:[Use;MAX_SNAKES],phase:[Use;MAX_SNAKES],
    stored:[u64;8],used:[u64;8],fizzled:[u64;8],situations:[[u64;7];8],
    flip:[Use;MAX_SNAKES],flip_situations:[u64;4],escape_survived:u64,ambush_wins:u64,
    phase_trapped:u64,phase_survived:u64,unused_phase_deaths:u64,dropped:[u64;8],orbits:[(u32,u64);MAX_SNAKES],holder_orbiters:u64,rival_orbiters:u64,brew_id:u64,brew_rival_seen:bool,brews_observed:u64,brews_with_rivals:u64,
}
impl Default for InventoryMetrics {fn default()->Self {Self {flip:[Use::default();MAX_SNAKES],flip_situations:[0;4],escape_survived:0,ambush_wins:0,before:[Before::default();MAX_SNAKES],pending:[Use::default();MAX_SNAKES],instant:[Use::default();MAX_SNAKES],phase:[Use::default();MAX_SNAKES],stored:[0;8],used:[0;8],fizzled:[0;8],situations:[[0;7];8],phase_trapped:0,phase_survived:0,unused_phase_deaths:0,dropped:[0;8],orbits:[(0,0);MAX_SNAKES],holder_orbiters:0,rival_orbiters:0,brew_id:0,brew_rival_seen:false,brews_observed:0,brews_with_rivals:0}}}
impl InventoryMetrics {
    pub fn before(&mut self,w:&World) {for s in w.snakes() {self.before[s.id as usize]=Before {generation:s.generation,kinds:s.inventory.kinds,life:s.inventory.life,flags:s.flags};}}
    pub fn after(&mut self,w:&World,obs:&[DesktopObservation;MAX_SNAKES]) {
        self.after_events(w,obs,w.frame_events());
    }
    fn after_events<'a>(&mut self,w:&World,obs:&[DesktopObservation;MAX_SNAKES],events:impl Iterator<Item=&'a FrameEvent>) {
        let mut collisions=[CollisionEvent::default();MAX_SNAKES];let mut count=0;
        for e in w.collision_events() {collisions[count]=*e;count+=1;}
        self.after_records(w,obs,events,&collisions[..count]);
    }
    fn after_records<'a>(&mut self,w:&World,obs:&[DesktopObservation;MAX_SNAKES],events:impl Iterator<Item=&'a FrameEvent>,collisions:&[CollisionEvent]) {
        let mut inventory=self.before.map(|b|StepInventory::new(b.generation,
            Inventory {kinds:b.kinds,life:b.life,count:b.kinds.iter().take_while(|&&k|k!=0).count() as u8,..Default::default()},
            w.config().rules==RuleSet::V2 && w.config().power_ups));
        for e in events {
            let k=e.other_snake_id as usize;let id=e.snake_id as usize;
            if k>=8 {continue;}
            match e.kind {
                EventKind::Stash=>self.stored[k]+=1,
                EventKind::Fizzle=>self.fizzled[k]+=1,
                EventKind::ItemSpawn if id<MAX_SNAKES=>self.dropped[k]+=1,
                EventKind::Use if id<MAX_SNAKES=> {
                    let d=obs[id];let b=self.before[id];
                    // Zero-duration Uses are full-inventory touches, independent
                    // of a held request that may still be winding up.
                    let request=if e.duration_ticks==0 {&mut self.instant[id]} else {&mut self.pending[id]};
                    *request=Use {situation:if e.duration_ticks!=0 && d.generation==e.generation {d.flip_situation} else {0},generation:e.generation,kind:k as u8,trap:(b.generation==e.generation && b.flags&flags::TRAPPED!=0) || w.snake(id).is_some_and(|s|s.generation==e.generation && s.flags&flags::TRAPPED!=0) || (d.generation==e.generation && d.safe_ticks<15),
                        expiry:lifecycle(e)==ItemLifecycle::HeldRequest && inventory[id].expiring_request(e),at:w.tick()};
                }
                EventKind::Pickup if id<MAX_SNAKES=> {
                    let held=e.flags&event_flags::HELD_ACTIVATION!=0;
                    self.used[k]+=1;let d=if obs[id].generation==e.generation {obs[id]} else {DesktopObservation::default()};let use_=if held {self.pending[id]} else {self.instant[id]};
                    let pending=use_.generation==e.generation && use_.kind==k as u8 && w.tick().saturating_sub(use_.at)<=4;
                    let category=if pending && use_.trap {0} else if d.contested {1} else if d.cutoff {2}
                        else if d.fleeing {3} else if k==2 {4} else if pending && use_.expiry {5} else {6};
                    self.situations[k][category]+=1;
                    if k==3 && pending && use_.trap {self.phase_trapped+=1;self.phase[id]=Use {at:w.tick(),..use_};}
                    if k==6 && pending {
                        self.flip_situations[use_.situation.min(3) as usize]+=1;
                        self.flip[id]=Use {at:w.tick(),..use_};
                    }
                    if held {self.pending[id]=Use::default();} else {self.instant[id]=Use::default();}
                }
                _=>{}
            }
            if id<MAX_SNAKES {inventory[id].apply(e,collisions.iter().any(|c|c.victim==e.snake_id && c.generation==e.generation));}
        }
        if let Some(v)=w.vortex() {if self.brew_id!=v.id {
            self.brew_id=v.id;self.brew_rival_seen=false;self.brews_observed+=1;
        }}
        for (id,d) in obs.iter().enumerate() {
            if d.target_kind==3 && matches!(d.mode,12|13) && self.orbits[id]!=(d.generation,d.target) {
                self.orbits[id]=(d.generation,d.target);
                if d.mode==12 {self.holder_orbiters+=1;} else {self.rival_orbiters+=1;
                    if d.target&!(1<<63)==self.brew_id && !self.brew_rival_seen {
                        self.brew_rival_seen=true;self.brews_with_rivals+=1;
                    }
                }
            }
        }
        for e in collisions {if inventory[e.victim as usize].carries(3,e.generation) {self.unused_phase_deaths+=1;}}
        for collision in collisions {if collision.reason==DeathReason::Head {for id in 0..w.snake_count() {
            let flip=self.flip[id];
            if flip.kind==6 && flip.situation==2 && collision.owner_mask&(1<<id)!=0
                && collision.owner_generations[id]==flip.generation && w.tick()<=flip.at+30
                && w.snake(id).is_some_and(|s|s.alive && s.generation==flip.generation) {
                self.ambush_wins+=1;self.flip[id]=Use::default();
            }
        }}}
        for id in 0..w.snake_count() {
            let flip=self.flip[id];
            if flip.kind==6 {let s=w.snake(id).unwrap();
                if !s.alive || s.generation!=flip.generation {self.flip[id]=Use::default();}
                else if w.tick()>=flip.at+30 {
                    if flip.situation==1 {self.escape_survived+=1;}self.flip[id]=Use::default();
                }
            }
            let phase=self.phase[id];if phase.kind==0 {continue;}
            let s=w.snake(id).unwrap();
            if !s.alive || s.generation!=phase.generation {self.phase[id]=Use::default();}
            else if w.tick()>=phase.at+30 {self.phase_survived+=1;self.phase[id]=Use::default();}
        }
    }
    pub fn write(&self,prefix:&str) {
        println!("whirlpool_holder_orbiters={} whirlpool_rival_orbiters={}",self.holder_orbiters,self.rival_orbiters);
        println!("whirlpool_brews_observed={} whirlpool_brews_with_rivals={}",self.brews_observed,self.brews_with_rivals);
        let mut out=BufWriter::new(File::create(format!("{prefix}.inventory.csv")).unwrap());
        writeln!(out,"kind,stored,used,fizzled,dropped,trapped,contest,cutoff,fleeing,food,expiry,other").unwrap();
        for k in 1..8 {let a=self.situations[k];writeln!(out,"{k},{},{},{},{},{},{},{},{},{},{},{}",self.stored[k],self.used[k],self.fizzled[k],self.dropped[k],a[0],a[1],a[2],a[3],a[4],a[5],a[6]).unwrap();}
        println!("inventory flip_escape={} flip_ambush={} flip_loot={} flip_other={} escape_survived_1s={} ambush_head_wins_1s={}",self.flip_situations[1],self.flip_situations[2],self.flip_situations[3],self.flip_situations[0],self.escape_survived,self.ambush_wins);
        println!("inventory phase_trapped={} phase_survived_1s={} unused_phase_deaths={}",self.phase_trapped,self.phase_survived,self.unused_phase_deaths);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snakes_core::controller::{Controller,Steering};
    #[test]
    fn unused_phase_death_uses_collision_inventory_after_all_transitions() {
        let w=World::new(Config {rules:RuleSet::V2,..Default::default()}).unwrap();
        let generation=w.snake(0).unwrap().generation;
        let event=|kind,item,slot,flags|FrameEvent {kind,generation,other_snake_id:item,cut_index:slot,flags,..Default::default()};
        let collision=CollisionEvent {victim:0,generation,reason:DeathReason::Wall,..Default::default()};
        for (kinds,life,events,expected) in [
            ([3,0,0],[1,0,0],vec![event(EventKind::Fizzle,3,0,0)],0),
            ([3,0,0],[100,0,0],vec![event(EventKind::Pickup,3,0,event_flags::HELD_ACTIVATION)],0),
            ([0,0,0],[0,0,0],vec![event(EventKind::Stash,3,0,0),event(EventKind::ItemSpawn,3,0,0)],1),
            ([0,0,0],[0,0,0],vec![event(EventKind::Stash,3,0,0),event(EventKind::Fizzle,3,0,0)],1),
            ([3,3,0],[1,100,0],vec![event(EventKind::Fizzle,3,0,0),event(EventKind::Fizzle,3,0,0)],1),
            ([3,0,0],[100,0,0],vec![FrameEvent {duration_ticks:4,..event(EventKind::Use,3,0,0)}],1),
            ([3,0,0],[100,0,0],vec![event(EventKind::EffectExpiry,3,0,0)],1),
        ] {
            let mut metrics=InventoryMetrics::default();metrics.before[0]=Before {generation,kinds,life,flags:0};
            metrics.after_records(&w,&[DesktopObservation::default();MAX_SNAKES],events.iter(),&[collision]);
            assert_eq!(metrics.unused_phase_deaths,expected,"events={events:?}");
        }
        let mut metrics=InventoryMetrics::default();metrics.before[0]=Before {generation,kinds:[3,0,0],life:[100,0,0],flags:0};
        metrics.after_records(&w,&[DesktopObservation::default();MAX_SNAKES],std::iter::empty(),&[CollisionEvent {generation:generation+1,..collision}]);
        assert_eq!(metrics.unused_phase_deaths,0,"newborn death cannot inherit inventory");
    }
    #[test]
    fn held_use_expiry_is_measured_after_decrement_and_fizzle_compaction() {
        let w=World::new(Config {rules:RuleSet::V2,..Default::default()}).unwrap();let generation=w.snake(0).unwrap().generation;
        let obs=[DesktopObservation {generation,safe_ticks:100,..Default::default()};MAX_SNAKES];
        for (slot,kind,expected) in [(0,1,false),(1,3,true)] {
            let mut metrics=InventoryMetrics::default();metrics.before[0]=Before {generation,kinds:[4,1,3],life:[1,1800,151],flags:0};
            let base=FrameEvent {generation,..Default::default()};
            metrics.after_events(&w,&obs,[FrameEvent {kind:EventKind::Fizzle,other_snake_id:4,..base},FrameEvent {kind:EventKind::Use,other_snake_id:kind,cut_index:slot,duration_ticks:4,..base}].iter());
            assert_eq!(metrics.pending[0].expiry,expected);
            metrics.after_events(&w,&obs,[FrameEvent {kind:EventKind::Pickup,other_snake_id:kind,cut_index:slot,flags:event_flags::HELD_ACTIVATION,..base}].iter());
            assert_eq!(metrics.situations[kind as usize][if expected {5} else {6}],1);
        }
    }
    #[test]
    fn stale_desktop_observations_cannot_label_current_inventory_use() {
        let w=World::new(Config {rules:RuleSet::V2,..Default::default()}).unwrap();let generation=w.snake(0).unwrap().generation;
        let mut metrics=InventoryMetrics::default();
        metrics.before[0]=Before {generation:generation+1,flags:flags::TRAPPED,..Default::default()};
        let obs=[DesktopObservation {generation:generation+1,safe_ticks:0,contested:true,..Default::default()};MAX_SNAKES];
        let use_=FrameEvent {kind:EventKind::Use,other_snake_id:3,generation,duration_ticks:4,..Default::default()};
        metrics.after_events(&w,&obs,[use_,FrameEvent {kind:EventKind::Pickup,flags:event_flags::HELD_ACTIVATION,..use_}].iter());
        assert_eq!(metrics.phase_trapped,0);assert_eq!(metrics.situations[3][6],1);
    }
    #[test]
    fn held_phase_attribution_survives_instant_touch_activation() {
        struct Forward;
        impl Controller for Forward {
            fn steer(&mut self,_:&World,s:SnakeView<'_>)->Steering {Steering {desired_angle:s.angle,rush:0.0}}
        }
        for touch_kind in [1,3] {for trapped in [false,true] {for touch_delay in [0,1,3] {
            let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,width:4000.0,height:2000.0,density:0.0,
                deadly_walls:false,self_collisions:false,world_events:false,seed:73,..Default::default()},
                &[(Point{x:1500.0,y:1000.0},0.0,30,0.0)],&[]).unwrap();
            let generation=w.snake(0).unwrap().generation;
            let obs=[DesktopObservation {generation,safe_ticks:100,..Default::default()};MAX_SNAKES];
            let mut metrics=InventoryMetrics::default();
            // Model a full inventory with an expiring Phase in the selected slot.
            metrics.before[0]=Before {generation,kinds:[3,1,2],life:[100,1800,1800],flags:if trapped {flags::TRAPPED} else {0}};
            let held=FrameEvent {kind:EventKind::Use,snake_id:0,generation,other_snake_id:3,
                cut_index:0,duration_ticks:4,..Default::default()};
            metrics.after_events(&w,&obs,[held].iter());
            for _ in 0..touch_delay {w.step(&mut Forward);}
            metrics.before[0].flags=0;
            let touch=FrameEvent {tick:w.tick(),kind:EventKind::Use,other_snake_id:touch_kind,
                cut_index:u16::MAX,duration_ticks:0,..held};
            metrics.after_events(&w,&obs,[touch,FrameEvent {kind:EventKind::Pickup,..touch}].iter());
            assert_eq!(metrics.used[touch_kind as usize],1);
            assert_eq!(metrics.situations[touch_kind as usize][6],1,"instant touch has its own attribution");
            assert_eq!(metrics.phase_trapped,0);
            assert_eq!(metrics.instant[0].kind,0);
            assert_eq!(metrics.pending[0].kind,3,"instant Pickup must retain held Phase request");
            assert_eq!(metrics.pending[0].trap,trapped);
            assert!(metrics.pending[0].expiry);
            while w.tick()<4 {w.step(&mut Forward);}
            let completion=FrameEvent {tick:w.tick(),kind:EventKind::Pickup,
                flags:event_flags::HELD_ACTIVATION,duration_ticks:0,..held};
            metrics.after_events(&w,&obs,[completion].iter());
            assert_eq!(metrics.used[3],1+u64::from(touch_kind==3));
            assert_eq!(metrics.situations[3][if trapped {0} else {5}],1,"held completion retains trap/expiry attribution");
            assert_eq!(metrics.pending[0].kind,0);
            assert_eq!(metrics.phase_trapped,u64::from(trapped));
            for _ in 0..29 {w.step(&mut Forward);metrics.after_events(&w,&obs,std::iter::empty());}
            assert_eq!(metrics.phase_survived,0);
            w.step(&mut Forward);metrics.after_events(&w,&obs,std::iter::empty());
            assert_eq!(metrics.phase_survived,u64::from(trapped));
            w.step(&mut Forward);metrics.after_events(&w,&obs,std::iter::empty());
            assert_eq!(metrics.phase_survived,u64::from(trapped),"survival counts only once");
        }}}
    }
    #[test]
    fn pickup_near_same_kind_landing_capsule_is_not_a_field_acquisition() {
        struct Forward;
        impl Controller for Forward {
            fn steer(&mut self,_:&World,s:SnakeView<'_>)->Steering {Steering {desired_angle:s.angle,rush:0.0}}
        }
        let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,width:4000.0,height:2000.0,density:0.0,
            deadly_walls:false,self_collisions:false,world_events:false,seed:73,..Default::default()},
            &[(Point{x:1500.0,y:1000.0},0.0,30,0.0)],&[]).unwrap();
        while w.items().len()==0 {w.step(&mut Forward);assert!(w.tick()<1000);}
        let items:Vec<_>=w.items().copied().collect();let item=items[0];
        assert!(item.pickable_from_tick>w.tick());
        let e=FrameEvent {tick:w.tick(),kind:EventKind::Pickup,snake_id:0,
            generation:w.snake(0).unwrap().generation,other_snake_id:item.kind as u32,
            position:Point{x:item.position.x+0.05,y:item.position.y},..Default::default()};
        assert!(w.distance_squared(item.position,e.position)<0.01);
        for flags in [0,event_flags::HELD_ACTIVATION] {
            let e=FrameEvent {flags,..e};
            assert!(field_acquisition(&w,&e,&items,&[]).is_none(),"unconsumed landing capsule must retain its acquisition/spawn tracking");
        }
    }
    #[test]
    fn collected_and_used_capsule_is_one_acquisition_through_world_step() {
        struct Collector;
        impl Controller for Collector {
            fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
                let a=w.items().next().map_or(s.angle,|i| {let d=w.displacement(s.segments[0].current,i.position);d.y.atan2(d.x)});
                Steering {desired_angle:a,rush:0.0}
            }
            fn use_request(&self,_:u32)->u32 {1}
        }
        let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,width:1600.0,height:1000.0,density:0.0,
            deadly_walls:false,self_collisions:false,world_events:false,seed:73,..Default::default()},
            &[(Point{x:800.0,y:500.0},0.0,30,0.0)],&[]).unwrap();
        let mut acquired=0;let mut stored=0;let mut activated=0;
        for _ in 0..5000 {
            let items:Vec<_>=w.items().copied().collect();w.step(&mut Collector);let mut captured=Vec::new();
            for e in w.frame_events() {
                if let Some(item)=field_acquisition(&w,e,&items,&captured) {
                    let held=FrameEvent {kind:EventKind::Pickup,flags:event_flags::HELD_ACTIVATION,..*e};
                    assert!(field_acquisition(&w,&held,&items,&captured).is_none(),"held completion is not acquisition even if a matching capsule was consumed this tick");
                    acquired+=1;captured.push((item.id|(1<<63),e.snake_id as usize));
                    assert!(field_acquisition(&w,e,&items,&captured).is_none());
                }
                if e.kind==EventKind::Stash {stored+=1;}
                if e.kind==EventKind::Pickup {activated+=1;}
            }
            if activated>0 {break;}
        }
        assert_eq!((acquired,stored,activated),(1,1,1));
    }
    #[test]
    fn flip_touch_does_not_inherit_a_pending_signature() {
        let w=World::new(Config {rules:RuleSet::V2,..Default::default()}).unwrap();
        let generation=w.snake(0).unwrap().generation;
        let mut obs=[DesktopObservation::default();MAX_SNAKES];obs[0].generation=generation;obs[0].flip_situation=2;
        let use_=FrameEvent {kind:EventKind::Use,generation,other_snake_id:6,duration_ticks:0,..Default::default()};
        let pickup=FrameEvent {kind:EventKind::Pickup,..use_};
        let mut metrics=InventoryMetrics::default();metrics.before(&w);
        metrics.after_records(&w,&obs,[use_,pickup].iter(),&[]);
        assert_eq!(metrics.flip_situations,[1,0,0,0]);
    }

}
