// SPDX-License-Identifier: GPL-3.0-or-later
//! Free bursts and faster movement; all counters remain owned by the world.
use super::{EffectHook, Modifiers};
use crate::{Point, SnakeView, World};
pub(super) struct Surge;
impl EffectHook for Surge {
    fn modifiers(_ticks: u16) -> Modifiers {
        Modifiers { speed: 1.25, free_boost: true, boost_cooldown: 18, ..Modifiers::default() }
    }

    fn activate(world: &mut World, id: usize) {
        let snake = &mut world.snakes[id];
        // Already spent pellets stay in the field. Forgive the unpaid balance
        // of an observed burst, and shorten an already running cooldown.
        snake.boost_cost = snake.boost_paid;
        snake.cooldown_ticks = snake.cooldown_ticks.min(18);
    }

    fn ai_bonus(world: &World, snake: SnakeView<'_>, _position: Point) -> f64 {
        let head = snake.segments[0].current;
        let reach2 = (25.0 * snake.radius).powi(2);
        if world.snakes().any(|prey| prey.alive && prey.id != snake.id && prey.flags & crate::flags::PHASED == 0
            && snake.segments.len() >= prey.segments.len() + 4
            && world.distance_squared(head, prey.segments[0].current) <= reach2) { 6.0 } else { 0.0 }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{Config, RuleSet, Segment, MAX_SEGMENTS, DeathReason, EventKind};
    use crate::world::{Item, effects::{self, EffectKind}};
    use crate::controller::{ScriptedController, Steering};

    pub(crate) fn grant_surge(w: &mut World, id: usize) {
        let position = w.segments[id * MAX_SEGMENTS].current;
        w.items.push(Item { id: 999, position, kind: EffectKind::Surge,
            life_ticks: 750, radius: 2.1 * w.config.base_radius(), ..Item::default() });
        w.pickup_items();
        assert_eq!(w.snakes[id].effect_kind, EffectKind::Surge as u8);
    }

    fn arena(walls: bool, self_collisions: bool, len: usize) -> World {
        let mut w = World::diagnostic_arena(Config {store_power_ups:false, width: 4000.0, height: 2000.0,
            rules: RuleSet::V2, deadly_walls: walls, self_collisions,
            ..Config::default() }, &[(Point { x: 1000.0, y: 1000.0 }, 0.0, len, 0.1)], &[]).unwrap();
        w.item_timer = u16::MAX;
        w
    }

    #[test]
    fn surge_speed_duration_and_exact_expiry() {
        let mut w = arena(true, true, 24);
        let ordinary = w.motion_limits(0, 0.0).unwrap();
        let rng = w.rng_state();
        grant_surge(&mut w, 0);
        assert_eq!(w.snakes[0].effect_ticks, 180);
        assert_eq!(w.motion_limits(0, 0.0).unwrap().0, ordinary.0 * 1.25);
        assert_eq!(w.motion_limits(0, 0.6).unwrap().0, ordinary.0 * 1.6 * 1.25);
        for _ in 0..179 { w.advance_items_and_effects(); }
        assert_eq!(w.snakes[0].effect_ticks, 1);
        assert_eq!(w.motion_limits(0, 0.0).unwrap().0, ordinary.0 * 1.25);
        w.advance_items_and_effects();
        assert_eq!((w.snakes[0].effect_kind, w.snakes[0].effect_ticks), (0, 0));
        assert_eq!(w.motion_limits(0, 0.0).unwrap(), ordinary);
        assert_eq!(w.rng_state(), rng);
        assert!(w.frame_events().any(|e| e.kind == EventKind::EffectExpiry && e.other_snake_id == 1));
        assert!(!effects::modifiers(1, 0).free_boost);
    }

    #[test]
    fn surge_free_bursts_chain_after_eighteen_tick_cooldown() {
        for len in [12, 16, 100, 300, 1600] {
            let mut w = arena(false, true, len);
            grant_surge(&mut w, 0);
            let food = w.food.len();
            for _ in 0..3 {
                assert!(w.boost_ready(0));
                assert_eq!(w.boost_segment_cost(0), Some(0));
                w.advance_boost(0, true);
                assert_eq!(w.snakes[0].boost_ticks, 24);
                for _ in 0..23 { w.advance_boost(0, true); }
                assert_eq!(w.snakes[0].boost_ticks, 1);
                w.advance_boost(0, false);
                assert_eq!(w.snakes[0].cooldown_ticks, 18);
                for _ in 0..17 { w.advance_boost(0, true); }
                assert_eq!(w.snakes[0].cooldown_ticks, 1);
                w.advance_boost(0, false);
                assert!(w.boost_ready(0));
            }
            assert_eq!(w.snakes[0].len, len);
            assert_eq!(w.food.len(), food);
        }
        let mut short = arena(false, false, 11);
        grant_surge(&mut short, 0);
        short.advance_boost(0, true);
        assert!(!short.boost_ready(0));
        assert_eq!(short.snakes[0].boost_ticks, 0);
    }

    #[test]
    fn surge_pickup_forgives_unpaid_cost_and_shortens_existing_cooldown() {
        let mut w = arena(false, true, 100);
        w.advance_boost(0, true);
        for _ in 0..8 { w.advance_boost(0, false); }
        assert!(w.snakes[0].boost_paid > 0);
        let length = w.snakes[0].len;
        let pellets = w.food.len();
        grant_surge(&mut w, 0);
        assert_eq!(w.boost_segment_cost(0), Some(0));
        for _ in 0..24 { w.advance_boost(0, false); }
        assert_eq!(w.snakes[0].len, length);
        assert_eq!(w.food.len(), pellets);
        w.snakes[0].cooldown_ticks = 36;
        grant_surge(&mut w, 0);
        assert_eq!(w.snakes[0].cooldown_ticks, 18);
    }

    #[test]
    fn surge_expiry_preserves_started_free_burst_and_restores_new_burst_price() {
        let mut w = arena(false, true, 100);
        grant_surge(&mut w, 0);
        w.snakes[0].effect_ticks = 2;
        w.advance_boost(0, true);
        w.advance_items_and_effects();
        w.advance_items_and_effects();
        for _ in 0..23 { w.advance_boost(0, false); }
        assert_eq!(w.snakes[0].len, 100);
        assert!(w.food.is_empty());
        w.advance_boost(0, false);
        assert_eq!(w.snakes[0].cooldown_ticks, 36);
        for _ in 0..36 { w.advance_boost(0, false); }
        assert_eq!(w.boost_segment_cost(0), Some(3));
        w.advance_boost(0, true);
        for _ in 0..21 { w.advance_boost(0, false); }
        assert_eq!(w.snakes[0].len, 97);
        assert_eq!(w.food.len(), 3);
    }

    #[test]
    fn surge_replacement_disable_and_death_clear_speed_without_cancelling_burst() {
        for end in 0..3 {
            let mut w = arena(false, true, 24);
            let ordinary = w.motion_limits(0, 0.0).unwrap().0;
            grant_surge(&mut w, 0);
            w.advance_boost(0, true);
            if end == 0 {
                w.items.push(Item { id: 1000, position: w.segments[0].current,
                    kind: EffectKind::Magnet, life_ticks: 750, radius: 10.0, ..Item::default() });
                w.pickup_items();
            } else if end == 1 { w.reconfigure(Config {store_power_ups:false, power_ups: false, ..w.config() }).unwrap(); }
            else { w.snakes[0].dying = DeathReason::Wall; w.explode_snake(0); }
            if end < 2 {
                assert_eq!(w.motion_limits(0, 0.0).unwrap().0, ordinary * 1.6);
                for _ in 0..24 { w.advance_boost(0, false); }
                assert_eq!(w.snakes[0].len, 24);
            } else {
                assert!(!w.snakes[0].alive);
                assert_eq!((w.snakes[0].effect_kind, w.snakes[0].effect_ticks), (0, 0));
            }
        }
    }

    #[test]
    fn surge_stays_tangible_and_respects_self_collision_setting_and_wrap() {
        for self_collisions in [false, true] {
            let mut w = arena(false, self_collisions, 24);
            grant_surge(&mut w, 0);
            let head = w.segments[0];
            w.segments[15] = head;
            w.mark_collisions();
            assert_eq!(w.snakes[0].dying, if self_collisions { DeathReason::SelfHit } else { DeathReason::None });
        }
        for walls in [false, true] {
            let mut w = arena(walls, true, 24);
            grant_surge(&mut w, 0);
            let points: Vec<_> = (0..24).map(|j| Point { x: 3999.0 - j as f64 * 7.08, y: 1000.0 }).collect();
            w.diagnostic_body(0, &points, 0.0).unwrap();
            let mut straight = ScriptedController::new(|_, s: SnakeView<'_>| Steering { desired_angle: s.angle, rush: 0.6 });
            w.step(&mut straight);
            assert_eq!(w.snakes[0].alive, !walls);
            if walls { assert_eq!(w.last_death_reason(0), Some(DeathReason::Wall)); }
            else { assert!(w.segments[0].current.x < 10.0); }
        }
        let mut w = arena(false, true, 24);
        grant_surge(&mut w, 0);
        // The world owns head/body collision rules; SURGE changes no flags.
        assert!(!effects::modifiers(1, 180).intangible);
        w.snakes[1] = w.snakes[0];
        w.snakes[1].len = 20;
        for j in 0..20 { w.segments[MAX_SEGMENTS + j] = Segment { current: Point { x: 1500.0, y: 1500.0 }, previous: Point { x: 1500.0, y: 1500.0 } }; }
        w.segments[MAX_SEGMENTS + 12] = w.segments[0];
        w.mark_collisions();
        assert_eq!(w.snakes[0].dying, DeathReason::Body);
    }

    #[test]
    fn surge_bonus_only_for_viable_nearby_prey_including_wrap() {
        for walls in [false, true] {
            let mut w = arena(walls, true, 24);
            w.snakes[1] = w.snakes[0];
            w.snakes[1].len = 20;
            let head = w.segments[0].current;
            for distance in [149.9, 150.0, 150.1] {
                w.segments[MAX_SEGMENTS].current = Point { x: head.x + distance, y: head.y };
                assert_eq!(Surge::ai_bonus(&w, w.snake(0).unwrap(), head), if distance <= 150.0 { 6.0 } else { 0.0 });
            }
            w.snakes[1].len = 21;
            w.segments[MAX_SEGMENTS].current = head;
            assert_eq!(Surge::ai_bonus(&w, w.snake(0).unwrap(), head), 0.0);
            w.snakes[1].len = 20;
            w.snakes[1].effect_kind = 3; w.snakes[1].effect_ticks = 120;
            assert_eq!(Surge::ai_bonus(&w, w.snake(0).unwrap(), head), 0.0);
            w.snakes[1].effect_kind = 0; w.snakes[1].effect_ticks = 0;
            w.snakes[1].alive = false;
            assert_eq!(Surge::ai_bonus(&w, w.snake(0).unwrap(), head), 0.0);
            w.snakes[1].alive = true;
            w.segments[0].current.x = 10.0;
            w.segments[MAX_SEGMENTS].current.x = 3990.0;
            assert_eq!(Surge::ai_bonus(&w, w.snake(0).unwrap(), head), if walls { 0.0 } else { 6.0 });
        }
    }
}
