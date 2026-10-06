// SPDX-License-Identifier: GPL-3.0-or-later
// Compile the private mechanics modules here to access the cap fixture without
// exposing mutation hooks in the release library. Other mechanics tests run in
// this binary too; counting is thread-local and covers only the measured ticks.
include!("../src/lib.rs");
use std::{ alloc::{ GlobalAlloc, Layout, System }, cell::Cell };
thread_local! {
    static ENABLED: Cell<bool>=const {
        Cell::new(false)
    };
    static COUNT: Cell<usize>=const {
        Cell::new(0)
    };
}
struct CountingAllocator;
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ENABLED.with(|e|if e.get() {
            COUNT.with(|c|c.set(c.get()+1));
        });
        unsafe {
            System.alloc(layout)
        }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ENABLED.with(|e|if e.get() {
            COUNT.with(|c|c.set(c.get()+1));
        });
        unsafe {
            System.alloc_zeroed(layout)
        }
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ENABLED.with(|e|if e.get() {
            COUNT.with(|c|c.set(c.get()+1));
        });
        unsafe {
            System.realloc(p, layout, size)
        }
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe {
            System.dealloc(p, layout)
        }
    }
}
#[global_allocator] static ALLOCATOR: CountingAllocator = CountingAllocator;
#[test] fn zero_allocations_at_caps_including_death_and_respawn() {
    let mut w = world::tests::allocation_fixture();
    let mut straight = controller::ScriptedController::new(|_, s: SnakeView<'_>|controller::Steering {
        desired_angle: s.angle,
        rush: 0.0
    });
    // Warm at maximum food/segment budgets, with feeding, grid and ring pruning.
    assert_eq!(w.stats().total_segments, 6000);
    w.step_n(&mut straight, 30);
    assert_eq!(w.stats().total_segments, 6000);
    assert_eq!(w.snake(0).unwrap().segments.len(), 1600);
    world::tests::refill_food_to_cap(&mut w);
    assert_eq!(w.stats().food, 450);
    COUNT.with(|c|c.set(0));
    ENABLED.with(|e|e.set(true));
    w.step_n(&mut straight, 1000);
    ENABLED.with(|e|e.set(false));
    assert_eq!(COUNT.with(Cell::get), 0);
    // Baseline self collisions exercise explosions and respawns, which must
    // also reuse all storage even after leaving the mature fixture.
    w.reconfigure(Config { self_collisions: true, ..w.config() }).unwrap();
    let old = w.stats().deaths;
    let mut baseline = controller::BaselineController;
    COUNT.with(|c|c.set(0));
    ENABLED.with(|e|e.set(true));
    w.step_n(&mut baseline, 2000);
    ENABLED.with(|e|e.set(false));
    assert_eq!(COUNT.with(Cell::get), 0);
    assert!(w.stats().deaths>old);
}

#[test]
fn zero_allocations_with_ai_at_caps_and_after_reconfiguration() {
    let mut w=world::tests::allocation_fixture();
    w.reconfigure(Config{intelligence:100.0,..w.config()}).unwrap();
    let mut ai=ai::AiController::new();
    let mut times=Vec::with_capacity(1000);
    assert_eq!(w.stats().total_segments,6000);
    assert_eq!(w.snake(0).unwrap().segments.len(),1600);
    assert_eq!(w.stats().food,450);
    COUNT.with(|c|c.set(0));ENABLED.with(|e|e.set(true));
    for _ in 0..1000 {
        let start=std::time::Instant::now();w.step(&mut ai);
        times.push(start.elapsed().as_secs_f64()*1000.0);
    }
    ENABLED.with(|e|e.set(false));assert_eq!(COUNT.with(Cell::get),0);
    let avg=times.iter().sum::<f64>()/times.len() as f64;
    times.sort_unstable_by(f64::total_cmp);
    println!("AI 6000-segment/450-food cap fixture: avg {avg:.4} ms p95 {:.4} ms p99 {:.4} ms max {:.4} ms final_segments {}",times[950],times[990],times[999],w.stats().total_segments);
    w.reconfigure(Config{self_collisions:true,deadly_walls:true,width:3440.0,height:1440.0,..w.config()}).unwrap();
    COUNT.with(|c|c.set(0));ENABLED.with(|e|e.set(true));
    w.step_n(&mut ai,2000);
    ENABLED.with(|e|e.set(false));assert_eq!(COUNT.with(Cell::get),0);
}

