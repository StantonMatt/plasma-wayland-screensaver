// SPDX-License-Identifier: GPL-3.0-or-later
// Include the private core to install deterministic pickups without exporting
// fixture mutation through the production API. Counting is thread-local.
include!("../src/lib.rs");
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static COUNT: Cell<usize> = const { Cell::new(0) };
}
struct Counting;
#[global_allocator]
static ALLOCATOR: Counting = Counting;
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ENABLED.with(|e| if e.get() { COUNT.with(|c| c.set(c.get() + 1)); });
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ENABLED.with(|e| if e.get() { COUNT.with(|c| c.set(c.get() + 1)); });
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ENABLED.with(|e| if e.get() { COUNT.with(|c| c.set(c.get() + 1)); });
        unsafe { System.realloc(p, layout, size) }
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) { unsafe { System.dealloc(p, layout) } }
}

#[test]
fn surge_first_decision_pickup_expiry_and_cutoffs_allocate_nothing() {
    let cfg = Config { width: 3440.0, height: 1440.0, density: 100.0, trails: 100.0,
        intelligence: 100.0, seed: 73, self_collisions: true, deadly_walls: false,
        rules: RuleSet::V2, ..Config::default() };
    let mut w = World::new(cfg).unwrap();
    let mut copy = w.diagnostic_snapshot();
    let mut ai = ai::AiController::new();
    let mut other = ai::AiController::new();
    let mut boosts = 0;
    let mut hunts = 0;
    COUNT.with(|c| c.set(0));
    ENABLED.with(|e| e.set(true));
    for tick in 0..1800 {
        if tick % 240 == 0 {
            for id in 0..w.snake_count() {
                if w.snakes[id].alive {
                    // The world's item vector is reserved to the physical cap.
                    w.snakes[id].effect_kind = 1;
                    w.snakes[id].effect_ticks = 180;
                    effects::activate(effects::EffectKind::Surge, &mut w, id);
                    copy.snakes[id].effect_kind = 1;
                    copy.snakes[id].effect_ticks = 180;
                    effects::activate(effects::EffectKind::Surge, &mut copy, id);
                }
            }
        }
        w.step(&mut ai);
        copy.step(&mut other);
        for s in w.snakes().filter(|s| s.alive && s.effect_kind == 1) {
            boosts += usize::from(s.flags & flags::BOOSTING != 0);
            hunts += usize::from(s.flags & flags::HUNTING != 0);
        }
    }
    ENABLED.with(|e| e.set(false));
    assert_eq!(COUNT.with(Cell::get), 0);
    assert!(boosts > 0 && hunts > 0, "boosts={boosts} hunts={hunts}");
    assert_eq!(w.rng_state(), copy.rng_state());
    assert_eq!(w.stats(), copy.stats());
    for (a, b) in w.snakes().zip(copy.snakes()) {
        assert_eq!((a.alive, a.angle, a.effect_kind, a.effect_ticks),
            (b.alive, b.angle, b.effect_kind, b.effect_ticks));
        assert_eq!(a.segments, b.segments);
    }
}

#[test]
fn widened_surge_collision_buckets_allocate_nothing() {
    let mut w = World::diagnostic_arena(Config { width: 1280.0, height: 720.0,
        scale: 25.0, speed: 250.0, rules: RuleSet::V2, deadly_walls: true,
        self_collisions: true, ..Config::default() },
        &[(Point { x: 700.0, y: 600.0 }, 0.0, 24, 0.0),
          (Point { x: 1000.0, y: 600.0 }, 0.0, 24, 0.0)], &[]).unwrap();
    for s in &mut w.snakes[..2] {
        s.base_radius = 5.13; s.radius = 5.13;
        s.rush = 0.6; s.effect_kind = 1; s.effect_ticks = 100;
    }
    w.segments[0] = Segment { previous: Point { x: 357.8, y: 305.4 },
        current: Point { x: 373.364, y: 300.4 } };
    w.segments[MAX_SEGMENTS + 23] = Segment { previous: Point { x: 361.8, y: 300.0 },
        current: Point { x: 345.453, y: 300.0 } };
    COUNT.with(|c| c.set(0));
    ENABLED.with(|e| e.set(true));
    for _ in 0..2000 { w.diagnostic_mark_collisions(); }
    ENABLED.with(|e| e.set(false));
    assert_eq!(COUNT.with(Cell::get), 0);
    assert_eq!(w.snakes[0].dying, DeathReason::Body);
}
