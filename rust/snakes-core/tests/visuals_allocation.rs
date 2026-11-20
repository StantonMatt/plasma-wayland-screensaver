// SPDX-License-Identifier: GPL-3.0-or-later
//! Public V2 AI path, including its first decision and emergency scratch.
use snakes_core::visuals::{
    primitives::{MAX_RECORDS, Primitive},
    settings::Config,
    *,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
thread_local! {
    static ENABLED:Cell<bool>=const {Cell::new(false)};
    static COUNT:Cell<usize>=const {Cell::new(0)};
}
struct Counting;
#[global_allocator]
static ALLOCATOR: Counting = Counting;
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ENABLED.with(|enabled| {
            if enabled.get() {
                COUNT.with(|count| count.set(count.get() + 1));
            }
        });
        unsafe { System.alloc(layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ENABLED.with(|enabled| {
            if enabled.get() {
                COUNT.with(|count| count.set(count.get() + 1));
            }
        });
        unsafe { System.realloc(pointer, layout, size) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[test]
fn primitive_first_and_steady_frames_allocate_nothing() {
    for visual in [2, 3] {
        let config = Config {
            abi_version: ABI_VERSION,
            struct_size: size_of::<Config>() as u32,
            visual,
            palette: 0,
            density: 100.0,
            scale: 2.0,
            speed: 3.0,
            trails: 1.0,
            seed: 1,
            reduced_motion: 0,
            epoch_ns: 0,
        };
        let core = unsafe { visuals_core_create(&config) };
        assert!(!core.is_null());
        let mut out = vec![Primitive::default(); MAX_RECORDS];
        let mut count = 0;
        let mut request = Request {
            width: 3440.0,
            height: 1440.0,
            advance: 1,
            ..Request::default()
        };
        COUNT.with(|c| c.set(0));
        ENABLED.with(|e| e.set(true));
        for frame in 0..1000 {
            request.presentation_ns = frame * 16_666_667;
            assert_eq!(
                unsafe {
                    visuals_core_primitives(core, &request, out.as_mut_ptr(), out.len(), &mut count)
                },
                0
            );
        }
        ENABLED.with(|e| e.set(false));
        assert_eq!(COUNT.with(Cell::get), 0);
        unsafe { visuals_core_destroy(core) };
    }
}
