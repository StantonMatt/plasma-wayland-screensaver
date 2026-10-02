// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::{Config, Segment, Traits};
fn arena(wrap:bool) -> World {
    let mut w=World::new(Config {width:1200.0,height:800.0,scale:70.0,density:0.0,
        self_collisions:true,deadly_walls:!wrap,intelligence:100.0,seed:73,..Config::default()}).unwrap();
    for s in &mut w.snakes {s.alive=false;s.len=0;s.respawn=1000.0;}
    w.food.clear();w
}
fn line(w:&mut World,id:usize,p:Point,angle:f64,len:usize) {
    let s=&mut w.snakes[id];s.alive=true;s.len=len;s.angle=angle;s.desired=angle;s.radius=6.0;
    s.base_radius=6.0;s.birth_len=len;s.growth=0.0;
    s.traits=Traits {speed_bias:1.0,aggression:1.0,..Traits::default()};
    for j in 0..len {
        let p=w.canonical_point(Point{x:p.x-angle.cos()*6.0*1.18*j as f64,y:p.y-angle.sin()*6.0*1.18*j as f64});
        w.segments[id*MAX_SEGMENTS+j]=Segment {current:p,previous:p};
    }
}
fn food(w:&mut World,p:Point) {
    w.food.push(crate::world::Food {id:100000,p,value:1.0,life:100.0,owner:-1,..Default::default()});
}
fn small_grid(w:&World,cols:usize,rows:usize) -> Spatial {
    let mut grid=Spatial::new();grid.rebuild(w);
    grid.cols=cols;grid.rows=rows;
    grid.dx=w.config().width/cols as f64;grid.dy=w.config().height/rows as f64;
    grid
}

#[test]
fn wrapped_food_discovery_shortlists_each_food_once() {
    // Inject degenerate coarse grids too: valid current arenas bottom out at
    // three cells, but enumeration must not depend on that configuration rule.
    for cols in 1..=5 {for rows in 1..=5 {
        let mut w=arena(true);
        line(&mut w,0,Point{x:400.0,y:400.0},0.0,1);
        food(&mut w,Point{x:700.0,y:400.0});
        let mut ai=AiController::new();ai.prepare(&w);
        ai.spatial=small_grid(&w,cols,rows);
        ai.spatial.food_heads.fill(-1);
        let key=ai.spatial.key(w.food[0].p);
        ai.spatial.food_heads[key]=0;ai.spatial.food_next[0]=-1;
        let mut state=State::default();
        ai.strategy(&w,w.snake(0).unwrap(),&mut state,0.0);
        assert_eq!(state.debug.target_count,1,"{cols}x{rows}");
        assert_eq!(state.debug.target_food_ids[0],w.food[0].id);
    }}
}

