// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use effects::{EffectKind, EndReason};
pub const MAX_ITEMS: usize = 3;
pub const ITEM_LIFE_TICKS: u16 = 750;
pub const ITEM_BLINK_TICKS: u16 = 90;
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Item {
    pub id: u64,
    pub kind: EffectKind,
    pub position: Point,
    pub age_ticks: u16,
    /// Remaining lifetime, in fixed physics ticks.
    pub life_ticks: u16,
    pub radius: f64,
}
impl Item {
    pub fn blinking(&self) -> bool { self.life_ticks <= ITEM_BLINK_TICKS }
}
impl World {
    pub fn items(&self) -> impl ExactSizeIterator<Item = &Item> { self.items.iter() }
    pub fn item_cap(&self) -> usize { ((self.snakes.len()+2)/4).clamp(1, MAX_ITEMS) }
    pub(super) fn items_enabled(&self) -> bool { self.config.rules == RuleSet::V2 && self.config.power_ups }
    pub(super) fn reset_item_timer(&mut self) { self.item_timer = 450 + (self.rng.random()*451.0) as u16; }
    pub(super) fn item_event(&mut self, item: Item, kind: EventKind, snake_id: u32) {
        // other_snake_id is the effect kind for item events, never a rival ID.
        self.push_event(FrameEvent {tick:self.tick.wrapping_add(1),position:item.position,
            snake_id,other_snake_id:item.kind as u32,color_index:0,kind});
    }
    pub(super) fn clear_items_and_effects(&mut self) {
        self.items.clear();
        self.item_timer = 0;
        self.event_count = 0; // No stale pickup/effect event after disabling.
        for id in 0..self.snakes.len() {
            let kind = EffectKind::from_byte(self.snakes[id].effect_kind);
            if kind != EffectKind::None { effects::end(kind,self,id,EndReason::Disabled); }
            self.snakes[id].effect_kind = 0;
            self.snakes[id].effect_ticks = 0;
        }
    }
    pub(super) fn advance_items_and_effects(&mut self) {
        for id in 0..self.snakes.len() {
            if !self.snakes[id].alive || self.snakes[id].effect_ticks == 0 { continue; }
            let kind = EffectKind::from_byte(self.snakes[id].effect_kind);
            self.snakes[id].effect_ticks -= 1;
            if self.snakes[id].effect_ticks == 0 {
                effects::end(kind,self,id,EndReason::Expired);
                self.snakes[id].effect_kind = 0;
                let item=Item {kind,position:self.segments[id*MAX_SEGMENTS].current,..Item::default()};
                self.item_event(item,EventKind::EffectExpiry,id as u32);
            } else { effects::tick(kind,self,id); }
        }
        let mut i=0;
        while i<self.items.len() {
            self.items[i].age_ticks += 1;
            self.items[i].life_ticks -= 1;
            if self.items[i].life_ticks == 0 {
                let item=self.items.remove(i);
                self.item_event(item,EventKind::ItemExpiry,u32::MAX);
            } else { i+=1; }
        }
        self.item_timer = self.item_timer.saturating_sub(1);
        if self.item_timer == 0 {
            if self.items.len()<self.item_cap() { self.spawn_item(); }
            self.reset_item_timer();
        }
    }
    pub(super) fn spawn_item(&mut self) {
        let r=self.config.base_radius();
        let margin=if self.config.deadly_walls {10.0*r} else {0.0};
        // Tiny deadly arenas may have no valid candidate; retry next timer.
        if self.config.width<margin*2.0 || self.config.height<margin*2.0 { return; }
        let mut best=None;let mut clearance=-1.0_f64;
        for _ in 0..12 {
            let p=Point {x:margin+self.rng.random()*(self.config.width-margin*2.0),
                y:margin+self.rng.random()*(self.config.height-margin*2.0)};
            // Existing items and spawn candidates are stationary. Body
            // clearance below exhaustively scans every continuous body edge;
            // there is no spatial reject that could omit a moving occupant.
            let spacing = Self::sweep_search_radius(20.0*r, 0.0);
            if self.items.iter().any(|item|self.distance_squared(p,item.position)<spacing.powi(2)) {continue;}
            let mut distance=f64::MAX;
            for (id,s) in self.snakes.iter().enumerate() {
                if !s.alive {continue;}
                let body=&self.segments[id*MAX_SEGMENTS..id*MAX_SEGMENTS+s.len];
                let mut nearest=self.distance_squared(p,body[0].current);
                for edge in body.windows(2) {
                    nearest=nearest.min(self.segment_distance_squared(p,edge[0].current,edge[1].current));
                }
                distance=distance.min(nearest.sqrt()-s.radius);
            }
            if best.is_none() || distance>clearance {best=Some(p);clearance=distance;}
        }
        let Some(position)=best else {return;};
        let total: u32=effects::ENABLED_KINDS.iter().filter(|&&k|k!=self.last_item_kind).map(|k|k.weight()).sum();
        if total==0 {return;}
        let mut roll=(self.rng.random()*total as f64) as u32;
        let mut kind=EffectKind::None;
        for &k in effects::ENABLED_KINDS {
            if k==self.last_item_kind {continue;}
            if roll<k.weight() {kind=k;break;}
            roll-=k.weight();
        }
        let item=Item {id:self.next_item,kind,position,age_ticks:0,life_ticks:ITEM_LIFE_TICKS,radius:2.1*r};
        self.next_item=self.next_item.wrapping_add(1);
        self.last_item_kind=kind;
        self.items.push(item);
        self.item_event(item,EventKind::ItemSpawn,u32::MAX);
    }
    pub(super) fn pickup_items(&mut self) {
        if self.items.is_empty() { return; }
        let g = self.config.geometry();
        let head_sweeps = self.head_sweeps();
        let mut i=0;
        while i<self.items.len() {
            let item=self.items[i];
            let owner=self.snakes.iter().enumerate().find(|(id,s)| {
                if !s.alive { return false; }
                let reach = 1.3*s.radius+item.radius;
                let search = Self::sweep_search_radius(reach, head_sweeps[*id]);
                let d = g.delta(item.position, self.segments[id*MAX_SEGMENTS].current);
                if d.x.abs()>search || d.y.abs()>search { return false; }
                // Pickup remains endpoint contact, including the <= boundary.
                d.x*d.x+d.y*d.y<=reach.powi(2)
            }).map(|(id,_)|id);
            if let Some(id)=owner {
                let old=EffectKind::from_byte(self.snakes[id].effect_kind);
                if old!=EffectKind::None {effects::end(old,self,id,EndReason::Replaced);}
                self.snakes[id].effect_kind=item.kind as u8;
                self.snakes[id].effect_ticks=item.kind.duration();
                effects::activate(item.kind,self,id);
                self.items.remove(i);
                self.item_event(item,EventKind::Pickup,id as u32);
            } else {i+=1;}
        }
    }
}
#[cfg(test)]
mod tests;
