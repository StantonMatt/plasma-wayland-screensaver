use super::super::{common::rng::Pcg32, ffi::Lease, world::WorldInput};
use super::{
    World,
    fly::State,
    terrain::{Meadow, between, smooth},
};
use crate::visuals::palette::color;
use std::sync::Arc;

// One RGBA32F texel: xy, exposure, species + depth. Rows are shared by all windows.
pub type Texel = [f32; 4];
pub struct Art {
    pub hues: [Texel; 8],
    pub bottom: Texel,
    pub blades: Vec<Texel>,
    pub terrain: Vec<Texel>,
    pub scale: f32,
    pub pace: f32,
    pub pitch: f32,
    pub left: f32,
    pub palette: u32,
    pub still: bool,
    pub seed: f32,
}
impl Art {
    pub fn new(input: &WorldInput, meadow: &Meadow, rng: &mut Pcg32, grass: bool) -> Self {
        let mapping = match input.palette {
            1 => [1, 2, 3, 0, 4, 1, 5, 0],
            2 => [1, 0, 5, 2, 3, 1, 4, 3],
            3 => [1, 0, 4, 2, 3, 1, 5, 2],
            4 => [0, 4, 1, 2, 3, 1, 5, 2],
            5 => [2, 3, 1, 0, 4, 3, 5, 0],
            _ => [1, 5, 0, 3, 2, 1, 4, 3],
        };
        let mut blades = vec![];
        if grass {
            // Scatter each row through clumps of sedge, leaving openings in the bank.
            let count = (meadow.bounds.width / 5.8).ceil().clamp(5., 8192.) as usize;
            for row in 0..2 {
                for i in 0..count {
                    let x = meadow.bounds.x as f32
                        + (i as f32 + rng.unit() as f32) * meadow.bounds.width as f32
                            / count as f32;
                    let intervals = input.layout.vertical_intervals(x as f64);
                    let Some(interval) = intervals.last() else {
                        continue;
                    };
                    let h = (interval.end - interval.start) as f32;
                    let tuft = (x / (h * 0.033).max(8.)).sin() * 0.5 + 0.5;
                    let height = h
                        * (if row == 0 { 0.055 } else { 0.085 })
                        * between(rng, 0.45, 1.55)
                        * (0.65 + 0.52 * tuft);
                    let lean = between(rng, -0.43, 0.38) + 0.21 * (x * 0.027).cos();
                    blades.push([x, interval.end as f32, height, lean]);
                }
            }
        }
        let mut hues = mapping.map(|i| color(input.palette, i));
        // RGB also lights the grass. The shader uses the spare alpha to select
        // the palette's lantern optics independently of that reflected light.
        hues[0][3] = input.palette as f32;
        Self {
            hues,
            bottom: input.environment.bottom,
            blades,
            terrain: meadow
                .columns
                .iter()
                .map(|c| [c.floor, c.height, 0., 0.])
                .collect(),
            scale: input.scale as f32,
            pace: (input.speed as f32).powf(0.7),
            pitch: meadow.pitch,
            left: meadow.bounds.x as f32,
            palette: input.palette,
            still: input.reduced_motion,
            seed: between(rng, 0., 988.),
        }
    }
}
#[derive(Clone)]
pub struct Frame {
    pub heads: Arc<[Texel]>,
    pub grass: Arc<[Texel]>,
    pub history: Arc<Vec<Arc<[Texel]>>>,
    pub art: Arc<Art>,
    pub trail_count: usize,
    pub head_serial: u64,
    pub history_serial: u64,
    pub mid: usize,
}
impl World {
    pub(super) fn export(&mut self, history: bool) {
        let mut heads = Vec::with_capacity(self.flies.len() + self.distant.len() + self.rain.len());
        let mut grass = vec![[0.; 4]; self.meadow.columns.len()];
        let mut emissions = [0.; 64];
        let weather = self.weather;
        for (index, g) in grass.iter_mut().enumerate() {
            let x = self.meadow.bounds.x as f32 + (index as f32 + 0.5) * self.meadow.pitch;
            g[3] = self.wind[index / 4][0] * 0.055 + weather.gust(x) * 0.35;
        }
        for f in &self.flies {
            let kind = if f.tree.is_some() && self.tree_ready {
                13
            } else if f.state == State::Fading && f.exchanges > 0 {
                6
            } else if f.meal > 0. {
                7
            } else {
                f.species as usize
            };
            let light = f.light * (0.987 + 0.013 * (f.jitter + self.wind_phase * 9.).sin());
            heads.push([f.x, f.y, light, kind as f32 + f.z]);
        }
        for f in &self.distant {
            let (kind, z) = if f.near {
                (9. + f.tint as f32, 0.9)
            } else if f.tree.is_some() {
                (13., 0.46)
            } else {
                (4., f.z)
            };
            heads.push([f.x, f.y, f.light, kind + z]);
        }
        for r in &self.rain {
            heads.push([r[0], r[1], weather.rain() * 0.055, 8.5]);
        }
        for &[fx, fy, light, style] in &heads {
            if light < 0.008 {
                continue;
            }
            let kind = style as usize;
            let col = self.meadow.index(fx);
            let c = self.meadow.columns[col];
            let local = (c.height / 1440.).clamp(0.18, 3.);
            let radius = match kind {
                4 => 12.,
                9..=12 => 42.,
                8 => 6.,
                _ => 14. + 19. * style.fract(),
            } * self.art.scale;
            let rgb = self.art.hues[if kind == 13 {
                1
            } else if kind >= 9 {
                kind - 9
            } else {
                kind.min(7)
            }];
            let luma = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
            for (i, o) in self.meadow.outputs.iter().enumerate() {
                if fx + radius >= o.x as f32
                    && fx - radius < o.right() as f32
                    && fy + radius >= o.y as f32
                    && fy - radius < o.bottom() as f32
                {
                    // Integral bound for the analytic halo, including the desaturated core.
                    emissions[i] += light * (0.2 + 0.8 * luma) * radius * radius * 0.43;
                }
            }
            let strength = light * smooth(1. - (c.floor - fy) / (220. * local));
            if !self.art.blades.is_empty() && strength > 0.008 {
                let span = 1 + (90. * local / self.meadow.pitch).ceil() as usize;
                for (j, g) in grass
                    .iter_mut()
                    .enumerate()
                    .take(col + span + 1)
                    .skip(col.saturating_sub(span))
                {
                    let x = self.art.left + (j as f32 + 0.5) * self.art.pitch;
                    let k = strength * smooth(1. - (fx - x).abs() / (115. * local));
                    for ch in 0..3 {
                        g[ch] = (g[ch] + k * rgb[ch]).min(1.);
                    }
                }
            }
        }
        // Fade a whole population gently through the 30-second hush between nights.
        let hush = if self.params.nightCycle && self.night_age > self.night_length {
            1. - 0.98 * smooth((self.night_age - self.night_length) / 27.)
        } else if self.params.nightCycle && self.night > 0 {
            0.02 + 0.98 * smooth(self.night_age / 18.)
        } else {
            1_f32
        };
        self.emission = 0.;
        for (i, o) in self.meadow.outputs.iter().enumerate() {
            let e = emissions[i] / o.area() as f32;
            self.emission = self.emission.max(e);
        }
        for r in &mut heads {
            let presence = if matches!(r[3] as usize, 2 | 4) {
                hush.max(0.58)
            } else {
                hush
            };
            r[2] *= presence;
        }
        for g in &mut grass {
            for v in &mut g[..3] {
                *v *= hush;
            }
        }
        let heads: Arc<[Texel]> = heads.into();
        if self.trail_count > 0 {
            let ring = Arc::make_mut(&mut self.history);
            if ring[0].is_empty() {
                ring.fill(heads.clone());
                self.history_serial += ring.len() as u64;
            } else if !self.relocated.is_empty() {
                for row in ring.iter_mut() {
                    let samples = Arc::make_mut(row);
                    for &id in &self.relocated {
                        samples[id] = heads[id];
                    }
                }
                // Preserve the slot and invalidate rows cached by every output.
                self.history_serial += ring.len() as u64;
            }
            if history {
                self.history_serial += 1;
                let slot = self.history_serial as usize % ring.len();
                ring[slot] = heads.clone();
            }
        }
        self.relocated.clear();
        self.frame = Some(Arc::new(Frame {
            heads,
            grass: grass.into(),
            history: self.history.clone(),
            art: self.art.clone(),
            mid: self.flies.len(),
            trail_count: self.trail_count,
            head_serial: self.iteration,
            history_serial: self.history_serial,
        }));
    }
    pub(super) fn finish_exposure(&mut self) {
        let mut keep = vec![false; self.flies.len()];
        for area in &self.meadow.outputs {
            let mut paths = Vec::new();
            for (i, fly) in self.flies.iter().enumerate() {
                if fly.species != super::Species::Ghost
                    || fly.female
                    || !area.contains(super::super::common::layout::Vec2 {
                        x: fly.x as f64,
                        y: fly.y as f64,
                    })
                {
                    continue;
                }
                let mut extent = [
                    f32::INFINITY,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                    f32::NEG_INFINITY,
                ];
                for row in self.history.iter() {
                    let [x, y, light, _] = row[i];
                    if light > 0.10 {
                        extent[0] = extent[0].min(x);
                        extent[1] = extent[1].min(y);
                        extent[2] = extent[2].max(x);
                        extent[3] = extent[3].max(y);
                    }
                }
                if extent[0].is_finite() {
                    paths.push(((extent[2] - extent[0]).hypot(extent[3] - extent[1]), i));
                }
            }
            paths.sort_by(|a, b| b.0.total_cmp(&a.0));
            let n = (4. * area.area() / (2560. * 1440.)).round().clamp(1., 8.) as usize;
            for &(_, i) in paths.iter().take(n) {
                keep[i] = true;
            }
        }
        let ring = Arc::make_mut(&mut self.history);
        for row in ring.iter_mut() {
            let samples = Arc::make_mut(row);
            for (i, f) in self.flies.iter().enumerate() {
                if f.species == super::Species::Ghost && !keep[i] {
                    samples[i][2] = 0.;
                }
            }
        }
        self.history_serial += ring.len() as u64;
        self.export(false);
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Row {
    ptr: *const Texel,
    len: u32,
}
impl Row {
    fn from(t: &[Texel]) -> Self {
        Self {
            ptr: t.as_ptr(),
            len: t.len() as u32,
        }
    }
}
#[repr(C)]
pub struct GpuFrame {
    pub old: Row,
    pub new: Row,
    pub grass_lower: Row,
    pub grass: Row,
    pub blades: Row,
    pub terrain: Row,
    pub history: [Row; 61],
    pub lower_serial: u64,
    pub upper_serial: u64,
    pub history_serial: u64,
    pub mid: u32,
    pub segments: u32,
    pub ring_len: u32,
    pub still: u32,
    pub colours: [Texel; 8],
    pub bottom: Texel,
    pub art: Texel,
}
/// Borrow immutable GPU rows for the lifetime of this sample lease.
/// # Safety
/// The lease is live and output points to writable GpuFrame storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_fireflies_frame(lease: *const Lease, out: *mut GpuFrame) -> bool {
    let Some(lease) = (unsafe { lease.as_ref() }) else {
        return false;
    };
    let (Some(lo), Some(hi)) = (
        lease
            .sample
            .lower
            .typed
            .as_ref()
            .and_then(|a| a.downcast_ref::<Frame>()),
        lease
            .sample
            .upper
            .typed
            .as_ref()
            .and_then(|a| a.downcast_ref::<Frame>()),
    ) else {
        return false;
    };
    if out.is_null() {
        return false;
    }
    // The history must never contain a sample in the future of the interpolated head.
    let a = &lo.art;
    unsafe {
        out.write(GpuFrame {
            old: Row::from(&lo.heads),
            new: Row::from(&hi.heads),
            grass_lower: Row::from(&lo.grass),
            grass: Row::from(&hi.grass),
            blades: Row::from(&a.blades),
            terrain: Row::from(&a.terrain),
            history: std::array::from_fn(|i| {
                Row::from(lo.history.get(i).map_or(&[], |r| r.as_ref()))
            }),
            lower_serial: lo.head_serial,
            upper_serial: hi.head_serial,
            history_serial: lo.history_serial,
            mid: lo.mid as u32,
            segments: lo.trail_count as u32,
            ring_len: lo.history.len() as u32,
            still: u32::from(a.still),
            colours: a.hues,
            bottom: [a.bottom[0], a.bottom[1], a.bottom[2], a.pace],
            art: [
                a.scale,
                a.pitch,
                a.left,
                a.seed + if a.palette == 5 { 1024. } else { 0. },
            ],
        })
    };
    true
}