#[test]
fn wrapped_safety_and_cache_charge_each_body_record_once() {
    for cols in 1..=3 {for rows in 1..=3 {
        let mut w=arena(true);
        line(&mut w,0,Point{x:400.0,y:400.0},0.0,1);
        line(&mut w,1,Point{x:700.0,y:650.0},0.0,121);
        let mut ai=AiController::new();ai.prepare(&w);
        ai.spatial=small_grid(&w,cols,rows);
        // Rebucket the only body into the injected grid.
        ai.spatial.heads.fill(-1);ai.spatial.occupied.fill(0);
        for j in 1..121 {
            let key=ai.spatial.key(w.segments[MAX_SEGMENTS+j].current);
            ai.spatial.next[MAX_SEGMENTS+j]=ai.spatial.heads[key];
            ai.spatial.heads[key]=(MAX_SEGMENTS+j) as i32;
            ai.spatial.occupied[key]=2;
        }
        let s=w.snake(0).unwrap();let p=s.segments[0].current;
        // Large padding covers both axes completely; released records still
        // consume narrow-phase work. Exactly enough budget must remain safe.
        for _ in 0..2 {
            let mut visits=NARROW_LIMIT-120;
            let result=ai.cached_body_blocked(&w,s,p,p,1000.0,1200.0,&mut visits);
            assert_eq!(result,(false,200.0,false),"{cols}x{rows}");
            assert_eq!(visits,NARROW_LIMIT,"{cols}x{rows}");
        }
        let mut visits=NARROW_LIMIT-119;
        assert!(ai.cached_body_blocked(&w,s,p,p,1000.0,1200.0,&mut visits).2);
    }}
}
struct Selective {ai:AiController}
impl Controller for Selective {
    fn steer(&mut self,w:&World,s:SnakeView<'_>) -> Steering {
        if s.id==0 {self.ai.steer(w,s)} else {Steering{desired_angle:s.angle,rush:0.0}}
    }
}
#[test]
fn avoids_losing_head_on_and_crossing_body() {
    for head_on in [false,true] {
        let mut w=arena(false);
        line(&mut w,0,Point{x:400.0,y:400.0},0.0,20);
        if head_on {line(&mut w,1,Point{x:540.0,y:400.0},std::f64::consts::PI,40);}
        else {line(&mut w,1,Point{x:500.0,y:650.0},std::f64::consts::FRAC_PI_2,60);}
        food(&mut w,Point{x:600.0,y:400.0});
        let mut c=Selective {ai:AiController::new()};
        for _ in 0..28 {w.step(&mut c);assert!(w.snake(0).unwrap().alive,"head_on={head_on} reason={:?}",w.last_death_reason(0));}
        assert!(w.snake(0).unwrap().segments[0].current.y!=400.0);
    }
}
#[test]
fn turns_early_for_walls_but_wraps_without_an_artificial_wall() {
    for wrap in [false,true] {
        let mut w=arena(wrap);
        line(&mut w,0,Point{x:1150.0,y:400.0},0.0,24);
        food(&mut w,Point{x:if wrap {80.0} else {1190.0},y:400.0});
        let mut ai=AiController::new();
        w.step_n(&mut ai,45);
        assert!(w.snake(0).unwrap().alive);
        assert_eq!(w.stats().wall_deaths,0);
        if wrap {assert!(w.snake(0).unwrap().segments[0].current.x<300.0);}
    }
}
#[test]
fn capped_fill_reports_uncertainty_and_respects_own_body_rule() {
    let mut w=arena(false);
    line(&mut w,0,Point{x:700.0,y:400.0},0.0,50);
    let mut grid=Spatial::new();grid.rebuild(&w);
    let (area,capped)=grid.area(Point{x:100.0,y:100.0},u16::MAX);
    assert!(capped);assert_eq!(area,spatial::FILL_LIMIT);
    let (open,_)=grid.area(Point{x:100.0,y:100.0},u16::MAX^1);
    assert!(open>=area);
}
#[test]
fn safety_checks_toroidal_body_across_the_seam() {
    let mut w=arena(true);
    line(&mut w,0,Point{x:1170.0,y:400.0},0.0,20);
    line(&mut w,1,Point{x:20.0,y:600.0},std::f64::consts::FRAC_PI_2,60);
    food(&mut w,Point{x:100.0,y:400.0});
    let mut c=Selective{ai:AiController::new()};
    for _ in 0..20 {w.step(&mut c);assert!(w.snake(0).unwrap().alive);}
}
#[test]
fn same_seed_repeats_and_query_does_not_consume_world_randomness() {
    let cfg=Config{width:1800.0,height:1000.0,self_collisions:true,..Config::default()};
    let mut a=World::new(cfg).unwrap();let mut b=World::new(cfg).unwrap();
    let mut ca=AiController::new();let mut cb=AiController::new();
    let initial=a.rng_state();
    for s in a.snakes().filter(|s|s.alive) {assert!(ca.steer(&a,s).is_valid());}
    assert_eq!(initial,a.rng_state());
    // Recreate after the read-only probe so both controller histories match.
    ca=AiController::new();
    for _ in 0..300 {
        a.step(&mut ca);b.step(&mut cb);
        assert_eq!(a.stats(),b.stats());assert_eq!(a.rng_state(),b.rng_state());
        for (sa,sb) in a.snakes().zip(b.snakes()) {assert_eq!(sa.angle,sb.angle);assert_eq!(sa.segments,sb.segments);}
    }
}

