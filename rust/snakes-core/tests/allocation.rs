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
                age_ticks:0,life_ticks:750,radius:w.config.base_radius()*2.1});
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
    let mut observed=[false;4];let mut simultaneous=false;
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
    assert!(observed[1..].iter().all(|&seen|seen));
    assert!(simultaneous,"all three effects must coexist during measured ticks");
    assert!(w.snakes().all(|s|s.effect_ticks==0));
}