#[test]
fn zero_allocations_v2_boost_pellets_events_corpses_and_ffi_export() {
    use ffi::*;
    let mut w=world::tests::allocation_fixture();
    w.config.rules=RuleSet::V2;
    let mut boost=controller::ScriptedController::new(|_,s:SnakeView<'_>|controller::Steering{desired_angle:s.angle,rush:1.0});
    let mut snakes=[SnakeRecord::default();MAX_SNAKES];
    let mut segments=vec![SegmentRecord::default();MAX_SNAKES*MAX_SEGMENTS];
    let mut items=[ItemRecord::default();MAX_ITEMS];
    let mut food=[FoodRecord::default();MAX_FOOD];let mut events=[EventRecord::default();MAX_EVENTS];
    // Use the public FFI path separately with preallocated high-water buffers.
    let cfg=CoreConfig::from(Config{rules:RuleSet::V2,self_collisions:true,seed:73,..Config::default()});
    let mut handle=std::ptr::null_mut();
    unsafe {assert_eq!(snakes_core_create(&cfg,&mut handle),OK);}
    let inputs:[SteeringInput;9]=std::array::from_fn(|id|SteeringInput{id:id as u32,desired_angle:0.0,rush:1.0,..SteeringInput::default()});
    unsafe {assert_eq!(snakes_core_set_steering(handle,inputs.as_ptr(),inputs.len()),OK);}
    let mut info=FrameInfo::default();let mut seen_pellet=false;let mut seen_event=false;let mut seen_corpse=false;
    COUNT.with(|c|c.set(0));ENABLED.with(|e|e.set(true));
    for _ in 0..1000 {
        w.step(&mut boost);
        unsafe {
            assert_eq!(snakes_core_step(handle,1),OK);
            assert_eq!(snakes_core_export_frame(handle,snakes.as_mut_ptr(),snakes.len(),segments.as_mut_ptr(),segments.len(),food.as_mut_ptr(),food.len(),&mut info),OK);
            let mut sizes=FrameSizes::default();assert_eq!(snakes_core_get_frame_sizes(handle,&mut sizes),OK);
            assert_eq!(snakes_core_export_extras(handle,items.as_mut_ptr(),items.len(),events.as_mut_ptr(),events.len()),OK);
            seen_pellet|=food[..sizes.food as usize].iter().any(|f|f.kind==FoodKind::Pellet as u8);
            seen_event|=sizes.events>0;
            seen_corpse|=snakes[..sizes.snakes as usize].iter().any(|s|s.flags & flags::CORPSE != 0);
        }
    }
    ENABLED.with(|e|e.set(false));assert_eq!(COUNT.with(Cell::get),0);
    assert!(seen_pellet && seen_event && seen_corpse);
    unsafe {snakes_core_destroy(handle);}
}

#[test]
fn zero_allocations_items_pickups_replacement_expiry_and_settings_off() {
    let mut w=World::new(Config {rules:RuleSet::V2,deadly_walls:false,..Config::default()}).unwrap();
    let mut ai=ai::AiController::new();
    // Include the very first spawn, pickup, replacement and expiry. The
    // fixture injection itself reuses the same three-record reserved storage.
    COUNT.with(|c|c.set(0));ENABLED.with(|e|e.set(true));
    for tick in 0..1200 {
        if tick==0 || tick==30 || tick==60 {
            w.items.clear();
            let s=w.snake(0).unwrap();
            w.items.push(Item {id:tick as u64+1,kind:effects::ENABLED_KINDS[tick/30],position:s.segments[0].current,
                age_ticks:0,life_ticks:750,radius:w.config.base_radius()*2.1,..Item::default()});
        }
        w.step(&mut ai);
    }
    w.reconfigure(Config {power_ups:false,..w.config()}).unwrap();
    ENABLED.with(|e|e.set(false));assert_eq!(COUNT.with(Cell::get),0);
    assert_eq!(w.items().len(),0);assert!(w.snakes().all(|s|s.effect_ticks==0 && s.effect_kind==0));
}

#[test]
fn zero_allocations_with_all_r3_effects_simultaneously_active() {
    let mut w=World::new(Config {rules:RuleSet::V2,width:3440.0,height:1440.0,
        density:100.0,trails:100.0,intelligence:100.0,self_collisions:true,
        deadly_walls:false,seed:73,..Config::default()}).unwrap();
    let mut ai=ai::AiController::new();
    let mut observed=[false;5];let mut simultaneous=false;
    COUNT.with(|c|c.set(0));ENABLED.with(|e|e.set(true));
    for tick in 0..2400 {
        if tick%360==0 {
            // Real pickups on separate live snakes exercise simultaneous
            // effects, replacement, activation and the first AI decision.
            w.items.clear();
            for id in 0..3 {
                if w.snakes[id].alive {
                    let kind=effects::ENABLED_KINDS[(id+tick/360)%3];
                    w.items.push(Item {id:(tick+id+1) as u64,kind,
                        position:w.segments[id*MAX_SEGMENTS].current,
                        life_ticks:750,radius:w.config.base_radius()*2.1,..Item::default()});
                }
            }
        }
        let mut active=0u8;
        for s in w.snakes().filter(|s|s.alive && s.effect_ticks>0) {
            active|=1<<s.effect_kind;
            observed[s.effect_kind as usize]=true;
        }
        simultaneous|=active&14==14;
        w.step(&mut ai);
    }
    w.reconfigure(Config {power_ups:false,..w.config()}).unwrap();
    w.step_n(&mut ai,60);
    ENABLED.with(|e|e.set(false));
    assert_eq!(COUNT.with(Cell::get),0);
    assert!(observed[1..4].iter().all(|&seen|seen));
    assert!(simultaneous,"all three effects must coexist during measured ticks");
    assert!(w.snakes().all(|s|s.effect_ticks==0));
}

#[test]
fn zero_allocations_prism_schedule_feast_bulges_and_ai() {
    let mut w=World::new(Config {rules:RuleSet::V2,deadly_walls:false,..Config::default()}).unwrap();
    let mut ai=ai::AiController::new();
    let (mut saw_seed,mut saw_feast,mut saw_two_gulps)=(false,false,false);
    COUNT.with(|c|c.set(0));ENABLED.with(|e|e.set(true));
    for tick in 0..2200 {
        // Repeated early feasts overlap both gulp slots; leave the remaining
        // run untouched so the real scheduler and 90-tick ripening execute.
        if tick<120 && tick%15==0 {
            let p=w.snakes().find(|s|s.alive).unwrap().segments[0].current;
            w.food.retain(|f|f.kind!=FoodKind::Prism && f.kind!=FoodKind::PrismSeed);
            w.food.push(world::Food {id:10000+tick,p,kind:FoodKind::Prism,value:5.0,life:30.0,owner:-1,..Default::default()});
        }
        w.step(&mut ai);
        saw_seed|=w.foods().any(|f|f.kind==FoodKind::PrismSeed);
        saw_feast|=w.frame_events().any(|e|e.kind==EventKind::Feast);
        saw_two_gulps|=w.snakes().any(|s|s.face.bulges.iter().all(|b|b.strength>0.0 && w.tick()<b.start_tick+b.duration_ticks as u64));
    }
    ENABLED.with(|e|e.set(false));assert_eq!(COUNT.with(Cell::get),0);
    assert!(saw_seed && saw_feast && saw_two_gulps,"scheduler/Feast/two-slot paths must execute");
}

#[test]
fn zero_allocations_sever_delay_and_orphan_release() {
    let mut w=world::venom::tests::fixture(1600,1248);
    let before=w.detached_points.as_ptr();
    COUNT.with(|c|c.set(0));ENABLED.with(|e|e.set(true));
    world::venom::tests::exercise(&mut w);
    ENABLED.with(|e|e.set(false));
    assert_eq!(COUNT.with(Cell::get),0);assert_eq!(before,w.detached_points.as_ptr());
    assert_eq!(w.snakes[1].len,1248);assert!(w.food.iter().any(|f|f.kind==FoodKind::Shard));
}

#[test]
fn zero_allocations_venom_hunting_strike_and_escape() {
    let mut w=world::venom::tests::hunt_fixture();let mut ai=ai::AiController::new();
    let mut bites=0;let mut boosts=0;let mut exit_until=0;let mut survived_exit=false;
    COUNT.with(|c|c.set(0));ENABLED.with(|e|e.set(true));
    for _ in 0..240 {
        w.step(&mut ai);
        let bite=w.frame_events().filter(|e|e.kind==EventKind::Sever && e.other_snake_id==0).count();
        boosts+=usize::from(w.snakes[0].boost_ticks>0);
        bites+=bite;if bite>0 {exit_until=w.tick()+12;}
        if exit_until>0 && w.tick()==exit_until {survived_exit=w.snakes[0].alive;}
    }
    ENABLED.with(|e|e.set(false));assert_eq!(COUNT.with(Cell::get),0);
    assert_eq!(bites,1);assert!(boosts>0);assert!(survived_exit);
}

#[test]
fn zero_allocations_frost_nova_freeze_thaw_and_ai() {
    let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,width:1600.0,height:1000.0,
        density:0.0,scale:200.0,self_collisions:false,deadly_walls:false,..Default::default()},
        &[(Point{x:500.0,y:400.0},0.0,24,1.0),(Point{x:610.0,y:500.0},0.0,48,0.0)],&[]).unwrap();
    let mut ai=ai::AiController::new();
    let mut saw_frozen=false;let mut saw_thaw=false;let mut saw_nova=false;
    COUNT.with(|c|c.set(0));ENABLED.with(|e|e.set(true));
    effects::activate(effects::EffectKind::Frost,&mut w,0);
    for _ in 0..160 {
        saw_frozen|=w.snakes().any(|s|s.face.frozen_ticks>0);
        saw_thaw|=w.snakes().any(|s|s.face.thaw_immunity_ticks>0);
        saw_nova|=w.frame_events().any(|e|e.kind==EventKind::Nova);
        w.step(&mut ai);
    }
    ENABLED.with(|e|e.set(false));assert_eq!(COUNT.with(Cell::get),0);
    assert!(saw_frozen && saw_thaw && saw_nova);
}

