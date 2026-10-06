// SPDX-License-Identifier: GPL-3.0-or-later
//! Fixed slots, tick clocks and deterministic drops; no RNG or allocation.
use super::*;
use effects::{EffectKind,EndReason};
pub const INVENTORY_SLOTS:usize=3;
pub const INVENTORY_WINDUP_TICKS:u8=4;
#[derive(Clone,Copy,Debug,Default)]
pub struct Inventory {
    pub kinds:[u8;3], pub life:[u16;3], pub count:u8,
    /// Selected slot+1, retained until activation (also retained on death).
    pub windup:u8, pub windup_ticks:u8, pub cooldown:u8,
}
impl Inventory {
    pub fn life_bytes(self)->[u8;3] {self.life.map(|t|((t.min(150) as u32*255+149)/150) as u8)}
    fn remove(&mut self,slot:usize) {
        let n=self.count as usize;
        self.kinds.copy_within(slot+1..n,slot);self.life.copy_within(slot+1..n,slot);
        self.count-=1;self.kinds[self.count as usize]=0;self.life[self.count as usize]=0;
        if self.windup as usize==slot+1 {self.windup=0;self.windup_ticks=0;}
        else if self.windup as usize>slot+1 {self.windup-=1;}
    }
}
impl World {
    pub(super) fn inventory_event(&mut self,id:usize,slot:usize,kind:EffectKind,event:EventKind,p:Point,duration:u16) {
        self.inventory_event_at(self.tick+1,id,slot,kind,event,p,duration);
    }
    fn inventory_event_at(&mut self,tick:u64,id:usize,slot:usize,kind:EffectKind,event:EventKind,p:Point,duration:u16) {
        self.push_event(FrameEvent {tick,position:p,snake_id:id as u32,
            generation:self.snakes[id].generation,other_snake_id:kind as u32,
            color_index:self.snakes[id].color,kind:event,flags:if event==EventKind::Pickup {event_flags::HELD_ACTIVATION} else {0},cut_index:slot as u16,
            duration_ticks:duration,..Default::default()});
    }
    pub(super) fn activate_item(&mut self,id:usize,kind:EffectKind) {
        if kind!=EffectKind::Frost {
            let old=EffectKind::from_byte(self.snakes[id].effect_kind);
            if old!=EffectKind::None {effects::end(old,self,id,EndReason::Replaced);}
            self.snakes[id].effect_kind=kind as u8;self.snakes[id].effect_ticks=kind.duration();
        }
        effects::activate(kind,self,id);
        let glyph=match kind {EffectKind::Surge=>Glyph::Surge,EffectKind::Magnet=>Glyph::Magnet,
            EffectKind::Phase=>Glyph::Phase,EffectKind::Venom=>Glyph::Venom,EffectKind::Frost=>Glyph::Frost,_=>return};
        self.emit_bubble(id,glyph);
    }
    pub(super) fn request_inventory_use(&mut self,id:usize,action:u32) {
        if !self.items_enabled() || !self.config.store_power_ups {return;}
        let inv=&mut self.snakes[id].inventory;
        if action==0 || action>inv.count as u32 || inv.windup!=0 || inv.cooldown!=0 {return;}
        inv.windup=action as u8;inv.windup_ticks=INVENTORY_WINDUP_TICKS;inv.cooldown=30;
        let kind=EffectKind::from_byte(inv.kinds[action as usize-1]);
        self.inventory_event(id,action as usize-1,kind,EventKind::Use,self.segments[id*MAX_SEGMENTS].current,4);
    }
    pub(super) fn advance_inventory(&mut self) {
        for id in 0..self.snakes.len() {
            if !self.snakes[id].alive {continue;}
            let inv=&mut self.snakes[id].inventory;inv.cooldown=inv.cooldown.saturating_sub(1);
            inv.windup_ticks=inv.windup_ticks.saturating_sub(1);
            let mut slot=0;
            while slot<(self.snakes[id].inventory.count as usize) {
                let inv=&mut self.snakes[id].inventory;
                inv.life[slot]=inv.life[slot].saturating_sub(1);
                if inv.life[slot]==0 {
                    let kind=EffectKind::from_byte(inv.kinds[slot]);inv.remove(slot);
                    self.inventory_event(id,slot,kind,EventKind::Fizzle,self.inventory_position(id,slot).0,8);
                } else {slot+=1;}
            }
        }
    }
    pub(super) fn activate_inventory(&mut self) {
        for id in 0..self.snakes.len() {
            if !self.snakes[id].alive {continue;}
            let inv=&mut self.snakes[id].inventory;
            if inv.windup!=0 && inv.windup_ticks==0 {
                let slot=inv.windup as usize-1;let kind=EffectKind::from_byte(inv.kinds[slot]);
                inv.remove(slot);self.activate_item(id,kind);
                self.inventory_event(id,slot,kind,EventKind::Pickup,self.segments[id*MAX_SEGMENTS].current,0);
            }
        }
    }
    pub(super) fn fizzle_inventories(&mut self) {
        for id in 0..self.snakes.len() {
            for slot in (0..self.snakes[id].inventory.count as usize).rev() {
                let kind=EffectKind::from_byte(self.snakes[id].inventory.kinds[slot]);
                // Reconfiguration exports immediately without completing a physics tick.
                self.inventory_event_at(self.tick,id,slot,kind,EventKind::Fizzle,self.inventory_position(id,slot).0,8);
            }
            self.snakes[id].inventory=Inventory::default();
        }
    }
    fn inventory_position(&self,id:usize,slot:usize)->(Point,Point) {
        let s=&self.snakes[id];let mut left=[2.2,4.3,6.4][slot]*s.radius;
        let mut p=self.segments[id*MAX_SEGMENTS].current;
        let mut tangent=Point{x:s.angle.cos(),y:s.angle.sin()};
        for edge in self.segments[id*MAX_SEGMENTS..id*MAX_SEGMENTS+s.len].windows(2) {
            let d=self.displacement(edge[0].current,edge[1].current);let len=(d.x*d.x+d.y*d.y).sqrt();
            if len<=1e-9 {continue;}
            tangent=Point{x:-d.x/len,y:-d.y/len};
            if left<=len {return (self.canonical_point(Point{x:edge[0].current.x+d.x*left/len,y:edge[0].current.y+d.y*left/len}),tangent);}
            left-=len;p=edge[1].current;
        }
        (p,tangent)
    }
    pub(super) fn drop_inventory(&mut self,id:usize) {
        let inv=self.snakes[id].inventory;
        for slot in 0..inv.count as usize {
            let kind=EffectKind::from_byte(inv.kinds[slot]);let (p,t)=self.inventory_position(id,slot);
            if self.items.len()>=self.item_cap() {
                self.inventory_event(id,slot,kind,EventKind::Fizzle,p,8);continue;
            }
            let r=3.0*self.snakes[id].radius;
            let d=match slot {0=>Point{x:-t.y*r,y:t.x*r},1=>Point{x:t.y*r,y:-t.x*r},_=>Point{x:-t.x*r,y:-t.y*r}};
            let mut p=self.canonical_point(Point{x:p.x+d.x,y:p.y+d.y});
            if self.config.deadly_walls {p.x=p.x.clamp(0.0,self.config.width);p.y=p.y.clamp(0.0,self.config.height);}
            let item=Item {id:self.next_item,kind,position:p,radius:2.1*self.config.base_radius(),
                life_ticks:300,pickable_from_tick:self.tick+1+15,dropped:true,
                leader_snake_id:u32::MAX,leader_eta:f32::INFINITY,contender_ids:[u32::MAX;MAX_CONTENDERS],
                contender_etas:[f32::INFINITY;MAX_CONTENDERS],guard_snake_id:u32::MAX,owner_snake_id:u32::MAX,..Default::default()};
            self.next_item=self.next_item.wrapping_add(1);self.items.push(item);
            self.inventory_event(id,slot,kind,EventKind::ItemSpawn,p,15);
        }
        self.snakes[id].inventory=Inventory::default();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn arena()->World {World::diagnostic_arena(Config {rules:RuleSet::V2,width:4000.0,height:2000.0,density:0.0,
        deadly_walls:false,self_collisions:false,..Default::default()},&[(Point{x:1500.0,y:1000.0},0.0,30,0.0)],&[]).unwrap()}
    fn stash(w:&mut World,kind:EffectKind) {w.items.push(Item {kind,position:w.segments[0].current,
        radius:2.1*w.config.base_radius(),life_ticks:750,..Default::default()});w.pickup_items();}
    #[test]
    fn inventory_stacks_winds_up_spaces_and_compacts() {
        let mut w=arena();for kind in [EffectKind::Phase,EffectKind::Phase,EffectKind::Surge] {stash(&mut w,kind);}
        assert_eq!(w.snakes[0].inventory.kinds,[3,3,1]);assert_eq!(w.snakes[0].effect_ticks,0);
        assert_eq!(w.frame_events().filter(|e|e.kind==EventKind::Stash).count(),3);
        w.request_inventory_use(0,2);assert_eq!(w.snakes[0].inventory.windup,2);
        for _ in 0..3 {w.advance_items_and_effects();w.pickup_items();assert_eq!(w.snakes[0].effect_kind,0);}
        w.advance_items_and_effects();w.pickup_items();assert_eq!(w.snakes[0].effect_kind,3);
        assert_eq!(w.snakes[0].effect_ticks,120);assert_eq!(w.snakes[0].inventory.kinds,[3,1,0]);
        assert_eq!(w.snakes[0].inventory.windup,0);w.request_inventory_use(0,1);assert_eq!(w.snakes[0].inventory.windup,0);
        for _ in 0..26 {w.advance_items_and_effects();w.pickup_items();}
        w.request_inventory_use(0,1);assert_eq!(w.snakes[0].inventory.windup,1);
    }
    #[test]
    fn inventory_full_touch_frost_preserves_effect_and_toggle_fizzles() {
        let mut w=arena();w.faces[0].guarding=true;for _ in 0..3 {stash(&mut w,EffectKind::Phase);}
        stash(&mut w,EffectKind::Surge);assert_eq!(w.snakes[0].effect_kind,1);assert_eq!(w.snakes[0].inventory.count,3);
        stash(&mut w,EffectKind::Frost);assert_eq!(w.snakes[0].effect_kind,1);assert_eq!(w.snakes[0].effect_ticks,180);
        let rng=w.rng_state();w.reconfigure(Config {store_power_ups:false,..w.config()}).unwrap();
        assert_eq!(w.snakes[0].inventory.count,0);assert_eq!(w.rng_state(),rng);
        assert_eq!(w.frame_events().filter(|e|e.kind==EventKind::Fizzle).count(),3);
        stash(&mut w,EffectKind::Magnet);assert_eq!(w.snakes[0].effect_kind,2);
    }
    #[test]
    fn inventory_shelf_life_drain_expiry_and_death_drop_cap() {
        let mut w=arena();stash(&mut w,EffectKind::Phase);w.snakes[0].inventory.life[0]=151;
        assert_eq!(w.snakes[0].inventory.life_bytes()[0],255);
        for _ in 0..150 {w.advance_items_and_effects();w.pickup_items();}
        assert_eq!(w.snakes[0].inventory.count,1);assert_eq!(w.snakes[0].inventory.life_bytes()[0],2);
        w.advance_items_and_effects();assert_eq!(w.snakes[0].inventory.count,0);
        for _ in 0..3 {stash(&mut w,EffectKind::Phase);}
        w.request_inventory_use(0,2);let rng=w.rng_state();w.drop_inventory(0);
        assert_eq!(w.items.len(),w.item_cap());assert_eq!(w.items[0].life_ticks,300);
        assert_eq!(w.items[0].pickable_from_tick,w.tick+16);assert!(w.items[0].dropped);
        assert_eq!(w.rng_state(),rng);assert_eq!(w.snakes[0].inventory.count,0);
        assert_eq!(w.frame_events().filter(|e|e.kind==EventKind::Fizzle).count(),3);
    }
}
