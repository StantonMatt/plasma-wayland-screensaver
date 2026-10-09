// SPDX-License-Identifier: GPL-3.0-or-later
//! Observer-only interpretation of the production item pass. Food and steering
//! use the old effect after clock decrement; activation precedes collisions.
use snakes_core::{event_flags, effects::EffectKind, EventKind, FrameEvent, Inventory};

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum ItemLifecycle {
    Stored, HeldRequest, TouchRequest, HeldActivation, FieldActivation,
    Fizzle, Drop, Spawn, FieldExpiry, EffectExpiry, Other,
}
pub fn lifecycle(e:&FrameEvent)->ItemLifecycle {
    use ItemLifecycle::*;
    match e.kind {
        EventKind::Stash=>Stored,
        EventKind::Use if e.duration_ticks>0=>HeldRequest,
        EventKind::Use=>TouchRequest,
        EventKind::Pickup if e.flags&event_flags::HELD_ACTIVATION!=0=>HeldActivation,
        EventKind::Pickup=>FieldActivation,
        EventKind::Fizzle=>Fizzle,
        EventKind::ItemSpawn if e.snake_id!=u32::MAX=>Drop,
        EventKind::ItemSpawn=>Spawn,
        EventKind::ItemExpiry=>FieldExpiry,
        EventKind::EffectExpiry=>EffectExpiry,
        _=>Other,
    }
}
impl ItemLifecycle {
    pub fn acquisition(self)->bool {matches!(self,Self::Stored|Self::FieldActivation)}
    pub fn activation(self)->bool {matches!(self,Self::HeldActivation|Self::FieldActivation)}
}
pub fn replaces_effect(e:&FrameEvent)->bool {
    lifecycle(e).activation() && !matches!(EffectKind::from_byte(e.other_snake_id as u8),EffectKind::Frost|EffectKind::Flip)
}

/// Slot identity alone never identifies an effect episode. Frost, Flip and Stash do
/// not replace it; both touch and held activation start a fresh episode.
#[derive(Clone,Copy,Debug,Default)]
pub struct EffectEpisode {pub generation:u32,pub kind:u8,pub used:bool}
impl EffectEpisode {
    pub fn observe(&mut self,e:&FrameEvent)->bool {
        if replaces_effect(e) {
            *self=Self {generation:e.generation,kind:e.other_snake_id as u8,used:false};return true;
        }
        if lifecycle(e)==ItemLifecycle::EffectExpiry && self.generation==e.generation && self.kind as u32==e.other_snake_id {
            *self=Self::default();
        }
        false
    }
    pub fn use_once(&mut self,kind:u8,generation:u32)->bool {
        if self.kind!=kind || self.generation!=generation || self.used {return false;}
        self.used=true;true
    }
}

