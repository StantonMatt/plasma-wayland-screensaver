use snakes_core::visuals::depth::{
    common::{
        clock::Sample,
        layout::{Layout, OutputRect, Rect},
    },
    ffi::{Lease, View, visuals_depth_available},
    ribbons::{DESCRIPTOR, Draw, PARAMETERS, Ribbons, VertexPair, visuals_ribbons_write},
    world::{Environment, Event, LivingWorld, MonitorMode, Snapshot, WorldInput},
};
use std::{
    alloc::{GlobalAlloc, Layout as AllocLayout, System},
    cell::Cell,
    sync::Arc,
    time::{Duration, Instant},
};
thread_local! {static COUNT:Cell<Option<usize>>=const{Cell::new(None)};}
struct Allocator;
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, l: AllocLayout) -> *mut u8 {
        COUNT.with(|n| n.set(n.get().map(|i| i + 1)));
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: AllocLayout) {
        unsafe { System.dealloc(p, l) }
    }
}
#[global_allocator]
static ALLOC: Allocator = Allocator;

fn inputs(still: bool, stress: bool) -> WorldInput {
    let mut outputs = vec![];
    for (id, (x, y, width, height)) in [
        (0., 0., 2560., 1440.),
        (2560., 0., 3440., 1440.),
        (6000., 195., 1920., 1080.),
    ]
    .into_iter()
    .enumerate()
    {
        outputs.push(OutputRect {
            id: id as u64,
            rect: Rect {
                x,
                y,
                width,
                height,
            },
            primary: id == 0,
        });
    }
    WorldInput {
        visual_id: 4,
        seed: 7,
        mode: MonitorMode::Spanning,
        layout: Layout::new(outputs, 1).unwrap(),
        speed: if stress { 3. } else { 1. },
        density: if stress { 100. } else { 50. },
        scale: if stress { 2. } else { 1. },
        trails: if stress { 1. } else { 0.35 },
        palette: 0,
        reduced_motion: still,
        parameters: vec![],
        environment: Environment::default(),
    }
}
fn export_lease(lo: &Arc<Snapshot>, hi: &Arc<Snapshot>, view: Rect, ns: i64, alpha: f64) -> Lease {
    Lease {
        sample: Sample {
            lower: lo.clone(),
            upper: hi.clone(),
            alpha,
            missed: false,
        },
        view: View {
            output_id: 0,
            layout_generation: 1,
            timeline_ns: ns,
            viewport: view,
            uniform_scale: 1.,
            offset_x: 0.,
            offset_y: 0.,
            dpr: 1.,
        },
        bounds: view,
    }
}
struct Buffers {
    nodes: Vec<VertexPair>,
    draws: Vec<Draw>,
}
impl Buffers {
    fn new() -> Self {
        Self {
            nodes: vec![VertexPair::default(); 10754],
            draws: vec![Draw::default(); 82],
        }
    }
    fn export(
        &mut self,
        lo: &Arc<Snapshot>,
        hi: &Arc<Snapshot>,
        view: Rect,
        ns: i64,
        alpha: f64,
    ) -> (usize, usize) {
        let lease = export_lease(lo, hi, view, ns, alpha);
        let mut n = 0;
        let d = unsafe {
            visuals_ribbons_write(
                &lease,
                self.nodes.as_mut_ptr(),
                self.nodes.len(),
                self.draws.as_mut_ptr(),
                self.draws.len(),
                &mut n,
            )
        };
        (n, d)
    }
}
#[test]
fn descriptor_and_world_contract() {
    assert_eq!(visuals_depth_available(4), 1);
    assert_eq!((DESCRIPTOR.rate, PARAMETERS.len()), (30, 5));
    assert_eq!(
        PARAMETERS
            .iter()
            .map(|p| p.default_value as i32)
            .collect::<Vec<_>>(),
        [40, 55, 55, 1, 1]
    );
    let input = inputs(false, false);
    let full = input.layout.bounds;
    let (mut a, mut b) = (Ribbons::new(input.clone()), Ribbons::new(input.clone()));
    let mut buf = Buffers::new();
    let mut phases = Vec::new();
    for i in 1..=900 {
        if i == 200 {
            for w in [&mut a, &mut b] {
                w.inject(Event {
                    id: 2,
                    at_ns: 8_000_000_000,
                    anchor: [2560., 720.],
                    strength: 1.,
                })
                .unwrap();
            }
        }
        a.step(i);
        b.step(i);
        if i % 100 == 0 {
            let h = a.state_hash();
            assert_eq!(h, b.state_hash());
            assert!(!phases.contains(&h));
            phases.push(h);
            let frame = Arc::new(a.snapshot());
            let (n, d) = buf.export(&frame, &frame, full, frame.time_ns, 0.);
            assert!((2..=41).contains(&d));
            assert!(n <= 40 * 128 + 513);
            for v in buf.nodes[..n].iter().flat_map(|s| [s.lower, s.upper]) {
                assert!(
                    v.pos
                        .iter()
                        .chain(&v.normal)
                        .chain([&v.facing, &v.half_width, &v.alpha])
                        .all(|v| v.is_finite())
                );
            }
            let ref_frame = Arc::new(b.snapshot());
            let mut other = Buffers::new();
            assert_eq!(
                other.export(&ref_frame, &ref_frame, full, frame.time_ns, 0.),
                (n, d)
            );
            assert_eq!(&buf.nodes[..n], &other.nodes[..n]);
        }
    }
}
#[test]
fn no_allocations_during_steady_step_publication_and_per_output_export() {
    let input = inputs(false, true);
    let mut w = Ribbons::new(input.clone());
    let mut buf = Buffers::new();
    let mut pair = [Arc::new(w.snapshot()), Arc::new(w.snapshot())];
    for i in 1..=1100 {
        w.step(i);
        *Arc::get_mut(&mut pair[(i % 2) as usize]).unwrap() = w.snapshot();
        if i == 10 {
            COUNT.with(|n| n.set(Some(0)));
        }
        for o in &input.layout.outputs {
            buf.export(&pair[0], &pair[1], o.rect, 0, 0.3);
        }
    }
    assert_eq!(COUNT.with(|n| n.replace(None)), Some(0));
}
#[test]
fn mixed_refresh_seam_samples_use_the_same_interpolated_world_positions() {
    let input = inputs(false, false);
    let mut w = Ribbons::new(input.clone());
    for i in 1..=899 {
        w.step(i);
    }
    let a = Arc::new(w.snapshot());
    w.step(900);
    let b = Arc::new(w.snapshot());
    let mut left = Buffers::new();
    let mut right = Buffers::new();
    let mut later = Buffers::new();
    let (l, _) = left.export(&a, &b, input.layout.outputs[0].rect, a.time_ns, 0.42);
    let (r, _) = right.export(&a, &b, input.layout.outputs[1].rect, a.time_ns, 0.42);
    let (n, _) = later.export(
        &a,
        &b,
        input.layout.outputs[1].rect,
        a.time_ns + 3_000_000,
        0.51,
    );
    let mut shared = 0;
    for v in &left.nodes[..l] {
        if (v.at(0.42).pos[0] - 2560.).abs() < 60.
            && let Some(j) = right.nodes[..r]
                .iter()
                .position(|w| w.lower == v.lower && w.upper == v.upper)
        {
            assert!(j < n);
            let p = later.nodes[j].at(0.51).pos;
            let v = v.at(0.42);
            assert!((v.pos[0] - p[0]).hypot(v.pos[1] - p[1]) < 3.);
            shared += 1;
        }
    }
    assert!(shared > 0, "a ribbon must actually cross this seam");
}
#[test]
fn layer_batches_preserve_strip_boundaries() {
    let mut input = inputs(false, false);
    input.seed = 1;
    input.speed = 3.;
    input.scale = 2.;
    input.trails = 1.;
    input.density = 55.;
    let mut world = Ribbons::new(input.clone());
    let mut buf = Buffers::new();
    let mut frames = [Arc::new(world.snapshot()), Arc::new(world.snapshot())];
    let mut ribbons = 0;
    let mut submissions = 0;
    let mut nodes = 0;
    let mut exports = 0;
    for tick in 1..=1050 {
        world.step(tick);
        let k = tick % 2;
        *Arc::get_mut(&mut frames[k as usize]).unwrap() = world.snapshot();
        if tick < 150 || tick % 15 != 0 {
            continue;
        }
        for o in &input.layout.outputs {
            let (n, count) = buf.export(
                &frames[(1 - k) as usize],
                &frames[k as usize],
                o.rect,
                0,
                0.5,
            );
            nodes += n;
            exports += 1;
            let mut batch = false;
            let mut layers = [false; 3];
            let mut previous_layer = 0;
            for draw in &buf.draws[..count] {
                let first = draw.first as usize;
                let end = first + draw.count as usize;
                if draw.motes != 0 {
                    assert!(
                        buf.nodes[first..end]
                            .iter()
                            .all(|v| v.tag >= 4096. && v.tag < 65535.)
                    );
                    batch = true;
                    continue;
                }
                let layer = draw.material / 6;
                assert!(layer >= previous_layer);
                previous_layer = layer;
                assert_eq!(buf.nodes[end - 1].tag, 65535.);
                assert!(
                    buf.nodes[first..end - 1]
                        .iter()
                        .all(|v| (v.tag as u32 & 31) == draw.material)
                );
                layers[layer as usize] = true;
                ribbons += 1;
            }
            submissions += usize::from(batch) + layers.into_iter().filter(|v| *v).count();
        }
    }
    println!(
        "{exports} Spanning exports: {ribbons} ribbons, {submissions} submissions, {:.0} bytes per new bracket",
        (nodes * size_of::<VertexPair>()) as f64 / exports as f64
    );
    assert!(ribbons > 0 && submissions <= 4 * exports);
}
#[test]
fn frozen_stills_and_ten_minute_dissolve() {
    let input = inputs(true, false);
    let mut w = Ribbons::new(input.clone());
    let a = Arc::new(w.snapshot());
    w.step(1);
    let b = Arc::new(w.snapshot());
    let mut buf = Buffers::new();
    let (n, d) = buf.export(&a, &b, input.layout.bounds, 0, 0.);
    let v = buf.nodes[..n].to_vec();
    assert_eq!(
        buf.export(&a, &b, input.layout.bounds, 60_000_000_000, 0.1),
        (n, d)
    );
    assert_eq!(&v[..], &buf.nodes[..n]);
    assert_eq!(w.next_wake_ns(0), Some(600_000_000_000));
    w.step(2);
    let c = Arc::new(w.snapshot());
    let (_, mid) = buf.export(&b, &c, input.layout.bounds, 604_000_000_000, 4. / 600.);
    assert!(mid > d);
    assert!(buf.draws[..mid].iter().all(|r| r.opacity == 0.5));
    let (end, _) = buf.export(&b, &c, input.layout.bounds, 608_000_000_000, 8. / 600.);
    assert_ne!(&buf.nodes[..end], &v[..]);
    assert_eq!(w.next_wake_ns(608_000_000_000), Some(1_200_000_000_000));
}
#[test]
#[ignore = "write conservative pixel bounds for the private A/60 s GPU capture"]
fn native_scene_support() {
    use snakes_core::visuals::depth::common::rng::output_seed;
    use std::io::Write;

    let output =
        std::env::var_os("PVS_RIBBON_SUPPORT").expect("Set PVS_RIBBON_SUPPORT to an output file");
    let mut input = inputs(false, false);
    input.mode = MonitorMode::Independent;
    input.seed = output_seed(7, 4, "Virtual-0");
    input.layout = Layout::new(vec![input.layout.outputs[0].clone()], 1).unwrap();
    let mut world = Ribbons::new(input.clone());
    for tick in 1..=1800 {
        world.step(tick);
    }
    let lo = Arc::new(world.snapshot());
    world.step(1801);
    let hi = Arc::new(world.snapshot());
    let mut buf = Buffers::new();
    let (_, draws) = buf.export(&lo, &hi, input.layout.bounds, 60_000_000_000, 0.);
    let mut file = std::io::BufWriter::new(std::fs::File::create(output).unwrap());
    for draw in &buf.draws[..draws] {
        let start = draw.first as usize;
        let n = draw.count as usize;
        let nodes = &buf.nodes[start..start + n];
        for i in 0..(if draw.motes != 0 { n } else { n - 1 }) {
            let a = nodes[i];
            let b = if draw.motes != 0 { a } else { nodes[i + 1] };
            let ends = [a.lower, a.upper, b.lower, b.upper];
            let width = ends.iter().map(|v| v.half_width).fold(0., f32::max);
            let view_pad = if draw.motes != 0 {
                2.4 * width + 1.
            } else {
                let facing = ends.iter().map(|v| v.facing.abs()).fold(0., f32::max);
                let chord = [a.lower, a.upper]
                    .into_iter()
                    .flat_map(|x| {
                        [b.lower, b.upper].map(|y| (x.pos[0] - y.pos[0]).hypot(x.pos[1] - y.pos[1]))
                    })
                    .fold(0., f32::max);
                // Uniform-cover scale is one. Include the complete Hermite control hull,
                // projected halo, near-layer soft edge and one raster sample.
                let cap = ends
                    .iter()
                    .filter(|v| v.cap_mix != 0.)
                    .map(|v| 1.2 * v.halo_radius)
                    .fold(0., f32::max);
                chord / 3.
                    + 2.18
                        * (2.04
                            + width * 1.27 * (0.12 + 0.88 * facing)
                            + cap.max(width * 7. * draw.silk.halo[3]))
                    + 1.
            };
            let min =
                |k: usize| ends.iter().map(|v| v.pos[k]).fold(f32::INFINITY, f32::min) - view_pad;
            let max = |k: usize| {
                ends.iter()
                    .map(|v| v.pos[k])
                    .fold(f32::NEG_INFINITY, f32::max)
                    + view_pad
            };
            writeln!(
                file,
                "{},{},{},{}",
                min(0).floor(),
                min(1).floor(),
                max(0).ceil(),
                max(1).ceil()
            )
            .unwrap();
        }
    }
}

