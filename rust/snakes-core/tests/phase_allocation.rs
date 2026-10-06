// SPDX-License-Identifier: GPL-3.0-or-later
use snakes_core::{ai::AiController, controller::{Controller, Steering}, Config, Point, RuleSet, SnakeView, World, flags};
use std::{alloc::{GlobalAlloc, Layout, System}, cell::Cell};
thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static COUNT: Cell<usize> = const { Cell::new(0) };
}
struct Allocator;
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ENABLED.with(|on| if on.get() { COUNT.with(|n| n.set(n.get()+1)); });
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ENABLED.with(|on| if on.get() { COUNT.with(|n| n.set(n.get()+1)); });
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ENABLED.with(|on| if on.get() { COUNT.with(|n| n.set(n.get()+1)); });
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) { unsafe { System.dealloc(ptr, layout) } }
}
#[global_allocator] static ALLOCATOR: Allocator = Allocator;
struct SeekItem;
impl Controller for SeekItem {
    fn steer(&mut self, w: &World, s: SnakeView<'_>) -> Steering {
        let angle = w.items().next().map(|item| {
            let d = w.displacement(s.segments[0].current, item.position);
            d.y.atan2(d.x)
        }).unwrap_or(s.angle);
        Steering { desired_angle: angle, rush: 0.0 }
    }
}
#[test]
fn phase_first_ai_decision_through_expiry_allocates_nothing() {
    // Acquire a real spawned item through public mechanics; no diagnostic
    // mutation or global spawning policy is needed in this integration test.
    let mut w = World::diagnostic_arena(Config { store_power_ups: false, rules: RuleSet::V2,
        deadly_walls: false, self_collisions: false, density: 0.0,
        ..Config::default() }, &[(Point { x: 450.0, y: 450.0 }, 0.0, 24, 0.0)], &[]).unwrap();
    let mut seek = SeekItem;
    for _ in 0..20_000 {
        w.step(&mut seek);
        if w.snake(0).unwrap().flags & flags::PHASED != 0 { break; }
    }
    assert_eq!(w.snake(0).unwrap().effect_ticks, 120);
    assert_ne!(w.snake(0).unwrap().flags & flags::PHASED, 0);
    let mut ai = AiController::new();
    COUNT.with(|n| n.set(0));
    ENABLED.with(|on| on.set(true));
    // Includes first preparation, all expiry-aware rollouts, and restoration of
    // the normal planner. This also covers simultaneous effect/boost counters.
    for _ in 0..180 { w.step(&mut ai); }
    ENABLED.with(|on| on.set(false));
    assert_eq!(COUNT.with(Cell::get), 0, "Phase must allocate/reallocate zero times");
    assert_eq!(w.snake(0).unwrap().flags & flags::PHASED, 0);
}
