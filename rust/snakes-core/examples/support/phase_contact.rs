// SPDX-License-Identifier: GPL-3.0-or-later
//! Phase-use accounting: a contact matters only if tangibility would kill the
//! holder. A winning head encounter never suppresses independent body checks.
use snakes_core::{SnakeView, World};
pub fn otherwise_lethal(w:&World,s:SnakeView<'_>)->bool {
    let head=s.segments[0];
    for rival in w.snakes().filter(|r|r.alive) {
        let same=rival.id==s.id;
        if !same && snakes_core::effects::modifiers(rival.effect_kind,rival.effect_ticks).intangible {continue;}
        if !same && s.segments.len()<rival.segments.len()+4
            && w.segments_distance_squared(head.previous,head.current,rival.segments[0].previous,rival.segments[0].current)<((s.radius+rival.radius)*0.82).powi(2) {return true;}
        if same && !w.config().self_collisions {continue;}
        for (j,b) in rival.segments.iter().enumerate().skip(if same {10} else {1}) {
            let radius=rival.radius*snakes_core::shape::taper(j as f64/rival.segments.len().saturating_sub(1).max(1) as f64);
            if w.segments_distance_squared(head.previous,head.current,b.previous,b.current)<((s.radius+radius)*if same {0.74} else {0.78}).powi(2) {return true;}
        }
    }
    false
}
#[cfg(test)]
mod tests {
    use super::*;
    use snakes_core::{Config,Point,RuleSet,DeathReason,controller::{ScriptedController,Steering}};
    #[test]
    fn winning_head_is_not_phase_use_but_ties_losses_and_bodies_are() {
        for own in [23,24,27,28,40] {
            let p=Point{x:400.0,y:400.0};
            let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,speed:0.0,self_collisions:false,density:0.0,..Config::default()},
                &[(p,0.0,own,0.0),(Point{x:p.x+9.0,y:p.y},std::f64::consts::PI,24,0.0)],&[]).unwrap();
            assert_eq!(otherwise_lethal(&w,w.snake(0).unwrap()),own<28,"length={own}");
            w.step(&mut ScriptedController::new(|_,s:SnakeView<'_>|Steering {desired_angle:s.angle,rush:0.0}));
            assert_eq!(w.snake(0).unwrap().alive,own>=28,"length={own} reason={:?}",w.last_death_reason(0));
            if own>=28 {assert_eq!(w.last_death_reason(1),Some(DeathReason::Head));}
        }
        let p=Point{x:400.0,y:400.0};
        let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,speed:0.0,self_collisions:false,density:0.0,..Config::default()},
            &[(p,0.0,40,0.0),(p,0.0,24,0.0)],&[]).unwrap();
        let mut body:Vec<Point>=w.snake(1).unwrap().segments.iter().map(|b|b.current).collect();body[8]=p;
        w.diagnostic_body(1,&body,0.0).unwrap();
        assert!(otherwise_lethal(&w,w.snake(0).unwrap()),"winning heads still check rival bodies");
    }
    #[test]
    fn self_contacts_obey_self_collision_setting() {
        for enabled in [false,true] {
            let p=Point{x:400.0,y:400.0};
            let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,self_collisions:enabled,density:0.0,..Config::default()},&[(p,0.0,40,0.0)],&[]).unwrap();
            let mut body:Vec<Point>=w.snake(0).unwrap().segments.iter().map(|b|b.current).collect();body[10]=p;
            w.diagnostic_body(0,&body,0.0).unwrap();
            assert_eq!(otherwise_lethal(&w,w.snake(0).unwrap()),enabled);
        }
    }
}
