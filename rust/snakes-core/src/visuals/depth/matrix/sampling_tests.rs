use super::*;
use crate::visuals::depth::matrix::tests::input;
use crate::visuals::depth::{common::clock::History, ffi::View};

#[test]
fn mixed_refresh_reads_the_same_ink_across_a_seam() {
    let mut world = MatrixWorld::new(input(7, true));
    let rate = world.rate();
    let mut history = History::new(5);
    history.push(0, world.snapshot());
    let mut tick = 0;
    let mut schedule: Vec<_> = [100, 175, 240]
        .into_iter()
        .enumerate()
        .flat_map(|(screen, hz)| {
            (0..=2 * hz).map(move |frame| (frame as i64 * 1_000_000_000 / hz as i64, screen))
        })
        .collect();
    schedule.sort_unstable();
    let mut seen = 0;
    let mut halo_matches = 0;
    for (ns, screen) in schedule {
        let bracket = rate.bracket(ns);
        while tick < bracket.upper {
            tick += 1;
            world.step(tick);
            history.push(tick, world.snapshot());
        }
        let rect = world.input.layout.outputs[screen].rect;
        let view = View {
            output_id: screen as u64,
            layout_generation: 1,
            timeline_ns: ns,
            viewport: rect,
            uniform_scale: 1.,
            offset_x: 0.,
            offset_y: 0.,
            dpr: 1.,
        };
        let lease = Lease {
            sample: history.sample(bracket).unwrap(),
            view,
            bounds: world.input.layout.bounds,
        };
        let mut local = std::mem::MaybeUninit::uninit();
        assert!(unsafe { visuals_rain_layer(&lease, std::ptr::null(), 1, local.as_mut_ptr()) });
        let local = unsafe { local.assume_init() };
        let u = &local.uniforms;
        assert!((u.section[0] as f64 - (PREWARM as f64 + ns as f64 * 3e-8)).abs() < 0.0001);
        assert_eq!(
            (u.viewport[0], u.viewport[1]),
            (rect.x as f32, rect.y as f32)
        );
        if screen == 1 {
            let heads = |l: &Lease| {
                let mut out = vec![Head::default(); 2000];
                let count =
                    unsafe { visuals_rain_heads(l, std::ptr::null(), out.as_mut_ptr(), out.len()) };
                out.truncate(count);
                out
            };
            let right_heads = if ns % 200000000 == 0 {
                heads(&lease)
            } else {
                vec![]
            };
            let mut other = lease;
            other.view.viewport = world.input.layout.outputs[0].rect;
            if !right_heads.is_empty() {
                let left_heads = heads(&other);
                for head in right_heads
                    .iter()
                    .filter(|h| (h.path[2] - 2560.).abs() < h.ink[0])
                {
                    assert!(left_heads.iter().any(|q| q.path == head.path
                        && q.ink == head.ink
                        && q.light == head.light));
                    halo_matches += 1;
                }
            }
            let mut left = std::mem::MaybeUninit::uninit();
            assert!(unsafe { visuals_rain_layer(&other, std::ptr::null(), 1, left.as_mut_ptr()) });
            let left = unsafe { left.assume_init() };
            assert_eq!(left.col_first + left.col_count - 1, local.col_first);
            assert_eq!(left.cells, local.cells);
            assert_eq!(left.uniforms.section[0], local.uniforms.section[0]);
            seen += 1;
        }
    }
    assert!(halo_matches > 0);
    assert_eq!(seen, 351);
    assert_eq!(tick, 61);
}
#[test]
fn still_refresh_shares_the_new_photo_and_releases_old_ones() {
    let mut i = input(8, true);
    i.reduced_motion = true;
    let world = MatrixWorld::new(i);
    let source = world.stills.as_ref().unwrap();
    let first = source.photo(1);
    let second_view = source.photo(1);
    assert!(Arc::ptr_eq(&first, &second_view));
    let next = source.photo(2);
    assert_ne!(first.heads[0].path, next.heads[0].path);
    let weak = Arc::downgrade(&first);
    drop(first);
    drop(second_view);
    assert!(weak.upgrade().is_none());
}
#[test]
fn pooled_incremental_exports_equal_a_fresh_export() {
    let mut world = MatrixWorld::new(input(2, true));
    let mut retained = std::collections::VecDeque::new();
    for tick in 1..=330 {
        world.step(tick);
        let f = world.snapshot();
        if tick % 11 == 0 {
            let frame = f.typed.as_ref().unwrap().downcast_ref::<Frame>().unwrap();
            for (s, l) in frame.layers.iter().zip(&world.layers) {
                assert!(s.cells == l.cells);
            }
        }
        retained.push_back(f);
        if retained.len() > 5 {
            retained.pop_front();
        }
    }
}

