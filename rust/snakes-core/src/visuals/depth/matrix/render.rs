use super::{
    field::{Cell, Head},
    *,
};
use crate::visuals::{
    depth::{common::layout::Rect, ffi::Lease},
    palette,
};
#[cfg(test)]
fn lightning_lift(age: f32) -> f32 {
    lightning::Rhythm::default().lift(age)
}
#[cfg(test)]
fn lightning_core(age: f32) -> f32 {
    lightning::Rhythm::default().core(age)
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Dirty {
    stamp: u32,
    lo: u16,
    hi: u16,
}
#[derive(Default)]
struct Ink {
    pitch: [f32; 2],
    rows: usize,
    cells: Vec<Cell>,
    dirty: Vec<Dirty>,
}
pub(super) struct StillSource {
    input: WorldInput,
    // Concurrent render threads on a spanning desk request the same photo.
    // A weak entry lets the last view release an old epoch immediately.
    latest: std::sync::Mutex<(u64, std::sync::Weak<Frame>)>,
}
impl StillSource {
    pub fn new(input: WorldInput) -> Self {
        Self {
            input,
            latest: Default::default(),
        }
    }
    fn photo(&self, epoch: u64) -> Arc<Frame> {
        let mut latest = self.latest.lock().unwrap();
        if latest.0 == epoch
            && let Some(photo) = latest.1.upgrade()
        {
            return photo;
        }
        let mut input = self.input.clone();
        input.seed = hash64(input.seed ^ hash64(epoch));
        let world = MatrixWorld::new(input);
        let photo = world.photo.expect("reduced-motion world has a photograph");
        *latest = (epoch, Arc::downgrade(&photo));
        photo
    }
}
#[derive(Default)]
pub struct Frame {
    layers: [Ink; 3],
    heads: Vec<Head>,
    stamp: u32,
    depth: f32,
    half: [f32; 3],
    colors: [[f32; 4]; 6],
    background: [f32; 4],
    sky_top: [f32; 4],
    bounds: Rect,
    background_id: u32,
    palette: u32,
    sweep: [f32; 2],
    strike_age: f32,
    strike_x: f32,
    rhythm: lightning::Rhythm,
    hush: f32,
    stills: Option<Arc<StillSource>>,
    pub still: bool,
}
impl Frame {
    pub(super) fn update(&mut self, w: &MatrixWorld) {
        let previous_stamp = self.stamp;
        self.stamp = if w.input.reduced_motion {
            w.photo_stamp
        } else {
            PREWARM + w.tick as u32
        };
        self.still = w.input.reduced_motion;
        self.stills.clone_from(&w.stills);
        self.heads.clear();
        // Each pooled frame allocates its bounded export storage only once.
        self.heads.reserve(OVERLAY_CAP);
        for (ink, l) in self.layers.iter_mut().zip(&w.layers) {
            ink.pitch = l.pitch;
            ink.rows = l.rows;
            if w.params.depth > 0. || l.index == 1 {
                if ink.cells.len() != l.cells.len() {
                    ink.cells.clone_from(&l.cells);
                    ink.dirty.resize(l.cols.len(), Dirty::default());
                } else if previous_stamp + 1 != self.stamp
                    && ink
                        .dirty
                        .iter()
                        .zip(&l.cols)
                        .filter(|(old, col)| old.stamp != col.revision)
                        .count()
                        * 2
                        >= l.cols.len()
                {
                    // Pooled frames can miss several revisions. Once most
                    // columns changed, one contiguous copy is cheaper than
                    // hundreds of small full-column copies of the same ink.
                    ink.cells.copy_from_slice(&l.cells);
                } else {
                    // An evicted frame can be several ticks old. Columns which
                    // changed once need only the last dirty interval. A missed
                    // revision takes a full column, never a second copy of the field.
                    for (index, (old, col)) in ink.dirty.iter().zip(&l.cols).enumerate() {
                        if old.stamp == col.revision {
                            continue;
                        }
                        let base = index * l.rows;
                        let range = if previous_stamp + 1 == self.stamp {
                            base + col.lo as usize..base + col.hi as usize + 1
                        } else {
                            base..base + l.rows
                        };
                        ink.cells[range.clone()].copy_from_slice(&l.cells[range]);
                    }
                }
                for (d, c) in ink.dirty.iter_mut().zip(&l.cols) {
                    *d = Dirty {
                        stamp: c.revision,
                        lo: c.lo,
                        hi: c.hi,
                    };
                }
                l.heads(&mut self.heads);
            }
        }
        // The additive discharge crosses all three planes, including a head
        // that happens to pass in front of one of its recorded cells.
        self.heads.extend_from_slice(
            &w.layers[1].bolt_segments[..w.layers[1].bolt_segments.len().min(OVERLAY_CAP)],
        );
        self.bounds = w.input.layout.bounds;
        self.depth = w.params.depth;
        self.palette = w.input.palette;
        self.background = w.input.environment.bottom;
        self.sky_top = w.input.environment.top;
        self.background_id = w.input.environment.background;
        self.half = w.layers.each_ref().map(|l| l.half);
        self.colors = std::array::from_fn(|i| {
            let mut color = palette::color(w.input.palette, i);
            if w.input.palette == 5 {
                // Keep each front's palette hue and peak, but give pink,
                // peach and mint enough pigment to read against Midnight.
                let peak = color[..3].iter().copied().fold(0., f32::max);
                let low = color[..3].iter().copied().fold(1., f32::min);
                let gain = 0.36 * peak / (peak - low).max(0.001);
                for channel in &mut color[..3] {
                    *channel = peak - (peak - *channel) * gain;
                }
            }
            color
        });
        let t = w.tick as f32 / 30.;
        let lull = w.weather.moments[LULL];
        self.hush = lull.envelope(t, 3.);
        self.background[3] = lull.x;
        let sweep = w.weather.moments[SWEEP];
        self.sweep = [
            sweep.age(t) / (sweep.end - sweep.start),
            w.input.layout.bounds.height as f32,
        ];
        let strike = w.weather.moments[LIGHTNING];
        self.strike_age = strike.age(t);
        self.strike_x = strike.x;
        self.rhythm = w.weather.rhythm;
    }
}
#[repr(C)]
#[derive(Default)]
pub struct Uniforms {
    viewport: [f32; 4],
    grid: [f32; 4],
    section: [f32; 4],
    light: [f32; 4],
    weather: [f32; 4],
    strike: [f32; 4],
    returns: [f32; 4],
    sky_top: [f32; 4],
    background: [f32; 4],
    colors: [[f32; 4]; 6],
}
#[repr(C)]
pub struct LayerView {
    cells: *const Cell,
    dirty: *const Dirty,
    rows: u32,
    col_first: u32,
    col_count: u32,
    row_first: u32,
    row_count: u32,
    stamp: u32,
    still: u32,
    uniforms: Uniforms,
}
fn bound<'a>(l: &'a Lease, upper: bool, still: Option<&'a Frame>) -> Option<&'a Frame> {
    still.or_else(|| {
        (if upper {
            &l.sample.upper
        } else {
            &l.sample.lower
        })
        .typed
        .as_ref()?
        .downcast_ref()
    })
}
/// # Safety
/// All returned slice pointers are borrowed from the immutable sample or retained still.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_rain_layer(
    lease: *const Lease,
    still: *const Frame,
    index: usize,
    out: *mut LayerView,
) -> bool {
    let (Some(l), Some(result)) = (unsafe { lease.as_ref() }, unsafe { out.as_mut() }) else {
        return false;
    };
    let Some(f) = bound(l, true, unsafe { still.as_ref() }) else {
        return false;
    };
    let Some(ink) = f.layers.get(index) else {
        return false;
    };
    let v = l.view.viewport;
    let x = (v.x - f.bounds.x) as f32;
    let y = (v.y - f.bounds.y) as f32;
    let [pw, ph] = ink.pitch;
    let slice = |origin: f32, end: f32, pitch: f32, size: usize| {
        let start = ((origin / pitch).floor() as usize).min(size);
        (
            start,
            ((end / pitch).ceil() as usize)
                .min(size)
                .saturating_sub(start),
        )
    };
    let (c, n) = slice(x, x + v.width as f32, pw, ink.dirty.len());
    let (r, m) = slice(y, y + v.height as f32, ph, ink.rows);
    let offset = if f.still || Arc::ptr_eq(&l.sample.lower, &l.sample.upper) {
        0.
    } else {
        l.sample.alpha as f32 - 1.
    };
    let shader_tick = f.stamp as f32 + offset;
    let layer_alpha = [
        (f.depth / 0.60).min(1.) * if f.background_id == 0 { 0.50 } else { 0.35 },
        1.,
        0.45 * (f.depth / 0.60).min(1.),
    ][index];
    let flash = if f.still {
        0.
    } else {
        f.rhythm.lift(f.strike_age + offset / 30.)
    };
    *result = LayerView {
        cells: ink.cells.as_ptr(),
        dirty: ink.dirty.as_ptr(),
        rows: ink.rows as u32,
        col_first: c as u32,
        col_count: n as u32,
        row_first: r as u32,
        row_count: m as u32,
        stamp: f.stamp,
        still: f.still as u32,
        uniforms: Uniforms {
            viewport: [x, y, v.width as f32, v.height as f32],
            grid: [pw, ph, c as f32, r as f32],
            section: [shader_tick, f.half[index], layer_alpha, index as f32],
            light: [
                f.palette as f32,
                f.background_id as f32,
                if f.still || Arc::ptr_eq(&l.sample.lower, &l.sample.upper) {
                    1.
                } else {
                    l.sample.alpha as f32
                },
                0.,
            ],
            weather: [
                f.sweep[0] * (f.sweep[1] + 300.) - 150.,
                flash,
                f.depth,
                f.hush,
            ],
            strike: [
                f.strike_x,
                f.bounds.height as f32,
                if f.still {
                    1.
                } else {
                    f.rhythm.core(f.strike_age + offset / 30.)
                },
                f.strike_age + offset / 30.,
            ],
            returns: [
                0.120 + f.rhythm.times[0],
                0.120 + f.rhythm.times[1],
                0.120 + f.rhythm.times[2],
                f.rhythm.count as f32,
            ],
            sky_top: f.sky_top,
            background: f.background,
            colors: f.colors,
        },
    };
    true
}
/// # Safety
/// The caller owns capacity writable 48-byte records; zero capacity queries the size.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_rain_heads(
    lease: *const Lease,
    still: *const Frame,
    out: *mut Head,
    capacity: usize,
) -> usize {
    let Some(l) = (unsafe { lease.as_ref() }) else {
        return 0;
    };
    let Some(f) = bound(l, true, unsafe { still.as_ref() }) else {
        return 0;
    };
    let v = l.view.viewport;
    let x = (v.x - f.bounds.x) as f32;
    let y = (v.y - f.bounds.y) as f32;
    let mut count = 0;
    for h in &f.heads {
        let pad = h.ink[0].max(h.ink[1]) * if h.light[1] == 2. { 5. } else { 3. };
        if h.path[0].max(h.path[2]) + pad < x
            || h.path[0].min(h.path[2]) - pad > x + v.width as f32
            || h.path[1].max(h.path[3]) + pad < y
            || h.path[1].min(h.path[3]) - pad > y + v.height as f32
        {
            continue;
        }
        if count < capacity && !out.is_null() {
            unsafe { out.add(count).write(*h) };
        }
        count += 1;
    }
    count
}
/// # Safety
/// The returned still belongs to the caller until visuals_rain_release_still.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_rain_still(lease: *const Lease, epoch: u64) -> *mut Frame {
    let source = (unsafe { lease.as_ref() })
        .and_then(|l| bound(l, false, None))
        .and_then(|f| f.stills.as_ref());
    source.map_or(std::ptr::null_mut(), |s| {
        Arc::into_raw(s.photo(epoch)).cast_mut()
    })
}
/// # Safety
/// Called once for each non-null pointer returned by visuals_rain_still.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_rain_release_still(ptr: *mut Frame) {
    if !ptr.is_null() {
        drop(unsafe { Arc::from_raw(ptr) });
    }
}

