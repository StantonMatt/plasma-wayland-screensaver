// SPDX-License-Identifier: GPL-3.0-or-later
//! Shared single-burst forecasts. Steering sees post-decrement counters; public
//! diagnostics enter before decrement. No future food or new bursts assumed.
use super::{EffectKind, modifiers, remaining_ticks};
use crate::World;

/// First movement's burst state, in the same order as World::advance_boost.
/// Price is latched here, so expiry cannot charge a burst started for free.
#[inline]
pub(crate) fn forecast_boost(w: &World, id: usize, rush: f64) -> (usize, usize, usize, u8) {
    forecast_boost_from(&w.snakes[id], rush)
}

#[inline]
fn forecast_boost_from(s: &crate::world::Snake, rush: f64) -> (usize, usize, usize, u8) {
    let effect = modifiers(s.effect_kind, remaining_ticks(s.effect_ticks, 1));
    let frozen = s.effect_kind == EffectKind::Frost as u8 && remaining_ticks(s.effect_ticks, 1) > 0;
    let mut ticks = s.boost_ticks as usize;
    let mut cooldown = s.cooldown_ticks;
    let mut cost = s.boost_cost as usize;
    let mut paid = s.boost_paid as usize;
    if (frozen && ticks > 0) || ticks == 1 { ticks = 0; cooldown = effect.boost_cooldown; }
    else if ticks > 1 { ticks -= 1; }
    else { cooldown = cooldown.saturating_sub(1); }
    if rush > 0.0 && !frozen && ticks == 0 && cooldown == 0 && s.len >= 12 {
        ticks = 24;
        cost = if effect.free_boost { 0 } else { 2 + s.len / 100 };
        paid = 0;
    }
    (ticks, cost, paid, cooldown)
}

#[inline]
pub(crate) fn forecast_state(w: &World, id: usize, rush: f64, offset: usize) -> crate::world::Snake {
    forecast_state_from(w, w.snakes[id], rush, offset)
}

#[inline]
fn forecast_state_from(w: &World, mut s: crate::world::Snake, rush: f64, offset: usize) -> crate::world::Snake {
    let (ticks, cost, paid, cooldown) = forecast_boost_from(&s, rush);
    let due = if ticks > 0 { cost * (25 - ticks + offset.min(21)).min(21) / 21 } else { paid };
    let payment = due.saturating_sub(paid);
    s.len = s.len.saturating_sub(payment);
    if payment > 0 { World::update_radius(&mut s); }
    s.boost_ticks = ticks.saturating_sub(offset).min(u8::MAX as usize) as u8;
    s.boost_cost = cost as u8;
    s.boost_paid = due as u8;
    s.cooldown_ticks = if ticks > 0 && offset >= ticks {
        let expiry = modifiers(s.effect_kind, remaining_ticks(s.effect_ticks, ticks + 1));
        expiry.boost_cooldown.saturating_sub((offset - ticks).min(u8::MAX as usize) as u8)
    } else if ticks > 0 { cooldown }
    else { cooldown.saturating_sub(offset.min(u8::MAX as usize) as u8) };
    s.effect_ticks = remaining_ticks(s.effect_ticks, offset.saturating_add(1));
    if s.effect_ticks == 0 { s.effect_kind = EffectKind::None as u8; }
    s.rush = if offset < ticks { 0.6 } else { 0.0 };
    s.blocked = s.len >= w.maximum_snake_segments(&s) || w.growth_slots == 0;
    s
}

pub(crate) fn forecast_motion(w: &World, id: usize, rush: f64, offset: usize) -> (f64, f64) {
    let s = forecast_state(w, id, rush, offset);
    (w.speed(&s), w.turn_rate(&s))
}

/// AI movement after forecast replacement, without changing mechanics.
/// Surge activation forgives unpaid burst segments at the pickup movement.
impl World {
    pub(crate) fn ai_forecast_radius(&self,id:usize,rush:f64,offset:usize,surge_step:Option<usize>)->f64 {
        self.ai_forecast_state(id,rush,offset,surge_step).radius
    }
    fn ai_forecast_state(&self,id:usize,rush:f64,offset:usize,surge_step:Option<usize>)->crate::world::Snake {
        let mut state=forecast_state(self,id,rush,offset);
        if let Some(step)=surge_step {
            let at_pickup=forecast_state(self,id,rush,step-1);
            state.len=at_pickup.len;state.radius=at_pickup.radius;
            state.boost_cost=at_pickup.boost_paid;state.boost_paid=at_pickup.boost_paid;
            state.blocked=state.len>=self.maximum_snake_segments(&state) || self.growth_slots==0;
        }
        state
    }
    pub(crate) fn ai_forecast_motion(&self,id:usize,rush:f64,offset:usize,kind:u8,ticks:u16,surge_step:Option<usize>)->(f64,f64) {
        let mut state=self.ai_forecast_state(id,rush,offset,surge_step);
        state.effect_kind=kind;state.effect_ticks=ticks;
        (self.speed(&state),self.turn_rate(&state))
    }
}

