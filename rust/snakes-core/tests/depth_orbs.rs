use snakes_core::visuals::depth::orbs::{DESCRIPTOR, PARAMETERS};
#[test]
fn registered_contract() {
    assert_eq!(DESCRIPTOR.rate, 30);
    assert_eq!(
        DESCRIPTOR.mirrored,
        snakes_core::visuals::depth::MirroredPolicy::SharedCanonical
    );
    assert_eq!(DESCRIPTOR.fit as u32, 0);
    assert_eq!(
        PARAMETERS
            .iter()
            .map(|s| s.default_value as u32)
            .collect::<Vec<_>>(),
        [0, 0, 50, 60, 1]
    );
}
#[cfg(feature = "visual-capture")]
mod capture {
    use snakes_core::visuals::depth::{
        common::layout::Rect,
        ffi::*,
        orbs::{OrbRecord, visuals_orbs_records},
    };
    use std::ptr;
    struct Owned(*mut World);
    impl Drop for Owned {
        fn drop(&mut self) {
            unsafe { visuals_depth_destroy(self.0) }
        }
    }
    fn world() -> Owned {
        let config = Config {
            visual: 8,
            palette: 0,
            seed: 311,
            speed: 1.,
            density: 80.,
            scale: 1.2,
            trails: 0.8,
            reduced_motion: 0,
            monitor_mode: 2,
        };
        let output = |id, x, y, width, height, refresh_hz| Output {
            id,
            viewport: Rect {
                x,
                y,
                width,
                height,
            },
            dpr: 1.,
            refresh_hz,
            primary: 0,
            padding: 0,
        };
        let outputs = [
            output(1, 0., 0., 2560., 1440., 100.),
            output(2, 2560., 0., 3440., 1440., 175.),
            output(3, 6000., 195., 1920., 1080., 240.),
        ];
        let env = Env {
            background: 1,
            frame_cap: 60,
            top: [0.; 4],
            bottom: [0.; 4],
        };
        let mut w = ptr::null_mut();
        let status = unsafe {
            visuals_depth_create(
                &config,
                ptr::null(),
                0,
                outputs.as_ptr(),
                3,
                &env,
                ptr::null(),
                0,
                1,
                100_000_000,
                &mut w,
            )
        };
        assert_eq!(status, 0);
        Owned(w)
    }
    fn read(w: &Owned, ns: i64, id: u64, view: Rect) -> (u64, Vec<OrbRecord>) {
        let v = View {
            output_id: id,
            layout_generation: 1,
            timeline_ns: ns,
            viewport: view,
            uniform_scale: 1.,
            offset_x: 0.,
            offset_y: 0.,
            dpr: 1.,
        };
        let mut lease = ptr::null_mut();
        unsafe {
            assert_eq!(visuals_depth_retain_sample(w.0, &v, &mut lease), 0);
            let n = visuals_orbs_records(lease, ptr::null_mut(), 0);
            assert!(n > 0);
            let mut r = vec![OrbRecord::default(); 2 * n];
            let allocations = super::allocation::measured(|| {
                assert_eq!(visuals_orbs_records(lease, r.as_mut_ptr(), n), n)
            });
            assert_eq!(allocations, 0);
            let sample = &(&*lease).sample;
            let tick = sample.lower.tick;
            assert!(!sample.missed);
            visuals_depth_sample_release(lease);
            (tick, r)
        }
    }
    #[test]
    fn mixed_refresh_reads_same_endpoints_and_seam_geometry() {
        let w = world();
        let solo = world();
        let left = Rect {
            x: 0.,
            y: 0.,
            width: 2560.,
            height: 1440.,
        };
        let middle = Rect {
            x: 2560.,
            width: 3440.,
            ..left
        };
        let mut samples = Vec::new();
        for (id, hz, r) in [
            (1, 100, left),
            (2, 175, middle),
            (
                3,
                240,
                Rect {
                    x: 6000.,
                    y: 195.,
                    width: 1920.,
                    height: 1080.,
                },
            ),
        ] {
            for n in 0..=2 * hz {
                samples.push((n * 1_000_000_000i64 / hz, id, r));
            }
        }
        samples.sort_by_key(|(ns, _, _)| *ns);
        let mut intersections = 0;
        for (ns, id, r) in samples {
            unsafe {
                assert_eq!(visuals_depth_ensure_through(w.0, ns, 5), 0);
            }
            let (tick, rows) = read(&w, ns, id, r);
            assert_eq!(tick, (ns as u64 * 30) / 1_000_000_000);
            if id == 1 {
                let (_, other) = read(&w, ns, 2, middle);
                for pair in rows.chunks_exact(2) {
                    if (f64::from(pair[0].body[0]) - 2560.).abs() < f64::from(pair[0].body[2]) {
                        assert!(other.chunks_exact(2).any(|b| b == pair));
                        intersections += 1;
                    }
                }
            }
        }
        unsafe {
            assert_eq!(visuals_depth_ensure_through(solo.0, 2_000_000_000, 64), 0);
            assert_eq!(
                visuals_depth_debug_hash(solo.0),
                visuals_depth_debug_hash(w.0)
            );
        }
        assert!(intersections > 0);
    }
}

#[cfg(feature = "visual-capture")]
mod allocation {
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        cell::Cell,
    };
    thread_local! {static COUNT:Cell<Option<usize>>=const {Cell::new(None)};}
    struct Counter;
    unsafe impl GlobalAlloc for Counter {
        unsafe fn alloc(&self, l: Layout) -> *mut u8 {
            COUNT.with(|c| {
                if let Some(n) = c.get() {
                    c.set(Some(n + 1));
                }
            });
            unsafe { System.alloc(l) }
        }
        unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
            unsafe { System.dealloc(p, l) }
        }
    }
    #[global_allocator]
    static ALLOCATOR: Counter = Counter;
    pub fn measured(f: impl FnOnce()) -> usize {
        COUNT.with(|c| c.set(Some(0)));
        f();
        COUNT.with(|c| c.replace(None).unwrap())
    }
}
