// SPDX-License-Identifier: GPL-3.0-or-later
//! Boundary fixtures drive the real step, taking the same pre-movement
//! observation as production steering. No hand-advanced world effect clocks.
use super::*;
use crate::{Config, FoodKind, Inventory, Item, RuleSet};
use crate::controller::{Controller, ScriptedController, Steering};
use crate::world::whirlpool::{LIFE, PULL_END, ESSENCE_BIT};

#[derive(Clone,Copy,Debug)]
enum Start { Existing(u16), Field(usize), Held(usize) }
struct Observe { source:Option<World> }
impl Controller for Observe {
    fn steer(&mut self,w:&World,s:SnakeView<'_>)->Steering {
        if self.source.is_none() {self.source=Some(w.diagnostic_snapshot());}
        Steering {desired_angle:s.angle,rush:0.0}
    }
}
fn arena()->World {arena_seed(1)}
fn arena_seed(seed:i32)->World {
    let w=World::diagnostic_arena(Config {seed,rules:RuleSet::V2,density:0.0,
        width:10000.0,height:4000.0,speed:0.0,world_events:false,
        store_power_ups:true,self_collisions:false,deadly_walls:false,..Default::default()},
        &[(Point{x:1500.0,y:1000.0},0.0,24,0.0)],&[]).unwrap();
    w
}
fn particle(w:&mut World,p:Point)->target::TargetFood {
    w.food.push(crate::world::Food {id:9999991,p,kind:FoodKind::Shard,value:1.0,
        life:100.0,original_life:100.0,owner:-1,size:2.0,..Default::default()});
    w.foods().last().unwrap().into()
}
fn fixture(start:Start,offset:f64)->(World,World,usize,usize) {
    let mut w=arena();
    let center=match start {Start::Held(_)=>w.segments[0].current,_=>Point{x:5000.0,y:2000.0}};
    let (pull,end)=match start {
        Start::Existing(age)=> {
            w.open_whirlpool(0,center);
            w.items[0].age_ticks=age;w.items[0].charge_ticks=age;w.items[0].life_ticks=LIFE-age;
            ((PULL_END-age) as usize,(LIFE-age) as usize)
        },
        Start::Field(n)=> {
            w.config.store_power_ups=false;
            // Large contact disk lets a remote holder activate a field brew
            // without feeding on the particles under observation.
            w.items.push(Item {id:42,kind:EffectKind::Whirlpool,position:center,
                radius:5000.0,life_ticks:750,pickable_from_tick:n as u64,..Default::default()});
            (n+PULL_END as usize,n+LIFE as usize)
        },
        Start::Held(n)=> {
            w.snakes[0].inventory=Inventory {count:1,kinds:[7,0,0],life:[1800,0,0],
                windup:1,windup_ticks:n as u8,..Default::default()};
            (n+PULL_END as usize,n+LIFE as usize)
        },
    };
    particle(&mut w,Point{x:center.x,y:center.y+offset});
    let mut observe=Observe {source:None};w.step(&mut observe);
    (w,observe.source.unwrap(),pull,end)
}
fn positions(w:&World)->[Point;MAX_SNAKES] {
    std::array::from_fn(|id|w.snake(id).filter(|s|s.alive).map_or(Point::default(),|s|s.segments[0].current))
}
/// Compare ordinary, cached, ordered and strategy forecast paths with World.
/// Every frame is checked, including both neighbors of each requested boundary.
fn transport(start:Start,offset:f64,last:usize)->(World,Timeline) {
    let (mut w,source,_,_)=fixture(start,offset);
    let initial=Timeline::new(&source);
    let mut direct=Forecast::new(&source,initial);
    let mut cached=Forecast::new(&source,initial);
    let mut ordered=Forecast::new(&source,initial);
    let mut ai=AiController::new();ai.prepare(&source);
    let strategy=ai.opportunities;
    let f:target::TargetFood=source.foods().find(|f|f.id==9999991).unwrap().into();
    let mut motion=FoodMotion::new(&source,f);
    let mut cache=Items::default();
    // These remote capsules cover the whole fixture path. Exercise the
    // static cached hit branch as well as the dynamic/ordered passes.
    cache.hits.fill([1;crate::MAX_ITEMS]);
    let mut straight=ScriptedController::new(|_,s:SnakeView<'_>|Steering {desired_angle:s.angle,rush:0.0});
    for step in 1..=last {
        if step>1 {w.step(&mut straight);}
        let p=positions(&w);
        // Preserve the pre-pickup brew just as rollout feeding does.
        motion.advance_brew(&source,direct.effects.brew,f.kind,step);
        direct.advance(&source,step,|id|p[id],0.0);
        // Static contacts reuse the precomputed hit mask.
        cached.advance_cached(&source,step,|id|p[id],0,&cache,0.0);
        let mut endpoints=p;
        ordered.advance_ordered(&source,step,&mut endpoints,|_,id|p[id],None,0.0);
        let raw=w.food.iter().find(|f|f.id==9999991);
        let available=raw.is_some_and(|f|f.captured_by==0) || w.consumption_events().any(|e|e.0==9999991);
        assert_eq!(!motion.unavailable(),available,"transport {start:?} step={step}");
        if let Some(raw)=raw {
            assert!(source.distance_squared(motion.position,raw.p)<1e-16,"position {start:?} step={step}: {:?} {:?}",motion.position,raw.p);
        }
        for (name,t) in [("direct",direct.effects),("cached",cached.effects),("ordered",ordered.effects)] {
            assert_eq!(t.food_available(&source,f,step),available,"{name} {start:?} step={step}");
            if let Some(v)=w.vortex() {
                assert_eq!(t.brew.end as usize,step+v.life_ticks as usize,"{name} end {start:?} step={step}");
                assert!(source.distance_squared(t.brew.center,v.position)<1e-16);
            }
        }
        // Opportunities deliberately predict only movement-one pickups.
        if !matches!(start,Start::Field(n)|Start::Held(n) if n>1) {
            assert_eq!(strategy.food_available(&source,f,step),available,"strategy {start:?} step={step}");
        }
    }
    (w,direct.effects)
}

#[test]
fn whirlpool_timing_start_age_zero_and_first_capture() {
    for start in [Start::Field(1),Start::Field(3),Start::Held(1),Start::Held(5)] {
        let n=match start {Start::Field(n)|Start::Held(n)=>n,_=>unreachable!()};
        let (mut w,source,_,_)=fixture(start,395.5);
        let mut forecast=Forecast::new(&source,Timeline::new(&source));
        let mut straight=ScriptedController::new(|_,s:SnakeView<'_>|Steering {desired_angle:s.angle,rush:0.0});
        for step in 1..=n+1 {
            if step>1 {w.step(&mut straight);}
            let p=positions(&w);forecast.advance(&source,step,|id|p[id],0.0);
            assert_eq!(w.vortex().map(|v|v.age_ticks),if step<n {None} else {Some((step-n) as u16)});
            assert_eq!(w.food.iter().find(|f|f.id==9999991).unwrap().captured_by!=0,step>n);
            let f=source.foods().find(|f|f.id==9999991).unwrap().into();
            assert_eq!(forecast.effects.food_available(&source,f,step),step<=n);
        }
        transport(start,395.5,n+2);
    }
    transport(Start::Existing(0),395.5,3);
}
#[test]
fn whirlpool_timing_pull_cutoff_138() {
    for start in [Start::Existing(134),Start::Field(1),Start::Held(5)] {
        let (_,_,pull,_)=fixture(start,395.5);
        // A survivor must stop moving at 138, without becoming edible.
        let (w,_)=transport(start,395.5,pull+1);
        assert!(w.vortex().is_some());assert_ne!(w.food.iter().find(|f|f.id==9999991).unwrap_or_else(||panic!("survivor missing {start:?}")).captured_by,0);
    }
}
#[test]
fn whirlpool_timing_absorption_radius_crossing() {
    // The pull crosses 0.9 r after observation, then the original identity
    // remains unavailable even at and after burst (nutrition becomes shards).
    let r=arena().config.base_radius();
    let decrement=22.0*r*0.25*(0.55+0.6)*STEP_SECONDS;
    for start in [Start::Existing(0),Start::Field(1),Start::Field(3)] {
        let n=match start {Start::Existing(_)=>0,Start::Field(n)=>n,_=>unreachable!()};
        let (w,_)=transport(start,0.9*r+2.5*decrement,n+LIFE as usize+1);
        assert!(!w.food.iter().any(|f|f.id==9999991));assert!(w.whirlpool_stats.captured_value>=1.0);
    }
}
#[test]
fn whirlpool_timing_burst_150() {
    for start in [Start::Existing(134),Start::Field(1),Start::Field(3),Start::Held(5)] {
        let (_,_,_,end)=fixture(start,395.5);
        let (w,t)=transport(start,395.5,end+1);
        assert_eq!(t.brew.end as usize,end);assert!(w.vortex().is_none());assert_eq!(w.whirlpool_stats.bursts,1);
    }
}
#[test]
fn whirlpool_timing_surviving_capture_release_on_burst_frame() {
    for start in [Start::Existing(134),Start::Field(1),Start::Held(5)] {
        let (_,_,_,end)=fixture(start,395.5);
        let (w,_)=transport(start,395.5,end+1);
        let f=w.food.iter().find(|f|f.id==9999991).unwrap_or_else(||panic!("survivor missing {start:?}"));
        assert_eq!(f.captured_by,0);assert!(f.pickup_eligible(w.tick()));
    }
}
#[test]
fn whirlpool_timing_shards_created_and_edible_on_burst_frame() {
    for start in [Start::Existing(134),Start::Field(1),Start::Field(3)] {
        let (mut w,_,_,end)=fixture(start,395.5);
        let mut straight=ScriptedController::new(|_,s:SnakeView<'_>|Steering {desired_angle:s.angle,rush:0.0});
        for step in 2..=end+1 {
            w.step(&mut straight);
            let shards:Vec<_>=w.food.iter().filter(|f|f.feast&ESSENCE_BIT!=0).collect();
            assert_eq!(shards.is_empty(),step<end);
            for shard in shards {
                let f:target::TargetFood=w.foods().find(|f|f.id==shard.id).unwrap().into();
                let c=target::Contact::forecast(w.snake(0).unwrap(),f,Timeline::new(&w).track(0));
                assert!(shard.pickup_eligible(w.tick()));assert!(c.reached(0.0,1));
                // update_food runs before creation: birth-frame positions
                // remain at the initial one-radius ring until the next tick.
                if step==end {assert!((w.distance_squared(Point{x:5000.0,y:2000.0},shard.p).sqrt()-w.config.base_radius()).abs()<1e-9);}
            }
        }
    }
}
#[test]
fn whirlpool_timing_exclusivity_allows_activation_on_burst_frame() {
    for delay in [0,1,2] {
        let mut w=arena();let center=Point{x:5000.0,y:2000.0};w.open_whirlpool(0,center);
        w.items[0].age_ticks=147;w.items[0].life_ticks=3;
        w.snakes[0].inventory=Inventory {count:1,kinds:[7,0,0],life:[1800,0,0],windup:1,windup_ticks:2+delay,..Default::default()};
        let mut observe=Observe {source:None};w.step(&mut observe);let source=observe.source.unwrap();
        let mut f=Forecast::new(&source,Timeline::new(&source));
        let mut straight=ScriptedController::new(|_,s:SnakeView<'_>|Steering {desired_angle:s.angle,rush:0.0});
        for step in 1..=5 {
            if step>1 {w.step(&mut straight);}
            let p=positions(&w);f.advance(&source,step,|id|p[id],0.0);
            if let Some(v)=w.vortex() {assert_eq!(f.effects.brew.end as usize,step+v.life_ticks as usize);assert!(source.distance_squared(f.effects.brew.center,v.position)<1e-16);}
        }
        assert_eq!(w.whirlpool_stats.opened,if delay==0 {1} else {2});
        assert_eq!(w.snakes[0].inventory.count,0);
    }
}

#[test]
fn whirlpool_timing_same_tick_flip_feeds_at_the_old_head() {
    for brewing in [false,true] {
    let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,density:0.0,speed:0.0,
        width:10000.0,height:4000.0,world_events:false,store_power_ups:true,
        self_collisions:false,deadly_walls:false,..Default::default()},
        &[(Point{x:1500.0,y:1000.0},0.0,24,0.0),(Point{x:1500.0,y:1250.0},0.0,24,0.0)],&[]).unwrap();
    for (id,kind) in [(0,EffectKind::Flip),(1,EffectKind::Whirlpool)] {
        if id==1 && !brewing {continue;}
        w.snakes[id].inventory=Inventory {count:1,kinds:[kind as u8,0,0],life:[1800,0,0],windup:1,windup_ticks:2,..Default::default()};
    }
    let travel=w.motion_limits(0,0.0).unwrap().0*STEP_SECONDS;
    let p=Point{x:1500.0+3.0*w.snakes[0].radius+2.0+1.5*travel,y:1000.0};
    particle(&mut w,p);
    let mut observe=Observe {source:None};w.step(&mut observe);let source=observe.source.unwrap();
    let f=source.foods().find(|f|f.id==9999991).unwrap();
    let state=State {target:f.id,target_index:0,goal:f.position,desired:0.0,track_goal:true,turn_until:u64::MAX,..Default::default()};
    let mut marked=source.diagnostic_snapshot();marked.food[0].feast=ESSENCE_BIT|42;
    let mut ordinary_ai=AiController::new();ordinary_ai.prepare(&source);
    let mut feast_ai=AiController::new();feast_ai.prepare(&marked);
    for horizon in 1..=3 {
        let mut a=Candidate::default();let mut b=Candidate::default();
        ordinary_ai.rollout_into(&source,source.snake(0).unwrap(),state,1,horizon,&mut a);
        feast_ai.rollout_into(&marked,marked.snake(0).unwrap(),state,1,horizon,&mut b);
        assert_eq!(a.steps,horizon);assert_eq!(b.steps,horizon);
        assert!((b.score-a.score-if horizon>=2 && (!brewing || horizon==2) {600.0} else {0.0}).abs()<1e-9,
            "horizon={horizon} ordinary={} feast={}",a.score,b.score);
        if horizon>1 {w.step(&mut ScriptedController::new(|_,s:SnakeView<'_>|Steering {desired_angle:s.angle,rush:0.0}));}
        if horizon<=2 || brewing {
            let raw=w.food.iter().find(|f|f.id==9999991).unwrap();
            assert_eq!(raw.owner,if horizon==2 {0} else {-1});
            assert_eq!(raw.captured_by!=0,horizon==3 && brewing);
        }
        if horizon==2 {
            assert!(w.frame_events().any(|e|e.kind==crate::EventKind::Flip));
            assert!(w.distance_squared(w.segments[0].current,p)>100.0*100.0);
            assert_eq!(w.vortex().map(|v|v.age_ticks),if brewing {Some(0)} else {None});
        }
    }
    }
}

#[test]
fn whirlpool_timing_same_tick_frost_keeps_brew_clock_and_feeding() {
    for frost_id in [0,1] {
        let brew_id=1-frost_id;
        let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,density:0.0,speed:0.0,
            width:10000.0,height:4000.0,world_events:false,store_power_ups:true,
            self_collisions:false,deadly_walls:false,..Default::default()},
            &[(Point{x:1500.0,y:1000.0},0.0,24,0.0),(Point{x:1500.0,y:1250.0},0.0,24,0.0)],&[]).unwrap();
        for (id,kind) in [(frost_id,EffectKind::Frost),(brew_id,EffectKind::Whirlpool)] {
            w.snakes[id].inventory=Inventory {count:1,kinds:[kind as u8,0,0],life:[1800,0,0],windup:1,windup_ticks:2,..Default::default()};
        }
        let travel=w.motion_limits(0,0.0).unwrap().0*STEP_SECONDS;
        let food_position=Point{x:1500.0+3.0*w.snakes[0].radius+2.0+1.5*travel,y:1000.0};
        particle(&mut w,food_position);
        let mut observe=Observe {source:None};w.step(&mut observe);let source=observe.source.unwrap();
        let mut f=Forecast::new(&source,Timeline::new(&source));
        let mut g=Forecast::new(&source,Timeline::new(&source));
        for step in 1..=3 {
            if step>1 {w.step(&mut ScriptedController::new(|_,s:SnakeView<'_>|Steering {desired_angle:s.angle,rush:0.0}));}
            let p=positions(&w);f.advance(&source,step,|id|p[id],0.0);
            let mut ordered=p;g.advance_ordered(&source,step,&mut ordered,|_,id|p[id],None,0.0);
            for t in [f.effects,g.effects] {
                assert_eq!(t.frozen_at(brew_id,step),w.snakes[brew_id].frozen_ticks);
                if let Some(v)=w.vortex() {
                    assert_eq!(v.age_ticks,(step-2) as u16);
                    assert_eq!(t.brew.start,3);assert_eq!(t.brew.end,2+LIFE);
                    assert!(source.distance_squared(t.brew.center,v.position)<1e-16);
                }
            }
            let raw=w.food.iter().find(|f|f.id==9999991);
            if step==3 && brew_id==0 {
                assert!(raw.is_none(),"the holder's central meal is absorbed on first pull");
            } else {
                let raw=raw.unwrap();
                assert_eq!(raw.owner,if step==2 {0} else {-1});
                assert_eq!(raw.captured_by!=0,step==3);
            }
        }
    }
}

#[test]
fn whirlpool_timing_new_activation_cannot_resurrect_previous_absorption() {
    let mut w=arena();let center=Point{x:5000.0,y:2000.0};
    w.open_whirlpool(0,center);w.items[0].age_ticks=134;w.items[0].life_ticks=16;
    w.config.store_power_ups=false;
    w.items.push(Item {id:42,kind:EffectKind::Whirlpool,position:Point{x:center.x,y:center.y-4.0},
        radius:7000.0,life_ticks:750,pickable_from_tick:16,..Default::default()});
    let r=w.config.base_radius();
    let damping=0.16_f64.powf(STEP_SECONDS);
    let velocity=(22.0*r+3.0-0.5*r)/(STEP_SECONDS*damping);
    // Observation is still outside the disk. The next world update captures
    // and absorbs it, so raw captured_by cannot hide lost forecast history.
    particle(&mut w,Point{x:center.x-22.0*r-3.0-velocity*STEP_SECONDS,y:center.y});
    w.food[0].velocity=Point{x:velocity,y:0.0};
    w.food.push(crate::world::Food {id:9999992,p:Point{x:center.x,y:center.y+395.5},
        kind:FoodKind::Shard,value:1.0,life:100.0,original_life:100.0,owner:-1,size:2.0,..Default::default()});
    let mut observe=Observe {source:None};w.step(&mut observe);let source=observe.source.unwrap();
    let food:target::TargetFood=source.foods().find(|f|f.id==9999991).unwrap().into();
    let survivor:target::TargetFood=source.foods().find(|f|f.id==9999992).unwrap().into();
    let initial=Timeline::new(&source);
    let mut direct=Forecast::new(&source,initial);let mut cached=Forecast::new(&source,initial);let mut ordered=Forecast::new(&source,initial);
    let mut actual_positions=[Point::default();18];
    for step in 1..=17 {
        if step>1 {w.step(&mut ScriptedController::new(|_,s:SnakeView<'_>|Steering {desired_angle:s.angle,rush:0.0}));}
        let p=positions(&w);direct.advance(&source,step,|id|p[id],0.0);
        cached.advance_cached(&source,step,|id|p[id],u16::MAX,&Items::default(),0.0);
        let mut endpoints=p;ordered.advance_ordered(&source,step,&mut endpoints,|_,id|p[id],None,0.0);
        for t in [direct.effects,cached.effects,ordered.effects] {
            assert_eq!(t.food_available(&source,food,step),step==1,"absorbed original step={step}");
            let available=w.food.iter().find(|f|f.id==9999992).unwrap().captured_by==0;
            assert_eq!(t.food_available(&source,survivor,step),available,"transported survivor step={step}");
            assert_eq!(available,step==16);
        }
        actual_positions[step]=w.food.iter().find(|f|f.id==9999992).unwrap().p;
        if step>=2 {assert!(!w.food.iter().any(|f|f.id==9999991));}
        if step==16 {assert_eq!(w.vortex().unwrap().age_ticks,0);assert_eq!(w.whirlpool_stats.opened,2);}
    }
    // The no-items shared rollout starts with this fully projected timeline.
    // Its food transport must still follow the old brew during the prefix.
    let mut absorbed=FoodMotion::new(&source,food);
    let mut surviving=FoodMotion::new(&source,survivor);
    for step in 1..=17 {
        let brew=direct.effects.feeding_brew(step);
        absorbed.advance_brew(&source,brew,food.kind,step);
        surviving.advance_brew(&source,brew,survivor.kind,step);
        assert_eq!(!absorbed.unavailable(),step==1,"shared absorption step={step}");
        assert_eq!(!surviving.unavailable(),step==16,"shared survivor step={step}");
        assert!(source.distance_squared(surviving.position,actual_positions[step])<1e-16,"shared position step={step}");
    }
}

#[test]
fn whirlpool_timing_long_arrival_value_recovers_on_new_burst_deadline() {
    let (mut w,source,_,end)=fixture(Start::Field(1),395.5);
    let mut ai=AiController::new();ai.prepare(&source);
    let food:target::TargetFood=source.foods().find(|f|f.id==9999991).unwrap().into();
    for step in 2..=end+1 {
        w.step(&mut ScriptedController::new(|_,s:SnakeView<'_>|Steering {desired_angle:s.angle,rush:0.0}));
        if step>=end-1 {
            let available=w.food.iter().find(|f|f.id==9999991).unwrap().captured_by==0;
            let value=ai.target_score(&source,source.snake(0).unwrap(),&State::default(),food,step as f64*STEP_SECONDS,0.0,food.value);
            assert_eq!(value>0.0,available,"long arrival step={step} value={value}");
        }
    }
}

#[test]
fn whirlpool_timing_first_shard_meal_and_release_precede_feeding() {
    let mut w=arena();let center=w.segments[0].current;
    w.open_whirlpool(0,center);w.items[0].age_ticks=148;w.items[0].life_ticks=2;
    particle(&mut w,center);w.food[0].captured_by=1;
    let mut observe=Observe {source:None};w.step(&mut observe);let source=observe.source.unwrap();
    let food=source.foods().find(|f|f.id==9999991).unwrap();
    assert!(!target::Contact::new(&source,source.snake(0).unwrap(),food).reached(0.0,1));
    assert!(!w.consumption_events().any(|e|e.0==9999991));
    assert_eq!(w.whirlpool_stats.holder_eaten,0);
    w.step(&mut ScriptedController::new(|_,s:SnakeView<'_>|Steering {desired_angle:s.angle,rush:0.0}));
    assert!(w.vortex().is_none());assert!(w.consumption_events().any(|e|e.0==9999991));
    assert!(w.whirlpool_stats.holder_eaten>0,"new shards must be edible in their creation tick");
    w.step(&mut ScriptedController::new(|_,s:SnakeView<'_>|Steering {desired_angle:s.angle,rush:0.0}));
    assert!(!w.food.iter().any(|f|f.id==9999991));
}

#[test]
fn whirlpool_timing_new_capture_stops_exactly_at_pull_cutoff() {
    for start in [Start::Existing(134),Start::Field(1),Start::Field(3)] {
        let cutoff=match start {Start::Existing(age)=>(PULL_END-age) as usize,Start::Field(n)=>n+PULL_END as usize,_=>unreachable!()};
        for crossing in [cutoff-1,cutoff,cutoff+1] {
            let mut w=arena();w.config.store_power_ups=false;
            let center=Point{x:5000.0,y:2000.0};
            match start {
                Start::Existing(age)=>{w.open_whirlpool(0,center);w.items[0].age_ticks=age;w.items[0].life_ticks=LIFE-age;},
                Start::Field(n)=>w.items.push(Item {id:42,kind:EffectKind::Whirlpool,position:center,
                    radius:5000.0,life_ticks:750,pickable_from_tick:n as u64,..Default::default()}),
                _=>unreachable!(),
            }
            let damping=0.16_f64.powf(STEP_SECONDS);
            let travel=800.0*STEP_SECONDS*(1.0-damping.powi(crossing as i32))/(1.0-damping);
            let position=Point{x:center.x+22.0*w.config.base_radius()+travel-1e-6,y:center.y};
            particle(&mut w,position);
            w.food[0].velocity=Point{x:-800.0,y:0.0};
            let mut observe=Observe {source:None};w.step(&mut observe);let source=observe.source.unwrap();
            let food:target::TargetFood=source.foods().find(|f|f.id==9999991).unwrap().into();
            let t=Timeline::new(&source);let mut f=Forecast::new(&source,t);let mut g=Forecast::new(&source,t);
            for step in 1..=cutoff+1 {
                if step>1 {w.step(&mut ScriptedController::new(|_,s:SnakeView<'_>|Steering {desired_angle:s.angle,rush:0.0}));}
                let p=positions(&w);f.advance(&source,step,|id|p[id],0.0);
                g.advance_cached(&source,step,|id|p[id],u16::MAX,&Items::default(),0.0);
                let captured=w.food.iter().find(|f|f.id==9999991).unwrap().captured_by!=0;
                assert_eq!(captured,crossing<cutoff && step>=crossing,"{start:?} crossing={crossing} step={step}");
                assert_eq!(f.effects.food_available(&source,food,step),!captured);
                assert_eq!(g.effects.food_available(&source,food,step),!captured);
            }
        }
    }
}

#[test]
fn whirlpool_timing_same_tick_flip_picks_capsules_at_the_new_head() {
    for brewing in [false,true] {
        let mut w=arena();
        if brewing {w.open_whirlpool(0,Point{x:5000.0,y:2000.0});}
        w.snakes[0].inventory=Inventory {count:1,kinds:[6,0,0],life:[1800,0,0],
            windup:1,windup_ticks:2,..Default::default()};
        let tail=w.segments[w.snakes[0].len-1].current;
        w.items.push(Item {id:42,kind:EffectKind::Magnet,position:tail,
            radius:20.0,life_ticks:750,..Default::default()});
        let mut observe=Observe {source:None};w.step(&mut observe);let source=observe.source.unwrap();
        let mut ordinary=AiController::new();ordinary.prepare(&source);
        let index=ordinary.food.iter().position(|f|f.is_some_and(|f|f.id==target::ITEM_BIT|42)).unwrap();
        let food=ordinary.food[index].unwrap();
        let state=State {target:food.id,target_index:index,goal:food.position,desired:0.0,
            track_goal:true,turn_until:u64::MAX,..Default::default()};
        let mut marked=AiController::new();marked.prepare(&source);
        // A utility marker isolates reached-contact valuation without changing
        // the actual capsule, its forecast effects or either rollout geometry.
        marked.food[index].as_mut().unwrap().feast_id=ESSENCE_BIT|42;
        for step in 1..=3 {
            let mut a=Candidate::default();let mut b=Candidate::default();
            ordinary.rollout_into(&source,source.snake(0).unwrap(),state,1,step,&mut a);
            marked.rollout_into(&source,source.snake(0).unwrap(),state,1,step,&mut b);
            assert_eq!(a.steps,step);assert_eq!(b.steps,step);
            assert!((b.score-a.score-if step>=2 {600.0} else {0.0}).abs()<1e-9,
                "brewing={brewing} step={step} ordinary={} marked={}",a.score,b.score);
            if step>1 {w.step(&mut ScriptedController::new(|_,s:SnakeView<'_>|Steering {desired_angle:s.angle,rush:0.0}));}
            assert_eq!(w.items.iter().any(|i|i.id==42),step<2);
            if step==2 {assert!(w.frame_events().any(|e|e.kind==crate::EventKind::Flip));}
        }
    }
}
