// SPDX-License-Identifier: GPL-3.0-or-later
//! Scavenging utility enters target selection before routing and safe rollouts.
//! Never adjust the final checked steering: increased reach is not immunity.
use crate::{effects::EffectKind, FoodKind};
use super::target::TargetFood;
#[cfg(test)] use crate::{SnakeView, World};

#[inline]
pub(super) fn scavenging(food:TargetFood)->bool {
    food.kind==FoodKind::Prism || food.feast_id!=0
}

#[cfg(test)]
#[inline]
pub(super) fn food_bonus(world: &World, snake: SnakeView<'_>, food: impl Into<TargetFood>, eta: f64) -> f64 {
    food_bonus_effect(food,super::forecast::Track::observed(world,snake).before(crate::effects::forecast_step(eta)))
}

pub(super) fn food_bonus_effect(food:impl Into<TargetFood>,effect:super::forecast::Effect)->f64 {
    let food=food.into();
    if !effect.is(EffectKind::Magnet) {return 0.0;}
    if food.kind==FoodKind::Prism {2.0} else if food.feast_id!=0 {1.5} else {0.0}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RuleSet, ai::{AiController, State}, Config, Segment, Traits, MAX_SEGMENTS};
    use crate::world::Food;

    fn scenario() -> World {
        let mut w = World::new(Config { rules: RuleSet::V2, density: 0.0,
            intelligence: 100.0, deadly_walls: false, ..Config::default() }).unwrap();
        for s in &mut w.snakes { s.alive = false; s.len = 0; }
        w.food.clear();
        let s = &mut w.snakes[0]; s.alive = true; s.len = 24; s.radius = 6.0;
        s.angle = 0.0; s.traits = Traits { speed_bias: 1.0, aggression: 0.0, ..Traits::default() };
        for j in 0..24 {
            let p = crate::Point { x: 400.0 - j as f64 * 7.08, y: 400.0 };
            w.segments[j] = Segment { current: p, previous: p };
        }
        w
    }
    fn target(w: &World) -> u64 {
        let mut ai = AiController::new(); ai.prepare(w);
        let mut state = State::default();
        ai.strategy(w, w.snake(0).unwrap(), &mut state, 0.0);
        state.target
    }
    #[test]
    fn magnet_ai_selects_death_fields_and_prism_over_nearer_equal_value_food() {
        for kind in [FoodKind::Shard, FoodKind::Prism] {
            let mut w = scenario();
            for (id, x, kind) in [(1, 540.0, FoodKind::Spark), (2, 620.0, kind)] {
                w.food.push(Food { id, p: crate::Point { x, y: 400.0 }, value: 1.0,
                    life: 30.0, owner: -1, kind,
                    feast: if kind == FoodKind::Shard { 1 } else { 0 }, ..Food::default() });
            }
            assert_eq!(target(&w), 1, "ordinary {kind:?}");
            w.snakes[0].effect_kind = EffectKind::Magnet as u8; w.snakes[0].effect_ticks = 300;
            assert_eq!(target(&w), 2, "magnet {kind:?}");
            w.snakes[0].effect_ticks = 1;
            assert_eq!(target(&w), 1, "expiry {kind:?}");
            w.snakes[0].effect_ticks = 300; w.config.rules = RuleSet::Classic;
            assert_eq!(target(&w), 1, "classic {kind:?}");
        }
    }
    #[test]
    fn magnet_scavenging_rejects_a_field_without_turnaround_space() {
        let mut w=scenario();
        w.snakes[0].effect_kind=EffectKind::Magnet as u8;
        w.snakes[0].effect_ticks=300;
        for (id,x,y,kind) in [(1,540.0,300.0,FoodKind::Spark),(2,620.0,400.0,FoodKind::Prism)] {
            w.food.push(Food {id,p:crate::Point{x,y},value:1.0,life:30.0,
                owner:-1,kind,..Food::default()});
        }
        assert_eq!(target(&w),2);
        let mut ai=AiController::new();ai.prepare(&w);
        // Use a real diagnostic coil to populate conservative navigable cells.
        let coil:Vec<_>=(0..160).map(|j| {
            let a=j as f64*std::f64::consts::TAU/159.0;
            crate::Point{x:620.0+55.0*a.cos(),y:400.0+55.0*a.sin()}
        }).collect();
        let other=w.snakes[0];w.snakes[1]=other;
        w.diagnostic_body(1,&coil,0.0).unwrap();
        ai.tick=u64::MAX;ai.prepare(&w);
        let mut state=State::default();
        ai.strategy(&w,w.snake(0).unwrap(),&mut state,0.0);
        assert_eq!(state.target,1);
    }

    #[test]
    fn magnet_does_not_turn_toward_food_already_within_pull_reach() {
        let mut w=scenario();
        w.snakes[0].effect_kind=EffectKind::Magnet as u8;
        w.snakes[0].effect_ticks=300;
        w.food.push(Food {id:7,p:crate::Point{x:430.0,y:425.0},value:8.0,
            life:30.0,owner:-1,kind:FoodKind::Prism,..Food::default()});
        assert_eq!(target(&w),0);
    }

    #[test]
    fn magnet_ai_takes_capsule_and_rivals_race_for_unclaimed_death_fields() {
        use crate::world::Item;
        let mut w = scenario();
        w.items.push(Item { id: 17, kind: EffectKind::Magnet,
            position: crate::Point { x: 560.0, y: 400.0 }, life_ticks: 750,
            radius: 12.0, ..Item::default() });
        assert_eq!(target(&w), 17 | (1 << 63));
        w.items.clear();
        w.food.push(Food { id: 25, p: crate::Point { x: 620.0, y: 400.0 }, value: 4.0,
            kind: FoodKind::Shard, feast: 9, owner: -1, life: 25.0, ..Food::default() });
        let other = w.snakes[0]; w.snakes[1] = other;
        w.snakes[1].effect_kind = EffectKind::Magnet as u8; w.snakes[1].effect_ticks = 300;
        for j in 0..24 { w.segments[MAX_SEGMENTS + j] = w.segments[j];
            w.segments[MAX_SEGMENTS + j].current.y += 160.0; }
        assert_eq!(target(&w), 25); // Non-magnet snake contests the same field.
        w.food[0].owner = 1;
        assert_eq!(target(&w), 0); // Claimed food cannot be stolen.
    }
}