/// The Linux per-thread resource record contains two timevals, twelve accounting
/// counters and the voluntary/involuntary context-switch counts.
#[cfg(target_os = "linux")]
fn thread_usage() -> (u128, std::ffi::c_long) {
    use std::ffi::{c_int, c_long};
    #[repr(C)]
    #[derive(Default)]
    struct Usage {
        user: [c_long; 2],
        system: [c_long; 2],
        counters: [c_long; 12],
        switches: [c_long; 2],
    }
    unsafe extern "C" {
        fn getrusage(who: c_int, usage: *mut Usage) -> c_int;
        fn clock_gettime(clock: c_int, time: *mut [c_long; 2]) -> c_int;
    }
    let mut data = Usage::default();
    // RUSAGE_THREAD supplies the context switches. Linux user/system buckets
    // can advance in coarse increments, so time the call on the thread clock.
    assert_eq!(unsafe { getrusage(1, &mut data) }, 0);
    let mut time = [0; 2];
    const CLOCK_THREAD_CPUTIME_ID: c_int = 3;
    assert_eq!(
        unsafe { clock_gettime(CLOCK_THREAD_CPUTIME_ID, &mut time) },
        0
    );
    let ns = time[0] as u128 * 1_000_000_000 + time[1] as u128;
    (ns, data.switches.iter().sum())
}

