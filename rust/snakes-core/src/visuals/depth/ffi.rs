//! Internal staticlib boundary. Mutating handles are exclusively GUI-owned;
//! sample handles own immutable Arc endpoints and are render-thread safe.
use super::{
    common::{
        clock::{History, Sample},
        layout::{Layout, OutputRect, Rect},
    },
    schema::{Parameter, Value},
    world::*,
};
use std::{ptr, slice, sync::Arc, time::Instant};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Config {
    pub visual: u32,
    pub palette: u32,
    pub seed: u64,
    pub speed: f64,
    pub density: f64,
    pub scale: f64,
    pub trails: f64,
    pub reduced_motion: u32,
    pub monitor_mode: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Output {
    pub id: u64,
    pub viewport: Rect,
    pub dpr: f64,
    pub refresh_hz: f64,
    pub primary: u32,
    pub padding: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Env {
    pub background: u32,
    pub frame_cap: u32,
    pub top: [f32; 4],
    pub bottom: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct View {
    pub output_id: u64,
    pub layout_generation: u64,
    pub timeline_ns: i64,
    pub viewport: Rect,
    pub uniform_scale: f64,
    pub offset_x: f64,
    pub offset_y: f64,
    pub dpr: f64,
}
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct Stats {
    pub ticks: u64,
    pub history_misses: u64,
    pub p99_ns: u64,
    pub max_ns: u64,
    pub snapshot_bytes: u64,
}
pub struct World {
    simulation: Box<dyn LivingWorld>,
    history: History<Snapshot>,
    tick: u64,
    stats: Stats,
    durations: Vec<u64>,
    layout_generation: u64,
    bounds: Rect,
    #[cfg(feature = "visual-capture")]
    initial: WorldInput,
    #[cfg(feature = "visual-capture")]
    events: Vec<Event>,
}
pub struct Lease {
    pub sample: Sample<Snapshot>,
    pub view: View,
    pub bounds: Rect,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SampleInfo {
    pub view: View,
    pub world_bounds: Rect,
    pub generations: Generations,
    pub lower_tick: u64,
    pub upper_tick: u64,
    pub alpha: f64,
    pub history_miss: u32,
    pub padding: u32,
}
#[repr(C)]
pub struct Endpoint {
    pub circles: *const Circle,
    pub circle_count: usize,
    pub texture_revision: u64,
    pub texture_width: u32,
    pub texture_height: u32,
    pub texture_bytes: *const u8,
    pub texture_size: usize,
    pub payload: *const u8,
    pub payload_size: usize,
    pub emission: f64,
}
fn construct(input: WorldInput) -> Option<Box<dyn LivingWorld>> {
    #[cfg(feature = "visual-capture")]
    if input.visual_id == 65535 {
        return Some(Box::new(super::fixture::Fixture::new(input)));
    }
    super::descriptor(input.visual_id)
        .filter(|d| d.available || cfg!(feature = "visual-capture"))
        .and_then(|d| (d.create)(input))
}
fn new_world(input: WorldInput, lead_ns: i64) -> Option<World> {
    let simulation = construct(input.clone())?;
    let mut history = History::new(simulation.rate().history_capacity(lead_ns));
    history.push(0, simulation.snapshot());
    Some(World {
        simulation,
        history,
        tick: 0,
        stats: Stats::default(),
        durations: Vec::with_capacity(4096),
        layout_generation: input.layout.generation,
        bounds: input.layout.bounds,
        #[cfg(feature = "visual-capture")]
        initial: input,
        #[cfg(feature = "visual-capture")]
        events: Vec::new(),
    })
}
impl World {
    fn ensure(&mut self, ns: i64, max_ticks: u32) -> bool {
        let target = self.simulation.rate().bracket(ns).upper;
        let end = target.min(self.tick.saturating_add(u64::from(max_ticks.min(1024))));
        while self.tick < end {
            let start = Instant::now();
            self.tick += 1;
            self.simulation.step(self.tick);
            let snapshot = self.simulation.snapshot();
            self.stats.snapshot_bytes = (snapshot.circles.len() * size_of::<Circle>()
                + snapshot.payload.len()
                + snapshot.texture.as_ref().map_or(0, |t| t.rgba.len()))
                as u64;
            self.history.push(self.tick, snapshot);
            let ns = start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
            self.stats.max_ns = self.stats.max_ns.max(ns);
            self.stats.ticks += 1;
            if self.durations.len() < 4096 {
                self.durations.push(ns);
            } else {
                let i = (self.stats.ticks % 4096) as usize;
                self.durations[i] = ns;
            }
        }
        self.tick >= target
    }
}
/// Enumerate the owned Rust descriptors without duplicating their ABI IDs in consumers.
/// The key is a borrowed UTF-8 slice valid for the lifetime of the program.
/// # Safety
/// Both output pointers must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_depth_registry_entry(
    index: usize,
    key: *mut *const u8,
    length: *mut usize,
) -> u32 {
    let Some(d) = super::DESCRIPTORS.get(index) else {
        return 0;
    };
    if key.is_null() || length.is_null() {
        return 0;
    }
    unsafe {
        key.write(d.key.as_ptr());
        length.write(d.key.len());
    }
    d.id
}

/// # Safety
/// All non-null output pointers are writable; static schema storage outlives callers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_depth_schema(
    visual: u32,
    out: *mut *const Parameter,
    count: *mut usize,
) -> i32 {
    let Some(d) = super::descriptor(visual) else {
        return -1;
    };
    if out.is_null() || count.is_null() {
        return -1;
    }
    unsafe {
        out.write(d.parameters.as_ptr());
        count.write(d.parameters.len());
    }
    0
}
#[unsafe(no_mangle)]
pub extern "C" fn visuals_depth_available(visual: u32) -> u32 {
    super::descriptor(visual).is_some_and(|d| d.available) as u32
}
#[unsafe(no_mangle)]
pub extern "C" fn visuals_depth_mirrored_policy(visual: u32) -> u32 {
    super::descriptor(visual).is_some_and(|d| d.mirrored == super::MirroredPolicy::PerOutputLocal)
        as u32
}
/// # Safety
/// Inputs are readable arrays of their declared lengths, result writable. No pointers retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_depth_create(
    config: *const Config,
    values: *const Value,
    value_count: usize,
    outputs: *const Output,
    output_count: usize,
    environment: *const Env,
    obstacles: *const Obstacle,
    obstacle_count: usize,
    layout_generation: u64,
    lead_ns: i64,
    result: *mut *mut World,
) -> i32 {
    if config.is_null()
        || environment.is_null()
        || outputs.is_null()
        || result.is_null()
        || output_count == 0
        || output_count > 64
        || value_count > 256
        || obstacle_count > 128
        || (value_count > 0 && values.is_null())
        || (obstacle_count > 0 && obstacles.is_null())
    {
        return -1;
    }
    let c = unsafe { *config };
    let env = unsafe { *environment };
    let outputs = unsafe { slice::from_raw_parts(outputs, output_count) };
    if c.monitor_mode > 2
        || c.reduced_motion > 1
        || c.palette > 5
        || !(0.1..=3.).contains(&c.speed)
        || !(10. ..=100.).contains(&c.density)
        || !(0.25..=2.).contains(&c.scale)
        || !(0. ..=1.).contains(&c.trails)
        || outputs.iter().any(|o| {
            !o.dpr.is_finite() || o.dpr <= 0. || !o.refresh_hz.is_finite() || o.refresh_hz <= 0.
        })
        || env
            .top
            .iter()
            .chain(env.bottom.iter())
            .any(|v| !v.is_finite())
    {
        return -1;
    }
    let Ok(layout) = Layout::new(
        outputs
            .iter()
            .map(|o| OutputRect {
                id: o.id,
                rect: o.viewport,
                primary: o.primary != 0,
            })
            .collect(),
        layout_generation,
    ) else {
        return -1;
    };
    let parameters = if value_count == 0 {
        Vec::new()
    } else {
        unsafe { slice::from_raw_parts(values, value_count) }.to_vec()
    };
    if let Some(d) = super::descriptor(c.visual) {
        for (i, v) in parameters.iter().enumerate() {
            if parameters[..i].iter().any(|p| p.id == v.id) {
                return -1;
            }
            let Some(p) = d.parameters.iter().find(|p| p.id == v.id) else {
                return -1;
            };
            if v.kind != p.kind
                || ((v.payload as i64) as f64) < p.minimum
                || ((v.payload as i64) as f64) > p.maximum
            {
                return -1;
            }
        }
    }
    let obstacles = if obstacle_count == 0 {
        Vec::new()
    } else {
        unsafe { slice::from_raw_parts(obstacles, obstacle_count) }.to_vec()
    };
    if obstacles
        .iter()
        .any(|o| !o.rect.valid() || !o.inflation.is_finite() || o.inflation < 0. || o.enabled > 1)
    {
        return -1;
    }
    let input = WorldInput {
        visual_id: c.visual,
        seed: c.seed,
        mode: match c.monitor_mode {
            1 => MonitorMode::Mirrored,
            2 => MonitorMode::Spanning,
            _ => MonitorMode::Independent,
        },
        layout,
        speed: c.speed,
        density: c.density,
        scale: c.scale,
        trails: c.trails,
        palette: c.palette,
        reduced_motion: c.reduced_motion != 0,
        parameters,
        environment: Environment {
            background: env.background,
            frame_cap: env.frame_cap,
            top: env.top,
            bottom: env.bottom,
            obstacles,
        },
    };
    let Some(world) = new_world(input, lead_ns) else {
        return -2;
    };
    unsafe {
        result.write(Box::into_raw(Box::new(world)));
    }
    0
}
/// # Safety
/// A live exclusively owned handle; destroy exactly once. Null accepted.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_depth_destroy(world: *mut World) {
    if !world.is_null() {
        drop(unsafe { Box::from_raw(world) });
    }
}
/// # Safety
/// Live exclusively owned handle. 0 ready, 1 needs another bounded call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_depth_ensure_through(
    world: *mut World,
    ns: i64,
    max_ticks: u32,
) -> i32 {
    let Some(w) = (unsafe { world.as_mut() }) else {
        return -1;
    };
    if w.ensure(ns, max_ticks) { 0 } else { 1 }
}
/// # Safety
/// World exclusively borrowed; view readable; result writable. Lease owns endpoints.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_depth_retain_sample(
    world: *mut World,
    view: *const View,
    result: *mut *mut Lease,
) -> i32 {
    let (Some(w), Some(v)) = (unsafe { world.as_mut() }, unsafe { view.as_ref() }) else {
        return -1;
    };
    if result.is_null() {
        return -1;
    }
    let Some(lease) = sample(w, v) else {
        return -1;
    };
    unsafe { result.write(Box::into_raw(Box::new(lease))) };
    0
}
/// # Safety
/// World and lease must both be exclusively borrowed. In particular no render
/// reader may hold this lease while its immutable endpoints are replaced.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_depth_update_sample(
    world: *mut World,
    view: *const View,
    lease: *mut Lease,
) -> i32 {
    let (Some(w), Some(v), Some(l)) = (
        unsafe { world.as_mut() },
        unsafe { view.as_ref() },
        unsafe { lease.as_mut() },
    ) else {
        return -1;
    };
    let Some(sample) = sample(w, v) else {
        return -1;
    };
    *l = sample;
    0
}
fn sample(w: &mut World, v: &View) -> Option<Lease> {
    if v.layout_generation != w.layout_generation
        || !v.viewport.valid()
        || !v.uniform_scale.is_finite()
        || v.uniform_scale <= 0.
        || !v.offset_x.is_finite()
        || !v.offset_y.is_finite()
        || !v.dpr.is_finite()
        || v.dpr <= 0.
    {
        return None;
    }
    let sample = w.history.sample(w.simulation.rate().bracket(v.timeline_ns))?;
    let mut view = *v;
    if sample.missed {
        w.stats.history_misses += 1;
        // Analytic envelopes/camera time must clamp with the retained data.
        view.timeline_ns = sample.lower.time_ns;
    }
    Some(Lease {
        sample,
        view,
        bounds: w.bounds,
    })
}
/// # Safety
/// A live lease, destroyed exactly once. Null accepted. May run on any thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_depth_sample_release(lease: *mut Lease) {
    if !lease.is_null() {
        drop(unsafe { Box::from_raw(lease) });
    }
}
/// # Safety
/// Lease readable (immutable, any thread); info writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_depth_sample_info(
    lease: *const Lease,
    info: *mut SampleInfo,
) -> i32 {
    let Some(l) = (unsafe { lease.as_ref() }) else {
        return -1;
    };
    if info.is_null() {
        return -1;
    }
    unsafe {
        info.write(SampleInfo {
            view: l.view,
            world_bounds: l.bounds,
            generations: l.sample.upper.generations,
            lower_tick: l.sample.lower.tick,
            upper_tick: l.sample.upper.tick,
            alpha: l.sample.alpha,
            history_miss: l.sample.missed as u32,
            padding: 0,
        });
    }
    0
}
/// # Safety
/// Lease readable (any thread); endpoint writable. Pointers remain valid for lease lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_depth_sample_endpoint(
    lease: *const Lease,
    upper: u32,
    endpoint: *mut Endpoint,
) -> i32 {
    let Some(l) = (unsafe { lease.as_ref() }) else {
        return -1;
    };
    if endpoint.is_null() || upper > 1 {
        return -1;
    }
    let s: &Arc<Snapshot> = if upper == 0 {
        &l.sample.lower
    } else {
        &l.sample.upper
    };
    let texture = s.texture.as_ref();
    unsafe {
        endpoint.write(Endpoint {
            circles: s.circles.as_ptr(),
            circle_count: s.circles.len(),
            texture_revision: texture.map_or(0, |t| t.revision),
            texture_width: texture.map_or(0, |t| t.width),
            texture_height: texture.map_or(0, |t| t.height),
            texture_bytes: texture.map_or(ptr::null(), |t| t.rgba.as_ptr()),
            texture_size: texture.map_or(0, |t| t.rgba.len()),
            payload: s.payload.as_ptr(),
            payload_size: s.payload.len(),
            emission: s.emission_estimate,
        });
    }
    0
}
/// # Safety
/// World exclusively borrowed; stats writable. Percentile is of latest 4096 ticks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_depth_stats(world: *const World, stats: *mut Stats) -> i32 {
    let Some(w) = (unsafe { world.as_ref() }) else {
        return -1;
    };
    if stats.is_null() {
        return -1;
    }
    let mut times = w.durations.clone();
    times.sort_unstable();
    let mut s = w.stats;
    s.p99_ns = if times.is_empty() {
        0
    } else {
        times[(times.len() * 99).div_ceil(100).saturating_sub(1)]
    };
    unsafe {
        stats.write(s);
    }
    0
}
#[cfg(feature = "visual-capture")]
/// # Safety
/// Exclusively borrowed live world. Replay preserves the initial input and injected script.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_depth_debug_seek(world: *mut World, ns: i64) -> i32 {
    let Some(w) = (unsafe { world.as_mut() }) else {
        return -1;
    };
    if ns < 0 {
        return -1;
    }
    let events = w.events.clone();
    let Some(mut replay) = new_world(w.initial.clone(), 1_000_000_000) else {
        return -1;
    };
    for e in &events {
        if replay.simulation.inject(*e).is_err() {
            return -1;
        }
    }
    while !replay.ensure(ns, 1024) {}
    replay.events = events;
    *w = replay;
    0
}
#[cfg(feature = "visual-capture")]
/// # Safety
/// Exclusively borrowed live world; event readable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_depth_debug_inject(world: *mut World, event: *const Event) -> i32 {
    let (Some(w), Some(e)) = (unsafe { world.as_mut() }, unsafe { event.as_ref() }) else {
        return -1;
    };
    if w.simulation.inject(*e).is_err() {
        return -1;
    }
    w.events.push(*e);
    0
}
#[cfg(feature = "visual-capture")]
/// # Safety
/// Live world exclusively borrowed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_depth_debug_hash(world: *const World) -> u64 {
    unsafe { world.as_ref() }.map_or(0, |w| w.simulation.state_hash())
}
const _: () = {
    assert!(size_of::<Config>() == 56);
    assert!(size_of::<Output>() == 64);
    assert!(size_of::<View>() == 88);
    assert!(size_of::<Circle>() == 72);
    assert!(size_of::<SampleInfo>() == 184);
    assert!(std::mem::offset_of!(Circle, birth_ns) == 56);
    assert!(std::mem::offset_of!(Circle, death_ns) == 64);
    assert!(size_of::<Parameter>() == 48);
    assert!(size_of::<Value>() == 16);
    assert!(size_of::<Env>() == 40);
    assert!(size_of::<Obstacle>() == 56);
    assert!(size_of::<Generations>() == 32);
    assert!(size_of::<Stats>() == 40);
    assert!(size_of::<Endpoint>() == 72);
    assert!(size_of::<Event>() == 40);
};
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exported_schema() {
        for (i, d) in super::super::DESCRIPTORS.iter().enumerate() {
            let mut key = ptr::null();
            let mut length = 0;
            assert_eq!(
                unsafe { visuals_depth_registry_entry(i, &mut key, &mut length) },
                d.id
            );
            assert_eq!(
                unsafe { slice::from_raw_parts(key, length) },
                d.key.as_bytes()
            );
            let mut p = ptr::null();
            let mut n = 0;
            assert_eq!(unsafe { visuals_depth_schema(d.id, &mut p, &mut n) }, 0);
            assert_eq!(n, d.parameters.len());
            assert!(!p.is_null());
            assert_eq!(visuals_depth_available(d.id), u32::from(d.available));
            let params = unsafe { slice::from_raw_parts(p, n) };
            for (i, p) in params.iter().enumerate() {
                assert!(p.minimum <= p.default_value && p.default_value <= p.maximum);
                assert!(!params[..i].iter().any(|q| p.id == q.id));
            }
        }
        let mut key = ptr::null();
        let mut length = 0;
        let end = super::super::DESCRIPTORS.len();
        assert_eq!(
            unsafe { visuals_depth_registry_entry(end, &mut key, &mut length) },
            0
        );
    }
}

