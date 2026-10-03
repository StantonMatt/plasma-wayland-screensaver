// SPDX-License-Identifier: GPL-3.0-or-later
//! Steering ends at World::feed_snakes' capture disk or pickup_items' contact
//! disk. Captured food is pulled/consumed by World without further pursuit;
//! Spark, Shard (including death fields), Pellet and Prism share that rule.
use crate::{effects, FoodView, Point, SnakeView, World};

pub(super) const ITEM_BIT: u64 = 1 << 63;

/// Cache outside rollout loops: no target searches or effect dispatch per step.
#[derive(Clone, Copy)]
pub(super) struct Contact {
    ordinary: f64,
    magnet: f64,
    effects: super::forecast::Track,
    claimed: bool,
    available: bool,
}
impl Contact {
    #[cfg(test)]
    #[inline]
    pub(super) fn new(w: &World, s: SnakeView<'_>, f: FoodView) -> Self {
        Self::forecast(s,f,super::forecast::Track::observed(w,s))
    }
    pub(super) fn forecast(s:SnakeView<'_>,f:FoodView,track:super::forecast::Track)->Self {
        let item = f.id & ITEM_BIT != 0;
        let ordinary = s.radius * if item { 1.3 } else { 3.0 } + f.size;
        let magnet = if !item {s.radius * effects::modifiers(effects::EffectKind::Magnet as u8,1).food_reach + f.size} else {ordinary};
        Self { ordinary, magnet, effects:track,
            claimed: !item && f.vacuum_owner == s.id as i32,
            available: item || f.vacuum_owner < 0 || f.vacuum_owner == s.id as i32 }
    }
    #[inline]
    pub(super) fn reach(self, step: usize) -> f64 {
        self.reach_with(self.effects.before(step))
    }
    #[inline]
    pub(super) fn reach_with(self,effect:super::forecast::Effect)->f64 {
        if effect.is(effects::EffectKind::Magnet) {self.magnet} else {self.ordinary}
    }
    pub(super) fn reached_with(self,distance_squared:f64,effect:super::forecast::Effect)->bool {
        self.available && (self.claimed || distance_squared<=self.reach_with(effect).powi(2))
    }
    pub(super) fn distance_with(self,distance:f64,effect:super::forecast::Effect)->f64 {
        if !self.available {f64::INFINITY} else if self.claimed {0.0} else {(distance-self.reach_with(effect)).max(0.0)}
    }
    /// Only an existing vacuum claim proves collection before movement.
    #[inline]
    pub(super) fn collected(self) -> bool { self.claimed || !self.available }
    #[inline]
    pub(super) fn reached(self, distance_squared: f64, step: usize) -> bool {
        self.available && (self.claimed || distance_squared <= self.reach(step).powi(2))
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
