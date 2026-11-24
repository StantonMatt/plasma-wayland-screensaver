// SPDX-License-Identifier: GPL-3.0-or-later
//! Horizontal ground-plane dynamics. Both horizontal axes are in kilometres here.
use super::{Pcg32, mix, noise, range, smooth};

#[derive(Clone, Copy, Default)]
pub(super) struct Node {
    pub x: f32,
    pub z: f32,
    vx: f32,
    vz: f32,
    pub rest_x: f32,
    rest_z: f32,
    pub memory_x: f32,
    pub memory_z: f32,
}

#[derive(Clone, Copy)]
struct Vortex {
    x: f32,
    z: f32,
    strength: f32,
    age: f32,
    life: f32,
    drift: f32,
    radius: f32,
}

pub(super) const KM_PER_PIXEL: f32 = 0.24;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Formation {
    Mature,
    Birth,
    Expansion,
}

pub(super) struct Curtain {
    pub nodes: Vec<Node>,
    pub mean: f32,
    pub light: f32,
    pub phase: f32,
    pub drift: f64,
    pub sharp: f32,
    pub partial: bool,
    pub ground_hem: bool,
    pub variation: f32,
    pub gain: f32,
    pub gain_baseline: f32,
    pub age: f32,
    pub id: u32,
    pub texture_row: u32,
    pub target: f32,
    velocity: f32,
    life: f32,
    direction: f32,
    next_eddy: f32,
    vortices: Vec<Vortex>,
    pub sheet: Vec<Node>,
    fade: Option<(f32, f32, f32)>,
    pub gain_target: f32,
    pub merge_into: Option<u32>,
    // Expansion sheets have a delayed rise and fade throughout storm recovery.
    pub expansion: Option<(f32, f32, f32)>,
}

