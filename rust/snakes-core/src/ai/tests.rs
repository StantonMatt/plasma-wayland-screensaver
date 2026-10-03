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
            let decision=observed.decision(sa.id as usize);
            if decision.reused_plan {
                assert!(decision.candidates.iter().all(|c|c.area>0),
                    "diagnostics must evaluate every candidate even when reusing slot 1");
            }
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
fn rival_forecast_ignores_private_desired_exit_and_orbit_plans() {
    let mut w=arena(false);line(&mut w,0,Point{x:500.0,y:400.0},0.0,24);
    let mut ai=AiController::new();ai.prepare(&w);
    let expected=ai.rivals[0].path;
    let mut other=ai.clone();
    other.states[0]=State {generation:w.snake(0).unwrap().generation,desired:3.0,turn_until:16,exit_angle:1.0,
        coil_center:Point{x:650.0,y:450.0},coil_radius:140.0,track_goal:true,..State::default()};
    w.snakes[0].desired=3.0;w.tick+=1;
    ai.prepare(&w);other.prepare(&w);
    assert_eq!(ai.rivals[0].path,expected);assert_eq!(other.rivals[0].path,expected);
    w.snakes[0].angle=0.1;w.tick+=1;ai.prepare(&w);
    assert!(ai.rivals[0].path[20].y>expected[20].y,"visible curvature must be observed");
}

#[test]
fn compound_escape_retains_its_second_turn_and_rush_schedule() {
    let mut w=arena(false);line(&mut w,0,Point{x:600.0,y:400.0},0.0,40);
    let mut ai=AiController::new();ai.prepare(&w);let s=w.snake(0).unwrap();
    let state=State {goal:Point{x:1000.0,y:400.0},turn_until:u64::MAX,rush:0.2,..State::default()};
    for kind in [9,10] {
        let plan=ai.rollout(&w,s,state,kind,60);
        assert_eq!(plan.steps,60);
        assert_eq!(plan.exit_angle,s.angle);
        let retained=State {desired:plan.desired,exit_angle:plan.exit_angle,turn_until:32,..state};
        let continuation=ai.rollout(&w,s,retained,1,60);
        assert_eq!(plan.path,continuation.path);
        assert_eq!(plan.angle,continuation.angle);
    }
}

#[test]
fn trajectory_area_detects_a_future_loop_and_does_not_leak_to_other_candidates() {
    let mut grid=Spatial::new();grid.cols=40;grid.rows=30;grid.dx=20.0;grid.dy=20.0;
    let center=Point{x:400.0,y:300.0};
    let mut path=[center;121];
    for (j,p) in path[..97].iter_mut().enumerate() {
        let theta=j as f64*std::f64::consts::TAU/96.0;
        *p=Point{x:center.x+theta.cos()*120.0,y:center.y+theta.sin()*120.0};
    }
    assert_eq!(grid.space(center,u16::MAX,512,4.0),(512,true));
    let trapped=grid.trajectory_space(center,u16::MAX,512,4.0,&path,24,200);
    assert!(!trapped.1);assert!(trapped.0<100,"{trapped:?}");
    assert_eq!(grid.space(center,u16::MAX,512,4.0),(512,true));
    assert_eq!(grid.trajectory_space(center,u16::MAX,512,4.0,&path,24,24),(512,true));
}

#[test]
fn hunters_use_flanks_and_foragers_keep_food_goals() {
    for aggression in [0.15,1.0] {
        let mut w=arena(false);
        line(&mut w,0,Point{x:550.0,y:400.0},0.0,72);
        line(&mut w,1,Point{x:700.0,y:400.0},0.0,24);
        w.snakes[0].traits.aggression=aggression;food(&mut w,Point{x:700.0,y:250.0});
        let mut ai=AiController::new();ai.prepare(&w);let s=w.snake(0).unwrap();
        let mut state=State {goal:Point{x:700.0,y:250.0},..State::default()};
        ai.tactics(&w,s,&mut state,1.0);
        if aggression>0.5 {assert_eq!(state.prey,2);assert_ne!(state.goal.y,400.0);assert_eq!(state.debug.flags&8,8);}
        else {assert_eq!(state.prey,0);assert_eq!(state.goal,Point{x:700.0,y:250.0});}
    }
}

#[test]
fn generation_change_ends_hunts_and_death_starts_corpse_harvesting() {
    let mut w=arena(false);
    line(&mut w,0,Point{x:600.0,y:400.0},0.0,72);
    line(&mut w,1,Point{x:750.0,y:400.0},0.0,24);
    w.snakes[0].traits.aggression=0.0;
    let generation=w.snake(1).unwrap().generation;
    let mut state=State {prey:2,prey_generation:generation,hunt_until:100,coil_center:Point{x:750.0,y:400.0},..State::default()};
    w.snakes[1].alive=false;w.snakes[1].len=0;
    let mut ai=AiController::new();ai.prepare(&w);ai.tactics(&w,w.snake(0).unwrap(),&mut state,1.0);
    assert_eq!(state.prey,0);assert_eq!(state.harvest_until,60);assert_eq!(state.goal,state.coil_center);
    line(&mut w,1,Point{x:750.0,y:400.0},0.0,24);w.snakes[1].generation+=1;
    state.prey=2;state.prey_generation=generation;state.harvest_until=0;
    ai.tactics(&w,w.snake(0).unwrap(),&mut state,1.0);
    assert_eq!(state.prey,0);assert_eq!(state.harvest_until,0);
}

#[test]
fn diagnostic_arena_validates_and_retains_the_dead_head_sweep() {
    let cfg=Config {density:0.0,..Config::default()};
    assert!(World::diagnostic_arena(cfg,&[(Point::default(),0.0,0,1.0)],&[]).is_err());
    let mut w=World::diagnostic_arena(cfg,&[(Point{x:400.0,y:400.0},0.0,32,1.0),
        (Point{x:417.0,y:400.0},std::f64::consts::PI,16,0.0)],&[]).unwrap();
    let mut c=Selective {ai:AiController::new()};w.step(&mut c);
    assert!(!w.snake(1).unwrap().alive);assert!(w.snake(1).unwrap().segments.is_empty());
    assert!(w.collision_head(1).unwrap().current.x<417.0);assert!(w.collision_head(99).is_none());
    assert!(w.diagnostic_body(99,&[Point::default()],0.0).is_err());
    assert!(w.diagnostic_body(0,&[],0.0).is_err());
}

#[test]
fn rush_changes_future_neck_age_and_body_cache_identity() {
    let mut w=arena(false);line(&mut w,0,Point{x:600.0,y:400.0},0.0,72);
    let mut ai=AiController::new();ai.prepare(&w);let s=w.snake(0).unwrap();
    let p=s.segments[1].current;
    let base=w.motion_limits(0,0.0).unwrap().0;let rushed=w.motion_limits(0,1.0).unwrap().0;
    let time=9.0*s.radius*1.18/((base+rushed)*0.5);
    ai.planning_speed[0]=base;let mut tests=0;
    let slow=ai.cached_body_blocked(&w,s,p,p,time,0.0,&mut tests);
    ai.planning_speed[0]=rushed;let mut tests=0;
    let fast=ai.cached_body_blocked(&w,s,p,p,time,0.0,&mut tests);
    assert!(!slow.0);assert!(fast.0);
}

#[test]
fn own_tracking_continuation_is_exact_but_rival_observations_do_not_reveal_it() {
    let mut w=arena(false);line(&mut w,0,Point{x:550.0,y:400.0},0.0,40);
    let mut ai=AiController::new();ai.prepare(&w);let s=w.snake(0).unwrap();
    let state=State {generation:s.generation,goal:Point{x:750.0,y:500.0},turn_until:u64::MAX,
        desired:s.angle,rush:0.25,track_goal:true,..State::default()};
    let goal_plan=ai.rollout(&w,s,state,0,36);
    let mut retained=State {desired:goal_plan.desired,..state};
    let continuation=ai.rollout(&w,s,retained,1,36);
    assert_eq!(goal_plan.path,continuation.path);
    w.snakes[0].desired=goal_plan.desired;w.snakes[0].rush=retained.rush;
    retained.desired=goal_plan.desired;ai.states[0]=retained;w.tick+=1;
    ai.prepare(&w);
    assert_ne!(&ai.rivals[0].path[..=36],&goal_plan.path[..=36]);
}

#[test]
fn coil_tracking_predicts_the_full_orbit_instead_of_stopping_at_one_tangent() {
    let mut w=arena(false);line(&mut w,0,Point{x:650.0,y:400.0},std::f64::consts::FRAC_PI_2,24);
    let mut ai=AiController::new();ai.prepare(&w);let s=w.snake(0).unwrap();
    let state=State {coil_center:Point{x:550.0,y:400.0},coil_radius:100.0,coil_initial_radius:100.0,coil_last_head:Point{x:650.0,y:400.0},coil_sign:1.0,
        goal:Point{x:600.0,y:450.0},track_goal:true,turn_until:u64::MAX,..State::default()};
    let orbit=ai.rollout(&w,s,state,0,60);
    let fixed=ai.rollout(&w,s,state,2,60);
    assert_eq!(orbit.steps,60);assert_eq!(fixed.steps,60);
    assert!((w.distance_squared(orbit.path[60],state.coil_center).sqrt()-100.0).abs()<5.0);
    assert!(w.distance_squared(fixed.path[60],state.coil_center)>200.0*200.0);
}

