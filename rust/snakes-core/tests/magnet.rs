// SPDX-License-Identifier: GPL-3.0-or-later
use snakes_core::{effects::{self, EffectKind}, ai::AiController, Config, RuleSet, World};

#[test]
fn magnet_public_modifiers_preserve_motion_and_collision_controls() {
    for ticks in [1, 36, 299, 300] {
        let magnet = effects::modifiers(EffectKind::Magnet as u8, ticks);
        let neutral = effects::modifiers(0, ticks);
        assert_eq!(magnet.food_reach, 9.0);
        assert_eq!(magnet.speed, neutral.speed);
        assert_eq!(magnet.free_boost, neutral.free_boost);
        assert_eq!(magnet.boost_cooldown, neutral.boost_cooldown);
        assert_eq!(magnet.intangible, neutral.intangible);
        assert_eq!(magnet.flags, neutral.flags);
    }
    assert_eq!(effects::modifiers(EffectKind::Magnet as u8, 0).food_reach, 3.0);
}

#[test]
fn magnet_item_ecosystem_replays_deterministically() {
    let config = Config { rules: RuleSet::V2, deadly_walls: false,
        intelligence: 100.0, seed: 73, ..Config::default() };
    let mut a = World::new(config).unwrap();
    let mut b = World::new(config).unwrap();
    let mut ai_a = AiController::new();
    let mut ai_b = AiController::new();
    for _ in 0..1000 {
        a.step(&mut ai_a); b.step(&mut ai_b);
        assert_eq!(a.rng_state(), b.rng_state());
        assert_eq!(a.items().copied().collect::<Vec<_>>(), b.items().copied().collect::<Vec<_>>());
        for (s, t) in a.snakes().zip(b.snakes()) {
            assert_eq!(s.alive, t.alive);
            assert_eq!((s.effect_kind, s.effect_ticks), (t.effect_kind, t.effect_ticks));
            assert_eq!(s.segments.first().map(|p|p.current), t.segments.first().map(|p|p.current));
        }
    }
}