// Scalar reference for the shader's integrated swap rate and stable glyph IDs.
// A faded cell stops the clock. A sweep can reveal exactly its last letter.
fn swap_sample(id: u16, col: u32, row: u32, age: f32, half: f32, power: f32) -> (u16, u16, f32) {
    use super::field::{BASE, COUNT};
    fn hash(mut x: u32) -> u32 {
        x ^= x >> 16;
        x = x.wrapping_mul(0x7feb352d);
        x ^= x >> 15;
        x = x.wrapping_mul(0x846ca68b);
        x ^ (x >> 16)
    }
    let family = BASE.iter().rposition(|&base| base <= id).unwrap();
    let seed = hash(
        col.wrapping_mul(3517)
            .wrapping_add(row.wrapping_mul(107))
            .wrapping_add(u32::from(id) * 5557),
    );
    let age = age.min(half * (power / 0.009).log2().max(0.));
    let decay = (-age / half).exp2();
    let clock = age * 0.30 + 5.7 * power * half * std::f32::consts::LOG2_E * (1. - decay);
    let bucket = clock.floor() as u32;
    let symbol = |b| {
        if b == 0 {
            id
        } else {
            let mut slot = (hash(seed ^ b) % u32::from(COUNT[family])) as u16;
            while (BASE[family] + slot) % 3 == ((col + row) % 3) as u16 {
                slot = (slot + 1) % COUNT[family];
            }
            BASE[family] + slot
        }
    };
    (
        symbol(bucket.saturating_sub(1)),
        symbol(bucket),
        (clock.fract() / ((0.3 + 5.7 * power * decay) * 0.07)).min(1.),
    )
}
#[test]
fn phosphor_swap_stays_in_dialect_and_dark_memory_stops() {
    let start = swap_sample(289, 15, 2, 0., 0.9, 0.85);
    assert_eq!((start.0, start.1), (289, 289));
    let mut transitions = 0;
    let mut prior = start.1;
    for tick in 1..=300 {
        let (old, new, mix) = swap_sample(289, 15, 2, tick as f32 / 60., 0.9, 0.85);
        assert!((225..321).contains(&old) && (225..321).contains(&new));
        if new != prior {
            assert_eq!(old, prior);
            assert!(mix < 1.);
            transitions += 1;
        }
        prior = new;
    }
    assert!(transitions >= 4);
    assert_eq!(
        swap_sample(289, 15, 2, 100., 0.9, 0.85),
        swap_sample(289, 15, 2, 9999., 0.9, 0.85)
    );
}

#[test]
fn cooling_trails_keep_three_consecutive_swaps_distinct_as_a_group() {
    for col in 9..16 {
        for step in 15..250 {
            let symbols: [u16; 3] = std::array::from_fn(|i| {
                swap_sample(321, col, 18 + i as u32, step as f32 / 30., 0.9, 0.85).1
            });
            assert!(symbols[0] != symbols[1] || symbols[1] != symbols[2]);
        }
    }
}

