// SPDX-License-Identifier: GPL-3.0-or-later
// Compile the private mechanics modules here to access the cap fixture without
// exposing mutation hooks in the release library. Other mechanics tests run in
// this binary too; counting is thread-local and covers only the measured ticks.
#[path = "../src/math.rs"] mod math;
#[path = "../src/rng.rs"] mod rng;
#[path = "../src/world.rs"] mod world;
#[path = "../src/controller.rs"] pub mod controller;
pub use math::{ Point, normalize_angle, wrap_coordinate };
pub use rng::WorldRng;
pub use world::*;
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
