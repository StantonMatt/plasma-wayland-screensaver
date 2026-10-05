// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::controller::{ScriptedController, Steering};
fn arena(len: usize) -> World {
    World::diagnostic_arena(Config { width:4096.0, height:1440.0, density:0.0,
        deadly_walls:false, rules:RuleSet::V2, ..Config::default() },
        &[(Point{x:2000.0,y:700.0},0.0,len,1.0)], &[]).unwrap()
}
#[test]
fn boost_duration_cost_schedule_tail_pellets_and_cooldown() {
    for len in [12,24,99,100,199,200,1600] {
        let mut w=arena(len);let rng=w.rng_state();let cost=2+len/100;
        let mut paid=0;
        for tick in 1..=24 {
            let tail=w.segments[w.snakes[0].len-1].current;
            w.advance_boost(0,true);
            assert_eq!(w.snakes[0].boost_ticks,25-tick);
            assert_eq!(w.snake(0).unwrap().flags & flags::BOOSTING,flags::BOOSTING);
            let due=cost*(tick as usize).min(21)/21;
            assert_eq!(w.snakes[0].len,len-due);
            assert_eq!(w.food.len(),due);
            if due>paid {assert!(w.distance_squared(w.food[paid].p,tail)<1e-18);}
            for f in &w.food {
                assert_eq!(f.kind,FoodKind::Pellet);assert_eq!(f.value,0.5);
                assert_eq!(f.life,8.0);assert_eq!(FoodView::from(f).life_fraction,255);
            }
            paid=due;
        }
        assert_eq!(w.rng_state(),rng,"pellets must never draw RNG");
        for tick in 0..36 {
            w.advance_boost(0,true);
            assert_eq!(w.snakes[0].boost_ticks,0);
            assert_eq!(w.snakes[0].cooldown_ticks,36-tick);
            assert_eq!(w.snake(0).unwrap().flags & flags::COOLDOWN,flags::COOLDOWN);
        }
        w.advance_boost(0,true);
        if len-cost>=12 {assert_eq!(w.snakes[0].boost_ticks,24);}
        else {assert_eq!(w.snakes[0].boost_ticks,0);}
    }
}
#[test]
fn boost_speed_digestion_turn_limit_minimum_and_free_rush_removed() {
    let mut w=arena(24);let base=w.speed(&w.snakes[0]);let turn=w.turn_rate(&w.snakes[0]);
    w.advance_boost(0,true);
    assert!((w.speed(&w.snakes[0])/base-1.6).abs()<1e-12);
    assert_eq!(w.turn_rate(&w.snakes[0]),turn);
    w.snakes[0].growth=1.0;
    assert!((w.speed(&w.snakes[0])/base-1.8).abs()<1e-12);
    w.snakes[0].boost_ticks=0;w.snakes[0].cooldown_ticks=10;
    w.advance_boost(0,true);assert_eq!(w.snakes[0].rush,0.0);
    let mut small=arena(11);small.advance_boost(0,true);
    assert_eq!(small.snakes[0].boost_ticks,0);assert!(small.food.is_empty());
    assert_eq!(small.motion_limits(0,1.0),small.motion_limits(0,0.0));
    let mut min=arena(12);assert!(min.boost_ready(0));min.advance_boost(0,true);
    assert_eq!(min.snakes[0].boost_ticks,24);
    let mut classic=arena(24);classic.config.rules=RuleSet::Classic;
    classic.snakes[0].rush=0.25;
    assert!((classic.speed(&classic.snakes[0])/base-1.25).abs()<1e-12);
}
#[test]
fn frozen_cuts_boost_and_pellets_expire() {
    let mut w=arena(24);w.advance_boost(0,true);
    w.snakes[0].frozen_ticks=75;
    assert!(!w.boost_ready(0));w.advance_boost(0,true);
    assert_eq!(w.snakes[0].boost_ticks,0);assert_eq!(w.snakes[0].cooldown_ticks,36);
    assert_eq!(w.snake(0).unwrap().flags & flags::FROZEN,flags::FROZEN);
    w.food.clear();w.add_food(Food {p:Point{x:50.0,y:50.0},value:0.5,life:8.0,kind:FoodKind::Pellet,..Food::default()});
    w.update_food(4.0);assert_eq!(w.food[0].life,4.0);
    assert_eq!(FoodView::from(&w.food[0]).life_fraction,128);
    let id=w.food[0].id;w.update_food(4.0);assert!(!w.food.iter().any(|f|f.id==id));
}
#[test]
fn leader_hysteresis_and_corpse_seventeen_frames() {
    let mut w=arena(30);w.update_leader(0);
    assert_eq!(w.leader,Some(0));
    w.snakes[1]=w.snakes[0];w.snakes[1].len=32;
    w.update_leader(1);assert_eq!(w.leader,Some(0));
    w.snakes[1].len=33;w.update_leader(2);assert_eq!(w.leader,Some(1));
    assert_eq!(w.frame_events().last().unwrap().kind,EventKind::Succession);
    w.snakes[1].alive=false;w.snakes[1].len=0;w.update_leader(3);assert_eq!(w.leader,Some(0));
    let before=w.segments[..30].to_vec();w.snakes[0].dying=DeathReason::Wall;
    w.explode_snake(0);assert_eq!(w.snake(0).unwrap().flags,flags::CORPSE);
    assert_eq!(w.stats().total_segments,0);assert_eq!(w.exported_segment_count(),30);
    let mut straight=ScriptedController::new(|_,s:SnakeView<'_>|Steering{desired_angle:s.angle,rush:0.0});
    for _ in 0..16 {w.step(&mut straight);assert_eq!(w.snake(0).unwrap().segments,before);}
    w.step(&mut straight);assert!(w.snake(0).unwrap().segments.is_empty());
    assert_eq!(w.snake(0).unwrap().flags,0);
}
#[test]
fn events_are_bounded_oldest_first_and_intent_hooks_persist() {
    let mut w=arena(24);
    for tick in 0..40 {w.push_event(FrameEvent{tick,..FrameEvent::default()});}
    assert_eq!(w.frame_events().len(),32);
    assert_eq!(w.frame_events().map(|e|e.tick).collect::<Vec<_>>(),(8..40).collect::<Vec<_>>());
    assert!(w.set_intent_flags(0,u32::MAX));assert!(!w.set_intent_flags(20,0));
    let mut c=ScriptedController::new(|_,s:SnakeView<'_>|Steering{desired_angle:s.angle,rush:0.0});
    w.step(&mut c);assert_eq!(w.snake(0).unwrap().flags,flags::HUNTING|flags::TRAPPED);
    assert_eq!(w.frame_events().len(),0);
}
#[test]
fn v2_deterministic_full_steps_and_snapshot() {
    let cfg=Config{seed:73,rules:RuleSet::V2,self_collisions:true,..Config::default()};
    let mut a=World::new(cfg).unwrap();let mut b=a.diagnostic_snapshot();
    let mut ca=ScriptedController::new(|tick,s:SnakeView<'_>|Steering{desired_angle:tick as f64*0.017+s.id as f64*0.41,rush:0.25});
    let mut cb=ScriptedController::new(|tick,s:SnakeView<'_>|Steering{desired_angle:tick as f64*0.017+s.id as f64*0.41,rush:1.0});
    for _ in 0..1000 {
        a.step(&mut ca);b.step(&mut cb);
        assert_eq!(a.rng_state(),b.rng_state());assert_eq!(a.stats(),b.stats());
        assert_eq!(a.frame_events().collect::<Vec<_>>(),b.frame_events().collect::<Vec<_>>());
        for (s,t) in a.snakes().zip(b.snakes()) {
            assert_eq!(s.segments,t.segments);assert_eq!(s.flags,t.flags);
            assert_eq!(s.boost_ticks,t.boost_ticks);assert_eq!(s.angle,t.angle);
        }
        for (f,g) in a.foods().zip(b.foods()) {assert_eq!(f.position,g.position);assert_eq!(f.kind,g.kind);assert_eq!(f.life_fraction,g.life_fraction);}
    }
}

#[test]
fn resizing_scales_the_entire_wrapped_event_ring() {
    let mut w=arena(24);
    for tick in 0..40 {
        w.push_event(FrameEvent{tick,position:Point{x:100.0+tick as f64,y:200.0+tick as f64},..FrameEvent::default()});
    }
    let rng=w.rng_state();
    w.resize(8192.0,2880.0).unwrap();
    assert_eq!(w.rng_state(),rng);
    assert_eq!(w.frame_events().len(),32);
    for (tick,event) in (8..40).zip(w.frame_events()) {
        assert_eq!(event.tick,tick);
        assert_eq!(event.position,Point{x:2.0*(100.0+tick as f64),y:2.0*(200.0+tick as f64)});
    }
}

#[test]
fn taper_profile_matches_the_renderer_contract_and_size_tiers() {
    use crate::shape::taper;
    for (u,expected) in [(-1.0,0.84),(0.0,0.84),(0.035,0.92),(0.07,1.0),
        (0.4,1.0),(0.6,1.0),(1.0,0.22),(2.0,0.22)] {
        assert!((taper(u)-expected).abs()<1e-12,"u={u}");
    }
    for len in [1,12,23,24,99,100,249,250,MAX_SEGMENTS] {
        for j in 0..len {
            let u=j as f64/len.saturating_sub(1).max(1) as f64;
            assert_eq!(taper::body_radius(6.0,j as f64,len),6.0*taper(u));
        }
    }
    assert_eq!(taper::span_radius(6.0,0.0,1.0),6.0);
    assert_eq!(taper::span_radius(6.0,0.8,1.0),6.0*taper(0.8));
}

#[test]
fn tapered_tail_contacts_match_the_radius_with_classic_unchanged() {
    for same in [false,true] {for wrap in [false,true] {
        for len in [24,100,250] {for j in [10,len-1] {
            for inside in [false,true] {for rules in [RuleSet::Classic,RuleSet::V2] {
                let mut w=arena(len);w.config.rules=rules;w.config.self_collisions=true;
                w.config.deadly_walls=!wrap;
                let owner=if same {0} else {1};
                if !same {w.snakes[1]=w.snakes[0];}
                for id in 0..=owner {for k in 0..len {
                    let p=Point{x:1000.0+k as f64*8.0,y:1100.0};
                    w.segments[id*MAX_SEGMENTS+k]=Segment{current:p,previous:p};
                }}
                let radius=taper::body_radius(6.0,j as f64,len);
                let tapered=taper::contact_radius(RuleSet::V2,6.0,radius,same);
                let full=taper::contact_radius(RuleSet::Classic,6.0,6.0,same);
                let distance=if inside {tapered-0.01} else if tapered<full {(tapered+full)*0.5} else {full+0.01};
                let head=Point{x:if wrap {2.0} else {500.0},y:700.0};
                let body=w.canonical_point(Point{x:head.x-distance,y:head.y});
                w.segments[0]=Segment{current:head,previous:head};
                w.segments[owner*MAX_SEGMENTS+j]=Segment{current:body,previous:body};
                w.mark_collisions();
                let hit=inside || (rules==RuleSet::Classic && tapered<full);
                assert_eq!(w.snakes[0].dying,if hit {
                    if same {DeathReason::SelfHit} else {DeathReason::Body}
                } else {DeathReason::None},"same={same} wrap={wrap} len={len} j={j} rules={rules:?} inside={inside}");
            }}
        }}
    }}
}

#[test]
fn swept_contact_with_the_thin_tail_is_still_lethal() {
    let mut w=arena(24);w.snakes[1]=w.snakes[0];
    let tail=Point{x:500.0,y:700.0};
    w.segments[MAX_SEGMENTS+23]=Segment{current:tail,previous:tail};
    w.segments[0]=Segment{previous:Point{x:500.0,y:690.0},current:Point{x:500.0,y:710.0}};
    w.mark_collisions();assert_eq!(w.snakes[0].dying,DeathReason::Body);
    assert_eq!(w.collisions[0].owner_mask,1<<1);
}