#[test]
fn sustained_turn_recovery_cancels_the_hunt_and_its_rush() {
    let mut w=arena(false);line(&mut w,0,Point{x:550.0,y:400.0},0.0,40);
    line(&mut w,1,Point{x:750.0,y:400.0},0.0,24);
    let mut ai=AiController::new();ai.prepare(&w);let s=w.snake(0).unwrap();
    ai.states[0]=State {generation:s.generation,last_angle:-0.1,turn_accum:2.75,
        desired:s.angle,turn_until:u64::MAX,prey:2,prey_generation:w.snake(1).unwrap().generation,
        hunt_until:120,rush:1.0,goal:Point{x:900.0,y:400.0},..State::default()};
    ai.steer(&w,s);
    assert_eq!(ai.states[0].prey,0);assert_eq!(ai.states[0].rush,0.0);
    assert_eq!(ai.states[0].orbit_until,36);assert!(!ai.states[0].track_goal);
}

#[test]
fn sluggish_giants_wait_for_a_reachable_cutoff_instead_of_chasing_fast_rivals() {
    let w=World::diagnostic_arena(Config {width:3440.0,height:1440.0,deadly_walls:false,density:0.0,
        ..Config::default()},&[(Point{x:3200.0,y:400.0},0.0,1600,1.0),
        (Point{x:3200.0,y:600.0},0.0,340,1.0)],&[]).unwrap();
    let mut ai=AiController::new();ai.prepare(&w);
    let mut state=State {goal:Point{x:3300.0,y:400.0},..State::default()};
    ai.tactics(&w,w.snake(0).unwrap(),&mut state,1.0);
    assert_eq!(state.prey,0);assert_eq!(state.goal,Point{x:3300.0,y:400.0});
}

#[test]
fn staged_cutoff_uses_the_discrete_stage_speed_and_continues_after_the_switch() {
    let mut w=World::diagnostic_arena(Config {width:1600.0,height:1000.0,density:0.0,deadly_walls:true,self_collisions:true,..Config::default()},
        &[(Point{x:700.0,y:400.0},0.0,72,1.0),(Point{x:750.0,y:700.0},0.0,24,0.6)],&[]).unwrap();
    let attack=Attack {valid:true,side:1,prey:2,prey_generation:w.snake(1).unwrap().generation,point:Point{x:750.0,y:700.0},start:0,turn_at:8,end:48,approach:0.4,crossing:1.3,burst:1.0,crossing_rush:0.15,..Attack::default()};
    let mut ai=AiController::new();ai.prepare(&w);
    let state=State {attack,prey:2,prey_generation:attack.prey_generation,hunt_until:120,goal:Point{x:1100.0,y:650.0},..State::default()};
    let plan=ai.rollout(&w,w.snake(0).unwrap(),state,1,30);
    assert_eq!(plan.steps,30);
    let fast=w.motion_limits(0,1.0).unwrap().0;let slow=w.motion_limits(0,0.15).unwrap().0;
    assert!((w.distance_squared(plan.path[7],plan.path[8]).sqrt()-fast*STEP_SECONDS).abs()<1e-9);
    assert!((w.distance_squared(plan.path[8],plan.path[9]).sqrt()-slow*STEP_SECONDS).abs()<1e-9);
    let mut controller=crate::controller::ScriptedController::new(|tick:u64,_:SnakeView<'_>| {
        let (desired_angle,rush)=attack.control(tick);Steering {desired_angle,rush}
    });
    for j in 1..=30 {w.step(&mut controller);assert!(w.snake(0).unwrap().alive);
        assert!(w.distance_squared(w.snake(0).unwrap().segments[0].current,plan.path[j])<1e-18,"tick {j}");}
}

#[test]
fn cutoff_library_rejects_unreachable_geometry_and_keeps_at_most_two_paths() {
    let w=World::diagnostic_arena(Config {width:3440.0,height:1440.0,density:0.0,..Config::default()},
        &[(Point{x:800.0,y:400.0},0.0,400,1.0),(Point{x:1400.0,y:650.0},0.0,24,0.6)],&[]).unwrap();
    let mut ai=AiController::new();ai.prepare(&w);
    assert!(ai.cutoffs(&w,w.snake(0).unwrap(),State {prey:2,..State::default()}).iter().all(|a|!a.valid));
}

#[test]
fn spiral_tracks_radius_without_the_old_inward_equilibrium_and_obeys_pitch_curvature() {
    let w=arena(false);let mut state=State {coil_center:Point{x:600.0,y:400.0},coil_radius:145.0,
        coil_initial_radius:145.0,coil_pitch:14.0,coil_sign:1.0,coil_last_head:Point{x:745.0,y:400.0},..State::default()};
    let mut head=state.coil_last_head;let mut angle=std::f64::consts::FRAC_PI_2;
    for _ in 0..240 {
        AiController::advance_spiral(&w,&mut state,head);
        let goal=AiController::trajectory_goal(&w,state,head);let d=w.displacement(head,goal);
        angle=normalize_angle(angle+normalize_angle(d.y.atan2(d.x)-angle).clamp(-4.0*STEP_SECONDS,4.0*STEP_SECONDS));
        head=Point{x:head.x+angle.cos()*100.0*STEP_SECONDS,y:head.y+angle.sin()*100.0*STEP_SECONDS};
        let actual=w.distance_squared(head,state.coil_center).sqrt();
        assert!((actual-state.coil_radius).abs()<5.0,"{actual} vs {}",state.coil_radius);
        assert!(100.0*AiController::spiral_curvature(state.coil_radius,state.coil_pitch)<4.0);
    }
    assert!(state.coil_progress>5.0);assert!(state.coil_distance>790.0);
}

#[test]
fn pocket_rejects_open_space_and_releases_by_actual_travel() {
    let w=World::diagnostic_arena(Config {width:1600.0,height:1000.0,density:0.0,deadly_walls:true,..Config::default()},
        &[(Point{x:900.0,y:500.0},std::f64::consts::FRAC_PI_2,130,1.0),
          (Point{x:750.0,y:500.0},0.0,24,0.6)],&[]).unwrap();
    let mut ai=AiController::new();ai.prepare(&w);
    let mut state=State {prey:2,..State::default()};
    assert!(!ai.pocket(&w,w.snake(0).unwrap(),&mut state));
    state=State {prey:2,coil_radius:145.0,coil_initial_radius:145.0,coil_center:Point{x:750.0,y:500.0},
        coil_pitch:14.0,coil_distance:200.0,coil_release_distance:220.0,coil_until:120,..State::default()};
    assert!(!ai.pocket(&w,w.snake(0).unwrap(),&mut state));assert_eq!(state.coil_radius,0.0);
}

#[test]
fn opponent_response_delay_never_skips_immediate_body_or_wall_safety() {
    let mut w=arena(false);line(&mut w,0,Point{x:1170.0,y:400.0},0.0,24);
    let mut ai=AiController::new();ai.prepare(&w);
    let state=State {revise_opponents:false,..State::default()};
    assert!(ai.rollout(&w,w.snake(0).unwrap(),state,2,72).steps<72);
    line(&mut w,0,Point{x:400.0,y:400.0},0.0,24);
    line(&mut w,1,Point{x:600.0,y:600.0},0.0,24);
    w.segments[MAX_SEGMENTS+1]=Segment {current:Point{x:410.0,y:400.0},previous:Point{x:410.0,y:400.0}};
    w.tick+=1;ai.prepare(&w);
    assert_eq!(ai.rollout(&w,w.snake(0).unwrap(),state,2,72).steps,0);
}

#[test]
fn existing_wall_u_can_enter_one_legal_pitch_inside_its_arms() {
    let center=Point{x:875.0,y:195.0};let radius=180.0;let theta=-0.4;
    let mut w=World::diagnostic_arena(Config {width:1600.0,height:1000.0,density:0.0,deadly_walls:true,intelligence:100.0,self_collisions:true,..Config::default()},
        &[(Point{x:1000.0,y:100.0},0.0,180,1.0),(center,-std::f64::consts::FRAC_PI_2,24,0.6)],&[]).unwrap();
    let mut body=Vec::new();
    for j in 0usize..180 {
        let phase=theta-(j.min(129) as f64)*7.08/radius;
        let tail=(j.saturating_sub(129)) as f64*7.08;
        body.push(Point{x:center.x+radius*phase.cos()+tail*(phase-std::f64::consts::FRAC_PI_2).cos(),
            y:center.y+radius*phase.sin()+tail*(phase-std::f64::consts::FRAC_PI_2).sin()});
    }
    w.diagnostic_body(0,&body,theta+std::f64::consts::FRAC_PI_2).unwrap();
    let mut ai=AiController::new();ai.prepare(&w);
    let mut state=State {prey:2,..State::default()};
    assert!(ai.pocket(&w,w.snake(0).unwrap(),&mut state));
    assert!(state.coil_pitch>6.0*1.48+2.0 && state.coil_pitch<2.0*12.0*0.78-2.0);
    assert!(state.coil_radius<radius-10.0);
    let plan=ai.rollout(&w,w.snake(0).unwrap(),state,0,72);
    assert_eq!(plan.steps,72,"pocket must have a checked continuation");
}


