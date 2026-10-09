// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use effects::{EffectKind, EndReason};
/// Three capsules plus the future single Whirlpool vortex record.
pub const MAX_ITEMS: usize = 4;
pub const MAX_CAPSULES:usize = 3;
pub const ITEM_LANDING_TICKS:u16 = 30;
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
    /// Absolute completed tick at which pickup first becomes legal; zero for fixtures.
    pub pickable_from_tick:u64,
    pub leader_snake_id:u32,
    pub leader_eta:f32,
    pub contender_count:u8,
    pub contender_ids:[u32;MAX_CONTENDERS],
    pub contender_etas:[f32;MAX_CONTENDERS],
    pub guard_snake_id:u32,
    /// Reserved Whirlpool state: charge ticks, captured value and owner generation.
    pub charge_ticks:u16,
    pub captured_value:f32,
    pub owner_generation:u32,
    pub owner_snake_id:u32,
    pub dropped:bool,
}
impl Item {
    pub(crate) const GUARD_RELEASE_TICKS:u16=30;
    /// Shared participant gate for mechanics, forecasts and target contact.
    #[inline]
    pub(crate) fn pickup_allowed(alive:bool,guarding:bool,effect_ticks:u16)->bool {
        alive && !(guarding && effect_ticks>Self::GUARD_RELEASE_TICKS)
    }
    #[inline]
    pub(crate) fn endpoint_eligible(tick:u64,ready:u64,elapsed:usize,life:usize,alive:bool)->bool {
        alive && tick>=ready && elapsed<life
    }
    pub fn blinking(&self) -> bool { self.life_ticks <= ITEM_BLINK_TICKS }
    /// Eligibility at an endpoint, before contact/lowest-ID arbitration.
    /// `elapsed` counts timer decrements since the post-decrement observation
    /// (zero in mechanics and movement one); effect ticks use that endpoint's
    /// current effect, including any earlier pickup in the same item pass.
    #[inline]
    pub(crate) fn pickup_eligible(&self, tick:u64, elapsed:usize, alive:bool, guarding:bool, effect_ticks:u16)->bool {
        Self::endpoint_eligible(tick,self.pickable_from_tick,elapsed,self.life_ticks as usize,alive)
            && Self::pickup_allowed(alive,guarding && self.kind!=EffectKind::Flip,effect_ticks)
    }
}
impl World {
    pub fn items(&self) -> impl ExactSizeIterator<Item = &Item> { self.items.iter() }
    pub fn item_cap(&self) -> usize { ((self.snakes.len()+2)/4).clamp(1, MAX_CAPSULES) }
    pub(super) fn items_enabled(&self) -> bool { self.config.rules == RuleSet::V2 && self.config.power_ups }
    pub(super) fn reset_item_timer(&mut self) { self.item_timer = 450 + (self.rng.random()*451.0) as u16; }
    pub(super) fn item_event(&mut self, item: Item, kind: EventKind, snake_id: u32) {
        // other_snake_id is the effect kind for item events, never a rival ID.
        self.push_event(FrameEvent {tick:self.tick.wrapping_add(1),position:item.position,
            snake_id,generation:self.snakes.get(snake_id as usize).map_or(0,|s|s.generation),other_snake_id:item.kind as u32,color_index:0,kind,..FrameEvent::default()});
    }
    pub(super) fn clear_items_and_effects(&mut self) {
        self.event_count=0; // Clear stale effects before publishing inventory fizzles.
        self.fizzle_inventories();
        self.items.clear();
        self.food.retain(|f| !matches!(f.kind,FoodKind::Prism|FoodKind::PrismSeed));
        self.prism_timer=0;
        for face in &mut self.faces {face.bulges=[Bulge::default();2];}
        self.item_timer = 0;
        for id in 0..self.snakes.len() {
            self.snakes[id].frozen_ticks=0;
            self.faces[id].frozen_ticks=0;self.faces[id].thaw_immunity_ticks=0;self.faces[id].breath_ticks=0;
            let kind = EffectKind::from_byte(self.snakes[id].effect_kind);
            if kind != EffectKind::None { effects::end(kind,self,id,EndReason::Disabled); }
            self.snakes[id].effect_kind = 0;
            self.snakes[id].effect_ticks = 0;
        }
    }
    pub(super) fn advance_items_and_effects(&mut self) {
        self.advance_frost();
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
        self.advance_inventory();
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
    pub(super) fn item_spawn_location(&mut self) -> Option<Point> {
        self.prize_spawn_location(false)
    }
    pub(super) fn prize_spawn_location(&mut self,prism:bool) -> Option<Point> {
        let r=self.config.base_radius();
        let margin=if self.config.deadly_walls {10.0*r} else {0.0};
        // Tiny deadly arenas may have no valid candidate; retry next timer.
        if self.config.width<margin*2.0 || self.config.height<margin*2.0 { return None; }
        let mut best=None;let mut clearance=f64::NEG_INFINITY;
        let available=self.snakes.iter().enumerate().filter(|(id,s)|s.alive && self.faces[*id].target_id==0).count();
        let free_heads=prism && available>0;
        let live=self.snakes.iter().enumerate().filter(|(id,s)|s.alive && (!free_heads || self.faces[*id].target_id==0)).count();
        for _ in 0..12 {
            // Both modes consume exactly twelve XY pairs. Capsules retain
            // their historical uniform, maximum-clearance placement. A seed
            // needs an early arrival within its three-second announcement.
            let x=self.rng.random();let y=self.rng.random();
            let mut p=Point {x:margin+x*(self.config.width-margin*2.0),
                y:margin+y*(self.config.height-margin*2.0)};
            if prism && live>0 {
                let (id,s)=self.snakes.iter().enumerate().filter(|(id,s)|s.alive && (!free_heads || self.faces[*id].target_id==0))
                    .nth(((x*live as f64) as usize).min(live-1)).unwrap();
                let (speed,turn)=self.motion_limits(id,0.0).unwrap();
                let orbit=(5.5*s.radius).max(1.35*speed/turn.max(0.01)).max(speed*0.7)
                    .max((s.len as f64*s.radius*1.18).min(speed*3.0)/std::f64::consts::TAU*1.1);
                let head=self.segments[id*MAX_SEGMENTS].current;
                // Announce a point on the first reachable arc just after
                // ripening. This avoids an unripe first pass and a second lap
                // through our own trail. Prefer heads free of capsule duties.
                let phase=speed*(3.1+0.4*y)/orbit;
                let forward=orbit*phase.sin();
                let side=orbit*(1.0-phase.cos())*if y<0.5 {-1.0} else {1.0};
                p=self.canonical_point(Point {x:head.x+s.angle.cos()*forward-s.angle.sin()*side,
                    y:head.y+s.angle.sin()*forward+s.angle.cos()*side});
                if self.config.deadly_walls {
                    p.x=p.x.clamp(margin,self.config.width-margin);
                    p.y=p.y.clamp(margin,self.config.height-margin);
                }
            }
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
            let score=if prism {
                // Reject occupied seeds, then favor two physically reachable
                // heads. No body/safety rules or pickup eligibility change.
                if distance<6.0*r {continue;}
                let mut etas=[f64::INFINITY;2];
                for (id,s) in self.snakes.iter().enumerate().filter(|(_,s)|s.alive) {
                    let (speed,turn)=self.motion_limits(id,0.0).unwrap();
                    let d=self.displacement(self.segments[id*MAX_SEGMENTS].current,p);
                    let length=(d.x*d.x+d.y*d.y).sqrt();
                    let bearing=normalize_angle(d.y.atan2(d.x)-s.angle).abs();
                    let radius=speed/turn.max(0.01);
                    let eta=(length-3.0*s.radius-0.62*r).max(0.0)/speed.max(1.0)
                        +bearing/turn.max(0.01)*0.6
                        +if bearing>1.2 && length<2.0*radius {(bearing-1.2)/turn.max(0.01)} else {0.0};
                    if eta<etas[0] {etas[1]=etas[0];etas[0]=eta;}
                    else if eta<etas[1] {etas[1]=eta;}
                }
                -40.0*etas[1].min(30.0)-20.0*etas[0].min(30.0)+distance.min(4.0*r)*0.05
            } else {distance};
            if best.is_none() || score>clearance {best=Some(p);clearance=score;}
        }
        best
    }
    pub(super) fn spawn_item(&mut self) {
        let r=self.config.base_radius();
        let Some(position)=self.item_spawn_location() else {return;};
        let total: u32=effects::ENABLED_KINDS.iter().filter(|&&k|k!=self.last_item_kind).map(|k|k.weight()).sum();
        if total==0 {return;}
        let mut roll=(self.rng.random()*total as f64) as u32;
        let mut kind=EffectKind::None;
        for &k in effects::ENABLED_KINDS {
            if k==self.last_item_kind {continue;}
            if roll<k.weight() {kind=k;break;}
            roll-=k.weight();
        }
        let item=Item {id:self.next_item,kind,position,age_ticks:0,life_ticks:ITEM_LIFE_TICKS+ITEM_LANDING_TICKS,radius:2.1*r,
            pickable_from_tick:self.tick+1+ITEM_LANDING_TICKS as u64,leader_snake_id:u32::MAX,
            leader_eta:f32::INFINITY,contender_ids:[u32::MAX;MAX_CONTENDERS],
            contender_etas:[f32::INFINITY;MAX_CONTENDERS],guard_snake_id:u32::MAX,owner_snake_id:u32::MAX,..Item::default()};
        self.next_item=self.next_item.wrapping_add(1);
        self.last_item_kind=kind;
        self.items.push(item);
        self.item_event(item,EventKind::ItemSpawn,u32::MAX);
    }
    pub(super) fn pickup_items(&mut self) {
        self.activate_inventory();
        if self.items.is_empty() { return; }
        let g = self.config.geometry();
        let head_sweeps = self.head_sweeps();
        let mut i=0;
        while i<self.items.len() {
            let item=self.items[i];
            if !item.pickup_eligible(self.tick+1,0,true,false,0) {i+=1;continue;}
            let owner=self.snakes.iter().enumerate().find(|(id,s)| {
                if !item.pickup_eligible(self.tick+1,0,s.alive,self.faces[*id].guarding && !self.config.store_power_ups,s.effect_ticks) {return false;}
                let reach = 1.3*s.radius+item.radius;
                let search = Self::sweep_search_radius(reach, head_sweeps[*id]);
                let d = g.delta(item.position, self.segments[id*MAX_SEGMENTS].current);
                if d.x.abs()>search || d.y.abs()>search { return false; }
                // Pickup remains endpoint contact, including the <= boundary.
                d.x*d.x+d.y*d.y<=reach.powi(2)
            }).map(|(id,_)|id);
            if let Some(id)=owner {
                self.resolve_denial(item,id);
                self.faces[id].happy_ticks=45;
                self.items.remove(i);
                if (self.config.store_power_ups || item.kind==EffectKind::Flip) && self.snakes[id].inventory.count<3 {
                    let slot=self.snakes[id].inventory.count as usize;
                    let inv=&mut self.snakes[id].inventory;
                    inv.kinds[slot]=item.kind as u8;inv.life[slot]=1800;inv.count+=1;
                    self.inventory_event(id,slot,item.kind,EventKind::Stash,item.position,0);
                } else {
                    if self.config.store_power_ups || item.kind==EffectKind::Flip {self.inventory_event(id,usize::MAX,item.kind,EventKind::Use,item.position,0);}
                    self.activate_item(id,item.kind);
                    self.item_event(item,EventKind::Pickup,id as u32);
                }
            } else {i+=1;}
        }
    }
}
#[cfg(test)]
mod tests;
