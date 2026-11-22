// SPDX-License-Identifier: GPL-3.0-or-later
//! Immutable projected endpoints. Strips are culled by all four projected corners,
//! including converging tops, so a fold can cross a seam in either direction.
use super::super::ffi::{Lease, View};
use super::{
    curtain::{Curtain, KM_PER_PIXEL},
    landscape::Sky,
    mix, smooth,
    weather::Weather,
};
use std::sync::Arc;
pub(super) const STAR: f32 = 1.;
pub(super) const CURTAIN: f32 = 2.;
pub(super) const GHOST: f32 = 3.;
const PATCH: f32 = 4.;
const METEOR: f32 = 5.;
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Quad {
    pub left: [f32; 4],
    pub right: [f32; 4],
    pub field: [f32; 4],
    pub material: [f32; 4],
    pub tangent_left: [f32; 4],
    pub tangent_right: [f32; 4],
    pub optics: [f32; 4],
}
impl Quad {
    pub(super) fn rect(x: f32, y: f32, x1: f32, y1: f32, kind: f32) -> Self {
        Self {
            left: [x, y, x, y1],
            right: [x1, y, x1, y1],
            field: [0., 1., 1., 1.],
            material: [kind, 0., 0., 0.],
            ..Self::default()
        }
    }
    /// Evaluate the centreline at a material parameter for the geometry checks.
    #[cfg(test)]
    pub(super) fn column(&self, u: f32) -> [f32; 4] {
        let v = 1. - u;
        std::array::from_fn(|k| {
            if self.material[0] == CURTAIN || self.material[0] == GHOST {
                (1. + 2. * u) * v * v * self.left[k]
                    + (3. - 2. * u) * u * u * self.right[k]
                    + u * v * v * self.tangent_left[k]
                    - u * u * v * self.tangent_right[k]
            } else {
                mix(self.left[k], self.right[k], u)
            }
        })
    }
    // A Hermite curve stays inside the hull of its Bezier handles.
    fn visible(&self, l: f32, r: f32) -> bool {
        let xs = [self.left[0], self.left[2], self.right[0], self.right[2]];
        let handles = [
            self.left[0] + self.tangent_left[0] / 3.,
            self.left[2] + self.tangent_left[2] / 3.,
            self.right[0] - self.tangent_right[0] / 3.,
            self.right[2] - self.tangent_right[2] / 3.,
        ];
        !(xs.iter().chain(&handles).all(|&x| x < l) || xs.iter().chain(&handles).all(|&x| x > r))
    }
}
#[derive(Clone, Copy, Default)]
struct Column {
    pos: [f32; 4],
    memory: [f32; 4],
    u: f32,
    power: f32,
    surge: f32,
    tangent: [f32; 4],
    memory_tangent: [f32; 4],
}
#[derive(Default)]
struct Ribbon {
    drift: f64,
    afterglow: f32,
    id: u32,
    retiring: bool,
    nodes: Vec<Column>,
    envelope: Vec<[f32; 4]>,
    row: f32,
    detail: f32,
    tint: f32,
}
impl Ribbon {
    fn finish_curve(
        &mut self,
        radius: f32,
        pitch: f32,
        amplitude: f32,
        wave_number: f32,
        phase: f32,
    ) {
        // Bound the hem slope before fitting cubic tangents. Two one-sided
        // envelopes avoid a directional bias at a fold. The same screen-space
        // radius smooths the two sides, including the returning side of a curl.
        let n = self.nodes.len();
        for ghost in [false, true] {
            if ghost && self.afterglow <= 0. {
                continue;
            }
            let pos = |c: &Column| if ghost { c.memory } else { c.pos };
            // A sheet has finite thickness at grazing incidence. Keep both
            // projected edges ordered as the physical ribbon turns through
            // the line of sight. Opposing sweeps share the correction.
            self.envelope.clear();
            self.envelope.extend(self.nodes.iter().map(|c| {
                let p = pos(c);
                [p[0], p[2], p[0], p[2]]
            }));
            for i in 1..n {
                let previous = self.envelope[i - 1];
                let current = &mut self.envelope[i];
                current[0] = current[0].max(previous[0] + pitch);
                current[1] = current[1].max(previous[1] + pitch);
            }
            for i in (0..n - 1).rev() {
                let next = self.envelope[i + 1];
                let current = &mut self.envelope[i];
                current[2] = current[2].min(next[2] - pitch);
                current[3] = current[3].min(next[3] - pitch);
            }
            for (node, e) in self.nodes.iter_mut().zip(&self.envelope) {
                let p = if ghost {
                    &mut node.memory
                } else {
                    &mut node.pos
                };
                p[0] = 0.5 * (e[0] + e[2]);
                p[2] = 0.5 * (e[1] + e[3]);
            }
            let hem = |c: &Column| {
                let p = pos(c);
                let u = if ghost { 0.12 / 0.39 } else { 0.12 };
                [mix(p[0], p[2], u), mix(p[1], p[3], u)]
            };
            self.envelope.clear();
            self.envelope.extend(self.nodes.iter().map(|c| {
                let [x, y] = hem(c);
                [x, y, y, y]
            }));
            for rev in [false, true] {
                for k in 1..n {
                    let (i, j) = if rev { (n - k - 1, n - k) } else { (k, k - 1) };
                    let a = self.envelope[j];
                    let p = &mut self.envelope[i];
                    let limit = 1.65 * (p[0] - a[0]).abs();
                    p[1] = p[1].min(a[1] + limit);
                    p[2] = p[2].max(a[2] - limit);
                }
            }
            // A symmetric exponential footprint rounds the tangent over at least
            // 40 logical pixels at 1440p; cubic subdivision fills each interval.
            let mut y = 0.5 * (self.envelope[0][1] + self.envelope[0][2]);
            for i in 0..n {
                let dx = if i == 0 {
                    radius
                } else {
                    (self.envelope[i][0] - self.envelope[i - 1][0]).abs()
                };
                let p = &mut self.envelope[i];
                y = mix(y, 0.5 * (p[1] + p[2]), dx / (dx + radius));
                p[3] = y;
            }
            y = self.envelope[n - 1][3];
            for i in (0..n).rev() {
                let dx = if i + 1 == n {
                    radius
                } else {
                    (self.envelope[i][0] - self.envelope[i + 1][0]).abs()
                };
                y = mix(y, self.envelope[i][3], dx / (dx + radius));
                let [foot_x, old_y] = hem(&self.nodes[i]);
                // The perspective projection already carries the altitude wave.
                // Add a transverse wave where longitudinal tension flattens it.
                // Centre this displacement on the emitting hem inside the strip.
                let delta = y - old_y + amplitude * (foot_x * wave_number + phase).sin();
                let p = if ghost {
                    &mut self.nodes[i].memory
                } else {
                    &mut self.nodes[i].pos
                };
                p[1] += delta;
                p[3] += delta;
            }
            for i in 0..n {
                let a = pos(&self.nodes[i.saturating_sub(1)]);
                let b = pos(&self.nodes[(i + 1).min(n - 1)]);
                let c = pos(&self.nodes[i]);
                let tangent = std::array::from_fn(|k| {
                    if k % 2 == 0 && i > 0 && i + 1 < n {
                        // Limit the cubic's horizontal slope through unequal
                        // intervals so a rounded turn cannot fold inside out.
                        2. * (c[k] - a[k]) * (b[k] - c[k]) / (b[k] - a[k]).max(0.001)
                    } else {
                        (b[k] - a[k]) * 0.5
                    }
                });
                if ghost {
                    self.nodes[i].memory_tangent = tangent
                } else {
                    self.nodes[i].tangent = tangent
                }
            }
        }
    }
}
fn north_distance(z: f32) -> f32 {
    let d = z - 32.;
    32. + 0.5 * (d + (d * d + 40. * 40.).sqrt())
}
pub(super) struct Frame {
    pub sky: Arc<Sky>,
    ribbons: Vec<Ribbon>,
    effects: Vec<Quad>,
    overlap: [[f32; 4]; 8],
    pub time: f64,
}
impl Frame {
    /// Project the illuminated portions of the sheets into column bands. Counts
    /// are fractional at a fading edge, so both overlap and coverage vary smoothly.
    fn sheet_counts<const X: usize, const Y: usize>(
        &self,
        bounds: super::super::common::layout::Rect,
    ) -> [[f32; Y]; X] {
        let mut grid = [[0.; Y]; X];
        for (i, column) in grid.iter_mut().enumerate() {
            let x = (bounds.x + (i as f64 + 0.5) * bounds.width / X as f64) as f32;
            for arc in &self.ribbons {
                let mid = |c: &Column| mix(c.pos[0], c.pos[2], 0.42);
                let at = arc.nodes.partition_point(|node| mid(node) < x);
                if at == 0 || at == arc.nodes.len() {
                    continue;
                }
                let a = &arc.nodes[at - 1];
                let b = &arc.nodes[at];
                let u = (x - mid(a)) / (mid(b) - mid(a)).max(0.001);
                let hem = |c: &Column| mix(c.pos[1], c.pos[3], 0.12);
                let foot = mix(hem(a), hem(b), u);
                let height = (foot - mix(a.pos[3], b.pos[3], u)).max(1.);
                let visible = smooth((mix(a.power, b.power, u) - 0.025) / 0.20);
                for (j, count) in column.iter_mut().enumerate() {
                    let y = (bounds.y + (j as f64 + 0.5) * bounds.height / Y as f64) as f32;
                    let v = (foot - y) / height;
                    *count += visible * smooth((v + 0.045) / 0.09) * smooth((0.96 - v) / 0.30);
                }
            }
        }
        grid
    }