#[test]
fn pending_response_survives_quota_phase_and_recovery_returns() {
    let mut w=World::new(Config {width:1200.0,height:800.0,density:45.0,scale:70.0,deadly_walls:true,self_collisions:true,..Config::default()}).unwrap();
    // Eight slots / two strategy slots / two-tick response: slot 2's
    // strategy is odd, whereas the old consumed response stayed even.
    for id in 0..8 {line(&mut w,id,Point{x:300.0+id as f64*100.0,y:650.0},0.0,1);}
    line(&mut w,2,Point{x:400.0,y:300.0},0.0,24);
    w.snakes.truncate(8);w.snakes[2].traits.turn_bias=-0.175;
    food(&mut w,Point{x:750.0,y:300.0});
    let mut ai=AiController::new();ai.prepare(&w);
    let s=w.snake(2).unwrap();
    ai.states[2]=State {generation:s.generation,target:100000,goal:Point{x:750.0,y:300.0},
        desired:0.0,turn_until:u64::MAX,best_distance:f64::MAX,..State::default()};
    ai.steer(&w,s);
    assert_eq!(ai.states[2].next_response,0,"quota wait must not consume a revision");
    assert_eq!(ai.states[2].next_forecast,2,"performed rollout revisions retain their normal cadence");
    for tick in 1..=13 {
        w.tick=tick;ai.steer(&w,w.snake(2).unwrap());
        if tick%4==1 {assert_eq!(ai.states[2].next_response,tick+2,"tactics must run on odd strategy slots");}
    }
    let mut state=State {revise_opponents:true,escape_until:100,next_response:0,..State::default()};
    ai.strategy(&w,w.snake(2).unwrap(),&mut state,1.0);
    assert_eq!(state.next_response,0);
    state.escape_until=0;state.orbit_until=100;
    ai.strategy(&w,w.snake(2).unwrap(),&mut state,1.0);
    assert_eq!(state.next_response,0);
}

#[test]
fn canceled_hunt_cannot_resurrect_cutoff_finalists_or_controls() {
    let mut w=arena(false);
    line(&mut w,0,Point{x:400.0,y:300.0},0.0,72);
    line(&mut w,1,Point{x:450.0,y:500.0},0.0,24);
    let mut ai=AiController::new();ai.prepare(&w);
    let generation=w.snake(1).unwrap().generation;
    let attack=Attack {valid:true,prey:2,prey_generation:generation,start:0,end:50,turn_at:10,
        point:Point{x:450.0,y:500.0},approach:1.0,burst:1.0,..Attack::default()};
    let base=State {prey:2,prey_generation:generation,hunt_until:100,attack,attack_options:[attack;2],
        desired:1.0,rush:1.0,turn_until:10,goal:Point{x:800.0,y:300.0},..State::default()};
    for reason in 0..8 {
        let mut state=base;
        match reason {
            0=>state.prey=0,
            1=>state.prey_generation=generation+1,
            2=>{state.attack.end=0;state.attack_options[0].end=0;state.attack_options[1].end=0;},
            3=>state.escape_until=50,
            4=>state.orbit_until=50,
            5=>state.coil_radius=100.0,
            6=>{state.attack.start=1;state.attack_options[0].start=1;state.attack_options[1].start=1;},
            _=>{w.tick=101;},
        }
        ai.validate_attack(&w,w.snake(0).unwrap(),&mut state);
        assert!(!state.attack.valid && state.attack_options.iter().all(|a|!a.valid),"reason {reason}");
        assert_eq!(state.desired,0.0);assert_eq!(state.rush,0.0);
        for kind in [1,7,8] {assert!(!ai.rollout(&w,w.snake(0).unwrap(),state,kind,8).attack.valid);}
    }
    w.tick=0;
    // A stale alternative is rejected even when there is no retained attack.
    let state=State {prey:0,attack:Attack::default(),..base};
    assert!(!ai.rollout(&w,w.snake(0).unwrap(),state,7,8).attack.valid);
    // Recovery strategy returns must drop scratch alternatives too.
    for (escape_until,orbit_until) in [(50,0),(0,50)] {
        let mut state=State {escape_until,orbit_until,..base};
        ai.strategy(&w,w.snake(0).unwrap(),&mut state,1.0);
        assert!(!state.attack.valid && state.attack_options.iter().all(|a|!a.valid));
        assert_eq!(state.desired,0.0);assert_eq!(state.rush,0.0);
    }
}

#[test]
fn asynchronous_head_crossing_keeps_the_rival_neck_lethal() {
    let mut w=arena(false);
    line(&mut w,0,Point{x:400.0,y:300.0},0.0,72);
    line(&mut w,1,Point{x:450.0,y:650.0},0.0,24);
    let mut ai=AiController::new();ai.prepare(&w);
    let state=State {revise_opponents:true,..State::default()};
    let open=ai.rollout(&w,w.snake(0).unwrap(),state,2,12);
    assert_eq!(open.steps,12);
    let x=open.path[4].x;
    // Rival crosses at tick 1, attacker reaches the deposited trail at 4.
    // Small forecast radius separates the two one-tick head sweeps.
    let r=&mut ai.rivals[1];r.radius=0.1;r.speed=900.0;
    for j in 0..=STEPS {r.path[j]=Point{x,y:300.0+(j as f64-1.0)*30.0};r.envelope[j]=0.0;}
    assert!(w.segments_distance_squared(open.path[0],open.path[4],r.path[0],r.path[4])<1e-12);
    let reach=(6.0+0.1)*0.82;
    for j in 1..=4 {assert!(w.segments_distance_squared(open.path[j-1],open.path[j],r.path[j-1],r.path[j])>reach*reach);}
    let checked=ai.rollout(&w,w.snake(0).unwrap(),state,2,12);
    assert!(checked.steps<12,"an asynchronous crossing must not suppress deposited neck geometry");
}

#[test]
fn prey_replies_obey_self_collision_setting_for_current_and_deposited_trails() {
    for deposited in [false,true] {
        let mut w=arena(false);w.config.self_collisions=false;
        if deposited {w.config.width=80.0;w.config.deadly_walls=false;}
        line(&mut w,0,Point{x:900.0,y:650.0},0.0,120);
        line(&mut w,1,Point{x:400.0,y:300.0},0.0,72);
        // Block every sampled reply against current own geometry. With self
        // collision off the attacker barrier remains the only lethal contact.
        if !deposited {
            for j in 10..72 {w.segments[MAX_SEGMENTS+j].current=Point{x:430.0,y:300.0};}
        }
        if deposited {for j in 1..72 {w.segments[MAX_SEGMENTS+j].current=Point{x:120.0,y:650.0};}}
        let mut ai=AiController::new();ai.prepare(&w);
        let generation=w.snake(1).unwrap().generation;
        let attack=Attack {valid:true,prey:2,prey_generation:generation,end:120,point:w.snake(0).unwrap().segments[0].current,..Attack::default()};
        let state=State {prey:2,prey_generation:generation,hunt_until:120,..State::default()};
        let mut c=Candidate {attack,body_len:w.snake(0).unwrap().segments.len(),..Candidate::default()};
        // Keep the hunter clear initially, then sweep across every reply at
        // tick 60. A U-turn reply crosses its own deposited trail beforehand.
        for j in 0..=STEPS {c.path[j]=Point{x:700.0,y:300.0};}
        c.path[59]=Point{x:250.0,y:300.0};c.path[60]=Point{x:750.0,y:300.0};
        if deposited {
            c.path.fill(Point{x:120.0,y:650.0});
            let speed=w.motion_limits(1,0.0).unwrap().0;
            let q=w.canonical_point(Point{x:400.0+speed*60.0*STEP_SECONDS,y:300.0});
            c.path[59]=q;c.path[60]=q;
        }
        let off=ai.replies_blocked(&w,w.snake(0).unwrap(),state,&c);
        w.config.self_collisions=true;
        let on=ai.replies_blocked(&w,w.snake(0).unwrap(),state,&c);
        assert!(off>on,"deposited={deposited}: off={off}, on={on}");
    }
}


