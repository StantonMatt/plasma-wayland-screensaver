// SPDX-License-Identifier: GPL-3.0-or-later
use super::super::{
    common::layout::Rect,
    world::{DataTexture, WorldInput},
};
use super::render::{Quad, STAR};
use super::{Params, Pcg32, mix, noise, range, smooth, unit};
use crate::visuals::palette;
use std::sync::Arc;

pub(super) struct Sky {
    pub bounds: Rect,
    pub still_seed: Option<u64>,
    pub(super) outputs: Vec<Rect>,
    pub stars: Vec<Quad>,
    pub horizon: Vec<u8>,
    pub texture: Arc<DataTexture>,
    pub colors: [[f32; 4]; 6],
    pub top: [f32; 4],
    pub bottom: [f32; 4],
    pub style: [f32; 4],
    pub zenith: f32,
    pub exposure: f32,
    pub diffusion: f32,
    pub mirror: bool,
}
impl Sky {
    pub fn new(input: &WorldInput, params: &Params) -> Self {
        let mut outputs: Vec<_> = input.layout.outputs.iter().map(|o| o.rect).collect();
        outputs.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
        let mut r = Pcg32::new(input.seed, 0xa730);
        let b = input.layout.bounds;
        // The six roles always come from the selected product palette.
        let roles = [
            [5, 1, 4, 3, 2, 0],
            [0, 2, 3, 4, 3, 1],
            [0, 1, 2, 3, 4, 5],
            [0, 2, 1, 3, 5, 4],
            [0, 1, 2, 3, 2, 4],
            [0, 2, 1, 4, 0, 3],
        ];
        let mut sky = Self {
            bounds: b,
            still_seed: input.reduced_motion.then_some(input.seed),
            outputs,
            stars: vec![],
            horizon: vec![],
            texture: Arc::new(ray_texture(input.seed)),
            colors: roles[input.palette as usize].map(|i| palette::color(input.palette, i)),
            top: input.environment.top,
            bottom: input.environment.bottom,
            style: [
                input.trails as f32,
                input.density as f32 / 100.,
                input.palette as f32,
                input.scale as f32,
            ],
            // Horizontal optical spread in canonical pixels. An active sky
            // contains unresolved neighbouring rays across a broader region.
            diffusion: (5.85 + 10.45 * smooth((params.activity as f32 - 40.) / 25.))
                * b.height as f32
                / 1440.,
            exposure: if input.environment.background == 2 {
                1.10
            } else {
                1.0
            },
            zenith: b.x as f32 + range(&mut r, 0.30, 0.68) * b.width as f32,
            mirror: input.mode == super::super::world::MonitorMode::Mirrored,
        };
        let start = b.x as f32;
        let w = b.width as f32;
        let y = b.y as f32;
        let h = b.height as f32;
        let far_seed = u64::from(r.next_u32());
        let near_seed = u64::from(r.next_u32());
        if params.stars {
            // Extra margin lets the real sidereal rotation bring new stars into view.
            let n = ((b.area() / 8310.).round() as usize).clamp(65, 1900);
            for _ in 0..n {
                let mag = (r.unit() as f32).powi(3);
                let x = start + range(&mut r, -0.16, 1.16) * w;
                let cy = y + range(&mut r, -0.27, 0.93) * h;
                let radius = range(&mut r, 1.50, 1.95) + 1.22 * mag;
                let mut q = Quad::rect(x - radius, cy - radius, x + radius, cy + radius, STAR);
                q.field = [0., 1., 0.30 + 0.57 * mag, 0.30 + 0.57 * mag];
                q.material[1] = range(&mut r, 0., 10.);
                sky.stars.push(q);
            }
        }
        let mountains = params.horizon == "mountains";
        let count = ((w / if mountains { 5. } else { 1.5 }).ceil() as usize + 1).clamp(2, 4096);
        sky.horizon.resize(count * 8, 0);
        let encode = |v: f32| (((v - y) / h / 1.10).clamp(0., 1.) * 65535.).round() as u16;
        for i in 0..count {
            let x = start + w * i as f32 / (count - 1) as f32;
            let base = sky.baseline(x);
            let ridge = |seed, far| {
                if params.horizon == "none" {
                    return y + 1.10 * h;
                }
                if mountains {
                    let ridge = 0.035
                        + 0.07 * (1. - noise((x - start) / 330., seed).abs())
                        + 0.032 * noise(x / 83., seed + 1)
                        + 0.008 * noise(x / 19., seed + 2);
                    base - h
                        * (if far {
                            ridge * 0.98
                        } else {
                            ridge * 0.62 - 0.012
                        })
                } else {
                    let u = (x - start) / h;
                    let hills = 0.018 * noise(u * 6., seed) + 0.009 * noise(u * 21., seed + 1);
                    // Irregular spruce heights, narrow spires and lateral boughs.
                    let cell = u * 90.;
                    let mut canopy = 0.0f32;
                    for j in cell.floor() as i32 - 2..=cell.floor() as i32 + 2 {
                        let peak = j as f32 + 0.16 + 0.68 * unit(j, seed + 2);
                        let dx = (cell - peak).abs();
                        let tall = 0.005 + 0.023 * unit(j, seed + 3).powi(2);
                        let spread = 0.27 + 0.55 * unit(j, seed + 4);
                        let taper = (1. - dx / spread).max(0.);
                        let bough = 0.70 + 0.30 * (taper * 15.).sin().abs();
                        canopy = canopy.max(tall * taper * bough);
                    }
                    base + h * (if far { 0.001 } else { 0.020 }) - h * (hills + canopy)
                }
            };
            let index = i * 4;
            sky.horizon[index..index + 2]
                .copy_from_slice(&encode(ridge(far_seed, true)).to_be_bytes());
            sky.horizon[index + 2..index + 4]
                .copy_from_slice(&encode(ridge(near_seed, false)).to_be_bytes());
            sky.horizon[count * 4 + index..count * 4 + index + 2]
                .copy_from_slice(&encode(base).to_be_bytes());
        }
        sky
    }
    pub fn skyline(&self, x: f32) -> f32 {
        let count = self.horizon.len() / 8;
        let u = ((x - self.bounds.x as f32) / self.bounds.width as f32).clamp(0., 1.)
            * (count - 1) as f32;
        let column = u as usize;
        let sample =
            |i: usize| u16::from_be_bytes([self.horizon[4 * i], self.horizon[4 * i + 1]]) as f32;
        self.bounds.y as f32
            + mix(
                sample(column),
                sample((column + 1).min(count - 1)),
                u.fract(),
            ) * (1.10 * self.bounds.height as f32 / 65535.)
    }
    pub fn optical_margin(&self) -> f32 {
        3. * self.diffusion.max(16.3 * self.bounds.height as f32 / 1440.) + 2.
    }
    pub fn target_scale(&self) -> f32 {
        // The densest towering sky covers more pixels. Preserve detail on the
        // default and spend the smaller target only on those overlapping sheets.
        0.71 - 0.11 * smooth((self.style[1] - 0.5) * 2.) * smooth(self.style[3] - 1.)
    }
    pub fn baseline(&self, x: f32) -> f32 {
        let target = |r: Rect| (r.y + 0.91 * r.height) as f32;
        let mut base = target(self.outputs[0]);
        for pair in self.outputs.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let width = (a.width.min(b.width) * 0.075) as f32;
            let start = a.right().min(b.x) as f32 - width;
            let end = b.x.max(a.right()) as f32 + width;
            if x <= start {
                break;
            }
            base = mix(base, target(b), smooth((x - start) / (end - start)));
        }
        base
    }
}
fn ray_texture(seed: u64) -> DataTexture {
    // Tileable layered 1-D ray spectra. Different spatial scales evolve continuously in the shader.
    const W: usize = 512;
    const H: usize = 64;
    let mut bytes = Vec::with_capacity(W * H * 4);
    for row in 0..8 {
        let mut samples = [[0f32; 4]; W];
        let seed = seed.wrapping_add((row * 17) as u64);
        for (i, sample) in samples.iter_mut().enumerate() {
            let value = |cells: i32, key: u64| {
                let x = i as f32 / W as f32 * cells as f32;
                let a = x.floor() as i32;
                mix(
                    unit(a, seed.wrapping_add(key)),
                    unit((a + 1) % cells, seed.wrapping_add(key)),
                    smooth(x.fract()),
                )
            };
            let bands = 0.50 * value(7, 1) + 0.30 * value(16, 2) + 0.20 * value(29, 3);
            let fine = (0.66 * value(61, 4) + 0.34 * value(117, 5)).powi(2);
            let fine2 = (0.68 * value(43, 6) + 0.32 * value(97, 7)).powi(2);
            *sample = [bands, fine, fine2, value(13, 8)];
        }
        // An exposure pyramid along the material coordinate only. A fast surge
        // draws wider, softer rays; neighbouring curtains never bleed together.
        for level in 0..8 {
            for sample in &samples {
                bytes.extend(sample.map(|v| (255. * v).round() as u8));
            }
            if level == 7 {
                break;
            }
            let prior = samples;
            let step = 1 << level;
            for (i, s) in samples.iter_mut().enumerate() {
                *s = std::array::from_fn(|k| {
                    0.5 * prior[i][k]
                        + 0.25 * (prior[(i + W - step) % W][k] + prior[(i + step) % W][k])
                });
            }
        }
    }
    DataTexture {
        revision: 1,
        width: W as u32,
        height: H as u32,
        rgba: bytes.into(),
    }
}