#[test]
fn zero_allocations_frost_power_up_toggles_from_disabled_world() {
    let mut w=World::new(Config {rules:RuleSet::V2,width:1600.0,height:1000.0,
        density:0.0,power_ups:false,..Default::default()}).unwrap();
    let mut ai=ai::AiController::new();
    COUNT.with(|c|c.set(0));ENABLED.with(|e|e.set(true));
    for enabled in [true,false,true,false] {
        w.reconfigure(Config {power_ups:enabled,..w.config()}).unwrap();
        if enabled {effects::activate(effects::EffectKind::Frost,&mut w,0);}
        w.step(&mut ai);
    }
    ENABLED.with(|e|e.set(false));assert_eq!(COUNT.with(Cell::get),0);
    assert!(w.snakes().all(|s|s.face.frozen_ticks==0 && s.face.thaw_immunity_ticks==0));
}

#[test]
fn zero_allocations_frost_growth_past_the_legacy_trail_cap() {
    let mut w=world::tests::allocation_fixture();
    w.config.rules=RuleSet::V2;
    w.reconfigure(Config {power_ups:true,..w.config()}).unwrap();
    assert_eq!(w.stats().total_segments,6000);
    // Leave the maximum-length titan alone: V2's continuous-body contacts
    // legitimately kill the crossing Classic cap fixture's other occupants.
    for s in w.snakes.iter_mut().skip(1) {s.alive=false;s.len=0;s.respawn=1000.0;}
    let mut straight=controller::ScriptedController::new(|_,s:SnakeView<'_>|controller::Steering {desired_angle:s.angle,rush:0.0});
    COUNT.with(|c|c.set(0));ENABLED.with(|e|e.set(true));
    for _ in 0..8 {
        for id in 0..w.snakes.len() {w.snakes[id].frozen_ticks=75;w.faces[id].frozen_ticks=75;}
        w.step_n(&mut straight,120);
    }
    ENABLED.with(|e|e.set(false));assert_eq!(COUNT.with(Cell::get),0);
    assert!(w.snake(0).unwrap().segments.len()>1600,"V2 must retain uncapped growth while frozen");
}

#[test]
fn zero_allocations_frost_nova_on_a_6000_segment_giant() {
    let mut w=World::diagnostic_arena(Config {rules:RuleSet::V2,width:16384.0,height:16384.0,
        density:0.0,scale:200.0,speed:0.0,self_collisions:false,deadly_walls:false,..Default::default()},
        &[(Point{x:8000.0,y:8000.0},0.0,6000,0.0),
          (Point{x:8000.0,y:8100.0},0.0,24,0.0)],&[]).unwrap();
    let mut straight=controller::ScriptedController::new(|_,s:SnakeView<'_>|controller::Steering {desired_angle:s.angle,rush:0.0});
    COUNT.with(|c|c.set(0));ENABLED.with(|e|e.set(true));
    effects::activate(effects::EffectKind::Frost,&mut w,1);
    assert_eq!(w.snakes[0].frozen_ticks,75);
    w.step_n(&mut straight,120);
    ENABLED.with(|e|e.set(false));
    assert_eq!(COUNT.with(Cell::get),0);
    assert!(w.snakes[0].alive);
    assert_eq!(w.snakes[0].len,6000);
    assert_eq!(w.snakes[0].frozen_ticks,0);
}

#[test]
fn zero_allocations_starfall_meteors_landing_night_fades_and_ai() {
    let mut w=World::new(Config {width:3440.0,height:1440.0,rules:RuleSet::V2,deadly_walls:false,
        density:80.0,trails:100.0,scale:200.0,speed:300.0,intelligence:100.0,self_collisions:true,..Default::default()}).unwrap();
    let mut ai=ai::AiController::new();
    w.diagnostic_event_schedule(1,1);
    let (mut meteors,mut stars,mut night)=(false,false,false);
    COUNT.with(|c|c.set(0));ENABLED.with(|e|e.set(true));
    for _ in 0..1000 {
        w.step(&mut ai);
        meteors|=w.foods().any(|f|f.kind==FoodKind::Meteor);
        stars|=w.foods().any(|f|f.kind==FoodKind::Star);
        night|=w.world_event.night==1.0;
    }
    ENABLED.with(|e|e.set(false));assert_eq!(COUNT.with(Cell::get),0);
    assert!(meteors && stars && night,"all real scheduler paths must execute");
    assert_eq!(w.world_event.night,0.0);assert_eq!(w.world_event_stats().nightfalls,1);
}