#[test]
fn four_k_storm_keeps_near_heads_and_lightning_without_orphan_masks() {
    use crate::visuals::depth::{
        common::layout::{Layout, OutputRect},
        schema::Value,
    };
    let mut config = input(7, false);
    config.layout = Layout::new(
        vec![OutputRect {
            id: 0,
            primary: true,
            rect: Rect {
                x: 0.,
                y: 0.,
                width: 3840.,
                height: 2160.,
            },
        }],
        1,
    )
    .unwrap();
    config.speed = 3.;
    config.density = 100.;
    config.scale = 0.25;
    config.trails = 1.;
    config.parameters = vec![
        Value {
            id: 1,
            kind: 3,
            payload: 2,
        },
        Value {
            id: 5,
            kind: 1,
            payload: 100,
        },
    ];
    let mut world = MatrixWorld::new(config);
    let mut heads = vec![Head::default(); 2000];
    let mut history = History::new(2);
    for tick in 1..=1209 {
        if tick == 1200 {
            world
                .inject(Event {
                    id: 3,
                    at_ns: 40_000_000_000,
                    anchor: [1920., 0.],
                    strength: 1.,
                })
                .unwrap();
        }
        world.step(tick);
    }
    history.push(1209, world.snapshot());
    let lease = Lease {
        sample: history
            .sample(world.rate().bracket(40_300_000_000))
            .unwrap(),
        view: View {
            output_id: 0,
            layout_generation: 1,
            timeline_ns: 40_300_000_000,
            viewport: world.input.layout.bounds,
            uniform_scale: 1.,
            offset_x: 0.,
            offset_y: 0.,
            dpr: 1.,
        },
        bounds: world.input.layout.bounds,
    };
    let count =
        unsafe { visuals_rain_heads(&lease, std::ptr::null(), heads.as_mut_ptr(), heads.len()) };
    assert!(count <= 2000);
    heads.truncate(count);
    let near = heads
        .iter()
        .filter(|h| h.light[1] != 3. && h.light[2] == 2.)
        .count();
    let bolts = heads.iter().filter(|h| h.light[1] == 3.).count();
    assert!(near > 0, "near heads were starved");
    assert_eq!(
        bolts,
        world.layers[1].bolt_segments.len(),
        "lightning was starved"
    );
    assert!(bolts > 0);
    assert!(
        heads
            .iter()
            .filter(|h| h.light[1] != 3. && h.light[2] == 0.)
            .count()
            < world.layers[0].streams.len()
    );
    // The small-glyph repro has no intentional floor-occlusion masks. Check
    // every masked texel against the overlays actually returned to the GPU.
    let frame = bound(&lease, true, None).unwrap();
    let mut masked = 0;
    for (index, ink) in frame.layers.iter().enumerate() {
        let [pw, ph] = ink.pitch;
        for (cell_index, cell) in ink.cells.iter().enumerate() {
            if cell.style & 128 == 0 || cell.power == 0 {
                continue;
            }
            let col = cell_index / ink.rows;
            let row = cell_index % ink.rows;
            assert!(
                heads.iter().any(|h| {
                    let [x, y, px, py] = h.path;
                    let (left, right, top, end) = if h.light[1] == 3. {
                        if index != 1 || u32::from_le_bytes(cell.stamp) != h.light[2] as u32 {
                            return false;
                        }
                        let c = ((x + px) * 0.5 / pw).floor() as usize;
                        let r = (y / ph).round() as usize;
                        (c, c + 1, r, r + 1)
                    } else if h.light[1] == 1. {
                        // Spray suppresses all planes within its swept footprint.
                        let layer = h.light[2] as usize;
                        let [dw, dh] = frame.layers[layer].pitch;
                        (
                            ((x.min(px) - 0.32 * dw) / pw).floor() as usize,
                            ((x.max(px) + 0.32 * dw) / pw).ceil() as usize,
                            ((y.min(py) - 0.32 * dh) / ph).floor() as usize,
                            ((y.max(py) + 0.32 * dh) / ph).ceil() as usize,
                        )
                    } else {
                        if h.light[2] != index as f32 {
                            return false;
                        }
                        (
                            ((x.min(px) / pw - 0.48).floor() as usize),
                            (x.max(px) / pw + 0.48).floor() as usize + 1,
                            (y.min(py) / ph - 1.).floor() as usize,
                            (y.max(py) / ph + 1.).ceil() as usize,
                        )
                    };
                    (left..right).contains(&col) && (top..end).contains(&row)
                }),
                "layer {index} cell ({col}, {row}) was masked without an exported overlay"
            );
            masked += 1;
        }
    }
    assert!(masked > 0);
    eprintln!("4K storm: records {count}, near {near}, bolts {bolts}, checked masks {masked}");
}