#[cfg(feature = "visual-capture")]
/// # Safety
/// Live exclusive handle; reset/replay changes only this unpublished/debug world.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_depth_debug_seed(world: *mut World, seed: u64) -> i32 {
    let Some(w) = (unsafe { world.as_mut() }) else {
        return -1;
    };
    let mut initial = w.initial.clone();
    initial.seed = seed;
    let Some(new) = new_world(initial, 1_000_000_000) else {
        return -1;
    };
    *w = new;
    0
}
#[cfg(feature = "visual-capture")]
/// # Safety
/// Live exclusive handle; performs at most 1024 fixed steps.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_depth_debug_step(world: *mut World, ticks: u32) -> i32 {
    let Some(w) = (unsafe { world.as_mut() }) else {
        return -1;
    };
    if ticks == 0 || ticks > 1024 {
        return -1;
    }
    let rate = w.simulation.rate();
    let numerator =
        i128::from(w.tick + u64::from(ticks) - 1) * 1_000_000_000 * i128::from(rate.denominator);
    let ns = ((numerator + i128::from(rate.numerator) - 1) / i128::from(rate.numerator))
        .min(i128::from(i64::MAX)) as i64;
    w.ensure(ns, ticks);
    0
}

#[cfg(all(test, feature = "visual-capture"))]
mod capture_tests {
    use super::*;
    fn fixture() -> World {
        new_world(
            WorldInput {
                visual_id: 65535,
                seed: 7,
                mode: MonitorMode::Spanning,
                layout: Layout::new(
                    vec![OutputRect {
                        id: 1,
                        rect: Rect {
                            x: 0.,
                            y: 0.,
                            width: 7920.,
                            height: 1440.,
                        },
                        primary: true,
                    }],
                    1,
                )
                .unwrap(),
                speed: 1.,
                density: 50.,
                scale: 1.,
                trails: 0.35,
                palette: 0,
                reduced_motion: false,
                parameters: Vec::new(),
                environment: Environment::default(),
            },
            1_000_000_000,
        )
        .unwrap()
    }
    #[test]
    fn bounded_ticks_replay_births_and_revisioned_data() {
        let mut world = fixture();
        assert!(!world.ensure(3_000_000_000, 8));
        assert_eq!(world.tick, 8);
        assert_eq!(unsafe { visuals_depth_debug_seed(&mut world, 7) }, 0);
        assert_eq!(unsafe { visuals_depth_debug_step(&mut world, 2) }, 0);
        assert_eq!(world.tick, 2);
        let event = Event {
            id: 1,
            at_ns: 1_000_000_000,
            anchor: [6000., 600.],
            strength: 1.,
        };
        assert_eq!(unsafe { visuals_depth_debug_inject(&mut world, &event) }, 0);
        assert_eq!(
            unsafe { visuals_depth_debug_seek(&mut world, 3_000_000_000) },
            0
        );
        let hash = world.simulation.state_hash();
        let snapshot = world.simulation.snapshot();
        let born = snapshot.circles.iter().find(|c| c.id == 3).unwrap();
        assert_eq!(born.generation, 1);
        assert_eq!(born.birth_ns, 3_000_000_000);
        assert_eq!(born.death_ns, 5_000_000_000);
        let texture = snapshot.texture.clone().unwrap();
        assert_eq!(texture.revision, 2);
        assert_eq!(
            unsafe { visuals_depth_debug_seek(&mut world, 2_000_000_000) },
            0
        );
        assert!(world.simulation.snapshot().texture.is_none());
        assert!(
            !world
                .simulation
                .snapshot()
                .circles
                .iter()
                .any(|c| c.id == 3)
        );
        assert_eq!(
            unsafe { visuals_depth_debug_seek(&mut world, 3_000_000_000) },
            0
        );
        assert_eq!(world.simulation.state_hash(), hash);
        let view = View {
            output_id: 1,
            layout_generation: 1,
            timeline_ns: 0,
            viewport: world.bounds,
            uniform_scale: 1.,
            offset_x: 0.,
            offset_y: 0.,
            dpr: 1.,
        };
        let mut lease = ptr::null_mut();
        assert_eq!(
            unsafe { visuals_depth_retain_sample(&mut world, &view, &mut lease) },
            0
        );
        let mut info = std::mem::MaybeUninit::uninit();
        assert_eq!(
            unsafe { visuals_depth_sample_info(lease, info.as_mut_ptr()) },
            0
        );
        let info = unsafe { info.assume_init() };
        assert_eq!(info.history_miss, 1);
        assert_eq!(info.view.timeline_ns, 2_000_000_000);
        drop(world);
        let mut endpoint = std::mem::MaybeUninit::uninit();
        assert_eq!(
            unsafe { visuals_depth_sample_endpoint(lease, 0, endpoint.as_mut_ptr()) },
            0
        );
        assert!(unsafe { endpoint.assume_init() }.circle_count > 0);
        unsafe { visuals_depth_sample_release(lease) };
        assert_eq!(texture.rgba.len(), 16);
        assert_eq!(snapshot.circles.len(), 4);
    }
    #[test]
    fn reusable_lease_preserves_readers_and_rejects_invalid_updates() {
        let mut world = fixture();
        assert!(world.ensure(0, 8));
        let mut view = View {
            output_id: 1,
            layout_generation: 1,
            timeline_ns: 0,
            viewport: world.bounds,
            uniform_scale: 1.,
            offset_x: 0.,
            offset_y: 0.,
            dpr: 1.,
        };
        let mut reusable = ptr::null_mut();
        let mut reader = ptr::null_mut();
        assert_eq!(unsafe { visuals_depth_retain_sample(&mut world, &view, &mut reusable) }, 0);
        assert_eq!(unsafe { visuals_depth_retain_sample(&mut world, &view, &mut reader) }, 0);
        let address = reusable;
        let original = unsafe { (&*reader).sample.lower.clone() };
        for i in 1..=100 {
            view.timeline_ns = i * 1000;
            assert_eq!(unsafe { visuals_depth_update_sample(&mut world, &view, reusable) }, 0);
            assert_eq!(reusable, address);
            assert_eq!(unsafe { (*reusable).view.timeline_ns }, view.timeline_ns);
            assert_eq!(unsafe { (*reader).view.timeline_ns }, 0);
            assert!(unsafe { Arc::ptr_eq(&(&*reader).sample.lower, &original) });
        }
        view.layout_generation += 1;
        assert_eq!(unsafe { visuals_depth_update_sample(&mut world, &view, reusable) }, -1);
        assert_eq!(unsafe { (*reusable).view.timeline_ns }, 100_000);
        assert_eq!(unsafe { visuals_depth_update_sample(&mut world, &view, ptr::null_mut()) }, -1);
        drop(world);
        assert!(!unsafe { (&*reusable).sample.lower.circles.is_empty() });
        assert!(!unsafe { (&*reader).sample.lower.circles.is_empty() });
        unsafe {
            visuals_depth_sample_release(reusable);
            visuals_depth_sample_release(reader);
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn visuals_depth_mirrored_fit(visual: u32) -> u32 {
    super::descriptor(visual).map_or(0, |d| d.fit as u32)
}
