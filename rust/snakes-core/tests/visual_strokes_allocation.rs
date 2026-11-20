// SPDX-License-Identifier: GPL-3.0-or-later
use snakes_core::visuals::{Request, settings::Config, strokes::*};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
thread_local! {static ENABLED:Cell<bool>=const {Cell::new(false)};static COUNT:Cell<usize>=const {Cell::new(0)};}
struct Allocator;
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ENABLED.with(|e| {
            if e.get() {
                COUNT.with(|c| c.set(c.get() + 1))
            }
        });
        unsafe { System.alloc(l) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        ENABLED.with(|e| {
            if e.get() {
                COUNT.with(|c| c.set(c.get() + 1))
            }
        });
        unsafe { System.alloc_zeroed(l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        ENABLED.with(|e| {
            if e.get() {
                COUNT.with(|c| c.set(c.get() + 1))
            }
        });
        unsafe { System.realloc(p, l, n) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
#[test]
fn no_allocations_for_animated_max_density_paths() {
    unsafe {
        for visual in [RIBBONS, KALEIDOSCOPE] {
            let config = Config {
                abi_version: snakes_core::visuals::ABI_VERSION,
                struct_size: 64,
                visual,
                palette: 0,
                density: 100.0,
                scale: 2.0,
                speed: 3.0,
                trails: 1.0,
                seed: -1197,
                reduced_motion: 0,
                epoch_ns: 0,
            };
            let mut request = Request {
                width: 3440.0,
                height: 1440.0,
                advance: 1,
                ..Request::default()
            };
            let core = visuals_strokes_create(&config);
            assert!(!core.is_null());
            let count = visuals_strokes_vertex_count(core, &request);
            let mut vertices = vec![Vertex::default(); count];
            COUNT.with(|c| c.set(0));
            ENABLED.with(|e| e.set(true));
            for frame in 0..1000 {
                request.presentation_ns = frame * 16_666_667;
                assert_eq!(
                    visuals_strokes_frame(core, &request, vertices.as_mut_ptr(), count),
                    count as i32
                );
            }
            ENABLED.with(|e| e.set(false));
            assert_eq!(COUNT.with(Cell::get), 0);
            visuals_strokes_destroy(core);
        }
    }
}