#[test]
fn tick_aligned_head_contacts_certify_only_the_matching_tick_including_seams() {
    for wrap in [false,true] {
        let w=arena(wrap);
        let x=if wrap {1198.0} else {500.0};
        let a=std::array::from_fn::<_,5,_>(|j|w.canonical_point(Point{x:x+(j as f64-2.0)*20.0,y:400.0}));
        let b=std::array::from_fn::<_,5,_>(|j|Point{x,y:400.0+(j as f64-2.0)*20.0});
        assert!(AiController::head_contact(&w,&a,&b,0,4,2.0));
        let delayed=std::array::from_fn::<_,5,_>(|j|Point{x,y:400.0+(j as f64-4.0)*20.0});
        assert!(!AiController::head_contact(&w,&a,&delayed,0,4,2.0));
    }
}

#[test]
fn retained_attack_and_alternatives_share_identity_advantage_and_deadline_checks() {
    let mut w=arena(false);
    line(&mut w,0,Point{x:400.0,y:300.0},0.0,72);
    line(&mut w,1,Point{x:450.0,y:500.0},0.0,24);
    let mut ai=AiController::new();ai.prepare(&w);
    let generation=w.snake(1).unwrap().generation;
    let attack=Attack {valid:true,prey:2,prey_generation:generation,end:50,turn_at:10,
        point:Point{x:450.0,y:500.0},burst:1.0,..Attack::default()};
    let base=State {prey:2,prey_generation:generation,hunt_until:100,attack,attack_options:[attack;2],..State::default()};
    for reason in 0..6 {
        let mut state=base;
        match reason {
            0=>{state.attack.prey=3;state.attack_options[0].prey=3;state.attack_options[1].prey=3;},
            1=>{w.snakes[1].generation=generation+1;},
            2=>{w.snakes[0].len=28;},
            3=>{w.tick=50;},
            4=>{state.attack.point.x=300.0;state.attack_options[0].point.x=300.0;state.attack_options[1].point.x=300.0;},
            _=>{w.snakes[1].alive=false;},
        }
        ai.validate_attack(&w,w.snake(0).unwrap(),&mut state);
        assert!(!state.attack.valid && state.attack_options.iter().all(|a|!a.valid),"reason {reason}");
        w.tick=0;w.snakes[0].len=72;w.snakes[1].generation=generation;w.snakes[1].alive=true;
    }
    // Expiry is immediate even on an unscheduled, not-yet-due response tick.
    w.tick=50;ai.prepare(&w);
    ai.states[0]=State {generation:w.snake(0).unwrap().generation,next_response:100,desired:1.0,rush:1.0,
        goal:Point{x:800.0,y:300.0},turn_until:u64::MAX,..base};
    ai.steer(&w,w.snake(0).unwrap());
    assert!(!ai.states[0].attack.valid);
    assert!(ai.states[0].attack_options.iter().all(|a|!a.valid));
}

#[test]
fn tactic_gates_only_reject_candidates_that_execute_that_tactic() {
    let mut w=arena(true);
    line(&mut w,0,Point{x:600.0,y:400.0},0.8,1);
    line(&mut w,1,Point{x:200.0,y:650.0},0.0,1);
    let mut ai=AiController::new();ai.prepare(&w);let s=w.snake(0).unwrap();
    let base=State {desired:s.angle,turn_until:u64::MAX,goal:Point{x:850.0,y:550.0},..State::default()};
    // The impossible coil must reject its own tracking paths while straight,
    // turn, escape, harvest and flee paths retain their ordinary safety checks.
    for mode in 0..5 {
        let mut tactical=State {coil_radius:1.0,coil_initial_radius:1.0,coil_pitch:14.0,
            coil_center:Point{x:599.0,y:400.0},coil_last_head:s.segments[0].current,
            track_goal:true,..base};
        match mode {
            0=>{},
            1=>tactical.harvest_until=100,
            2=>tactical.dodge_until=100,
            3=>tactical.escape_until=100,
            _=>tactical.orbit_until=100,
        }
        for kind in 2..11 {
            let ordinary=ai.rollout(&w,s,base,kind,36);
            let alternative=ai.rollout(&w,s,tactical,kind,36);
            assert_eq!(alternative.steps,ordinary.steps,"mode {mode}, kind {kind}");
            assert_eq!(alternative.path,ordinary.path,"mode {mode}, kind {kind}");
        }
        for kind in [0,1,11,12] {assert_eq!(ai.rollout(&w,s,tactical,kind,36).steps,0);}
    }
    // A valid attack has its own motion schedule, also independent of a stale
    // coil's curvature. The live consumer normally cancels that conflict.
    let attack=Attack {valid:true,prey:2,prey_generation:w.snake(1).unwrap().generation,
        end:100,turn_at:20,approach:0.8,crossing:1.1,point:Point{x:800.0,y:550.0},..Attack::default()};
    line(&mut w,0,Point{x:600.0,y:400.0},0.8,24);w.tick+=1;ai.prepare(&w);
    let state=State {attack,prey:2,prey_generation:attack.prey_generation,hunt_until:100,..base};
    for kind in [0,2,3,4,5,6,9,10] {
        assert!(!ai.rollout(&w,w.snake(0).unwrap(),state,kind,36).attack.valid,"kind {kind}");
    }
}

#[test]
fn infeasible_retained_pocket_is_canceled_before_a_delayed_revision() {
    for change in 0..11 {
        let mut w=arena(false);
        line(&mut w,2,Point{x:700.0,y:400.0},std::f64::consts::FRAC_PI_2,90);
        line(&mut w,1,Point{x:550.0,y:400.0},0.0,24);
        let mut ai=AiController::new();ai.prepare(&w);
        let s=w.snake(2).unwrap();
        let mut state=State {generation:s.generation,prey:2,prey_generation:w.snake(1).unwrap().generation,
            hunt_until:100,coil_center:Point{x:550.0,y:400.0},coil_radius:150.0,coil_initial_radius:150.0,
            coil_body:Some((90,6.0)),
            coil_pitch:14.0,coil_sign:1.0,coil_last_head:s.segments[0].current,coil_release_distance:600.0,
            coil_until:100,track_goal:true,next_response:100,last_strategy:0,desired:s.angle,
            last_angle:s.angle,turn_until:u64::MAX,goal:Point{x:700.0,y:500.0},..State::default()};
        assert!(ai.pocket(&w,s,&mut state),"baseline must be a feasible retained pocket");
        match change {
            0=>w.config.speed=1000.0,
            1=>w.snakes[2].len=89,
            2=>w.snakes[2].radius=12.0,
            3=>state.hunt_until=0,
            4=>state.escape_until=100,
            5=>state.orbit_until=100,
            6=>state.dodge_until=100,
            7=>w.config.deadly_walls=false,
            8=>state.coil_progress=100.0,
            9=>state.coil_until=0,
            _=>w.segments[MAX_SEGMENTS].current=Point{x:1000.0,y:400.0},
        }
        ai.states[2]=state;w.tick=1;ai.prepare(&w);ai.urgent_used=URGENT_QUOTA;
        ai.enable_diagnostics();ai.steer(&w,w.snake(2).unwrap());
        assert_eq!(ai.states[2].coil_radius,0.0,"change {change}");
        assert!(!ai.states[2].track_goal,"change {change}");
        assert_eq!(ai.states[2].next_response,100,"validation cannot wait for tactics");
        let d=ai.decision(2);assert!(d.candidates[d.selected].checked);
    }
}

#[test]
fn exhausted_rollouts_publish_a_checked_heading_instead_of_default_zero() {
    for trapped in [false,true] {
        let mut w=arena(false);
        let head=Point{x:if trapped {1199.0} else {600.0},y:400.0};
        line(&mut w,2,head,0.8,1);
        let mut ai=AiController::new();ai.prepare(&w);ai.enable_diagnostics();
        let s=w.snake(2).unwrap();
        ai.states[2]=State {generation:s.generation,coil_radius:1.0,coil_initial_radius:1.0,
            coil_center:Point{x:head.x-1.0,y:head.y},coil_last_head:head,coil_sign:1.0,coil_pitch:14.0,
            track_goal:true,next_response:100,last_angle:s.angle,desired:s.angle,turn_until:u64::MAX,
            goal:Point{x:900.0,y:500.0},..State::default()};
        ai.urgent_used=URGENT_QUOTA;
        let steering=ai.steer(&w,s);let d=ai.decision(2);
        let selected=d.candidates[d.selected];
        assert!(selected.checked,"a feasibility rejection is never a checked fallback");
        assert_eq!(steering.desired_angle,selected.desired);
        assert_ne!(steering.desired_angle,0.0,"never publish an uninitialized heading");
        if trapped {assert!(d.candidates.iter().all(|c|c.safe_ticks==0));}
        else {assert_eq!(selected.safe_ticks,d.horizon);}
        assert_eq!(selected.safe_ticks,d.candidates.iter().filter(|c|c.checked).map(|c|c.safe_ticks).max().unwrap());
        if trapped {assert_eq!(selected.area,d.candidates.iter().filter(|c|c.checked && c.safe_ticks==selected.safe_ticks).map(|c|c.area).max().unwrap());}
    }
}

