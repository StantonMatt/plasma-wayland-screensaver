//! Digital Rain. Rust owns the ink, streams, weather and the photographed still.
mod field;
#[path = "../common/glyph_sdf.rs"]
mod glyph_sdf;
mod lightning;
mod render;
mod weather;
use super::{
    Descriptor, MirroredPolicy,
    common::{clock::Rate, rng::hash64},
    schema::Parameter,
    world::*,
};
use field::Layer;
use std::{cell::RefCell, sync::Arc};
use weather::*;
fn unit(bits: u64) -> f32 {
    ((bits >> 40) as u32) as f32 * (1. / 16_777_216.)
}
#[derive(Clone, Copy, Debug)]
pub struct Params {
    weather: u8,
    wind: f32,
    lightning: bool,
    glyphs: u8,
    depth: f32,
}
impl Default for Params {
    fn default() -> Self {
        Self {
            weather: 1,
            wind: 0.40,
            lightning: true,
            glyphs: 0,
            depth: 0.60,
        }
    }
}
impl Params {
    fn read(input: &WorldInput) -> Self {
        let mut p = Self::default();
        for v in &input.parameters {
            match v.id {
                1 => p.weather = v.payload as u8,
                2 => p.wind = v.payload as f32 / 100.,
                3 => p.lightning = v.payload != 0,
                4 => p.glyphs = v.payload as u8,
                5 => p.depth = v.payload as f32 / 100.,
                _ => {}
            }
        }
        p
    }
}
pub const PARAMETERS: &[Parameter] = &[
    Parameter::enumeration(1, c"weather", c"steady|changing|stormy", 2., 1.),
    Parameter::integer(2, c"wind", 0., 100., 40.),
    Parameter::boolean(3, c"lightning", true),
    Parameter::enumeration(
        4,
        c"glyphs",
        c"weather|numbers|hex|katakana|letters|lines-blocks|braille",
        6.,
        0.,
    ),
    Parameter::integer(5, c"depth", 0., 100., 60.),
];
pub const DESCRIPTOR: Descriptor = Descriptor {
    id: 6,
    key: "matrix",
    available: true,
    rate: 30,
    mirrored: MirroredPolicy::PerOutputLocal,
    fit: super::MirroredFit::Cover,
    parameters: PARAMETERS,
    create: |input| Some(Box::new(MatrixWorld::new(input))),
};
const PREWARM: u32 = 900;
// Keep the GPU data texture and its power-of-two geometry bounded at 2048 rows.
const OVERLAY_CAP: usize = 2000;
pub struct MatrixWorld {
    input: WorldInput,
    params: Params,
    weather: Weather,
    layers: [Layer; 3],
    landing: field::LandingStrip,
    tick: u64,
    photo_stamp: u32,
    photo: Option<Arc<render::Frame>>,
    last_lantern: u32,
    stills: Option<Arc<render::StillSource>>,
    frames: RefCell<Vec<Arc<render::Frame>>>,
    empty: Snapshot,
}
impl MatrixWorld {
    pub fn new(input: WorldInput) -> Self {
        let params = Params::read(&input);
        let layers = std::array::from_fn(|i| Layer::new(i, &input, &params));
        let weather = Weather::new(
            input.seed,
            input.layout.bounds.width as f32,
            input.speed as f32,
            &params,
        );
        let stills = input
            .reduced_motion
            .then(|| Arc::new(render::StillSource::new(input.clone())));
        let mut w = Self {
            stills,
            input,
            params,
            layers,
            landing: Default::default(),
            weather,
            tick: 0,
            photo_stamp: PREWARM,
            photo: None,
            last_lantern: 0,
            frames: RefCell::default(),
            empty: Snapshot::default(),
        };
        // A coarse, deterministic 30 s preroll covers slow profiles and the frozen
        // composition. Traversed cells are all written, even on a coarse step.
        let mut best = f32::MAX;
        let end = if w.input.reduced_motion {
            35 * 30
        } else {
            PREWARM
        };
        for stamp in (5..=end).step_by(5) {
            let t = (stamp as f32 - PREWARM as f32) / 30.;
            w.advance(stamp, t, 5);
            if w.input.reduced_motion && stamp >= 25 * 30 && stamp.is_multiple_of(15) {
                let layer = &w.layers[1];
                let mut occupied = vec![false; layer.cols.len()];
                for head in &layer.streams {
                    occupied[head.col] = true;
                }
                let duty = occupied.iter().filter(|x| **x).count() as f32 / occupied.len() as f32;
                let score = (duty - 0.40).abs();
                if score < best {
                    best = score;
                    w.photo_stamp = stamp;
                    let mut frame = render::Frame::default();
                    frame.update(&w);
                    w.photo = Some(Arc::new(frame));
                }
            }
        }
        for l in &mut w.layers {
            for s in &mut l.streams {
                s.previous = s.position;
            }
        }
        w
    }
    fn advance(&mut self, stamp: u32, t: f32, stride: u32) {
        self.weather.step(t, &self.params);
        let dt = stride as f32 / 30.;
        let lantern = self.weather.moments[LANTERN];
        let hush = self.weather.moments[LULL];
        // Step independent planes in budget priority order so each can mask
        // its selected heads immediately, while its ink is still cache-hot.
        // A growing strike reserves its bounded worst case until its exact
        // segment count is known; established strikes reserve their real size.
        let age = self.weather.moments[LIGHTNING].age(t);
        let bolt_budget = if age >= 6.5 {
            0
        } else if (0. ..0.14 + dt).contains(&age) {
            self.layers[1].rows + 13
        } else {
            self.layers[1].bolt_segments.len()
        };
        let mut remaining = OVERLAY_CAP.saturating_sub(bolt_budget);
        for index in [2, 1, 0] {
            let layer = &mut self.layers[index];
            if stamp <= stride || stamp % 30 < stride {
                layer.weather(&self.weather, stamp);
            }
            if layer.index == 1 {
                let pitch = layer.pitch[0];
                let last = layer.cols.len() - 1;
                let col = |x: f32| ((x / pitch) as usize).min(last);
                if self.last_lantern != self.weather.counts[LANTERN] {
                    self.last_lantern = self.weather.counts[LANTERN];
                    layer.spawn(col(lantern.x), lantern.key, true, true, &self.params);
                }
                if !self.weather.last_lull_drop
                    && t >= hush.end - (layer.rows as f32 / (12. * layer.speed())).clamp(2., 9.)
                {
                    layer.spawn(col(hush.x), hush.key, false, true, &self.params);
                    self.weather.last_lull_drop = true;
                }
            }
            layer.step_ink(stamp, dt, &self.weather, &self.params);
            layer.overlay_limit = remaining.min(layer.overlay_count());
            remaining -= layer.overlay_limit;
            layer.mask_heads(stamp);
            if layer.index == 1 {
                let bolt = self.weather.moments[LIGHTNING];
                layer.bolt(bolt, bolt.age(t), dt, stamp);
            }
        }
        self.landing.settle(&mut self.layers, stamp);
        self.layers[1].mask_bolts(stamp);
    }
}
impl LivingWorld for MatrixWorld {
    fn rate(&self) -> Rate {
        Rate::new(30, 1).unwrap()
    }
    fn step(&mut self, tick: u64) {
        self.tick = tick;
        if !self.input.reduced_motion {
            self.advance((tick as u32).wrapping_add(PREWARM), tick as f32 / 30., 1);
        }
    }
    fn snapshot(&self) -> Snapshot {
        let frame = self.photo.clone().unwrap_or_else(|| {
            let mut pool = self.frames.borrow_mut();
            let slot = pool
                .iter()
                .position(|v| Arc::strong_count(v) == 1)
                .unwrap_or_else(|| {
                    pool.push(Arc::new(render::Frame::default()));
                    pool.len() - 1
                });
            Arc::get_mut(&mut pool[slot]).unwrap().update(self);
            pool[slot].clone()
        });
        Snapshot {
            tick: self.tick,
            time_ns: self.rate().tick_ns(self.tick),
            generations: Generations {
                layout: self.input.layout.generation,
                data: if self.input.reduced_motion {
                    1
                } else {
                    self.tick + 1
                },
                sample: self.tick,
                ..Generations::default()
            },
            emission_estimate: self.emission_estimate(),
            typed: Some(frame),
            ..self.empty.clone()
        }
    }
    fn emission_estimate(&self) -> f64 {
        let age = self.weather.moments[LIGHTNING].age(self.tick as f32 / 30.);
        let wet_area: f64 = self
            .layers
            .iter()
            .map(|layer| {
                (layer.streams.len() as f64)
                    * f64::from(layer.pitch[0] * layer.pitch[1])
                    * [0.18, 0.7, 0.35][layer.index]
            })
            .sum();
        // Endpoint diagnostics report the unbounded rain estimate and the
        // same strike envelope used by the renderer; they do not limit light.
        wet_area / self.input.layout.visible_area
            + if self.input.reduced_motion {
                0.
            } else {
                f64::from(self.weather.rhythm.lift(age))
            }
    }
    fn inject(&mut self, mut e: Event) -> Result<(), &'static str> {
        e.anchor[0] -= self.input.layout.bounds.x;
        self.weather.inject(e)
    }
    fn state_hash(&self) -> u64 {
        let mut h = hash64(self.input.seed);
        for l in &self.layers {
            for c in &l.cells {
                let id = u16::from_le_bytes(c.glyph) as u64;
                h = hash64(
                    h ^ u32::from_le_bytes(c.stamp) as u64
                        ^ (id << 32)
                        ^ ((c.style as u64) << 48)
                        ^ ((c.power as u64) << 56),
                );
            }
            for s in &l.streams {
                h = hash64(h ^ s.y.to_bits() as u64 ^ ((s.col as u64) << 32));
            }
        }
        h
    }
}
#[cfg(test)]
mod tests;
