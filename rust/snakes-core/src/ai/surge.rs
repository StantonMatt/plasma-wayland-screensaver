// SPDX-License-Identifier: GPL-3.0-or-later
//! SURGE policy inputs to the existing checked cutoff and safety planners.
use super::*;

#[inline]
pub(super) fn active(w: &World, s: SnakeView<'_>) -> bool {
    forecast::Effect::observed(w,s).is(crate::effects::EffectKind::Surge)
}

/// Free bursts make even a shy snake interested in checked cutoffs. Changing
/// this borrowed view cannot affect the world's traits or scripted controls.
#[inline]
pub(super) fn planner_view<'a>(w: &World, mut s: SnakeView<'a>) -> SnakeView<'a> {
    if active(w, s) { s.traits.aggression = s.traits.aggression.max(0.85*(2.0*aggression::level(w)).min(1.0)); }
    s
}

#[inline]
pub(super) fn advantage(w: &World, s: SnakeView<'_>, ordinary: usize) -> usize {
    if active(w, s) { 4 + w.boost_segment_cost(s.id as usize).unwrap_or(0) } else { ordinary }
}


/// Spend a free burst closing on a real prey. The existing boosted candidate
/// competes with unboosted controls and must pass the full physical rollout.
/// No burst on exploration, a coil, or a sharp turn inside its turning disk.
pub(super) fn pursuit_burst(w:&World,s:SnakeView<'_>,state:State)->bool {
    if !active(w,s) || state.prey==0 || state.coil_radius>0.0 || !w.boost_ready(s.id as usize) {return false;}
    let goal=state.waypoint.unwrap_or(state.goal);
    let d=w.displacement(s.segments[0].current,goal);
    let angle=normalize_angle(d.y.atan2(d.x)-s.angle).abs();
    let (speed,turn)=w.motion_limits(s.id as usize,0.6).unwrap();
    angle<0.6 && (d.x*d.x+d.y*d.y).sqrt()>speed/turn.max(0.01)*(1.0-angle.cos()).max(0.3)+s.radius*2.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Config, RuleSet};
    use std::f64::consts::{PI, FRAC_PI_2};

    fn duel(surged: bool) -> World {
        let mut w = World::diagnostic_arena(Config {store_power_ups:false,aggression:50, width: 1600.0, height: 1000.0,
            density: 0.0, trails: 0.0, scale: 70.0, seed: 73, intelligence: 100.0,
            rules: RuleSet::V2, self_collisions: true, deadly_walls: true,
            ..Config::default() }, &[
                (Point { x: 800.0, y: 450.0 }, FRAC_PI_2, 72, 0.1),
                (Point { x: 870.0, y: 525.0 }, PI, 24, 0.6),
            ], &[]).unwrap();
        if surged { crate::effects::grant_surge(&mut w, 0); }
        w
    }

    #[test]
    fn surge_bursts_need_a_prey_and_a_forward_turnable_approach() {
        let mut w=duel(true);let s=w.snake(0).unwrap();
        let mut state=State {goal:Point{x:800.0,y:700.0},desired:s.angle,..State::default()};
        assert!(!pursuit_burst(&w,s,state),"never burst while wandering");
        state.prey=2;
        assert!(pursuit_burst(&w,s,state));
        state.goal=Point{x:800.0,y:460.0};
        assert!(!pursuit_burst(&w,s,state),"do not overshoot a close prey goal");
        state.goal=Point{x:850.0,y:450.0};
        assert!(!pursuit_burst(&w,s,state),"do not burst into a sharp turn");
        state.goal=Point{x:800.0,y:700.0};w.snakes[0].cooldown_ticks=10;
        assert!(!pursuit_burst(&w,w.snake(0).unwrap(),state));
        w.snakes[0].cooldown_ticks=0;w.snakes[0].effect_ticks=0;
        assert!(!pursuit_burst(&w,w.snake(0).unwrap(),state),"paid chase stays forbidden");
    }
    #[test]
    fn surge_planning_is_aggressive_without_changing_traits_or_classic() {
        let mut w = duel(true);
        let original = w.snake(0).unwrap();
        let planned = planner_view(&w, original);
        assert_eq!(planned.traits.aggression, 0.85);
        assert_eq!(advantage(&w, planned, 6), 4);
        assert_eq!(w.snake(0).unwrap().traits.aggression, 0.1);
        w.config.rules = RuleSet::Classic;
        assert_eq!(planner_view(&w, w.snake(0).unwrap()).traits.aggression, 0.1);
        assert_eq!(advantage(&w, w.snake(0).unwrap(), 8), 8);
    }

    #[test]
    fn surge_cutoff_admission_and_checked_ai_use_in_controlled_crossing() {
        let plain = duel(false);
        let mut passive = AiController::new();
        passive.steer(&plain, plain.snake(0).unwrap());
        assert_eq!(passive.competition_debug(0).unwrap().prey, None);

        let mut w = duel(true);
        let mut ai = AiController::new();
        let mut attacks = 0;
        let mut boosts = 0;
        let mut was_boosting = false;
        for _ in 0..120 {
            let s = w.snake(0).unwrap();
            if !s.alive || !w.snake(1).unwrap().alive { break; }
            let input = ai.steer(&w, s);
            let debug = ai.competition_debug(0).unwrap();
            attacks += usize::from(debug.attack_stage > 0);
            if input.rush > 0.0 && !was_boosting { boosts += 1; }
            was_boosting = s.boost_ticks > 0 || input.rush > 0.0;
            let mut scripted = crate::controller::ScriptedController::new(|_, v: SnakeView<'_>| {
                if v.id == 0 { input } else { Steering { desired_angle: v.angle, rush: 0.0 } }
            });
            w.step(&mut scripted);
        }
        assert!(attacks > 0 && boosts > 0, "attacks={attacks} boosts={boosts}");
        assert_eq!(w.stats().self_deaths, 0);
        assert_eq!(w.stats().wall_deaths, 0);
    }

    #[test]
    fn free_cutoff_retains_its_zero_payment_after_the_first_movement() {
        let mut w=duel(true);
        let mut ai=AiController::new();
        let input=ai.steer(&w,w.snake(0).unwrap());
        let state=ai.states[0];
        assert!(state.attack.valid);
        assert!(state.attack.free_boost);
        let mut scripted=crate::controller::ScriptedController::new(|_,s:SnakeView<'_>| {
            if s.id==0 {input} else {Steering {desired_angle:s.angle,rush:0.0}}
        });
        w.step(&mut scripted);
        assert!(ai.attack_usable(&w,w.snake(0).unwrap(),state,state.attack));
    }

    #[test]
    fn free_cutoff_retains_planned_speed_change_at_surge_expiry() {
        let mut w=duel(true);
        w.snakes[0].effect_ticks=2;
        let mut ai=AiController::new();
        let input=ai.steer(&w,w.snake(0).unwrap());
        let state=ai.states[0];
        assert!(state.attack.valid && state.attack.free_boost);
        let mut scripted=crate::controller::ScriptedController::new(|_,s:SnakeView<'_>| {
            if s.id==0 {input} else {Steering {desired_angle:s.angle,rush:0.0}}
        });
        w.step_n(&mut scripted,2);
        assert_eq!(w.snake(0).unwrap().effect_ticks,0);
        assert!(w.snake(0).unwrap().boost_ticks>0);
        assert_eq!(w.snake(0).unwrap().segments.len(),72);
        assert!(ai.attack_usable(&w,w.snake(0).unwrap(),state,state.attack));
        // A changed effect or unplanned speed is still a reason to replan.
        crate::effects::grant_surge(&mut w,0);
        assert!(!ai.attack_usable(&w,w.snake(0).unwrap(),state,state.attack));
    }

    #[test]
    fn surge_rival_reach_and_margin_expire_at_exact_forecast_tick() {
        let mut w = duel(true);
        w.snakes[0].effect_ticks = 2;
        let mut ai = AiController::new();
        ai.prepare(&w);
        assert_eq!(ai.effects.at(0,1).ticks,2);
        assert_eq!(ai.effects.reach_scale(0,1),1.6);
        assert_eq!(ai.effects.reach_scale(0,2),1.6);
        assert_eq!(ai.effects.reach_scale(0,3),1.0);
        // Exact predicted motion still uses physical speed, never virtual 1.6x
        // speed. An inflated path could award imaginary winning head contacts.
        let speed = crate::effects::forecast_motion(&w, 0, 0.0, 0).0;
        assert!((w.distance_squared(ai.rivals[0].path[0], ai.rivals[0].path[1]).sqrt()
            - speed * STEP_SECONDS).abs() < 1e-10);
    }

    #[test]
    fn surge_rival_body_safety_margin_is_wider_but_own_body_is_physical() {
        let mut w = duel(false);
        // A side-by-side route outside ordinary contact, inside SURGE reserve.
        let body: Vec<_> = (0..24).map(|j| Point { x: 850.0 - j as f64 * 7.08, y: 462.0 }).collect();
        w.diagnostic_body(1, &body, 0.0).unwrap();
        let mut ordinary = AiController::new();
        ordinary.prepare(&w);
        let p = Point { x: 750.0, y: 450.0 };
        assert!(!ordinary.body_blocked(&w, w.snake(0).unwrap(), p, p, 0.0, 0.0, &mut 0).0);
        crate::effects::grant_surge(&mut w, 1);
        let mut surged = AiController::new();
        surged.prepare(&w);
        assert!(surged.body_blocked(&w, w.snake(0).unwrap(), p, p, 0.0, 0.0, &mut 0).0);
        // After expiry the same route becomes available again.
        assert!(!surged.body_blocked(&w, w.snake(0).unwrap(), p, p, 6.0, 0.0, &mut 0).0);
    }

    #[test]
    fn surge_forecast_latches_free_price_and_covers_expiry_beyond_burst_table() {
        let mut w = duel(true);
        for remaining in [1, 2, 8, 25, 36, 100, 180] {
            w.snakes[0].effect_ticks = remaining;
            let motion = Motion::forecast(&w, 0, 0.6);
            for offset in 0..=137 {
                assert_eq!(motion.at(offset), crate::effects::forecast_motion(&w, 0, 0.6, offset),
                    "remaining={remaining} offset={offset}");
            }
        }
        w.snakes[0].effect_ticks = 8;
        let surged = Motion::forecast(&w, 0, 0.6);
        // At expiry the ongoing burst is still free and at the original length.
        w.snakes[0].effect_ticks = 0;
        let full_length_speed = w.motion_limits(0, 0.6).unwrap().0;
        assert_eq!(surged.at(8).0, full_length_speed);
    }

    #[test]
    fn surge_chains_checked_cutoffs_on_a_second_encounter() {
        let mut w = duel(true);
        let mut ai = AiController::new();
        for encounter in 0..2 {
            let input = ai.steer(&w, w.snake(0).unwrap());
            assert_eq!(ai.competition_debug(0).unwrap().prey, Some(1));
            assert!(ai.competition_debug(0).unwrap().attack_stage > 0);
            assert_eq!(input.rush, 0.6, "encounter={encounter}");
            let mut burst = crate::controller::ScriptedController::new(|_, s: SnakeView<'_>| {
                if s.id == 0 { input } else { Steering { desired_angle: s.angle, rush: 0.0 } }
            });
            w.step(&mut burst);
            assert_eq!(w.snake(0).unwrap().boost_ticks, 24);
            // A fresh controlled crossing after the real burst/cooldown runs
            // out. Do not reset the boost, effect duration or attack planner.
            w.snakes[1].alive = false;
            let mut straight = crate::controller::ScriptedController::new(|_, s: SnakeView<'_>| Steering { desired_angle: s.angle, rush: 0.0 });
            w.step_n(&mut straight, 42);
            assert!(w.boost_ready(0));
            assert_eq!(w.snake(0).unwrap().segments.len(), 72);
            let bodies = [(800.0, 450.0, FRAC_PI_2, 72), (870.0, 525.0, PI, 24)];
            for (id, (x, y, angle, len)) in bodies.into_iter().enumerate() {
                let points: Vec<_> = (0..len).map(|j| Point { x: x - angle.cos() * 7.08 * j as f64,
                    y: y - angle.sin() * 7.08 * j as f64 }).collect();
                w.diagnostic_body(id, &points, angle).unwrap();
                if id == 1 { w.snakes[id].generation += 1; }
            }
        }
        assert_eq!(w.stats().self_deaths, 0);
        assert_eq!(w.stats().wall_deaths, 0);
        assert!(w.snake(0).unwrap().effect_ticks > 0);
    }
}