#[test]
fn straight_candidate_does_not_alias_a_retained_staged_attack() {
    let mut w=arena(false);
    line(&mut w,2,Point{x:70.0,y:400.0},0.0,72);
    line(&mut w,1,Point{x:250.0,y:650.0},0.0,24);
    let mut ai=AiController::new();ai.prepare(&w);ai.enable_diagnostics();
    let s=w.snake(2).unwrap();
    let attack=Attack {valid:true,prey:2,prey_generation:w.snake(1).unwrap().generation,
        end:100,turn_at:20,approach:0.0,crossing:1.0,burst:1.0,crossing_rush:0.15,
        point:Point{x:250.0,y:650.0},..Attack::default()};
    ai.states[2]=State {generation:s.generation,attack,prey:2,prey_generation:attack.prey_generation,
        hunt_until:100,next_response:100,last_angle:s.angle,desired:s.angle,turn_until:u64::MAX,
        goal:Point{x:400.0,y:450.0},..State::default()};
    ai.steer(&w,s);let d=ai.decision(2);
    assert!(!d.reused_plan,"the fixture must search alternatives");
    assert_eq!(d.candidates[1].attack_turn_ticks,20);
    assert_eq!(d.candidates[2].attack_turn_ticks,0,"straight must remain an independent escape");
}

#[test]
fn retained_attack_cancels_changed_motion_and_geometry_without_waiting() {
    for change in 0..3 {
        let mut w=arena(false);
        line(&mut w,2,Point{x:500.0,y:300.0},0.0,72);
        line(&mut w,1,Point{x:750.0,y:500.0},0.0,24);
        let mut attack=Attack {valid:true,prey:2,prey_generation:w.snake(1).unwrap().generation,
            end:100,turn_at:20,approach:0.4,crossing:1.2,burst:1.0,crossing_rush:0.15,
            point:Point{x:750.0,y:500.0},..Attack::default()};
        attack.limits=Some(AiController::attack_limits(&w,w.snake(2).unwrap(),attack));
        match change {
            0=>w.config.speed=120.0,
            1=>w.snakes[2].len=70,
            _=>w.snakes[2].radius=7.0,
        }
        let mut ai=AiController::new();ai.prepare(&w);let s=w.snake(2).unwrap();
        ai.states[2]=State {generation:s.generation,attack,prey:2,prey_generation:attack.prey_generation,
            hunt_until:100,next_response:100,last_angle:s.angle,desired:attack.approach,turn_until:20,
            track_goal:false,commit_until:50,rush:1.0,goal:Point{x:900.0,y:300.0},..State::default()};
        ai.steer(&w,s);
        assert!(!ai.states[2].attack.valid,"change {change}");
        assert_eq!(ai.states[2].next_response,100);
    }
}

#[test]
fn selecting_a_checked_straight_continuation_abandons_the_old_pocket() {
    let mut w=arena(false);
    line(&mut w,2,Point{x:700.0,y:400.0},std::f64::consts::FRAC_PI_2,90);
    line(&mut w,1,Point{x:550.0,y:400.0},0.0,24);
    let mut ai=AiController::new();ai.prepare(&w);ai.enable_diagnostics();
    let s=w.snake(2).unwrap();
    ai.states[2]=State {generation:s.generation,prey:2,prey_generation:w.snake(1).unwrap().generation,
        hunt_until:100,coil_center:Point{x:550.0,y:400.0},coil_radius:150.0,coil_initial_radius:150.0,
        coil_pitch:14.0,coil_sign:1.0,coil_last_head:s.segments[0].current,coil_release_distance:600.0,
        coil_until:100,track_goal:false,next_response:100,last_strategy:0,desired:s.angle,
        last_angle:s.angle,turn_until:u64::MAX,goal:Point{x:700.0,y:700.0},..State::default()};
    let steering=ai.steer(&w,s);let d=ai.decision(2);
    assert!(d.reused_plan && d.candidates[d.selected].checked);
    assert_eq!(steering.desired_angle,s.angle);
    assert_eq!(ai.states[2].coil_radius,0.0,"the checked straight continuation must not resurrect a coil next tick");
    assert!(!ai.states[2].track_goal);
}


// Advisory regression classes; single replies isolate legality from aggregate utility.
fn reply_fixture(w:&World)->(Candidate,[f64;73]) {
    let mut c=Candidate {body_len:w.snake(0).unwrap().segments.len(),..Candidate::default()};c.path.fill(Point{x:600.0,y:700.0});
    let distance=[0.0;73];(c,distance)
}
#[test]
fn advisory_reply_wall_exit_is_terminal_even_when_it_returns_next_tick() {
    let mut w=arena(false);w.config.self_collisions=false;
    line(&mut w,0,Point{x:600.0,y:700.0},0.0,120);
    line(&mut w,1,Point{x:1199.6,y:300.0},1.3,24);
    let victim=w.snake(1).unwrap();let (speed,turn)=w.motion_limits(1,0.0).unwrap();
    let mut q=victim.segments[0].current;let mut angle=victim.angle;
    let (mut c,distance)=reply_fixture(&w);
    for j in 1..=4 {
        angle+=turn*STEP_SECONDS;q.x+=angle.cos()*speed*STEP_SECONDS;q.y+=angle.sin()*speed*STEP_SECONDS;
        if j==1 {assert!(q.x>w.config.width);}
        if j==2 {assert!(q.x<w.config.width);}
    }
    c.path[4]=q;
    let ai=AiController::new();
    assert!(!ai.reply_blocked(&w,w.snake(0).unwrap(),&c,victim,1.6,0,0.0,&distance));
    w.config.deadly_walls=false;
    assert!(ai.reply_blocked(&w,w.snake(0).unwrap(),&c,w.snake(1).unwrap(),1.6,0,0.0,&distance));
}
#[test]
fn advisory_reply_releases_deposits_after_the_tail_passes_in_wrap() {
    let mut w=arena(true);w.config.width=400.0;w.config.speed=1000.0;
    line(&mut w,0,Point{x:200.0,y:700.0},0.0,120);
    line(&mut w,1,Point{x:100.0,y:300.0},0.0,24);
    let (speed,_)=w.motion_limits(1,0.0).unwrap();assert_eq!(speed,712.0);
    let (mut c,distance)=reply_fixture(&w);
    c.path[20]=w.canonical_point(Point{x:100.0+speed*20.0*STEP_SECONDS,y:300.0});
    assert!(AiController::new().reply_blocked(&w,w.snake(0).unwrap(),&c,w.snake(1).unwrap(),0.0,0,0.0,&distance));
}
#[test]
fn advisory_reply_hits_barrier_between_former_sampled_intervals() {
    for wrap in [false,true] {
        let mut w=arena(wrap);w.config.self_collisions=false;
        let y=if wrap {0.0} else {300.0};
        line(&mut w,0,Point{x:139.3333333333,y:y-72.0},std::f64::consts::FRAC_PI_2,120);
        line(&mut w,1,Point{x:100.0,y},0.0,24);
        let mut c=Candidate {body_len:w.snake(0).unwrap().segments.len(),..Candidate::default()};let mut distance=[0.0;73];
        for j in 0..=72 {c.path[j]=w.canonical_point(Point{x:139.3333333333,y:y-72.0+j as f64*24.0});distance[j]=j as f64*24.0;}
        assert!(AiController::new().reply_blocked(&w,w.snake(0).unwrap(),&c,w.snake(1).unwrap(),0.0,0,0.0,&distance),"wrap={wrap}");
    }
}
#[test]
fn advisory_pocket_admission_checks_the_inner_radius_before_installing() {
    let center=Point{x:700.0,y:170.0};let radius=145.0;let theta=-0.4_f64;
    let mut w=arena(false);
    line(&mut w,0,Point{x:center.x+radius*theta.cos(),y:center.y+radius*theta.sin()},theta+std::f64::consts::FRAC_PI_2,180);
    line(&mut w,1,center,0.0,24);
    for id in [0,1] {w.snakes[id].radius=18.0;w.snakes[id].base_radius=18.0;}
    for j in 10..22 {let a=(j-10) as f64*std::f64::consts::TAU/12.0;
        w.segments[j].current=Point{x:center.x+radius*a.cos(),y:center.y+radius*a.sin()};}
    let mut ai=AiController::new();ai.prepare(&w);
    let mut state=State {prey:2,prey_generation:w.snake(1).unwrap().generation,hunt_until:120,..State::default()};
    assert!(!ai.pocket(&w,w.snake(0).unwrap(),&mut state));
    assert_eq!(state.coil_radius,0.0,"rejected admission must leave no retained plan");
}
#[test]
fn advisory_attack_sweeps_split_before_stage_speed_changes_for_all_attack_slots() {
    for kind in [1,7,8] {
        let mut w=arena(true);w.config.speed=1000.0;w.config.self_collisions=false;
        line(&mut w,0,Point{x:400.0,y:200.0},0.0,32);w.snakes[0].traits.speed_bias=1.14;
        line(&mut w,1,Point{x:900.0,y:650.0},0.0,24);
        let attack=Attack {valid:true,prey:2,prey_generation:w.snake(1).unwrap().generation,turn_at:7,end:48,
            approach:2.5,crossing:2.5,burst:1.0,crossing_rush:0.15,point:Point{x:900.0,y:650.0},..Attack::default()};
        let state=State {attack,attack_options:[attack;2],prey:2,prey_generation:attack.prey_generation,hunt_until:120,..State::default()};
        let mut ai=AiController::new();ai.prepare(&w);
        let open=ai.rollout(&w,w.snake(0).unwrap(),state,kind,8);assert_eq!(open.steps,8);
        let a=open.path[4];let b=open.path[8];let p=open.path[6];let ab=w.displacement(a,b);let ap=w.displacement(a,p);
        let f=(ap.x*ab.x+ap.y*ab.y)/(ab.x*ab.x+ab.y*ab.y);
        let deviation=Point{x:ap.x-ab.x*f,y:ap.y-ab.y*f};let norm=(deviation.x*deviation.x+deviation.y*deviation.y).sqrt();
        let obstacle=Point{x:p.x+deviation.x/norm*9.3,y:p.y+deviation.y/norm*9.3};
        assert!(w.segment_distance_squared(obstacle,open.path[5],open.path[6])<9.36_f64.powi(2));
        line(&mut w,2,Point{x:1000.0,y:700.0},0.0,100);
        for j in 1..100 {w.segments[2*MAX_SEGMENTS+j]=Segment{current:obstacle,previous:obstacle};}
        ai.tick=u64::MAX;ai.prepare(&w);
        let checked=ai.rollout(&w,w.snake(0).unwrap(),state,kind,8);
        assert!(checked.steps<8,"attack slot {kind} accepted an actual body contact");
    }
}

