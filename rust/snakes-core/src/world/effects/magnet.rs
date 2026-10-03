// SPDX-License-Identifier: GPL-3.0-or-later
//! Magnet changes acquisition reach only. Food already acquired stays attached
//! after expiry/replacement, just like the ordinary 3r vacuum.
use super::{EffectHook, Modifiers};
use crate::{Point, SnakeView, World};
pub(super) struct Magnet;
impl EffectHook for Magnet {
    fn modifiers(_ticks: u16) -> Modifiers {
        Modifiers { food_reach: 9.0, ..Modifiers::default() }
    }

    fn ai_bonus(world: &World, snake: SnakeView<'_>, position: Point) -> f64 {
        // Value the patch at the capsule, with a modest density bonus. This is
        // at most three bounded scans per strategy update, never per rollout.
        let reach2 = (snake.radius * 24.0).powi(2);
        let mut density = 0.0;
        for food in world.foods() {
            if food.vacuum_owner < 0 && world.distance_squared(position, food.position) <= reach2 {
                density += food.value.min(2.0);
                if density >= 16.0 { break; }
            }
        }
        density.min(16.0) * 0.25
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{controller::{ScriptedController, Steering}, Config, DeathReason, FoodKind,
        RuleSet, Segment, MAX_SEGMENTS, STEP_SECONDS};
    use crate::world::{Food, Item};
    use crate::effects::EffectKind;

    fn arena(wrap: bool) -> World {
        let mut w = World::new(Config { rules: RuleSet::V2, deadly_walls: !wrap,
            density: 0.0, self_collisions: true, ..Config::default() }).unwrap();
        for s in &mut w.snakes { s.alive = false; s.len = 0; s.respawn = 1000.0; }
        w.food.clear(); w.items.clear();
        w.item_timer = 900;
        line(&mut w, 0, Point { x: 400.0, y: 400.0 }, 24);
        w
    }
    fn line(w: &mut World, id: usize, head: Point, len: usize) {
        let s = &mut w.snakes[id];
        s.alive = true; s.len = len; s.radius = 6.0; s.base_radius = 6.0;
        s.birth_len = len; s.angle = 0.0; s.desired = 0.0;
        for j in 0..len {
            let p = Point { x: head.x - j as f64 * 7.08, y: head.y };
            w.segments[id * MAX_SEGMENTS + j] = Segment { current: p, previous: p };
        }
    }
    fn activate(w: &mut World) {
        w.items.push(Item { id: 1, kind: EffectKind::Magnet,
            position: w.segments[0].current, life_ticks: 750, radius: 12.0, ..Item::default() });
        w.pickup_items();
    }
    fn food(w: &mut World, p: Point, kind: FoodKind) {
        w.food.push(Food { id: w.food.len() as u64 + 1, p, size: 2.0, value: 1.0,
            life: 20.0, original_life: 20.0, owner: -1, kind, ..Food::default() });
    }

    #[test]
    fn magnet_acquires_all_food_kinds_at_9r_boundary_only() {
        for kind in [FoodKind::Spark, FoodKind::Shard, FoodKind::Pellet, FoodKind::Prism] {
            for (offset, active, expected) in [(55.99, true, 0), (56.01, true, -1),
                (20.01, false, -1), (19.99, false, 0)] {
                let mut w = arena(false);
                if active { activate(&mut w); }
                food(&mut w, Point { x: 400.0 + offset, y: 400.0 }, kind);
                w.feed_snakes(STEP_SECONDS);
                assert_eq!(w.food[0].owner, expected, "{kind:?}, {offset}, {active}");
            }
        }
    }
    #[test]
    fn magnet_exact_expiry_restores_reach_and_retains_already_pulled_food() {
        let mut w = arena(false); activate(&mut w);
        assert_eq!(w.snakes[0].effect_ticks, 300);
        for _ in 0..299 { w.advance_items_and_effects(); }
        assert_eq!(w.snakes[0].effect_ticks, 1);
        food(&mut w, Point { x: 455.0, y: 400.0 }, FoodKind::Prism);
        w.feed_snakes(0.0);
        assert_eq!(w.food[0].owner, 0);
        w.advance_items_and_effects();
        assert_eq!(w.snakes[0].effect_kind, 0);
        food(&mut w, Point { x: 450.0, y: 401.0 }, FoodKind::Shard);
        let old_x = w.food[0].p.x;
        w.feed_snakes(STEP_SECONDS);
        assert_eq!(w.food[1].owner, -1);
        assert_eq!(w.food[0].owner, 0);
        assert!(w.food[0].p.x < old_x);
    }
    #[test]
    fn magnet_wrap_pull_and_closest_rival_counterplay() {
        let mut w = arena(true);
        line(&mut w, 0, Point { x: 5.0, y: 400.0 }, 24); activate(&mut w);
        let width = w.config.width;
        food(&mut w, Point { x: width - 40.0, y: 400.0 }, FoodKind::Shard);
        w.feed_snakes(0.0); assert_eq!(w.food[0].owner, 0);
        let mut w = arena(false); activate(&mut w);
        line(&mut w, 1, Point { x: 460.0, y: 400.0 }, 24);
        food(&mut w, Point { x: 448.0, y: 400.0 }, FoodKind::Shard);
        w.feed_snakes(0.0);
        assert_eq!(w.food[0].owner, 1); // Ordinary rival wins by arriving closer.
        w.snakes[1].alive = false;
        w.feed_snakes(0.0); assert_eq!(w.food[0].owner, 0);
    }
    #[test]
    fn magnet_replacement_disable_and_death_clear_effect() {
        for mode in 0..3 {
            let mut w = arena(false); activate(&mut w);
            match mode {
                0 => { w.items.push(Item { kind: EffectKind::Surge, position: w.segments[0].current,
                    life_ticks: 750, radius: 12.0, ..Item::default() }); w.pickup_items(); }
                1 => { w.reconfigure(Config { power_ups: false, ..w.config() }).unwrap(); }
                _ => { w.snakes[0].dying = DeathReason::Wall; w.explode_snake(0); }
            }
            assert_ne!(w.snakes[0].effect_kind, EffectKind::Magnet as u8);
            assert_eq!(crate::effects::modifiers(EffectKind::Magnet as u8, 0).food_reach, 3.0);
        }
    }
    #[test]
    fn magnet_boost_is_paid_and_motion_forecasts_unchanged() {
        let mut w = arena(true);
        let mut plain = w.diagnostic_snapshot(); activate(&mut w);
        assert_eq!(w.forecast_motion_schedule(0, 0.6), plain.forecast_motion_schedule(0, 0.6));
        let mut boost = ScriptedController::new(|_, s: SnakeView<'_>| Steering { desired_angle: s.angle, rush: 1.0 });
        w.step(&mut boost); plain.step(&mut boost);
        assert_eq!(w.snakes[0].boost_ticks, 24);
        assert_eq!(w.snakes[0].len, plain.snakes[0].len);
        assert_eq!(w.segments[0].current, plain.segments[0].current);
        assert_eq!(w.snakes[0].cooldown_ticks, plain.snakes[0].cooldown_ticks);
        w.step_n(&mut boost, 20); plain.step_n(&mut boost, 20);
        assert_eq!(w.snakes[0].len, 22);
        assert_eq!(w.snakes[0].len, plain.snakes[0].len);
    }
    #[test]
    fn magnet_remains_tangible_and_respects_wall_and_self_settings() {
        for self_on in [false, true] {
            let mut w = arena(false); w.config.self_collisions = self_on; activate(&mut w);
            w.segments[12] = w.segments[0];
            w.mark_collisions();
            assert_eq!(w.snakes[0].dying, if self_on { DeathReason::SelfHit } else { DeathReason::None });
        }
        let mut w = arena(false); activate(&mut w);
        line(&mut w, 1, Point { x: 408.0, y: 400.0 }, 24);
        w.mark_collisions(); assert_ne!(w.snakes[0].dying, DeathReason::None);
        for wrap in [false, true] {
            let mut w = arena(wrap); activate(&mut w);
            let p = Point { x: w.config.width - 0.01, y: 400.0 };
            w.segments[0] = Segment { current: p, previous: p };
            let mut straight = ScriptedController::new(|_, s: SnakeView<'_>| Steering { desired_angle: s.angle, rush: 0.0 });
            w.step(&mut straight);
            assert_eq!(w.snakes[0].alive, wrap);
            if wrap { assert!(w.segments[0].current.x < 10.0); }
        }
    }
    #[test]
    fn magnet_density_value_is_local_bounded_and_ignores_claimed_food() {
        let mut w = arena(true);
        let p = w.segments[0].current;
        assert_eq!(Magnet::ai_bonus(&w, w.snake(0).unwrap(), p), 0.0);
        for _ in 0..20 { food(&mut w, Point { x: p.x + 50.0, y: p.y }, FoodKind::Shard); }
        assert_eq!(Magnet::ai_bonus(&w, w.snake(0).unwrap(), p), 4.0);
        for f in &mut w.food { f.owner = 1; }
        assert_eq!(Magnet::ai_bonus(&w, w.snake(0).unwrap(), p), 0.0);
    }
}