#[test]
#[ignore = "run the stress distribution on a quiet CPU using the Release profile"]
fn stress_distribution() {
    let input = inputs(false, true);
    let start = Instant::now();
    let mut w = Ribbons::new(input.clone());
    let setup = start.elapsed();
    let mut samples = Vec::with_capacity(18_000);
    let mut exports = Vec::with_capacity(54_000);
    let mut simulation = Duration::ZERO;
    let mut publication = Duration::ZERO;
    let mut endpoints = [Arc::new(w.snapshot()), Arc::new(w.snapshot())];
    let mut buffers = packing::Packets::new();
    let mut draw_count = 0;
    let mut batch_count = 0;
    let mut vertex_bytes = 0;
    #[cfg(target_os = "linux")]
    let mut max_cpu_ns = 0;
    #[cfg(target_os = "linux")]
    let mut scheduler_outliers = 0;
    for i in 1..=18_000 {
        #[cfg(target_os = "linux")]
        let before = thread_usage();
        let start = Instant::now();
        w.step(i);
        let publish_start = Instant::now();
        simulation += publish_start - start;
        let j = i as usize % 2;
        *Arc::get_mut(&mut endpoints[j]).unwrap() = w.snapshot();
        publication += publish_start.elapsed();
        for o in &input.layout.outputs {
            let export_start = Instant::now();
            let (nodes, draws) = buffers.export(&endpoints[1 - j], &endpoints[j], o.rect, 0, 0.5);
            exports.push(export_start.elapsed().as_nanos());
            draw_count += draws;
            let mut layers = [false; 4];
            for d in &buffers.draws[..draws] {
                layers[if d.motes != 0 {
                    3
                } else {
                    (d.material / 6) as usize
                }] = true;
            }
            batch_count += layers.into_iter().filter(|v| *v).count();
            vertex_bytes += buffers.bytes(nodes);
        }
        let wall_ns = start.elapsed().as_nanos();
        samples.push(wall_ns);
        #[cfg(target_os = "linux")]
        {
            let after = thread_usage();
            let cpu_ns = after.0 - before.0;
            max_cpu_ns = max_cpu_ns.max(cpu_ns);
            if wall_ns > 2_000_000 {
                let switches = after.1 - before.1;
                println!(
                    "tick {i}: {wall_ns} ns wall, {cpu_ns} ns thread CPU, {switches} context switches"
                );
                if cpu_ns <= 2_000_000 && switches > 0 {
                    scheduler_outliers += 1;
                }
            }
        }
    }
    exports.sort_unstable();
    println!(
        "{} viewport exports: p50 {} ns p99 {} ns, average {:.0} uploaded bytes, {batch_count} layer submissions for {draw_count} source ranges",
        exports.len(),
        exports[27000],
        exports[53459],
        vertex_bytes as f64 / 54000.
    );
    println!(
        "Mean per tick simulation {:?}, snapshot {:?}, three exports {} ns",
        simulation / 18000,
        publication / 18000,
        exports.iter().sum::<u128>() / 18000
    );
    samples.sort_unstable();
    println!(
        "stress 18000 ticks setup {setup:?}, p50 {} ns p99 {} ns max {} ns",
        samples[9000], samples[17819], samples[17999]
    );
    assert!(samples[17819] <= 500_000);
    #[cfg(target_os = "linux")]
    {
        println!("max thread CPU {max_cpu_ns} ns, {scheduler_outliers} evidenced preemptions");
        assert!(max_cpu_ns <= 2_000_000);
        assert_eq!(
            samples.iter().filter(|ns| **ns > 2_000_000).count(),
            scheduler_outliers
        );
    }
    #[cfg(not(target_os = "linux"))]
    assert!(samples[17999] <= 2_000_000);
}
#[test]
#[ignore = "two hours of product simulation time, with retained endpoint and RSS checks"]
fn two_hour_soak() {
    let input = inputs(false, true);
    let mut w = Ribbons::new(input.clone());
    let mut buf = Buffers::new();
    let mut pair = [Arc::new(w.snapshot()), Arc::new(w.snapshot())];
    let mut readings = Vec::new();
    for i in 1..=216000 {
        w.step(i);
        let k = i as usize % 2;
        *Arc::get_mut(&mut pair[k]).unwrap() = w.snapshot();
        for o in &input.layout.outputs {
            buf.export(&pair[1 - k], &pair[k], o.rect, 0, 0.33);
        }
        if i % 18000 == 0 {
            let status = std::fs::read_to_string("/proc/self/status").unwrap();
            let rss = status
                .lines()
                .find(|s| s.starts_with("VmRSS:"))
                .unwrap()
                .split_whitespace()
                .nth(1)
                .unwrap()
                .parse::<u64>()
                .unwrap();
            readings.push(rss);
        }
    }
    println!(
        "216000 ticks, 648000 per-output samples, RSS KiB at ten-minute intervals {readings:?}"
    );
    assert!(readings.iter().max().unwrap() - readings.iter().min().unwrap() < 4096);
}

#[path = "../../../tests/depth/ribbons/folds.rs"]
mod folds;

#[path = "../../../tests/depth/ribbons/packing.rs"]
mod packing;