impl Curtain {
    pub fn new(
        r: &mut Pcg32,
        width: f32,
        mean: f32,
        density: f32,
        activity: f32,
        id: u32,
        formation: Formation,
    ) -> Self {
        let width = width * KM_PER_PIXEL;
        let phase = range(r, 0., 100.);
        let initial = formation == Formation::Mature;
        let partial = id != 0 && r.unit() < 0.20 && formation != Formation::Expansion;
        let margin = (width * 0.23).max(95.);
        let start = if partial {
            range(r, -0.12, 0.44) * width
        } else {
            -margin
        };
        let end = if partial {
            start + range(r, 0.52, 0.88) * width
        } else {
            width + margin
        };
        let n = (128. + density * 0.64) as usize;
        let seed = r.next_u32() as u64;
        let direction = range(r, -1., 1.);
        let mut nodes = Vec::with_capacity(n);
        let kink = range(r, 0.22, 0.8) * width;
        let radius = range(r, 70., 115.);
        let curl = range(r, -1., 1.).signum() * range(r, 1.55, 2.40) * (0.48 + 0.72 * activity);
        for i in 0..n {
            let u = i as f32 / (n - 1) as f32;
            let x = mix(start, end, u);
            let wave = 0.50 * (x / 210. + phase).sin()
                + 0.48 * noise(x / 250., seed)
                + 0.22 * noise(x / 77., seed + 1)
                + direction * (x / width - 0.5) * 0.8;
            let z = mean * (1. + 0.64 * wave).clamp(0.37, 1.85);
            // A mature curl at activation. Continued evolution is driven by the live vortices.
            let d = x - kink;
            let rotation = curl * (-0.5 * (d / radius).powi(2)).exp();
            let (s, c) = rotation.sin_cos();
            let (px, pz) = (x + d * (c - 1.), z + d * s);
            nodes.push(Node {
                x: px,
                z: pz,
                rest_x: x,
                rest_z: z,
                memory_x: px,
                memory_z: pz,
                ..Node::default()
            });
        }
        let life = range(r, 900., 2400.);
        let gain = range(r, 0.78, 1.12);
        let mut arc = Self {
            sheet: nodes.clone(),
            fade: None,
            gain_target: gain,
            gain_baseline: gain,
            merge_into: None,
            expansion: None,
            nodes,
            mean,
            target: mean,
            light: if initial { 1. } else { 0. },
            phase,
            drift: r.unit() * 500.,
            sharp: range(r, 0.35, 0.85),
            partial,
            ground_hem: false,
            variation: r.unit() as f32,
            gain,
            age: if initial {
                range(r, 60., life * 0.65)
            } else {
                0.
            },
            life: if id == 0 && density >= 70. {
                f32::INFINITY
            } else {
                life
            },
            id,
            texture_row: id % 8,
            velocity: range(r, -0.15, 0.15),
            direction: if r.unit() < 0.23 { -1. } else { 1. },
            next_eddy: range(r, 1., 22.),
            vortices: Vec::with_capacity(8),
        };
        for _ in 0..(1. + 4. * activity).round() as usize {
            arc.spawn_eddy(r, true, activity);
        }
        arc.redistribute();
        arc
    }
    fn spawn_eddy(&mut self, r: &mut Pcg32, mature: bool, activity: f32) {
        let node = self.nodes[r.bounded(self.nodes.len() as u32).unwrap() as usize];
        self.vortices.push(Vortex {
            x: node.x,
            z: node.z,
            strength: range(r, 35., 125.) * (0.24 + activity) * range(r, -1., 1.).signum(),
            age: if mature { range(r, 30., 50.) } else { 0. },
            life: range(r, 55., 120.),
            drift: range(r, -1.3, 1.1),
            radius: range(r, 27., 45.),
        });
    }
    pub fn successor(
        &self,
        r: &mut Pcg32,
        width: f32,
        density: f32,
        activity: f32,
        id: u32,
    ) -> Self {
        let mut next = Self::new(r, width, self.mean, density, activity, id, Formation::Birth);
        next.nodes.clone_from(&self.nodes);
        next.sheet.clone_from(&self.sheet);
        next.partial = self.partial;
        next.ground_hem = self.ground_hem;
        next.texture_row = self.texture_row;
        next.phase = self.phase;
        next.drift = self.drift;
        next.direction = self.direction;
        next.variation = self.variation;
        next.sharp = self.sharp;
        // A new generation starts at this same luminous fold. Its subsequent
        // weather is independent, while the dying sheet funds its first light.
        next.light = 1.;
        next.gain = 0.;
        next.gain_target = 0.;
        next.target = (self.mean + range(r, -85., 85.)).clamp(60., 680.);
        next
    }
    pub fn excite(&mut self, r: &mut Pcg32, activity: f32) {
        for _ in 0..2 {
            if self.vortices.len() < 8 {
                self.spawn_eddy(r, false, activity);
            }
        }
    }
    pub fn fade_to(&mut self, target: f32) {
        self.fade = Some((self.light, target, 0.));
    }
    pub fn retire(&mut self) {
        self.fade_to(0.);
        self.life = self.age;
    }
    pub fn retiring(&self) -> bool {
        self.age >= self.life || self.expansion.is_some_and(|(_, end, _)| self.age >= end)
    }
    pub fn step(&mut self, dt: f32, t: f32, activity: f32, storm: f32, r: &mut Pcg32) {
        let was_retiring = self.retiring();
        self.age += dt;
        if !was_retiring && self.retiring() {
            self.fade_to(0.);
        }
        let wanted = if self.retiring() { 0. } else { 1. };
        if let Some((delay, end, recovery)) = self.expansion {
            self.light = smooth((self.age - delay) / 45.) * smooth((end - self.age) / recovery);
        } else if let Some((from, to, elapsed)) = self.fade {
            let progress = if to == 0. {
                (dt / 8.).min(0.05 / 2.5)
            } else {
                0.05 / 0.4
            };
            let elapsed = (elapsed + progress).min(1.);
            self.light = mix(from, to, smooth(elapsed));
            self.fade = (elapsed < 1.).then_some((from, to, elapsed));
        } else {
            self.light += (wanted - self.light) * (1. - (-dt / 7.).exp());
        }
        // A merge loans its light to the surviving sheet rather than raising
        // that sheet's permanent exposure. Keep the transfer continuous, then
        // relax with a 30-second real-time decay at every speed. A successor
        // starts at zero to inherit its dying fold continuously, and also
        // returns gradually to its own seeded baseline after the handoff.
        self.gain_target +=
            (self.gain_baseline - self.gain_target) * (1. - (-0.05_f32 / 30.).exp());
        self.gain += (self.gain_target - self.gain) * 0.22;
        self.velocity += ((self.target - self.mean) * 0.009 - self.velocity) * dt * 0.07;
        let dz = self.velocity.clamp(-0.7, 0.7) * dt;
        self.mean += dz;
        self.drift +=
            f64::from(self.direction * (dt * (9. + 49. * activity + 62. * storm)).min(7.5));
        self.sharp +=
            (0.56 + 0.34 * (t * 0.21 + self.phase).sin() + 0.10 * storm - self.sharp) * dt * 0.24;
        self.next_eddy -= dt;
        self.vortices.retain(|v| v.age < v.life);
        if self.next_eddy <= 0. {
            if self.vortices.len() < (1. + 6. * activity + storm).round() as usize {
                self.spawn_eddy(r, false, activity);
            }
            self.next_eddy = range(r, 8., 38.) / (0.6 + activity + storm);
            for i in 1..self.vortices.len() {
                let a = self.vortices[i - 1];
                let b = self.vortices[i];
                if a.strength * b.strength > 0. && (a.x - b.x).hypot(a.z - b.z) < 24. {
                    self.vortices[i - 1].strength =
                        (a.strength + b.strength * 0.6).clamp(-185., 185.);
                    self.vortices.remove(i);
                    break;
                }
            }
        }
        for v in &mut self.vortices {
            v.age += dt;
            v.x += v.drift * dt;
            v.z += dz;
        }
        // Regularised Biot-Savart velocity (bounded even at a vortex centre).
        let mut fields = [(0., 0., 0., 0.); 8];
        for (f, v) in fields.iter_mut().zip(&self.vortices) {
            *f = (
                v.x,
                v.z,
                v.radius * v.radius,
                v.strength * smooth(v.age / 8.) * smooth((v.life - v.age) / 12.),
            );
        }
        let mut prev = self.nodes[0];
        for i in 0..self.nodes.len() {
            let next = self.nodes[(i + 1).min(self.nodes.len() - 1)];
            let n = &mut self.nodes[i];
            let old = *n;
            n.rest_z += dz;
            let mut vx = (n.rest_x - n.x) * 0.009 + (prev.x + next.x - 2. * n.x) * 0.032;
            let mut vz = (n.rest_z - n.z) * 0.014 + (prev.z + next.z - 2. * n.z) * 0.032;
            for &(x, z, r2, k) in &fields[..self.vortices.len()] {
                let dx = n.x - x;
                let dz = n.z - z;
                let omega = k / (dx * dx + dz * dz + r2);
                vx -= dz * omega;
                vz += dx * omega;
            }
            // Deflect an approaching curl smoothly before it passes overhead. A
            // hard near-plane stop flattens its projected hem into a shelf.
            vz += (48. - n.z).max(0.) * 0.09;
            n.vx += (vx - n.vx) * dt * 0.64;
            n.vz += (vz - n.vz) * dt * 0.64;
            n.x += n.vx * dt;
            n.z = (n.z + n.vz * dt + dz).clamp(-120., 1100.);
            n.memory_x += (n.x - n.memory_x) * dt / 55.;
            n.memory_z += (n.z - n.memory_z) * dt / 55.;
            prev = old;
        }
    }
    // Physics keeps its Lagrangian points. Publish a separate, equally spaced
    // arclength grid each tick, so the same material fraction has a continuous
    // trajectory when the sheet stretches or curls.
    pub fn redistribute(&mut self) {
        let n = self.nodes.len();
        let dist = |a: Node, b: Node| (a.x - b.x).hypot(a.z - b.z);
        let total: f32 = self.nodes.windows(2).map(|p| dist(p[0], p[1])).sum();
        let spacing = total / (n - 1) as f32;
        let mut j = 0;
        let mut traversed = 0.;
        for i in 1..n - 1 {
            let along = i as f32 * spacing;
            let mut d = dist(self.nodes[j], self.nodes[j + 1]);
            while traversed + d < along && j + 2 < n {
                traversed += d;
                j += 1;
                d = dist(self.nodes[j], self.nodes[j + 1]);
            }
            let a = self.nodes[j];
            let b = self.nodes[j + 1];
            let u = ((along - traversed) / d.max(0.01)).clamp(0., 1.);
            let blend = |x, y| mix(x, y, u);
            self.sheet[i] = Node {
                x: blend(a.x, b.x),
                z: blend(a.z, b.z),
                vx: blend(a.vx, b.vx),
                vz: blend(a.vz, b.vz),
                rest_x: blend(a.rest_x, b.rest_x),
                rest_z: blend(a.rest_z, b.rest_z),
                memory_x: blend(a.memory_x, b.memory_x),
                memory_z: blend(a.memory_z, b.memory_z),
            };
        }
        self.sheet[0] = self.nodes[0];
        self.sheet[n - 1] = self.nodes[n - 1];
    }
}