#[test]
fn advisory_reply_checks_current_self_geometry_on_previously_skipped_ticks() {
    let mut w=arena(false);
    line(&mut w,0,Point{x:600.0,y:700.0},0.0,120);
    line(&mut w,1,Point{x:400.0,y:300.0},0.0,24);
    let speed=w.motion_limits(1,0.0).unwrap().0;
    let q1=Point{x:400.0+speed*STEP_SECONDS,y:300.0};
    w.segments[MAX_SEGMENTS+10]=Segment {current:q1,previous:q1};
    let (mut c,distance)=reply_fixture(&w);c.path[2]=Point{x:400.0+speed*2.0*STEP_SECONDS,y:300.0};
    let ai=AiController::new();
    assert!(!ai.reply_blocked(&w,w.snake(0).unwrap(),&c,w.snake(1).unwrap(),0.0,0,0.0,&distance));
    w.config.self_collisions=false;
    assert!(ai.reply_blocked(&w,w.snake(0).unwrap(),&c,w.snake(1).unwrap(),0.0,0,0.0,&distance));
}

#[test]
fn bounded_angle_fast_path_matches_general_remainder_bit_for_bit() {
    use std::f64::consts::{PI,TAU};
    for angle in [-0.0,0.0,PI,-PI,TAU,-TAU,TAU-f64::EPSILON*4.0,
                  -TAU+f64::EPSILON*4.0,1e300,-1e300] {
        assert_eq!(normalize_angle(angle).to_bits(),crate::normalize_angle(angle).to_bits());
    }
    for i in -10000..=10000 {
        let angle=i as f64*TAU/1001.0;
        assert_eq!(normalize_angle(angle).to_bits(),crate::normalize_angle(angle).to_bits());
    }
    for angle in [f64::NAN,f64::INFINITY,f64::NEG_INFINITY] {
        assert!(normalize_angle(angle).is_nan());
    }
}

#[test]
fn candidate_scratch_never_reads_a_previous_decisions_unused_path() {
    for rules in [crate::RuleSet::Classic,crate::RuleSet::V2] {
    let cfg=Config {rules,width:3440.0,height:1440.0,density:100.0,trails:100.0,intelligence:100.0,
        deadly_walls:false,self_collisions:true,seed:20260814,..Config::default()};
    let mut a=World::new(cfg).unwrap();let mut b=a.diagnostic_snapshot();
    let mut plain=AiController::new();let mut poisoned=AiController::new();
    for _ in 0..3000 {
        for c in poisoned.candidates.as_mut().unwrap().iter_mut() {
            c.path.fill(Point{x:f64::NAN,y:f64::NAN});
            c.steps=STEPS;c.score=f64::NAN;c.area=usize::MAX;c.checked=true;
        }
        poisoned.rollout_distance.fill(f64::NAN);
        a.step(&mut plain);b.step(&mut poisoned);
        assert_eq!(a.stats(),b.stats());assert_eq!(a.rng_state(),b.rng_state());
        for (sa,sb) in a.snakes().zip(b.snakes()) {
            assert_eq!(sa.angle,sb.angle,"tick {} snake {}",a.tick(),sa.id);
            assert_eq!(sa.segments,sb.segments);
        }
    }
    }
}