/// Adapt a pre-tick diagnostic observation to the steering forecast's input.
/// Decrement only the effect here: burst advancement belongs to the shared
/// single-burst forecast, and its new price must be latched on movement one.
pub(crate) fn forecast_motion_before_tick(w: &World, id: usize, rush: f64, offset: usize) -> (f64, f64) {
    let s = forecast_state_before_tick(w, id, rush, offset);
    (w.speed(&s), w.turn_rate(&s))
}

/// Reuse limits until payment, boost expiry or effect expiry changes motion.
pub(crate) fn forecast_schedule_before_tick(w: &World, id: usize, rush: f64) -> [(f64, f64); 25] {
    let mut previous = forecast_state_before_tick(w, id, rush, 0);
    let first = (w.speed(&previous), w.turn_rate(&previous));
    let mut schedule = [first; 25];
    for offset in 1..25 {
        let state = forecast_state_before_tick(w, id, rush, offset);
        let changed = state.len != previous.len || state.rush != previous.rush
            || modifiers(state.effect_kind,state.effect_ticks).speed
                != modifiers(previous.effect_kind,previous.effect_ticks).speed;
        schedule[offset] = if changed { (w.speed(&state), w.turn_rate(&state)) }
            else { schedule[offset - 1] };
        previous = state;
    }
    schedule
}

fn forecast_state_before_tick(w: &World, id: usize, rush: f64, offset: usize) -> crate::world::Snake {
    let mut observed = w.snakes[id];
    observed.effect_ticks = observed.effect_ticks.saturating_sub(1);
    if observed.effect_ticks == 0 { observed.effect_kind = EffectKind::None as u8; }
    forecast_state_from(w, observed, rush, offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Config, Point, RuleSet};

    #[test]
    fn pre_tick_forecasts_latch_price_and_phase_expiry_like_mechanics() {
        for kind in [EffectKind::Surge, EffectKind::Magnet, EffectKind::Phase, EffectKind::Frost] {
            for ticks in [1, 2, 8, 24, 25, 26, 36, 100] {
                for burst in [0, 1, 2, 24] {
                    for cooldown in [0, 1, 2, 18, 36] {
                        let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,
                            density:0.0,self_collisions:false,..Config::default()},
                            &[(Point{x:500.0,y:400.0},0.0,120,0.0)],&[]).unwrap();
                        w.snakes[0].effect_kind=kind as u8;
                        w.snakes[0].effect_ticks=ticks;
                        w.snakes[0].boost_ticks=burst;
                        w.snakes[0].cooldown_ticks=cooldown;
                        let schedule=w.forecast_motion_schedule(0,0.6).unwrap();
                        for offset in 0..139 {
                            assert_eq!(w.forecast_motion_limits(0,0.6,offset).unwrap(),
                                forecast_motion_before_tick(&w,0,0.6,offset),
                                "public scalar kind={kind:?} ticks={ticks} burst={burst} cooldown={cooldown} offset={offset}");
                        }
                        let predicted:Vec<_>=(0..139).map(|offset|
                            (forecast_state_before_tick(&w,0,0.6,offset),
                             forecast_motion_before_tick(&w,0,0.6,offset))).collect();
                        for (offset,(state,motion)) in predicted.into_iter().enumerate() {
                            w.advance_items_and_effects();
                            w.advance_boost(0,offset==0);
                            let actual=w.snakes[0];
                            assert_eq!((state.effect_kind,state.effect_ticks,state.boost_ticks,
                                state.cooldown_ticks,state.boost_cost,state.boost_paid,state.len),
                                (actual.effect_kind,actual.effect_ticks,actual.boost_ticks,
                                actual.cooldown_ticks,actual.boost_cost,actual.boost_paid,actual.len),
                                "kind={kind:?} ticks={ticks} burst={burst} cooldown={cooldown} offset={offset}");
                            assert_eq!(modifiers(state.effect_kind,state.effect_ticks).intangible,
                                modifiers(actual.effect_kind,actual.effect_ticks).intangible);
                            assert_eq!(motion,w.motion_limits(0,0.0).unwrap());
                            if offset<25 { assert_eq!(schedule[offset],motion); }
                        }
                    }
                }
            }
        }
    }
}