/// Replay the ordered inventory events after decrementing shelf clocks, but
/// defer compaction to Fizzle records (their indices are sequential). Death
/// drops/cap fizzles retain the collision inventory for attribution; they use
/// original drop slots, unlike shelf fizzles and held activations.
#[derive(Clone,Copy,Default)]
pub struct StepInventory {pub generation:u32,pub inventory:Inventory}
impl StepInventory {
    pub fn new(generation:u32,mut inventory:Inventory,decrement:bool)->Self {
        if decrement {for life in &mut inventory.life { *life=life.saturating_sub(1); }}
        Self {generation,inventory}
    }
    pub fn expiring_request(&self,e:&FrameEvent)->bool {
        let slot=e.cut_index as usize;
        self.generation==e.generation && slot<self.inventory.count as usize
            && self.inventory.kinds[slot] as u32==e.other_snake_id
            && (1..=150).contains(&self.inventory.life[slot])
    }
    pub fn apply(&mut self,e:&FrameEvent,died:bool) {
        let stage=lifecycle(e);
        if !matches!(stage,ItemLifecycle::Stored|ItemLifecycle::HeldActivation|ItemLifecycle::Fizzle) {return;}
        if self.generation!=e.generation {*self=Self::new(e.generation,Inventory::default(),false);}
        let slot=e.cut_index as usize;let inv=&mut self.inventory;let n=inv.count as usize;
        if stage==ItemLifecycle::Stored {
            if slot==n && n<3 {inv.kinds[n]=e.other_snake_id as u8;inv.life[n]=1800;inv.count+=1;}
        } else if slot<n && inv.kinds[slot] as u32==e.other_snake_id
            && !(stage==ItemLifecycle::Fizzle && died && inv.life[slot]>0) {
            inv.kinds.copy_within(slot+1..n,slot);inv.life.copy_within(slot+1..n,slot);
            inv.count-=1;inv.kinds[n-1]=0;inv.life[n-1]=0;
        }
    }
    pub fn carries(&self,kind:u8,generation:u32)->bool {
        self.generation==generation && self.inventory.kinds[..self.inventory.count as usize].contains(&kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(kind:EventKind,item:u32,slot:u16)->FrameEvent {FrameEvent {kind,generation:7,other_snake_id:item,cut_index:slot,..Default::default()}}
    #[test]
    fn episodes_follow_activation_and_preserve_frost_storage_and_identity() {
        for kind in 1..=4 {for held in [false,true] {
            let mut episode=EffectEpisode::default();let activate=FrameEvent {flags:if held {event_flags::HELD_ACTIVATION} else {0},..event(EventKind::Pickup,kind,0)};
            assert!(episode.observe(&activate));assert!(!episode.use_once(kind as u8,8));
            for e in [event(EventKind::Stash,4,0),event(EventKind::Pickup,5,0),event(EventKind::Use,4,0),event(EventKind::Fizzle,4,0)] {assert!(!episode.observe(&e));}
            assert!(episode.use_once(kind as u8,7));assert!(!episode.use_once(kind as u8,7));
            episode.observe(&event(EventKind::Pickup,5,0));assert!(!episode.use_once(kind as u8,7));
            episode.observe(&activate);assert!(episode.use_once(kind as u8,7));
            episode.observe(&event(EventKind::EffectExpiry,kind,0));assert!(!episode.use_once(kind as u8,7));
        }}
    }
    #[test]
    fn feeding_precedes_reactivation_but_phase_and_bites_follow_it() {
        let mut episode=EffectEpisode::default();let magnet=event(EventKind::Pickup,2,0);
        episode.observe(&magnet);assert!(episode.use_once(2,7));
        // A later same-kind held activation must not erase or move the food
        // benefit to its new episode; that new episode remains unused.
        episode.observe(&FrameEvent {flags:event_flags::HELD_ACTIVATION,..magnet});
        assert!(!episode.used);
        episode.observe(&event(EventKind::Pickup,3,0));assert!(episode.use_once(3,7));
        episode.observe(&event(EventKind::Pickup,4,0));assert!(episode.use_once(4,7));
    }
    #[test]
    fn compacted_request_and_collision_inventory_follow_event_order() {
        let mut step=StepInventory::new(7,Inventory {kinds:[3,1,3],life:[1,1800,100],count:3,..Default::default()},true);
        step.apply(&event(EventKind::Fizzle,3,0),true);
        assert!(step.expiring_request(&event(EventKind::Use,3,1)));
        assert!(!step.expiring_request(&event(EventKind::Use,1,0)));
        step.apply(&FrameEvent {flags:event_flags::HELD_ACTIVATION,..event(EventKind::Pickup,3,1)},true);
        assert!(!step.carries(3,7));
        step.apply(&event(EventKind::Stash,3,1),true);assert!(step.carries(3,7));
        step.apply(&event(EventKind::ItemSpawn,1,0),true);
        step.apply(&event(EventKind::Fizzle,3,1),true);assert!(step.carries(3,7),"cap rejection retains collision inventory");
        assert!(!step.carries(3,8));
    }
    #[test]
    fn multiple_shelf_fizzles_compact_and_replacement_lives_start_empty() {
        let mut step=StepInventory::new(7,Inventory {kinds:[3,4,2],life:[1,1,151],count:3,..Default::default()},true);
        for kind in [3,4] {step.apply(&event(EventKind::Fizzle,kind,0),false);}
        assert_eq!(step.inventory.kinds,[2,0,0]);assert!(step.expiring_request(&event(EventKind::Use,2,0)));
        step.apply(&FrameEvent {generation:8,..event(EventKind::Stash,3,0)},true);
        assert_eq!(step.inventory.kinds,[3,0,0]);assert!(step.carries(3,8));
    }
    #[test]
    fn inventory_replay_matches_production_storage_fizzles_and_held_activation() {
        use snakes_core::{World,Config,RuleSet,Point,SnakeView,controller::{Controller,Steering}};
        struct Collector {use_now:bool}
        impl Controller for Collector {
            fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
                let angle=w.items().next().map_or(s.angle,|item| {let d=w.displacement(s.segments[0].current,item.position);d.y.atan2(d.x)});
                Steering {desired_angle:angle,rush:0.0}
            }
            fn use_request(&self,_:u32)->u32 {u32::from(self.use_now)}
        }
        let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,width:1600.0,height:1000.0,density:0.0,
            deadly_walls:false,self_collisions:false,world_events:false,seed:73,..Default::default()},
            &[(Point{x:800.0,y:500.0},0.0,30,0.0)],&[]).unwrap();
        let mut c=Collector {use_now:false};let (mut stored,mut fizzled,mut activated)=(0,0,0);
        for tick in 0..6000 {
            c.use_now=tick>=3000;
            let s=w.snake(0).unwrap();let mut replay=StepInventory::new(s.generation,s.inventory,true);
            w.step(&mut c);
            for e in w.frame_events().filter(|e|e.snake_id==0) {
                stored+=usize::from(lifecycle(e)==ItemLifecycle::Stored);
                fizzled+=usize::from(lifecycle(e)==ItemLifecycle::Fizzle);
                activated+=usize::from(lifecycle(e)==ItemLifecycle::HeldActivation);
                replay.apply(e,false);
            }
            let actual=w.snake(0).unwrap().inventory;
            assert_eq!((replay.inventory.kinds,replay.inventory.life,replay.inventory.count),(actual.kinds,actual.life,actual.count),"tick={tick}");
        }
        assert!(stored>0 && fizzled>0 && activated>0,"fixture must cover each transition: {stored}/{fizzled}/{activated}");
    }
    #[test]
    fn lifecycle_distinguishes_acquisition_use_and_drops() {
        for (kind,flags,expected) in [(EventKind::Stash,0,(true,false)),(EventKind::Pickup,0,(true,true)),(EventKind::Pickup,event_flags::HELD_ACTIVATION,(false,true))] {
            let stage=lifecycle(&FrameEvent {flags,..event(kind,4,0)});assert_eq!((stage.acquisition(),stage.activation()),expected);
        }
        assert_eq!(lifecycle(&event(EventKind::ItemSpawn,3,0)),ItemLifecycle::Drop);
        assert_eq!(lifecycle(&FrameEvent {snake_id:u32::MAX,..event(EventKind::ItemSpawn,3,0)}),ItemLifecycle::Spawn);
    }
}