#[test]
fn v2_requests_only_fixed_ready_boosts_and_exports_intent() {
    let mut w=World::new(Config{rules:crate::RuleSet::V2,..Config::default()}).unwrap();
    let s=w.snake(0).unwrap();
    assert_eq!(AiController::boost_request(&w,s,0.25),0.0);
    assert_eq!(AiController::boost_request(&w,s,0.49),0.0);
    assert_eq!(AiController::boost_request(&w,s,0.5),0.6);
    assert_eq!(AiController::boost_request(&w,s,1.0),0.6);
    let mut ai=AiController::new();
    ai.states[0].prey=2;ai.states[0].debug.flags=32;
    assert_eq!(ai.intent_flags(0),Some(crate::flags::HUNTING|crate::flags::TRAPPED));
    w.step(&mut crate::controller::ScriptedController::new(|_,s:SnakeView<'_>|Steering{desired_angle:s.angle,rush:1.0}));
    assert_eq!(AiController::boost_request(&w,w.snake(0).unwrap(),1.0),0.0);
}

#[test]
fn v2_forecast_tracks_payments_expiry_and_observed_burst_from_tick_one() {
    let mut w=arena(true);w.config.rules=crate::RuleSet::V2;
    line(&mut w,0,Point{x:400.0,y:400.0},0.0,120);
    let mut ai=AiController::new();ai.prepare(&w);
    let predicted=ai.rollout(&w,w.snake(0).unwrap(),State {rush:0.6,desired:0.9,turn_until:u64::MAX,..State::default()},1,72);
    assert_eq!(predicted.steps,72);
    let schedule=Motion::forecast(&w,0,0.6);
    for j in 0..36 {
        let expected=w.forecast_motion_limits(0,if j==0 {0.6} else {0.0},0).unwrap();
        assert_eq!(schedule.at(j),expected,"payment/expiry tick {j}");
        w.step(&mut crate::controller::ScriptedController::new(|_,s:SnakeView<'_>|Steering {desired_angle:0.9,rush:if s.boost_ticks==0 && s.cooldown_ticks==0 {0.6} else {0.0}}));
        let actual=w.motion_limits(0,w.observed_rush(0).unwrap()).unwrap();
        assert_eq!(schedule.at(j),actual,"actual motion tick {j}");
        assert!(w.distance_squared(predicted.path[j+1],w.snake(0).unwrap().segments[0].current)<1e-15,"rollout tick {j}");
        if j==0 {
            assert_ne!(w.snake(0).unwrap().flags & crate::flags::BOOSTING,0);
            let mut observer=AiController::new();observer.prepare(&w);
            let rival=&observer.rivals[0];
            let m=Motion::forecast(&w,0,0.0);
            assert!((w.distance_squared(rival.path[0],rival.path[1]).sqrt()-m.at(0).0*STEP_SECONDS).abs()<1e-10);
            assert!(m.at(23).0<m.at(22).0,"observed burst expires after remaining 23 ticks");
        }
    }
    assert_eq!(w.snake(0).unwrap().segments.len(),117);
}

#[test]
fn v2_cutoffs_require_ready_budget_but_do_not_spend_on_an_ordinary_chase() {
    let mut w=arena(true);w.config.rules=crate::RuleSet::V2;
    line(&mut w,0,Point{x:400.0,y:400.0},0.0,15);
    line(&mut w,1,Point{x:450.0,y:470.0},-std::f64::consts::FRAC_PI_2,6);
    let mut ai=AiController::new();ai.prepare(&w);
    let state=State {prey:2,prey_generation:w.snake(1).unwrap().generation,hunt_until:180,
        goal:Point{x:550.0,y:400.0},..State::default()};
    assert!(ai.cutoffs(&w,w.snake(0).unwrap(),state).iter().all(|a|!a.valid));
    assert_eq!(ai.rush_for(&w,w.snake(0).unwrap(),state),0.0);
    line(&mut w,0,Point{x:400.0,y:400.0},0.0,72);w.tick+=1;ai.prepare(&w);
    assert!(ai.cutoffs(&w,w.snake(0).unwrap(),state).iter().any(|a|a.valid));
    w.snakes[0].cooldown_ticks=1;w.tick+=1;ai.prepare(&w);
    assert!(ai.cutoffs(&w,w.snake(0).unwrap(),state).iter().all(|a|!a.valid));
}

#[test]
fn v2_food_race_spends_only_on_valuable_close_competition_with_turn_room() {
    let mut w=arena(true);w.config.rules=crate::RuleSet::V2;
    line(&mut w,0,Point{x:400.0,y:400.0},0.0,24);
    line(&mut w,1,Point{x:600.0,y:500.0},-1.0,24);
    food(&mut w,Point{x:600.0,y:400.0});w.food[0].value=3.0;
    let mut ai=AiController::new();ai.prepare(&w);
    let state=State {target:100000,goal:w.food[0].p,..State::default()};
    assert!(!ai.food_race(&w,w.snake(0).unwrap(),state),"rival is half as far, outside the 25% band");
    line(&mut w,1,Point{x:600.0,y:620.0},-1.0,24);w.tick+=1;ai.prepare(&w);
    assert!(ai.food_race(&w,w.snake(0).unwrap(),state));
    w.food[0].value=1.0;w.tick+=1;ai.prepare(&w);
    assert!(!ai.food_race(&w,w.snake(0).unwrap(),state));
    w.food[0].value=3.0;w.snakes[0].angle=2.0;w.tick+=1;ai.prepare(&w);
    assert!(!ai.food_race(&w,w.snake(0).unwrap(),state),"target requires an unsafe boosted reversal");
}

#[test]
fn v2_open_coil_admission_requires_body_for_one_point_two_loops() {
    for wrap in [false,true] {
        let mut w=arena(wrap);w.config.rules=crate::RuleSet::V2;
        line(&mut w,0,Point{x:500.0,y:400.0},std::f64::consts::FRAC_PI_2,150);
        line(&mut w,1,Point{x:420.0,y:400.0},0.0,24);
        let mut ai=AiController::new();ai.prepare(&w);
        let mut state=State {prey:2,prey_generation:w.snake(1).unwrap().generation,hunt_until:180,..State::default()};
        assert!(ai.pocket(&w,w.snake(0).unwrap(),&mut state));
        assert!(state.coil_radius>=4.2*w.snake(0).unwrap().radius);
        assert!(ai.pocket_usable(&w,w.snake(0).unwrap(),state));
        state.clear_coil(w.snake(0).unwrap().angle);
        line(&mut w,1,Point{x:340.0,y:400.0},0.0,24);w.tick+=1;ai.prepare(&w);
        assert!(!ai.pocket(&w,w.snake(0).unwrap(),&mut state),"outside 20 radii and too much circumference");
        line(&mut w,0,Point{x:500.0,y:400.0},std::f64::consts::FRAC_PI_2,149);w.tick+=1;ai.prepare(&w);
        assert!(!ai.pocket(&w,w.snake(0).unwrap(),&mut state),"below mature length threshold");
    }
}

#[test]
fn v2_retained_cutoff_survives_its_payments_but_rejects_unplanned_growth() {
    let mut w=arena(true);w.config.rules=crate::RuleSet::V2;
    line(&mut w,0,Point{x:400.0,y:400.0},0.0,72);
    line(&mut w,1,Point{x:450.0,y:470.0},-std::f64::consts::FRAC_PI_2,24);
    let mut ai=AiController::new();ai.prepare(&w);
    let mut state=State {prey:2,prey_generation:w.snake(1).unwrap().generation,hunt_until:180,..State::default()};
    let mut attack=ai.cutoffs(&w,w.snake(0).unwrap(),state).into_iter().find(|a|a.valid).unwrap();
    // Keep prey distant and unpassed while testing the retained motion budget.
    attack.point=Point{x:600.0,y:200.0};attack.prey_heading=0.0;attack.end=60;
    state.attack=attack;
    line(&mut w,1,Point{x:200.0,y:200.0},0.0,24);w.snakes[1].traits.speed_bias=0.2;
    for j in 0..30 {
        assert!(ai.attack_usable(&w,w.snake(0).unwrap(),state,attack),"retained tick {j}");
        w.step(&mut crate::controller::ScriptedController::new(|_,s:SnakeView<'_>|Steering {desired_angle:s.angle,rush:if s.id==0 && j==0 {0.6} else {0.0}}));
    }
    w.snakes[0].len+=1;
    assert!(!ai.attack_usable(&w,w.snake(0).unwrap(),state,attack));
}

#[test]
fn advisory_retained_hunt_refresh_does_not_rebudget_paid_boost_segments() {
    // Cover the base cost and its size-dependent siblings at both boundaries.
    for length in [30,100,200] {
        let mut w=arena(true);w.config.rules=crate::RuleSet::V2;
        w.config.width=3440.0;w.config.height=1440.0;
        line(&mut w,0,Point{x:1800.0,y:400.0},0.0,length);
        line(&mut w,1,Point{x:1600.0,y:200.0},0.0,length-4-2-length/100);
        w.snakes[1].traits.speed_bias=0.2;
        let mut ai=AiController::new();ai.prepare(&w);
        let mut attack=Attack {valid:true,prey:2,prey_generation:w.snake(1).unwrap().generation,
            start:0,turn_at:18,end:60,approach:0.0,crossing:0.0,burst:0.6,
            point:Point{x:2000.0,y:200.0},..Attack::default()};
        attack.limits=Some(AiController::attack_limits(&w,w.snake(0).unwrap(),attack));
        let mut state=State {prey:2,prey_generation:attack.prey_generation,hunt_until:180,attack,..State::default()};
        for j in 0..30 {
            ai.prepare(&w);
            assert!(ai.attack_usable(&w,w.snake(0).unwrap(),state,attack),"length {length}, tick {j}");
            ai.tactics(&w,w.snake(0).unwrap(),&mut state,1.0);
            assert_eq!(state.prey,2,"refresh rebudgeted length {length}, tick {j}");
            assert!(state.attack.valid,"refresh cancelled length {length}, tick {j}");
            w.step(&mut crate::controller::ScriptedController::new(|_,s:SnakeView<'_>|Steering {
                desired_angle:s.angle,rush:if s.id==0 && j==0 {0.6} else {0.0}}));
        }
        assert_eq!(w.snake(0).unwrap().segments.len(),length-2-length/100);
        // Retention is conditional on validation, never permission to keep a
        // stale attack after an unrelated body change loses winning advantage.
        w.snakes[0].len-=1;w.tick+=1;ai.prepare(&w);
        ai.tactics(&w,w.snake(0).unwrap(),&mut state,1.0);
        assert!(!state.attack.valid);
        assert_eq!(state.prey,0);
    }
}

#[test]
fn advisory_straight_alias_preserves_unboosted_food_race_alternative() {
    let mut w=arena(true);w.config.rules=crate::RuleSet::V2;
    // Slot 2 has no strategy quota on this tick, retaining its straight plan.
    line(&mut w,2,Point{x:400.0,y:400.0},0.0,24);
    line(&mut w,1,Point{x:600.0,y:630.0},-1.0,24);
    food(&mut w,Point{x:600.0,y:420.0});w.food[0].value=3.0;
    let mut ai=AiController::new();ai.prepare(&w);ai.enable_diagnostics();
    let s=w.snake(2).unwrap();
    let state=State {generation:s.generation,target:100000,goal:w.food[0].p,
        last_angle:s.angle,desired:s.angle,turn_until:u64::MAX,best_distance:f64::MAX,..State::default()};
    assert!(ai.food_race(&w,s,state));
    ai.states[2]=state;
    ai.steer(&w,s);let d=ai.decision(2);
    assert_eq!(d.candidates[1].rush,0.6);
    for kind in [2,9,10,12] {
        assert_eq!(d.candidates[kind].rush,0.0,"unboosted straight sibling {kind}");
        assert_eq!(d.candidates[kind].desired,s.angle);
    }
    assert_eq!(d.candidates[11].rush,0.0);
    assert_ne!(d.candidates[11].desired,s.angle,"unboosted food bearing cannot replace straight");
}

#[test]
fn advisory_crossing_diagnostics_export_validity_for_all_attack_slots() {
    for rules in [crate::RuleSet::Classic,crate::RuleSet::V2] {
        let mut w=arena(true);w.config.rules=rules;w.tick=20;
        line(&mut w,2,Point{x:400.0,y:400.0},0.0,72);
        line(&mut w,1,Point{x:200.0,y:200.0},0.0,24);
        w.snakes[1].traits.speed_bias=0.2;
        let mut ai=AiController::new();ai.prepare(&w);ai.enable_diagnostics();
        let s=w.snake(2).unwrap();
        let attack=Attack {valid:true,prey:2,prey_generation:w.snake(1).unwrap().generation,
            start:0,turn_at:18,end:80,approach:0.7,crossing:0.0,burst:0.6,
            crossing_rush:if rules==crate::RuleSet::Classic {0.15} else {0.0},
            point:Point{x:600.0,y:200.0},..Attack::default()};
        ai.states[2]=State {generation:s.generation,attack,prey:2,prey_generation:attack.prey_generation,
            hunt_until:180,next_response:100,last_angle:s.angle,desired:attack.crossing,
            turn_until:22,exit_angle:-1.2,..State::default()};
        ai.steer(&w,s);let d=ai.decision(2);
        let c=d.candidates[1];
        assert!(c.attack_valid);
        assert_eq!(c.attack_turn_ticks,0);
        assert_eq!(c.attack_crossing,c.desired);
        assert_eq!(c.attack_crossing_rush,c.rush);
        assert_eq!(c.turn_ticks,2);
        assert_ne!(c.exit_angle,c.desired);
        for (slot,c) in d.candidates.iter().enumerate() {
            assert_eq!(c.attack_valid,ai.candidates.as_ref().unwrap()[slot].attack.valid,"{rules:?}, slot {slot}");
        }
        assert!(!d.candidates[2].attack_valid,"straight aliases must not inherit attack validity");
    }
}

#[test]
fn v2_escape_boost_opens_a_path_when_all_unboosted_controls_fail_imminently() {
    let mut w=arena(true);w.config.rules=crate::RuleSet::V2;
    line(&mut w,0,Point{x:500.0,y:400.0},0.0,24);
    line(&mut w,1,Point{x:515.0,y:360.0},1.57,72);
    let mut ai=AiController::new();ai.prepare(&w);
    let state=State {desired:0.0,turn_until:u64::MAX,goal:Point{x:800.0,y:400.0},revise_opponents:true,..State::default()};
    for kind in [0,2,3,4,5,6,7,8] {
        assert!(ai.rollout(&w,w.snake(0).unwrap(),state,kind,72).steps<18);
    }
    let boosted=ai.rollout(&w,w.snake(0).unwrap(),State {rush:0.6,escape_boost:true,..state},3,72);
    assert_eq!(boosted.steps,72);
    ai.enable_diagnostics();
    let chosen=ai.steer(&w,w.snake(0).unwrap());
    assert_eq!(chosen.rush,0.6);
    assert_eq!(ai.decision(0).candidates[ai.decision(0).selected].safe_ticks,72);
    for j in 0..24 {
        w.step(&mut crate::controller::ScriptedController::new(|_,s:SnakeView<'_>|Steering {
            desired_angle:if s.id==0 {chosen.desired_angle} else {1.57},rush:if s.id==0 && j==0 {0.6} else {0.0}}));
        assert!(w.snake(0).unwrap().alive,"physical escape tick {j}");
    }
    // An open world with a complete unboosted path must retain its tail.
    w.snakes[1].alive=false;w.tick+=36;w.snakes[0].boost_ticks=0;w.snakes[0].cooldown_ticks=0;
    let mut fresh=AiController::new();
    assert_eq!(fresh.steer(&w,w.snake(0).unwrap()).rush,0.0);
}

#[test]
fn v2_crossing_stage_cannot_switch_off_an_active_burst() {
    let mut w=arena(true);w.config.rules=crate::RuleSet::V2;
    line(&mut w,0,Point{x:400.0,y:400.0},0.0,72);
    line(&mut w,1,Point{x:900.0,y:650.0},0.0,24);
    let attack=Attack {valid:true,prey:2,prey_generation:w.snake(1).unwrap().generation,
        start:0,turn_at:7,end:60,approach:0.3,crossing:1.0,burst:0.6,crossing_rush:0.0,
        point:Point{x:900.0,y:650.0},..Attack::default()};
    let state=State {prey:2,prey_generation:attack.prey_generation,hunt_until:60,attack,
        attack_options:[attack;2],..State::default()};
    let mut ai=AiController::new();ai.prepare(&w);
    for kind in [1,7,8] {
        let c=ai.rollout(&w,w.snake(0).unwrap(),state,kind,36);
        assert_eq!(c.steps,36);
        let mut actual=w.diagnostic_snapshot();
        for j in 0..36 {
            actual.step(&mut crate::controller::ScriptedController::new(|tick:u64,s:SnakeView<'_>| {
                let (angle,rush)=if s.id==0 {attack.control(tick)} else {(s.angle,0.0)};
                Steering {desired_angle:angle,rush}
            }));
            assert!(actual.snake(0).unwrap().alive);
            assert!(actual.distance_squared(c.path[j+1],actual.snake(0).unwrap().segments[0].current)<1e-15,"slot {kind}, step {j}");
            if j==8 {assert_ne!(actual.snake(0).unwrap().flags & crate::flags::BOOSTING,0);}
            if j==24 {assert_eq!(actual.snake(0).unwrap().flags & crate::flags::BOOSTING,0);}
        }
    }
}

#[test]
fn validated_candidate_commit_preserves_staged_continuations() {
    let mut w=arena(true);w.config.rules=crate::RuleSet::V2;
    line(&mut w,0,Point{x:500.0,y:400.0},0.0,72);
    let mut ai=AiController::new();ai.prepare(&w);
    let original=State {desired:0.6,turn_until:7,exit_angle:-0.2,
        goal:Point{x:800.0,y:500.0},..State::default()};
    for kind in [1,12] {
        let c=ai.rollout(&w,w.snake(0).unwrap(),original,kind,36);
        assert_eq!(c.steps,36);
        let mut committed=original;
        State::commit_candidate(&mut committed,&c,kind,&w,w.snake(0).unwrap(),ai.rivals[0].turn,0);
        assert_eq!(committed.turn_until,original.turn_until,"kind {kind}");
        assert_eq!(committed.exit_angle,original.exit_angle,"kind {kind}");
        let retained=ai.rollout(&w,w.snake(0).unwrap(),committed,1,36);
        for j in 0..=36 {
            assert!(w.distance_squared(c.path[j],retained.path[j])<1e-15,"kind {kind}, tick {j}");
        }
    }
}

#[test]
fn all_candidate_controls_survive_alias_slots_and_escape_bookkeeping() {
    for rules in [crate::RuleSet::Classic,crate::RuleSet::V2] {
        let mut w=arena(true);w.config.rules=rules;
        line(&mut w,0,Point{x:500.0,y:400.0},0.0,72);
        line(&mut w,1,Point{x:900.0,y:650.0},0.0,24);
        let mut ai=AiController::new();ai.prepare(&w);
        let attack=Attack {valid:true,prey:2,prey_generation:w.snake(1).unwrap().generation,
            start:0,turn_at:7,end:60,approach:0.3,crossing:1.0,burst:0.6,crossing_rush:0.0,
            point:Point{x:900.0,y:650.0},..Attack::default()};
        for variant in 0..5 {
            let state=State {desired:0.6,turn_until:7,exit_angle:-0.2,rush:if variant==0 {0.0} else {0.6},
                escape_boost:variant==1,track_goal:variant==2 || variant==3,
                target:if variant==2 {7} else {0},prey:if variant==4 {2} else {0},
                prey_generation:attack.prey_generation,hunt_until:60,
                attack:if variant==4 {attack} else {Attack::default()},
                attack_options:if variant==4 {[attack;2]} else {[Attack::default();2]},
                coil_center:Point{x:500.0,y:500.0},coil_radius:if variant==3 {140.0} else {0.0},
                coil_initial_radius:if variant==3 {140.0} else {0.0},coil_sign:1.0,coil_pitch:14.0,
                coil_last_head:Point{x:500.0,y:400.0},coil_until:100,
                goal:Point{x:800.0,y:500.0},waypoint:Some(Point{x:700.0,y:450.0}),..State::default()};
            for kind in 0..CANDIDATES {
                let c=ai.rollout(&w,w.snake(0).unwrap(),state,kind,36);
                if !c.checked {continue;}
                for slot in [kind,1,12] {
                    let mut committed=state;
                    // Force the narrow-space bookkeeping, including coil and
                    // attack cleanup. Scratch aliases must not change controls.
                    State::commit_candidate(&mut committed,&c,slot,&w,w.snake(0).unwrap(),ai.rivals[0].turn,usize::MAX);
                    assert_eq!(committed.turn_until,c.turn_until);
                    assert_eq!(committed.exit_angle,c.exit_angle);
                    assert_eq!(committed.track_goal,c.tracks_goal);
                    assert_eq!(committed.attack.valid,c.attack.valid);
                    assert_eq!(committed.rush,c.rush);
                    let retained=ai.rollout(&w,w.snake(0).unwrap(),committed,1,36);
                    assert_eq!(c.steps,retained.steps,"{rules:?} variant {variant} kind {kind} slot {slot}");
                    for j in 0..=c.steps {
                        assert!(w.distance_squared(c.path[j],retained.path[j])<1e-15,
                            "{rules:?} variant {variant} kind {kind} slot {slot} tick {j}");
                    }
                }
            }
        }
    }
}