#[test]
fn smallest_and_largest_valid_arenas_do_not_panic() {
    for width in [80.0,16384.0] {
        for deadly_walls in [false,true] {
            let mut w=World::new(Config{width,height:80.0,deadly_walls,seed:991,
                intelligence:0.0,self_collisions:true,density:100.0,..Config::default()}).unwrap();
            let mut ai=AiController::new();
            w.step_n(&mut ai,30);
            for s in w.snakes().filter(|s|s.alive) {
                assert!(s.angle.is_finite());
                assert!(s.segments.iter().all(|p|p.current.x.is_finite() && p.current.y.is_finite()));
            }
        }
    }
}

#[test]
fn size_advantage_can_win_a_food_contest_by_head_collision_or_cutoff() {
    let mut w=arena(false);
    line(&mut w,0,Point{x:400.0,y:400.0},0.0,32);
    line(&mut w,1,Point{x:500.0,y:400.0},std::f64::consts::PI,16);
    food(&mut w,Point{x:445.0,y:400.0});
    let mut c=Selective{ai:AiController::new()};
    let mut won=false;
    for _ in 0..24 {
        let alive=w.snake(1).unwrap().alive;
        w.step(&mut c);
        assert!(w.snake(0).unwrap().alive);
        if alive && !w.snake(1).unwrap().alive {
            assert!(matches!(w.last_death_reason(1),Some(crate::DeathReason::Head|crate::DeathReason::Body)));
            won=true;break;
        }
    }
    assert!(won,"the bigger snake should take a safe, favorable contest");
}

#[test]
fn continuation_honors_the_turn_then_straight_deadline() {
    let mut w=arena(false);
    line(&mut w,0,Point{x:500.0,y:400.0},0.0,24);
    let mut ai=AiController::new();ai.prepare(&w);
    let s=w.snake(0).unwrap();
    let initial=State {goal:Point{x:900.0,y:400.0},desired:0.0,turn_until:u64::MAX,..State::default()};
    let plan=ai.rollout(&w,s,initial,7,24);
    assert_eq!(plan.steps,24);
    let retained=State {desired:plan.desired,turn_until:16,exit_angle:plan.exit_angle,..initial};
    let continuation=ai.rollout(&w,s,retained,1,24);
    assert_eq!(plan.path,continuation.path);
    assert_eq!(plan.angle,plan.exit_angle);
    w.tick=16;
    let after_deadline=ai.rollout(&w,w.snake(0).unwrap(),retained,1,24);
    assert_eq!(after_deadline.desired,plan.exit_angle);
}

#[test]
fn diagnostic_snapshot_replays_mechanics_and_randomness_exactly() {
    let mut a=World::new(Config {self_collisions:true,seed:20260814,..Config::default()}).unwrap();
    let mut ca=AiController::new();a.step_n(&mut ca,100);
    let mut b=a.diagnostic_snapshot();let mut cb=ca.clone();
    for _ in 0..100 {
        a.step(&mut ca);b.step(&mut cb);
        assert_eq!(a.stats(),b.stats());assert_eq!(a.rng_state(),b.rng_state());
        for (a,b) in a.snakes().zip(b.snakes()) {assert_eq!(a.angle,b.angle);assert_eq!(a.segments,b.segments);}
        for (a,b) in a.foods().zip(b.foods()) {assert_eq!(a.id,b.id);assert_eq!(a.position,b.position);}
    }
}

#[test]
fn area_cache_is_invalidated_on_world_rebuild() {
    let mut w=arena(false);let mut grid=Spatial::new();grid.rebuild(&w);
    let p=Point{x:100.0,y:100.0};
    let open=grid.area(p,u16::MAX);
    assert_eq!(open,grid.area(p,u16::MAX));
    // A closed square in cell occupancy changes the same query's answer.
    grid.rebuild(&w);
    let key=grid.key(p);
    for (x,y) in [(1,0),(-1,0),(0,1),(0,-1)] {if let Some(k)=grid.offset(key,x,y) {grid.occupied[k]=1;}}
    assert_eq!(grid.area(p,u16::MAX),(1,false));
    w.tick+=1;grid.rebuild(&w);
    assert_eq!(grid.area(p,u16::MAX),open);
}

