// SPDX-License-Identifier: GPL-3.0-or-later
//! R3 effect seam. Workers implement their module's hook; dispatch stays here.
//! Hooks run without allocation or RNG. Replacement calls end before activate.
mod surge;
mod motion;
pub(crate) use motion::{forecast_daylight_motion, forecast_motion, forecast_boost, forecast_motion_before_tick, forecast_schedule_before_tick};
mod magnet;
mod phase;
pub(crate) mod frost;
#[cfg(test)]
pub(crate) use surge::tests::grant_surge;
use super::World;

/// Counters seen by steering have already decremented for the current tick.
/// Step one is its movement; step `ticks + 1` is the first expired movement.
#[inline]
pub(crate) fn remaining_ticks(ticks: u16, step: usize) -> u16 {
    ticks.saturating_sub(step.saturating_sub(1).min(u16::MAX as usize) as u16)
}

/// Map an arrival/sweep endpoint to its movement step, including time zero.
#[inline]
pub(crate) fn forecast_step(time: f64) -> usize {
    (time / crate::STEP_SECONDS - 1e-7).ceil().max(1.0) as usize
}

use crate::{Point, SnakeView};
use crate::controller::Steering;
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EffectKind { #[default] None, Surge, Magnet, Phase, Venom, Frost, Flip, Whirlpool }
/// Extend for R4; weights of disabled kinds are never included in the draw.
pub const ENABLED_KINDS: &[EffectKind] = &[EffectKind::Surge, EffectKind::Magnet, EffectKind::Phase, EffectKind::Venom, EffectKind::Frost, EffectKind::Flip];
pub const WARNING_TICKS: u16 = 36;
impl EffectKind {
    pub fn from_byte(kind: u8) -> Self {
        match kind { 1=>Self::Surge,2=>Self::Magnet,3=>Self::Phase,4=>Self::Venom,5=>Self::Frost,6=>Self::Flip,7=>Self::Whirlpool,_=>Self::None }
    }
    pub const fn duration(self) -> u16 {
        match self {Self::Surge=>180,Self::Magnet=>300,Self::Phase=>120,Self::Venom=>240,Self::Frost=>75,Self::Flip=>300,Self::Whirlpool=>150,Self::None=>0}
    }
    pub const fn weight(self) -> u32 {
        match self {Self::Surge=>20,Self::Magnet=>24,Self::Phase=>12,Self::Venom=>16,Self::Frost=>10,Self::Flip=>10,Self::Whirlpool=>8,Self::None=>0}
    }
    pub const fn base_value(self) -> f64 {
        match self {Self::Surge=>6.0,Self::Magnet=>4.0,Self::Phase=>3.0,Self::Venom=>5.0,Self::Frost=>3.0,Self::Flip=>4.0,Self::Whirlpool=>4.0,Self::None=>0.0}
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EndReason { Replaced, Expired, Disabled, Died }
/// Pure mechanics controls. Inert defaults compile away in step A. Workers
/// override these in their own module, including predictions and ABI flags.
#[derive(Clone, Copy, Debug)]
pub struct Modifiers {
    pub speed: f64,
    pub food_reach: f64,
    pub free_boost: bool,
    pub boost_cooldown: u8,
    pub intangible: bool,
    pub flags: u32,
}
impl Default for Modifiers {
    fn default() -> Self { Self {speed:1.0,food_reach:3.0,free_boost:false,boost_cooldown:36,intangible:false,flags:0} }
}
pub(crate) trait EffectHook {
    fn modifiers(_ticks: u16) -> Modifiers { Modifiers::default() }
    fn activate(_world: &mut World, _snake: usize) {}
    fn tick(_world: &mut World, _snake: usize) {}
    fn end(_world: &mut World, _snake: usize, _reason: EndReason) {}
    fn ai_steering(_world: &World, _snake: SnakeView<'_>, input: Steering) -> Steering { input }
    fn ai_bonus(_world: &World, _snake: SnakeView<'_>, _position: Point) -> f64 { 0.0 }
}
macro_rules! dispatch {
    ($kind:expr, $method:ident, $($arg:expr),*) => {
        match $kind {
            EffectKind::Surge => surge::Surge::$method($($arg),*),
            EffectKind::Magnet => magnet::Magnet::$method($($arg),*),
            EffectKind::Phase => phase::Phase::$method($($arg),*),
            EffectKind::Frost => frost::Frost::$method($($arg),*),
            _ => DefaultHook::$method($($arg),*),
        }
    }
}
struct DefaultHook;
impl EffectHook for DefaultHook {}
pub(crate) fn activate(kind: EffectKind, w: &mut World, id: usize) { dispatch!(kind, activate, w, id); }
pub(crate) fn tick(kind: EffectKind, w: &mut World, id: usize) { dispatch!(kind, tick, w, id); }
pub(crate) fn end(kind: EffectKind, w: &mut World, id: usize, why: EndReason) { dispatch!(kind, end, w, id, why); }
pub(crate) fn ai_bonus(kind: EffectKind, w: &World, s: SnakeView<'_>, p: Point) -> f64 { dispatch!(kind, ai_bonus, w, s, p) }

#[inline]
pub fn modifiers(kind: u8, ticks: u16) -> Modifiers {
    if ticks==0 {Modifiers::default()} else {dispatch!(EffectKind::from_byte(kind), modifiers, ticks)}
}

/// Active-effect policy hook, applied only to smart AI (never scripted inputs).
#[inline]
pub(crate) fn ai_steering(w: &World, s: SnakeView<'_>, input: Steering) -> Steering {
    if s.effect_ticks==0 {input} else {dispatch!(EffectKind::from_byte(s.effect_kind), ai_steering, w, s, input)}
}

#[cfg(test)]
mod timing_tests {
    use super::*;
    use crate::{Config, RuleSet};

    #[test]
    fn effect_forecasts_match_post_decrement_execution_at_expiry() {
        for kind in [EffectKind::Surge, EffectKind::Magnet, EffectKind::Phase, EffectKind::Frost] {
            for ticks in [1, 2, 8, 24, 25, 26, 36, 100] {
                for observed in [false, true] {
                    let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,density:0.0,
                        self_collisions:false,..Config::default()},
                        &[(Point{x:500.0,y:400.0},0.0,120,0.0)],&[]).unwrap();
                    if observed {w.advance_boost(0,true);}
                    w.food.resize(w.config.food_count(),crate::world::Food {p:Point{x:100.0,y:100.0},life:1000.0,value:0.0,owner:-1,..Default::default()});
                    w.snakes[0].effect_kind=kind as u8;w.snakes[0].effect_ticks=ticks;
                    let predictions:Vec<_>=(0..139).map(|offset|(forecast_motion(&w,0,0.6,offset),motion::forecast_state(&w,0,0.6,offset))).collect();
                    w.snakes[0].effect_ticks+=1; // Input above is post-decrement.
                    for (offset,(prediction,state)) in predictions.into_iter().enumerate() {
                        w.step(&mut crate::controller::ScriptedController::new(|_,s:SnakeView<'_>|Steering {desired_angle:s.angle,rush:if offset==0 {0.6} else {0.0}}));
                        let actual=w.snakes[0];
                        assert_eq!((state.effect_kind,state.effect_ticks,state.boost_ticks,state.cooldown_ticks,state.boost_cost,state.boost_paid,state.len),
                            (actual.effect_kind,actual.effect_ticks,actual.boost_ticks,actual.cooldown_ticks,actual.boost_cost,actual.boost_paid,actual.len),
                            "counters kind={kind:?} ticks={ticks} observed={observed} offset={offset}");
                        assert_eq!(prediction,w.motion_limits(0,0.0).unwrap(),
                            "kind={kind:?} ticks={ticks} observed={observed} offset={offset}");
                    }
                }
            }
        }
    }
    #[test]
    fn forecast_boost_cooldowns_and_phase_contacts_match_world_transitions() {
        for kind in [EffectKind::Surge,EffectKind::Magnet,EffectKind::Phase,EffectKind::Frost] {
            for ticks in [1,2,24,25,26,100] {for cooldown in [0,1,2,18,36] {for burst in [0,1,2,24] {
                let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,density:0.0,
                    self_collisions:false,..Config::default()},
                    &[(Point{x:500.0,y:400.0},0.0,120,0.0)],&[]).unwrap();
                w.food.resize(w.config.food_count(),crate::world::Food {p:Point{x:100.0,y:100.0},life:1000.0,value:0.0,owner:-1,..Default::default()});
                w.snakes[0].effect_kind=kind as u8;w.snakes[0].effect_ticks=ticks;
                w.snakes[0].cooldown_ticks=cooldown;w.snakes[0].boost_ticks=burst;
                let predictions:Vec<_>=(0..139).map(|offset|motion::forecast_state(&w,0,0.6,offset)).collect();
                w.snakes[0].effect_ticks+=1; // Input above is post-decrement.
                for (offset,state) in predictions.into_iter().enumerate() {
                    w.step(&mut crate::controller::ScriptedController::new(|_,s:SnakeView<'_>|Steering {desired_angle:s.angle,rush:if offset==0 {0.6} else {0.0}}));
                    assert_eq!(modifiers(kind as u8,remaining_ticks(ticks,offset+1)).intangible,
                        modifiers(w.snakes[0].effect_kind,w.snakes[0].effect_ticks).intangible);
                    let actual=w.snakes[0];
                    assert_eq!((state.boost_ticks,state.cooldown_ticks,state.len),
                        (actual.boost_ticks,actual.cooldown_ticks,actual.len),
                        "kind={kind:?} ticks={ticks} cooldown={cooldown} burst={burst} offset={offset}");
                }
            }}}
        }
    }

}
