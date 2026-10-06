// SPDX-License-Identifier: GPL-3.0-or-later
use super::{EffectHook, Modifiers};
use crate::{flags, Point, SnakeView, World};

pub(super) struct Phase;
impl EffectHook for Phase {
    fn modifiers(_ticks: u16) -> Modifiers {
        Modifiers { intangible: true, flags: flags::PHASED, ..Modifiers::default() }
    }

    fn ai_bonus(_world: &World, snake: SnakeView<'_>, _position: Point) -> f64 {
        // The item scorer already supplies the base value of three.
        if snake.flags & flags::TRAPPED != 0 { 12.0 } else { 0.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::EffectKind;
    use crate::{ai::AiController, controller::{Controller, ScriptedController, Steering}, Config, DeathReason, Item, RuleSet, Segment, MAX_SEGMENTS};

    fn arena(wrap: bool, self_collisions: bool) -> World {
        World::diagnostic_arena(Config {store_power_ups:false, rules: RuleSet::V2, width: 1200.0, height: 900.0,
            density: 0.0, deadly_walls: !wrap, self_collisions, ..Config::default() },
            &[(Point { x: 450.0, y: 450.0 }, 0.0, 24, 0.0),
              (Point { x: 800.0, y: 700.0 }, 0.0, 80, 0.0)], &[]).unwrap()
    }
    fn activate(w: &mut World, id: usize, ticks: u16) {
        w.snakes[id].effect_kind = EffectKind::Phase as u8;
        w.snakes[id].effect_ticks = ticks;
    }
    fn point(w: &mut World, id: usize, j: usize, p: Point) {
        w.segments[id * MAX_SEGMENTS + j] = Segment { current: p, previous: p };
    }
    fn straight() -> impl Controller {
        ScriptedController::new(|_, s: SnakeView<'_>| Steering { desired_angle: s.angle, rush: 0.0 })
    }

    #[test]
    fn phase_all_bodies_head_contests_and_self_setting() {
        for wrap in [false, true] { for self_collisions in [false, true] {
            let mut w = arena(wrap, self_collisions);
            let p = w.segments[0].current;
            point(&mut w, 1, 20, p);
            w.mark_collisions();
            assert_eq!(w.snakes[0].dying, DeathReason::Body);
            activate(&mut w, 0, 120);
            w.mark_collisions();
            assert_eq!(w.snakes[0].dying, DeathReason::None);
            assert_ne!(w.snake(0).unwrap().flags & flags::PHASED, 0);
            // The ghost's body also cannot kill an ordinary rival.
            let q = w.segments[MAX_SEGMENTS].current;
            point(&mut w, 0, 12, q);
            w.mark_collisions();
            assert_eq!(w.snakes[1].dying, DeathReason::None);
            // Even a much smaller phased head ignores a losing contest.
            point(&mut w, 1, 20, Point { x: 900.0, y: 700.0 });
            point(&mut w, 1, 0, p);
            w.mark_collisions();
            assert_eq!(w.snakes[0].dying, DeathReason::None);
            assert_eq!(w.snakes[1].dying, DeathReason::None);
        }}
        let mut w = arena(false, true);
        w.snakes[1].alive = false;
        let p = w.segments[0].current;
        point(&mut w, 0, 12, p);
        w.mark_collisions(); assert_eq!(w.snakes[0].dying, DeathReason::SelfHit);
        activate(&mut w, 0, 120);
        w.mark_collisions(); assert_eq!(w.snakes[0].dying, DeathReason::None);
    }

    #[test]
    fn phase_duration_and_expiry_restore_collision_rules() {
        for wrap in [false, true] { for self_hit in [false, true] { for enabled in [false, true] {
            let mut w = arena(wrap, enabled);
            activate(&mut w, 0, 120);
            for _ in 0..119 { w.advance_items_and_effects(); }
            assert_eq!(w.snakes[0].effect_ticks, 1);
            assert!(super::super::modifiers(3, 1).intangible);
            let p = w.segments[0].current;
            if self_hit { point(&mut w, 0, 12, p); w.snakes[1].alive = false; }
            else { point(&mut w, 1, 20, p); }
            w.advance_items_and_effects();
            assert_eq!(w.snakes[0].effect_ticks, 0);
            assert_eq!(w.snakes[0].effect_kind, 0);
            assert!(!super::super::modifiers(3, 0).intangible);
            w.mark_collisions();
            assert_eq!(w.snakes[0].dying, if self_hit {
                if enabled { DeathReason::SelfHit } else { DeathReason::None }
            } else { DeathReason::Body });
        }}}
    }

    #[test]
    fn phase_expiry_inside_body_dies_before_it_can_move_clear() {
        let mut w = arena(false, true);
        let p = w.segments[0].current;
        point(&mut w, 1, 30, p);
        // Rebuild a stationary fixture trail with a body through the head.
        w.rebuild_trail(1);
        activate(&mut w, 0, 1);
        w.step(&mut straight());
        assert!(!w.snakes[0].alive);
        assert_eq!(w.stats().body_deaths, 1);
    }

    #[test]
    fn phase_walls_wrap_boost_replacement_and_disabled() {
        let mut w = arena(false, true);
        activate(&mut w, 0, 120);
        point(&mut w, 0, 0, Point { x: -0.01, y: 450.0 });
        w.mark_collisions(); assert_eq!(w.snakes[0].dying, DeathReason::Wall);
        let mut w = arena(true, true);
        activate(&mut w, 0, 120);
        let points: Vec<_> = (0..24).map(|j| Point { x: 1199.0-j as f64*7.08, y: 450.0 }).collect();
        w.diagnostic_body(0, &points, 0.0).unwrap();
        w.step(&mut straight());
        assert!(w.snakes[0].alive); assert!(w.segments[0].current.x < 20.0);
        let len = w.snakes[0].len;
        for _ in 0..21 { w.advance_boost(0, true); }
        assert_eq!(w.snakes[0].len, len-2); // Phase retains paid boost.
        assert!(w.snakes[0].boost_ticks > 0);
        let p = w.segments[0].current;
        w.items.push(Item { id: 1, kind: EffectKind::Magnet, position: p,
            life_ticks: 750, radius: 12.6, ..Item::default() });
        w.pickup_items();
        assert_eq!(w.snakes[0].effect_kind, EffectKind::Magnet as u8);
        assert_eq!(w.snake(0).unwrap().flags & flags::PHASED, 0);
        activate(&mut w, 0, 120);
        w.reconfigure(Config {store_power_ups:false, power_ups: false, ..w.config() }).unwrap();
        assert_eq!(w.snakes[0].effect_ticks, 0);
        assert_eq!(w.snake(0).unwrap().flags & flags::PHASED, 0);
    }

    #[test]
    fn phase_ai_trapped_value_and_coil_escape_with_clear_expiry() {
        let mut w = arena(false, true);
        // The large rival encloses us in a complete coil at radius 85.
        let coil: Vec<_> = (0..160).map(|j| {
            let a = j as f64 * std::f64::consts::TAU / 159.0;
            Point { x: 450.0+85.0*a.cos(), y: 450.0+85.0*a.sin() }
        }).collect();
        w.diagnostic_body(1, &coil, std::f64::consts::FRAC_PI_2).unwrap();
        w.set_intent_flags(0, flags::TRAPPED);
        assert_eq!(EffectKind::Phase.base_value()+Phase::ai_bonus(&w,w.snake(0).unwrap(),coil[0]),15.0);
        w.set_intent_flags(0,0);
        assert_eq!(EffectKind::Phase.base_value()+Phase::ai_bonus(&w,w.snake(0).unwrap(),coil[0]),3.0);
        activate(&mut w,0,120);
        let mut ai=AiController::new(); ai.enable_diagnostics();
        let steering=ai.steer(&w,w.snake(0).unwrap());
        assert!(steering.is_valid());
        assert!(ai.decision(0).horizon>=138);
        // Rival holds its ring while the real AI moves the escaping snake.
        // Mark collisions each step using an unchanged coil, to avoid a fake
        // escape caused by an enclosing diagnostic snake straightening out.
        for _ in 0..121 {
            w.advance_items_and_effects();
            if !w.snakes[0].alive {break;}
            let input=ai.steer(&w,w.snake(0).unwrap());
            w.snakes[0].desired=input.desired_angle;
            w.advance_boost(0,input.rush>0.0);
            w.move_snake(0,crate::STEP_SECONDS);
            w.mark_collisions();
            assert_eq!(w.snakes[0].dying,DeathReason::None,"expiry-safe coil escape at {} remaining",w.snakes[0].effect_ticks);
            w.tick+=1;
        }
        let d=w.displacement(Point{x:450.0,y:450.0},w.segments[0].current);
        assert!(d.x.hypot(d.y)>110.0,"AI must actually cross the enclosing body");
        assert_eq!(w.snakes[0].effect_ticks,0);
    }
    #[test]
    fn phase_ai_rivals_ignore_ghost_bodies_but_forecast_expiry() {
        let mut w=arena(false,true);
        let column: Vec<_>=(0..80).map(|j|Point{x:510.0,y:160.0+j as f64*7.08}).collect();
        w.diagnostic_body(1,&column,std::f64::consts::FRAC_PI_2).unwrap();
        let mut normal=AiController::new(); normal.enable_diagnostics();
        normal.steer(&w,w.snake(0).unwrap());
        let blocked=normal.decision(0).candidates[2].safe_ticks;
        activate(&mut w,1,120);
        let mut ghost=AiController::new(); ghost.enable_diagnostics();
        ghost.steer(&w,w.snake(0).unwrap());
        assert!(ghost.decision(0).candidates[2].safe_ticks>blocked);
        assert_eq!(ghost.decision(0).candidates[2].safe_ticks,ghost.decision(0).horizon);
        // A rival expiring before our arrival is an ordinary obstacle again.
        activate(&mut w,1,2);
        let mut expiry=AiController::new(); expiry.enable_diagnostics();
        expiry.steer(&w,w.snake(0).unwrap());
        assert_eq!(expiry.decision(0).candidates[2].safe_ticks,blocked);
    }

    #[test]
    fn phase_ai_rejects_body_at_own_expiry_with_and_without_boost() {
        for boost in [false,true] {
            let mut w=arena(false,true);
            if boost {w.advance_boost(0,true);}
            let speed=w.motion_limits(0,0.0).unwrap().0;
            let x=450.0+speed*crate::STEP_SECONDS*9.0;
            let column:Vec<_>=(0..80).map(|j|Point{x,y:160.0+j as f64*7.08}).collect();
            w.diagnostic_body(1,&column,std::f64::consts::FRAC_PI_2).unwrap();
            activate(&mut w,0,9);
            let mut expiry=AiController::new(); expiry.enable_diagnostics();
            let chosen=expiry.steer(&w,w.snake(0).unwrap());
            let d=expiry.decision(0);
            assert!(d.candidates[2].safe_ticks<d.horizon,"straight enters body at expiry");
            assert!(chosen.is_valid());
            assert!(d.candidates[d.selected].safe_ticks>d.candidates[2].safe_ticks,"AI must choose an exit rather than the expiring crossing");
            activate(&mut w,0,120);
            let mut full=AiController::new(); full.enable_diagnostics();
            full.steer(&w,w.snake(0).unwrap());
            assert_eq!(full.decision(0).candidates[2].safe_ticks,full.decision(0).horizon);
        }
    }

}