#[test]
fn cached_body_queries_match_uncached_and_preserve_work_limits() {
    let mut w=arena(false);
    line(&mut w,0,Point{x:400.0,y:400.0},0.0,24);
    line(&mut w,1,Point{x:500.0,y:600.0},std::f64::consts::FRAC_PI_2,60);
    let mut ai=AiController::new();ai.prepare(&w);
    let s=w.snake(0).unwrap();let a=s.segments[0].current;let b=Point{x:510.0,y:400.0};
    let mut original_visits=0;
    let expected=ai.body_blocked(&w,s,a,b,0.6,1.0,&mut original_visits);
    assert!(expected.0);assert!(!expected.2);
    for _ in 0..2 {
        let mut visits=0;
        assert_eq!(ai.cached_body_blocked(&w,s,a,b,0.6,1.0,&mut visits),expected);
        assert_eq!(visits,original_visits);
    }
    let mut visits=NARROW_LIMIT;
    assert!(ai.cached_body_blocked(&w,s,a,b,0.6,1.0,&mut visits).2);
}

#[test]
fn turning_space_releases_tails_and_respects_the_self_collision_switch() {
    let mut w=arena(false);
    line(&mut w,0,Point{x:500.0,y:400.0},0.0,24);
    let mid=Point{x:400.0,y:400.0};
    for j in 1..24 {
        let angle=std::f64::consts::TAU*j as f64/23.0;
        let p=Point{x:mid.x+angle.cos()*28.0,y:mid.y+angle.sin()*28.0};
        w.segments[j]=Segment {current:p,previous:p};
    }
    let mut grid=Spatial::new();grid.rebuild(&w);
    let now=grid.space(mid,u16::MAX,64,0.0).0;
    let released=grid.space(mid,u16::MAX,64,10.0).0;
    let disabled=grid.space(mid,u16::MAX^1,64,0.0).0;
    assert!(now<released);assert_eq!(released,disabled);
}

#[test]
fn sustained_turn_escape_interrupts_tail_recovery() {
    let mut w=arena(false);line(&mut w,0,Point{x:500.0,y:400.0},0.0,24);
    let mut ai=AiController::new();ai.prepare(&w);
    let s=w.snake(0).unwrap();
    ai.states[0]=State {generation:s.generation,last_angle:s.angle,desired:2.5,turn_accum:3.3,
        turn_until:u64::MAX,escape_until:45,goal:s.segments.last().unwrap().current,..State::default()};
    let steering=ai.steer(&w,s);
    assert_eq!(ai.states[0].escape_until,0);
    assert_eq!(ai.states[0].orbit_until,36);
    assert_eq!(steering.desired_angle,0.0);
}

#[test]
fn diagnostics_and_profiling_do_not_change_decisions() {
    let cfg=Config {width:3440.0,height:1440.0,density:100.0,trails:100.0,intelligence:100.0,deadly_walls:true,self_collisions:true,seed:20260814,..Config::default()};
    let mut a=World::new(cfg).unwrap();let mut b=a.diagnostic_snapshot();
    let mut plain=AiController::new();let mut observed=AiController::new();
    observed.enable_diagnostics();observed.enable_profile();
    for _ in 0..5000 {
        a.step(&mut plain);b.step(&mut observed);
        assert_eq!(a.stats(),b.stats(),"tick {}",a.tick());assert_eq!(a.rng_state(),b.rng_state());
        for (sa,sb) in a.snakes().zip(b.snakes()) {
            assert_eq!(sa.angle,sb.angle,"tick {} snake {}",a.tick(),sa.id);
            assert_eq!(sa.segments,sb.segments);
            assert_eq!(a.nutrition(sa.id as usize),b.nutrition(sb.id as usize));
        }
        for (fa,fb) in a.foods().zip(b.foods()) {assert_eq!(fa.id,fb.id);assert_eq!(fa.position,fb.position);}
    }
}

#[test]
fn first_step_broad_phase_includes_observed_body_motion() {
    let mut w=arena(false);
    line(&mut w,0,Point{x:400.0,y:400.0},0.0,24);
    line(&mut w,1,Point{x:600.0,y:600.0},std::f64::consts::FRAC_PI_2,40);
    w.segments[MAX_SEGMENTS+1]=Segment {current:Point{x:500.0,y:400.0},previous:Point{x:600.0,y:400.0}};
    let mut ai=AiController::new();ai.prepare(&w);
    assert_eq!(ai.spatial.max_motion,100.0);
    let s=w.snake(0).unwrap();let mut tests=0;
    let result=ai.body_blocked(&w,s,Point{x:400.0,y:400.0},Point{x:404.0,y:400.0},STEP_SECONDS,0.0,&mut tests);
    assert!(result.0);assert!(!result.2);
}