#[cfg(test)]
mod polish_tests {
    use super::*;

    #[test]
    fn return_stroke_waits_for_the_leader_and_holds_a_sampled_peak() {
        assert_eq!(std::mem::size_of::<Uniforms>(), 240);
        for frame in 0..120 {
            let age = frame as f32 / 1000.;
            assert!((0.03..0.061).contains(&lightning_lift(age)));
            assert!(lightning_core(age) < 0.51);
        }
        for hz in [60, 100, 175, 240] {
            for phase in 0..100 {
                let phase = phase as f32 / (100. * hz as f32);
                let peak = (0..hz)
                    .map(|n| lightning_lift(n as f32 / hz as f32 + phase))
                    .fold(0., f32::max);
                assert!(
                    (peak - 0.30).abs() < 0.00001,
                    "{hz} Hz, phase {phase}: {peak}"
                );
            }
        }
        for (age, power) in [(0.120, 1.), (0.220, 0.65), (0.330, 0.40)] {
            assert!((lightning_lift(age) - 0.30 * power).abs() < 0.00001);
            assert!(lightning_core(age) > lightning_core(age - 0.001));
        }
        assert_eq!(lightning_lift(-0.01), 0.);
        assert_eq!(lightning_lift(1.2), 0.);
        assert_eq!(lightning_core(2.), 1.);
    }

    #[test]
    fn pastel_fronts_keep_their_own_hue_and_saturation() {
        let mut input = super::super::tests::input(11, false);
        input.palette = 5;
        let world = MatrixWorld::new(input);
        let mut frame = Frame::default();
        frame.update(&world);
        for (i, color) in frame.colors.iter().enumerate() {
            let original = palette::color(5, i);
            let peak = color[..3].iter().copied().fold(0., f32::max);
            let low = color[..3].iter().copied().fold(1., f32::min);
            let saturation = (peak - low) / peak;
            assert!(
                (0.30..=0.40).contains(&saturation),
                "front {i}: {saturation}"
            );
            // The channel ratios retain the source hue; no adjacent front or
            // neutral highlight can substitute for that front's body colour.
            let source_low = original[..3].iter().copied().fold(1., f32::min);
            let source_peak = original[..3].iter().copied().fold(0., f32::max);
            for channel in 0..3 {
                assert!(
                    ((peak - color[channel]) / (peak - low)
                        - (source_peak - original[channel]) / (source_peak - source_low))
                        .abs()
                        < 0.0001
                );
            }
        }
    }
}

#[cfg(test)]
#[path = "sampling_tests.rs"]
mod sampling_tests;