    pub(super) fn lit_share(&self) -> f32 {
        let mut lit = 0.;
        let mut area = 0.;
        for output in &self.sky.outputs {
            let grid = self.sheet_counts::<24, 24>(*output);
            let filled = grid.iter().flatten().filter(|&&count| count > 0.45).count();
            lit += output.area() * filled as f64 / (24. * 24.);
            area += output.area();
        }
        (lit / area.max(1.)) as f32
    }

    pub(super) fn flare_site(
        &self,
        outputs: &[super::super::common::layout::OutputRect],
        preferred: f32,
    ) -> Option<(u32, f32, f32)> {
        let mut best = None;
        for output in outputs {
            let rect = output.rect;
            let margin = (rect.width * 0.19).min(280.) as f32;
            let lo = rect.x as f32 + margin;
            let hi = rect.right() as f32 - margin;
            for r in self.ribbons.iter().filter(|r| !r.retiring) {
                for nodes in r.nodes.windows(2) {
                    let hem =
                        |c: &Column| [mix(c.pos[0], c.pos[2], 0.12), mix(c.pos[1], c.pos[3], 0.12)];
                    let [x0, y0] = hem(&nodes[0]);
                    let [x1, y1] = hem(&nodes[1]);
                    let l = lo.max(x0.min(x1));
                    let h = hi.min(x0.max(x1));
                    if l > h || (x1 - x0).abs() < 0.001 {
                        continue;
                    }
                    let x = preferred.clamp(l, h);
                    let u = (x - x0) / (x1 - x0);
                    let y = mix(y0, y1, u);
                    let height = (f64::from(y) - rect.y) / rect.height;
                    let power = mix(nodes[0].power, nodes[1].power, u);
                    if !(0.29..0.86).contains(&height)
                        || power < 0.10
                        || y > self.sky.skyline(x) - (rect.height as f32 * 0.04).min(55.)
                    {
                        continue;
                    }
                    let score = height + 0.15 * f64::from(power.min(1.))
                        - f64::from((preferred - x).abs()) / 300.;
                    if best.is_none_or(|(prior, _, _, _)| score > prior) {
                        best = Some((score, r.id, x, y));
                    }
                }
            }
        }
        best.map(|(_, id, x, y)| (id, x, y))
    }
    #[cfg(test)]
    pub(super) fn max_material_step(&self, previous: &Self) -> f32 {
        let mut max = 0.0f32;
        for r in &self.ribbons {
            if let Some(old) = previous.ribbons.iter().find(|p| p.id == r.id) {
                assert_eq!(old.nodes.len(), r.nodes.len());
                for (a, b) in old.nodes.windows(2).zip(r.nodes.windows(2)) {
                    for ghost in [false, true] {
                        let quad = |c: &[Column]| {
                            let (lo, hi) = (c[0], c[1]);
                            Quad {
                                left: if ghost { lo.memory } else { lo.pos },
                                right: if ghost { hi.memory } else { hi.pos },
                                tangent_left: if ghost { lo.memory_tangent } else { lo.tangent },
                                tangent_right: if ghost { hi.memory_tangent } else { hi.tangent },
                                material: [CURTAIN, 0., 0., 0.],
                                ..Quad::default()
                            }
                        };
                        let (a, b) = (quad(a), quad(b));
                        let count = super::mesh::subdivisions(&a, 1.)
                            .max(super::mesh::subdivisions(&b, 1.));
                        for step in 0..=count {
                            let u = step as f32 / count as f32;
                            let (p, q) = (a.column(u), b.column(u));
                            for k in [0, 2] {
                                max = max.max((p[k] - q[k]).hypot(p[k + 1] - q[k + 1]));
                            }
                        }
                    }
                }
            }
        }
        max
    }
    pub fn new(sky: Arc<Sky>) -> Self {
        Self {
            sky,
            ribbons: vec![],
            effects: Vec::with_capacity(16),
            overlap: [[0.; 4]; 8],
            time: 0.,
        }
    }
    pub fn update(&mut self, arcs: &[Curtain], weather: &Weather, pace: f32) {
        self.time = weather.time;
        let sky = &self.sky;
        let b = sky.bounds;
        let h = b.height as f32;
        let left = b.x as f32;
        self.ribbons.resize_with(arcs.len(), Ribbon::default);
        let crown = weather.crown.map(|c| {
            let age = (weather.time - c.born) as f32;
            (
                c.arc,
                c.x,
                c.power * smooth(age / 11.) * smooth((c.life - age) / 15.),
            )
        });
        let storm = weather.storm.map(|s| {
            let age = ((weather.time - s.start) as f32) / pace;
            (
                s.arc,
                s.origin,
                s.origin - (age - 2.3).max(0.) * s.speed,
                smooth(age / 1.8) * s.power * smooth((s.expansion - age * pace) / 12.),
                smooth((6. - age) / 3.7),
            )
        });
        // In a crowded sky the ground-plane folds retain their depth, while
        // increased longitudinal tension keeps many old sheets from doubling
        // back across the same line of sight to form opaque walls.
        let tension = 0.30 + 0.48 * smooth((sky.style[1] - 0.65) / 0.35);
        for (arc, ribbon) in arcs.iter().zip(&mut self.ribbons) {
            ribbon.nodes.clear();
            ribbon.id = arc.id;
            ribbon.retiring = arc.retiring();
            ribbon.drift = arc.drift;
            ribbon.afterglow = if sky.style[0] > 0. && arc.expansion.is_none() {
                smooth((435. - arc.mean) / 230.)
            } else {
                0.
            };
            let material_speed = pace * (15. + 44. * weather.activity + 60. * weather.expansion);
            let exposure = (material_speed / 24.).max(1.).log2().min(7.);
            ribbon.row = arc.texture_row as f32 * 8. + exposure;
            ribbon.detail = (0.68 + 0.45 * arc.sharp)
                * (0.45 + 0.55 * smooth(weather.activity / 0.40))
                * (1. + 0.50 * weather.expansion);
            ribbon.tint = if sky.style[2] == 5. {
                (arc.texture_row % 3) as f32 / 2.
            } else {
                arc.variation
            };
            let (zenith, corona) = crown
                .filter(|c| c.0 == arc.id)
                .map_or((sky.zenith, 0.), |(_, x, strength)| (x, strength));
            let surge = storm.filter(|s| s.0 == arc.id);
            let shift = weather.growth * 10. + weather.expansion * 8.;
            let mut along = 0.;
            let mut prev = arc.sheet[0];
            let cover = arc.sheet.len() as f32 - 1.;
            let temporal = 0.83 + 0.17 * (self.time * 0.030 + f64::from(arc.phase)).cos() as f32;
            for (i, node) in arc.sheet.iter().enumerate() {
                let x = left + mix(node.x, node.rest_x, tension) / KM_PER_PIXEL;
                let base = sky.baseline(x);
                let fraction = i as f32 / cover;
                let onset = surge.map_or(0., |(_, anchor, front, power, stationary)| {
                    let d = x - front;
                    let core = (-0.5 * (d / 142.).powi(2)).exp();
                    let tail = if d > 0. {
                        (1. - core) * (-d / 590.).exp() * 0.47
                    } else {
                        0.
                    };
                    let flare = (-0.5 * ((x - anchor) / 218.).powi(2)).exp() * stationary;
                    (core + tail).max(flare) * power
                });
                let depth = if arc.expansion.is_some() {
                    42. + 0.70 * north_distance(node.z - 42.)
                } else {
                    north_distance(node.z - shift - 8. * onset)
                };
                let z = mix(depth, 57. + 0.13 * depth, corona);
                let hem_alt = 101. + 5. * (fraction * 15. + arc.phase).sin();
                let altitude = hem_alt
                    + 160.
                        * sky.style[3]
                        * (1. + 0.42 * (sky.style[3] - 1.).max(0.))
                        * (0.85 + 0.22 * arc.variation + 0.4 * weather.expansion);
                let project = |x: f32, z: f32, base: f32| {
                    let elevation = base - b.y as f32;
                    let ordinary =
                        base - elevation * std::f32::consts::FRAC_2_PI * (hem_alt / z).atan();
                    // At grazing distances the rays meet the ground haze and end
                    // behind the trees, breaking up a straight luminous floor.
                    let ordinary = mix(
                        ordinary,
                        base + 0.0056 * elevation,
                        smooth((z - 660.) / 200.),
                    );
                    // A travelling breakup lifts its lower edge out of the
                    // ground haze where a mountain would eclipse the surge.
                    let lifted = if onset > 0.001 {
                        mix(ordinary, ordinary.min(sky.skyline(x) - 0.065 * h), onset)
                    } else {
                        ordinary
                    };
                    let top = if arc.expansion.is_some() {
                        // Tall newly precipitating rays can extend through the zenith.
                        b.y as f32 - 0.13 * h + 0.09 * h * (x / 840. + arc.phase).sin()
                    } else {
                        base - elevation * std::f32::consts::FRAC_2_PI * (altitude / z).atan()
                    };
                    // Look obliquely along the geomagnetic field. Both sides
                    // converge continuously on one magnetic meridian. Far
                    // sheets project the shortest, least-inclined rays.
                    let toward = sky.zenith - x;
                    let lean = (0.31 + 0.32 / (1. + z / 210.)) * toward / toward.hypot(0.085 * h);
                    let quiet = x + lean * (lifted - top);
                    let (hem, cap, tip) = if corona > 0. {
                        let arc_height =
                            0.23 + 0.32 * (1. - (-((x - zenith) / (1.03 * h)).powi(2)).exp());
                        let hem = mix(lifted, b.y as f32 + h * arc_height, corona);
                        let cap = mix(top, b.y as f32 + h * 0.055, corona);
                        let height = (hem - cap).max(1.);
                        let slant = (zenith - x) / (height * 1.4);
                        (
                            hem,
                            cap,
                            mix(
                                quiet,
                                x + 0.67 * height * slant / (1. + slant.abs()),
                                corona,
                            ),
                        )
                    } else {
                        (lifted, top, quiet)
                    };
                    // The bottom 12% of the strip makes room for the hem halo.
                    [
                        x - (tip - x) * 0.13636,
                        hem + (hem - cap) * 0.13636,
                        tip,
                        cap,
                    ]
                };
                let pos = project(x, z, base);
                let memory = if sky.style[0] > 0. && ribbon.afterglow > 0. {
                    let mx = left + mix(node.memory_x, node.rest_x, tension) / KM_PER_PIXEL;
                    let mut p =
                        project(mx, north_distance(node.memory_z - shift), sky.baseline(mx));
                    // Lingering oxygen emission is confined to the older lower hem.
                    p[2] = mix(p[0], p[2], 0.39);
                    p[3] = mix(p[1], p[3], 0.39);
                    p
                } else {
                    pos
                };
                along += (node.x - prev.x).hypot(node.z - prev.z) / KM_PER_PIXEL;
                prev = *node;
                // Only two mature sheets form sharp low hems. As another
                // fold sinks into this layer its emission dissolves into haze.
                let ground = if arc.ground_hem {
                    1.
                } else {
                    let hem = mix(pos[1], pos[3], 0.12);
                    let relative = (hem - b.y as f32) / (base - b.y as f32);
                    1. - smooth((relative - 0.68) / 0.08)
                        * (1. - smooth((weather.activity - 0.4) / 0.3))
                };
                let extinction = ground
                    * (0.47 + 0.53 / (1. + (z / 315.).powi(2)))
                    * (1. - 0.48 * smooth((z - 590.) / 230.));
                let p = arc.sheet[i.saturating_sub(1)];
                let q = arc.sheet[(i + 1).min(arc.sheet.len() - 1)];
                let dx = mix(q.x - p.x, q.rest_x - p.rest_x, tension);
                let dz = q.z - p.z;
                // Optical path through an edge-on sheet; clamp to finite thickness.
                let normal = dx / dx.hypot(dz).max(0.001);
                let path = 1. / normal.abs().max(0.33334);

                let back = 0.70 + 0.30 * smooth((normal + 0.2) / 0.4);
                // Keep depth in the quiet sky and leave the lower sheets
                // display headroom for a local release of storm energy.
                let quiet = 1. - 0.57 * smooth((z - 100.) / 112.);
                let excitation = 0.5 * onset;
                let power = sky.exposure
                    * (1. + 0.08 * weather.growth)
                    * path
                    * back
                    * arc.light
                    * arc.gain
                    * extinction
                    * temporal
                    * (quiet * (0.94 + 0.12 * weather.expansion + 0.38 * corona)
                        + 6.7 * excitation);
                // A vortex can temporarily reverse local material advection.
                let slip = (node.x - node.memory_x) * 4.0;
                let u = along / (1390. - 360. * sky.style[1]) + slip / 280. + arc.phase;
                ribbon.nodes.push(Column {
                    pos,
                    memory,
                    u,
                    power,
                    surge: excitation,
                    ..Column::default()
                });
            }
            let distance_per_node = (along / cover).max(1.);
            let crowding = smooth((sky.style[1] - 0.60) / 0.25);
            let amplitude = mix(6., 12., crowding) * h / 1440.;
            let wavelength = (mix(600., 408., crowding) + 32. * arc.variation) * h / 1440.;
            ribbon.finish_curve(
                48. * h / 1440.,
                0.32 * distance_per_node,
                amplitude,
                std::f32::consts::TAU / wavelength,
                arc.phase + (self.time * 0.012) as f32 + std::f32::consts::FRAC_PI_2,
            );
            let half_width = ((50. * h / 1440.) / distance_per_node).ceil().max(1.) as usize;
            for i in 0..ribbon.nodes.len() {
                let a = i.saturating_sub(half_width);
                let b = (i + half_width).min(ribbon.nodes.len() - 1);
                let left = ribbon.nodes[a].pos;
                let right = ribbon.nodes[b].pos;
                let distance = (b - a) as f32 * distance_per_node;
                let width = (right[0] - left[0]).abs().min((right[2] - left[2]).abs());
                let transmission = smooth(width / (0.46 * distance));
                let end_fade = if arc.partial {
                    let edge = (i.min(ribbon.nodes.len() - 1 - i)) as f32 * distance_per_node;
                    smooth(edge / (0.14 * along).max(150.))
                } else {
                    1.
                };
                ribbon.nodes[i].power *= transmission * end_fade;
            }
        }
        if sky.style[2] == 5. && sky.style[1] > 0.6 && sky.style[3] > 1. {
            self.overlap = self.sheet_counts(sky.bounds);
        }
        self.effects.clear();
        if let Some(s) = weather.storm {
            let age = (weather.time - s.start) as f32;
            for (i, p) in weather.patches.iter().enumerate() {
                if weather.recovery <= 0.0001 {
                    break;
                }
                let period = p.period.max(3.5 * pace);
                let edge = (0.65 * pace / period).max(0.13);
                let cycle = (age / period + p.phase).rem_euclid(1.);
                let pulse = smooth(cycle / edge) * smooth((p.duty + edge - cycle) / edge);
                let x = left + p.x + age * 0.35;
                let y = b.y as f32 + p.y;
                let mut q = Quad::rect(x - p.width, y - p.height, x + p.width, y + p.height, PATCH);
                q.material[1] = (i % 8) as f32;
                q.field[2] = weather.recovery * (0.13 + 0.59 * pulse);
                q.field[3] = q.field[2];
                self.effects.push(q);
            }
        }
        if let Some(m) = weather.meteor {
            let age = (weather.time - m.born) as f32;
            let u = (age / m.duration).clamp(0., 1.);
            let head = [m.x + (u - 0.5) * m.dx, m.y + (u - 0.5) * m.dy];
            let tail = if m.fire { 0.62 } else { 0.43 };
            // Fade the source over at least half a second of presentation
            // time at every speed, independently of the blurred streak's travel.
            let rise = (m.duration * 0.24).max(0.5 * pace);
            let fall = (m.duration * 0.32).max(0.5 * pace);
            let light = smooth(age / rise) * smooth((m.duration - age) / fall);
            let length = m.dx.hypot(m.dy);
            let segment = |a: [f32; 2], b: [f32; 2], thickness: f32, light: f32, kind: f32| {
                let nx = -m.dy / length * thickness;
                let ny = m.dx / length * thickness;
                Quad {
                    left: [a[0] - nx, a[1] - ny, a[0] + nx, a[1] + ny],
                    right: [b[0] - nx, b[1] - ny, b[0] + nx, b[1] + ny],
                    field: [0., 1., light, light],
                    material: [METEOR, kind, 0., 0.],
                    ..Quad::default()
                }
            };
            if m.fire {
                let wind = 1. - (-(age - m.duration).max(0.) / 11.).exp();
                let part = |f: f32| {
                    let y = m.y - m.dy * 0.5 + u * m.dy * f;
                    [
                        m.x - m.dx * 0.5 + u * m.dx * f + wind * f * 21.,
                        y + wind * (64. * f + 9. * (f * 10.).sin()),
                    ]
                };
                let fade = 0.35
                    * smooth(age / (0.7 * pace))
                    * (-age / 7.).exp()
                    * smooth((22. - age) / 4.)
                    * m.power;
                for k in 0..5 {
                    let lo = k as f32 / 5.;
                    let hi = (k + 1) as f32 / 5.;
                    let mut q = segment(part(lo), part(hi), 2. + 8. * wind, fade, 1. + wind);
                    q.field[0] = lo;
                    q.field[1] = hi;
                    self.effects.push(q);
                }
            }
            if light > 0. {
                self.effects.push(segment(
                    [head[0] - tail * m.dx, head[1] - tail * m.dy],
                    head,
                    if m.fire { 7.8 } else { 3.8 },
                    light * m.power,
                    0.,
                ));
            }
        }
    }
}
#[repr(C)]
#[derive(Default)]
pub struct Atmosphere {
    pub bounds: [f32; 4],
    pub colors: [[f32; 4]; 6],
    pub top: [f32; 4],
    pub bottom: [f32; 4],
    pub style: [f32; 4],
    pub clock: [f32; 4],
    pub view: [f32; 4],
    pub optics: [f32; 4],
    pub overlap: [[f32; 4]; 8],
}
fn frame(lease: &Lease, upper: bool) -> Option<&Frame> {
    let ep = if upper {
        &lease.sample.upper
    } else {
        &lease.sample.lower
    };
    ep.typed.as_ref()?.downcast_ref::<Frame>()
}
pub(super) fn endpoints(lease: &Lease, side: i32) -> Option<(&Frame, &Frame)> {
    Some((frame(lease, side == 1)?, frame(lease, side != 0)?))
}
/// # Safety
/// A retained immutable sample; negative means normal motion.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_aurora_dissolve(lease: *const Lease) -> f32 {
    unsafe { lease.as_ref() }
        .and_then(|l| {
            let seed = frame(l, false)?.sky.still_seed?;
            Some(super::still_transition(seed, l.view.timeline_ns).0)
        })
        .unwrap_or(-1.)
}
fn vertical(s: &Sky, v: &View) -> f32 {
    if s.mirror {
        (v.viewport.height / s.bounds.height) as f32
    } else {
        v.uniform_scale as f32
    }
}
/// # Safety
/// Receives a retained lease and one writable uniform block.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_aurora_atmosphere(
    lease: *const Lease,
    side: i32,
    out: *mut Atmosphere,
) -> f32 {
    let Some(l) = (unsafe { lease.as_ref() }) else {
        return 0.;
    };
    let (Some((a, b)), Some(out)) = (endpoints(l, side), unsafe { out.as_mut() }) else {
        return 0.;
    };
    let sky = &b.sky;
    let bounds = sky.bounds;
    let v = l.view;
    let time = a.time + (b.time - a.time) * l.sample.alpha;
    let angle = (time * std::f64::consts::PI / 43200.).rem_euclid(std::f64::consts::TAU);
    let sy = vertical(sky, &v);
    *out = Atmosphere {
        bounds: [
            bounds.x as f32,
            bounds.y as f32,
            bounds.width as f32,
            bounds.height as f32,
        ],
        colors: sky.colors,
        overlap: std::array::from_fn(|i| {
            std::array::from_fn(|j| mix(a.overlap[i][j], b.overlap[i][j], l.sample.alpha as f32))
        }),
        top: sky.top,
        bottom: sky.bottom,
        style: sky.style,
        optics: [
            mix(sky.colors[0][0], sky.colors[5][0], 0.45),
            mix(sky.colors[0][1], sky.colors[5][1], 0.45),
            mix(sky.colors[0][2], sky.colors[5][2], 0.45),
            sky.diffusion,
        ],
        clock: [
            (time % 32768.) as f32,
            angle.sin() as f32,
            angle.cos() as f32,
            sky.zenith,
        ],
        view: [
            (v.offset_x - v.viewport.x * v.uniform_scale) as f32,
            if sky.mirror {
                0.
            } else {
                (v.offset_y - v.viewport.y * f64::from(sy)) as f32
            },
            v.uniform_scale as f32,
            sy,
        ],
    };
    sky.target_scale()
}
pub(super) fn sample(
    a: &Frame,
    b: &Frame,
    v: &View,
    t: f32,
    layer: u32,
    mut emit: impl FnMut(Quad),
) {
    let l = (v.viewport.x - v.offset_x / v.uniform_scale) as f32;
    // Include the optical footprint beyond each output for seamless views.
    let guard = b.sky.optical_margin();
    let r = l + (v.viewport.width / v.uniform_scale) as f32 + guard;
    let l = l - guard;
    if layer == 0 {
        // Rotation can bring a star into this viewport later. Upload the fixed
        // field once; the vertex shader rotates and clips at presentation time.
        for q in &b.sky.stars {
            emit(*q)
        }
        return;
    }
    let interpolate = |a: [f32; 4], b: [f32; 4]| std::array::from_fn(|i| mix(a[i], b[i], t));
    for rb in &b.ribbons {
        // Both endpoints are on the same normalized arclength grid. Never pair
        // unlike grids or a recycled ribbon slot after a birth or retirement.
        let old = a
            .ribbons
            .iter()
            .find(|ra| ra.id == rb.id && ra.nodes.len() == rb.nodes.len());
        // A fresh material already has its first 50 ms of birth opacity in the
        // upper snapshot. Reconstruct the zero-opacity lower endpoint.
        let birth = if old.is_some() { 1. } else { t };
        let ra = old.unwrap_or(rb);
        let detail = mix(ra.detail, rb.detail, t);
        let afterglow = mix(ra.afterglow, rb.afterglow, t);
        let row = mix(ra.row, rb.row, t);
        // Interpolate before wrapping. One repeat of the broad and fine fields
        // together is 2048 material units, in either direction.
        let shift =
            ((ra.drift + (rb.drift - ra.drift) * f64::from(t)) / 1024.).rem_euclid(2.) as f32;
        // Large precipitation regions drift more slowly than the fine raylets.
        let cloud_shift = (((ra.drift + (rb.drift - ra.drift) * f64::from(t)) * 0.10) / 1024.)
            .rem_euclid(2.) as f32;
        for ghost in [true, false] {
            if ghost && (b.sky.style[0] <= 0. || afterglow <= 0.001) {
                continue;
            }
            let mut previous = None;
            for (p, q) in ra.nodes.iter().zip(&rb.nodes) {
                let col = (
                    if ghost {
                        interpolate(p.memory, q.memory)
                    } else {
                        interpolate(p.pos, q.pos)
                    },
                    mix(p.u, q.u, t) + shift,
                    mix(p.power, q.power, t) * birth * if ghost { afterglow } else { 1. },
                    if ghost {
                        interpolate(p.memory_tangent, q.memory_tangent)
                    } else {
                        interpolate(p.tangent, q.tangent)
                    },
                    mix(p.u, q.u, t) + cloud_shift,
                    mix(p.surge, q.surge, t),
                );
                if let Some((pos, u, power, tangent, knot, surge)) = previous {
                    let quad = Quad {
                        left: pos,
                        right: col.0,
                        field: [u, col.1, power, col.2],
                        material: [if ghost { GHOST } else { CURTAIN }, row, detail, rb.tint],
                        tangent_left: tangent,
                        tangent_right: col.3,
                        optics: [knot, col.4, surge, col.5],
                    };
                    if quad.visible(l, r) {
                        emit(quad)
                    }
                }
                previous = Some(col);
            }
        }
    }
    // Analytical effects use paired endpoints where topology matches. Births
    // and deaths start/end at zero, independently of the output's frame rate.
    for (i, q) in b.effects.iter().enumerate() {
        let prev = a
            .effects
            .get(i)
            .filter(|p| p.material == q.material)
            .unwrap_or(q);
        let quad = Quad {
            left: interpolate(prev.left, q.left),
            right: interpolate(prev.right, q.right),
            field: interpolate(prev.field, q.field),
            material: q.material,
            ..*q
        };
        if quad.visible(l, r) {
            emit(quad)
        }
    }
}
/// # Safety
/// Requires a live immutable lease. An upper bound avoids walking all of the
/// projected material twice when an output first creates its instance buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_aurora_quad_capacity(
    lease: *const Lease,
    layer: u32,
    side: i32,
) -> usize {
    let Some((_, frame)) = (unsafe { lease.as_ref() }).and_then(|l| endpoints(l, side)) else {
        return 0;
    };
    match layer {
        0 => frame.sky.stars.len(),
        1 => {
            frame.effects.len()
                + frame
                    .ribbons
                    .iter()
                    .map(|r| {
                        let copies = if frame.sky.style[0] > 0. { 2 } else { 1 };
                        copies * r.nodes.len().saturating_sub(1)
                    })
                    .sum::<usize>()
        }
        _ => 0,
    }
}
/// # Safety
/// A live immutable lease and capacity writable quads. Returns the full count
/// even when capacity is zero, permitting one allocation for each static layer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_aurora_quads(
    lease: *const Lease,
    layer: u32,
    side: i32,
    out: *mut Quad,
    capacity: usize,
) -> usize {
    let Some(l) = (unsafe { lease.as_ref() }) else {
        return 0;
    };
    let Some((a, b)) = endpoints(l, side) else {
        return 0;
    };
    if layer > 1 || (capacity > 0 && out.is_null()) {
        return 0;
    }
    let mut n = 0;
    sample(a, b, &l.view, l.sample.alpha as f32, layer, |q| {
        if n < capacity {
            unsafe { out.add(n).write(q) }
        }
        n += 1;
    });
    n
}

/// # Safety
/// Returns a borrowed two-row RGBA8 skyline, valid for the retained lease.
/// Width is writable, each pair of channels encodes a 16-bit altitude.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_aurora_horizon(
    lease: *const Lease,
    side: i32,
    width: *mut u32,
) -> *const u8 {
    let (Some(lease), Some(width)) = (unsafe { lease.as_ref() }, unsafe { width.as_mut() }) else {
        return std::ptr::null();
    };
    let Some(b) = frame(lease, side != 0) else {
        return std::ptr::null();
    };
    *width = (b.sky.horizon.len() / 8) as u32;
    b.sky.horizon.as_ptr()
}