#[test]
fn size_advantage_accepts_a_winning_immediate_head_contact() {
    let mut w=arena(false);
    line(&mut w,0,Point{x:400.0,y:400.0},0.0,32);
    line(&mut w,1,Point{x:417.0,y:400.0},std::f64::consts::PI,16);
    let mut ai=AiController::new();ai.prepare(&w);
    let s=w.snake(0).unwrap();
    assert_eq!(ai.rollout(&w,s,State {goal:Point{x:600.0,y:400.0},..State::default()},2,1).steps,1);
    let mut c=Selective {ai};w.step(&mut c);
    assert!(w.snake(0).unwrap().alive);
    assert!(!w.snake(1).unwrap().alive);
    assert_eq!(w.last_death_reason(1),Some(crate::DeathReason::Head));
}

#[test]
fn growth_release_delay_uses_stretch_cycles_and_the_segment_cap() {
    let mut w=arena(false);line(&mut w,0,Point{x:500.0,y:400.0},0.0,80);
    w.snakes[0].radius=18.0;w.snakes[0].base_radius=18.0;
    w.snakes[0].growth=10.0;w.snakes[0].stretch=0.0;
    let speed=w.motion_limits(0,0.0).unwrap().0;
    let expected=10.0*18.0*1.18/(speed*0.65*0.62);
    assert_eq!(w.tail_growth_delay(0),Some(expected));
    assert!(expected>10.0*0.2,"constant time per nutrition unit released growing tails too early");
    w.snakes[0].len=MAX_SEGMENTS;
    assert_eq!(w.tail_growth_delay(0),Some(0.0));
    assert_eq!(w.tail_growth_delay(MAX_SNAKES),None);
}

#[test]
fn tail_recovery_ends_when_current_space_is_open() {
    let mut w=arena(false);line(&mut w,0,Point{x:500.0,y:400.0},0.0,24);
    let mut ai=AiController::new();ai.prepare(&w);
    let s=w.snake(0).unwrap();
    ai.states[0]=State {generation:s.generation,last_angle:s.angle,desired:2.5,
        turn_until:u64::MAX,escape_until:45,goal:s.segments.last().unwrap().current,..State::default()};
    let steering=ai.steer(&w,s);
    assert_eq!(ai.states[0].escape_until,0);
    assert_eq!(steering.desired_angle,0.0);
}

#[test]
fn truncated_turn_plan_keeps_its_intended_exit_heading() {
    let mut w=arena(false);
    line(&mut w,0,Point{x:400.0,y:400.0},0.0,24);
    line(&mut w,1,Point{x:400.0,y:500.0},std::f64::consts::FRAC_PI_2,30);
    let mut ai=AiController::new();ai.prepare(&w);
    let s=w.snake(0).unwrap();
    let plan=ai.rollout(&w,s,State {goal:Point{x:900.0,y:400.0},..State::default()},7,24);
    assert!(plan.steps<16);
    assert!(plan.exit_angle>1.0,"an unvisited exit must not default to heading zero");
}

#[test]
fn rival_forecast_honors_a_known_turn_then_straight_plan() {
    let mut w=arena(false);line(&mut w,0,Point{x:500.0,y:400.0},0.0,24);
    let mut ai=AiController::new();ai.prepare(&w);
    let s=w.snake(0).unwrap();let initial=State {desired:3.0,turn_until:16,..State::default()};
    let plan=ai.rollout(&w,s,initial,7,24);
    ai.states[0]=State {generation:s.generation,desired:plan.desired,turn_until:16,exit_angle:plan.exit_angle,..initial};
    w.snakes[0].desired=plan.desired;
    ai.tick=u64::MAX;ai.geometry=w.geometry_generation();
    // Force rebuilding forecasts without resetting the controller history.
    ai.tick=w.tick()+1;
    w.tick+=2;
    ai.prepare(&w);
    assert!(ai.rivals[0].path[24].y>ai.rivals[0].path[16].y);
    let a=ai.rivals[0].path[23];let b=ai.rivals[0].path[24];
    assert!((normalize_angle((b.y-a.y).atan2(b.x-a.x)-plan.exit_angle)).abs()<1e-9);
}
