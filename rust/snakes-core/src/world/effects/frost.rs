// SPDX-License-Identifier: GPL-3.0-or-later
//! Nova eligibility is shared by mechanics, valuation and all pickup forecasts.
use super::{EffectHook, EffectKind};
use crate::{World, Point, SnakeView, FrameEvent, EventKind, MAX_SEGMENTS};
pub const FROZEN_TICKS:u16=75;
pub const THAW_IMMUNITY_TICKS:u16=45;
pub const NOVA_TICKS:u16=21;
#[inline]
pub(crate) fn freeze_eligible(alive:bool,owner:bool,frozen:u16,immunity:u16,distance2:f64,base_radius:f64)->bool {
    alive && !owner && frozen==0 && immunity==0 && distance2<=(16.0*base_radius).powi(2)
}
pub(super) struct Frost;
impl EffectHook for Frost {
    fn activate(w:&mut World,owner:usize) {
        let center=w.segments[owner*MAX_SEGMENTS].current;
        let r=w.config.base_radius();
        for id in 0..w.snakes.len() {
            if !freeze_eligible(w.snakes[id].alive,id==owner,w.snakes[id].frozen_ticks,w.faces[id].thaw_immunity_ticks,
                w.distance_squared(center,w.segments[id*MAX_SEGMENTS].current),r) {continue;}
            w.snakes[id].frozen_ticks=FROZEN_TICKS;
            // Nova is after feeding/pickup movement but before collisions.
            // It cancels the burst immediately without consuming its unpaid tail.
            if w.snakes[id].boost_ticks>0 {w.snakes[id].cooldown_ticks=super::modifiers(w.snakes[id].effect_kind,w.snakes[id].effect_ticks).boost_cooldown;}
            w.snakes[id].boost_ticks=0;w.snakes[id].rush=0.0;
            w.faces[id].frozen_ticks=FROZEN_TICKS;w.faces[id].breath_ticks=FROZEN_TICKS;
        }

        w.push_event(FrameEvent {tick:w.tick+1,position:center,snake_id:owner as u32,
            generation:w.snakes[owner].generation,other_snake_id:EffectKind::Frost as u32,
            kind:EventKind::Nova,duration_ticks:NOVA_TICKS,value:(16.0*r) as f32,..Default::default()});
    }
    fn ai_bonus(w:&World,s:SnakeView<'_>,p:Point)->f64 {crate::ai::frost::item_bonus(w,s,p)}
}
impl World {
    #[cfg(test)]
    pub(crate) fn test_frost_tick(&mut self) {
        self.step(&mut crate::controller::ScriptedController::new(|_,s:SnakeView<'_>| crate::controller::Steering {desired_angle:s.angle,rush:0.0}));
    }
    pub(crate) fn advance_frost(&mut self) {
        for id in 0..self.snakes.len() {
            if self.snakes[id].frozen_ticks==0 {continue;}
            self.snakes[id].frozen_ticks-=1;
            self.faces[id].frozen_ticks=self.snakes[id].frozen_ticks;
            if self.snakes[id].frozen_ticks!=0 || !self.snakes[id].alive {continue;}
            // Effects advance before presentation; include its current-tick decrement.
            self.faces[id].thaw_immunity_ticks=THAW_IMMUNITY_TICKS+1;
            self.faces[id].scared_ticks=37;self.faces[id].breath_ticks=0;
            // Existing expiry namespace + Frost payload identifies the crack.
            self.push_event(FrameEvent {tick:self.tick+1,position:self.segments[id*MAX_SEGMENTS].current,
                snake_id:id as u32,generation:self.snakes[id].generation,other_snake_id:5,
                kind:EventKind::EffectExpiry,duration_ticks:15,..Default::default()});
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Config,RuleSet,Item,flags};
    fn arena()->World {World::diagnostic_arena(Config {store_power_ups:false,rules:RuleSet::V2,width:1400.0,height:1000.0,density:0.0,
        self_collisions:false,deadly_walls:false,..Default::default()},&[(Point{x:500.0,y:500.0},0.0,24,0.0),
        (Point{x:600.0,y:550.0},0.0,48,0.0)],&[]).unwrap()}
    #[test]
    fn nova_preserves_rival_effects_cancels_boost_and_thaws_once() {
        for kind in [EffectKind::None,EffectKind::Surge,EffectKind::Magnet,EffectKind::Phase,EffectKind::Venom] {
            let mut w=arena();w.snakes[1].effect_kind=kind as u8;w.snakes[1].effect_ticks=kind.duration();
            let speed=w.speed(&w.snakes[1]);let turn=w.turn_rate(&w.snakes[1]);
            w.advance_boost(1,true);Frost::activate(&mut w,0);
            assert_eq!(w.snakes[1].effect_kind,kind as u8);assert_eq!(w.snakes[1].boost_ticks,0);
            assert!(!w.boost_ready(1));assert_eq!(w.speed(&w.snakes[1]),speed*0.5);
            assert_eq!(w.turn_rate(&w.snakes[1]),turn*0.6);assert_eq!(w.snakes[0].effect_ticks,0);
            assert_ne!(w.snake(1).unwrap().flags&flags::FROZEN,0);
            for _ in 0..74 {w.test_frost_tick();}
            assert_eq!(w.snakes[1].frozen_ticks,1);
            w.test_frost_tick();
            assert_eq!(w.faces[1].thaw_immunity_ticks,45);assert_eq!(w.faces[1].scared_ticks,36);
            assert_eq!(w.frame_events().filter(|e|e.kind==EventKind::EffectExpiry && e.other_snake_id==5).count(),1);
            Frost::activate(&mut w,0);assert_eq!(w.snakes[1].frozen_ticks,0);
            for _ in 0..45 {w.test_frost_tick();}
            w.segments[0].current=Point{x:w.segments[MAX_SEGMENTS].current.x,y:w.segments[MAX_SEGMENTS].current.y-50.0};
            Frost::activate(&mut w,0);assert_eq!(w.snakes[1].frozen_ticks,75);
        }
    }
    #[test]
    fn radius_seams_guard_and_disable_share_pickup_rules() {
        let mut w=arena();let r=w.config.base_radius();
        assert!(freeze_eligible(true,false,0,0,(16.0*r).powi(2),r));
        for (alive,owner,frozen,immunity) in [(false,false,0,0),(true,true,0,0),(true,false,1,0),(true,false,0,1)] {
            assert!(!freeze_eligible(alive,owner,frozen,immunity,0.0,r));
        }
        w.config.deadly_walls=false;w.segments[0].current=Point{x:1.0,y:500.0};w.segments[MAX_SEGMENTS].current=Point{x:1399.0,y:500.0};
        w.snakes[0].effect_kind=4;w.snakes[0].effect_ticks=240;w.faces[0].guarding=true;
        w.items.push(Item {kind:EffectKind::Frost,position:w.segments[0].current,life_ticks:750,radius:2.1*r,..Default::default()});
        w.snakes[1].alive=false;w.pickup_items();assert_eq!(w.items.len(),1);
        w.snakes[1].alive=true;w.faces[0].guarding=false;w.pickup_items();assert_eq!(w.snakes[1].frozen_ticks,75);
        w.reconfigure(Config {store_power_ups:false,power_ups:false,..w.config()}).unwrap();
        assert_eq!(w.snakes[1].frozen_ticks,0);assert_eq!(w.faces[1].breath_ticks,0);
    }
    #[test]
    fn frozen_motion_forecasts_match_thaw_and_boost_payment() {
        for remaining in [1,2,8,24,75] {for burst in [false,true] {
            let mut w=arena();if burst {w.advance_boost(1,true);}
            w.snakes[1].frozen_ticks=remaining;
            let schedule=w.forecast_motion_schedule(1,0.6).unwrap();
            let predicted:Vec<_>=(0..90).map(|j|w.forecast_motion_limits(1,0.6,j).unwrap()).collect();
            for (j,limits) in predicted.into_iter().enumerate() {
                w.step(&mut crate::controller::ScriptedController::new(|_,s:SnakeView<'_>|crate::controller::Steering {desired_angle:s.angle,rush:if s.id==1 && j==0 {0.6} else {0.0}}));
                assert_eq!(limits,w.motion_limits(1,0.0).unwrap(),"remaining={remaining} burst={burst} step={j}");
                if j<25 {assert_eq!(schedule[j],limits);}
            }
        }}
    }
}
