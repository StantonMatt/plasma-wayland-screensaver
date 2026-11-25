//! Rounded convex cores. A bead has one support point, a capsule two, a shard
//! three to six. Contacts use both face axes and the closest rounded features.
use super::{Pcg32, range};
use std::{
    f64::consts::{PI, TAU},
    ops::{Add, Mul, Neg, Sub},
};

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub(super) struct V(pub f64, pub f64);
impl Add for V {
    type Output = V;
    fn add(self, b: Self) -> V {
        V(self.0 + b.0, self.1 + b.1)
    }
}
impl Sub for V {
    type Output = V;
    fn sub(self, b: Self) -> V {
        V(self.0 - b.0, self.1 - b.1)
    }
}
impl Mul<f64> for V {
    type Output = V;
    fn mul(self, k: f64) -> V {
        V(self.0 * k, self.1 * k)
    }
}
impl Neg for V {
    type Output = V;
    fn neg(self) -> V {
        V(-self.0, -self.1)
    }
}
impl V {
    fn dot(self, b: V) -> f64 {
        self.0 * b.0 + self.1 * b.1
    }
    fn cross(self, b: V) -> f64 {
        self.0 * b.1 - self.1 * b.0
    }
    fn perp(self) -> V {
        V(-self.1, self.0)
    }
    fn norm2(self) -> f64 {
        self.dot(self)
    }
    fn rotate(self, (s, c): (f64, f64)) -> V {
        V(c * self.0 - s * self.1, s * self.0 + c * self.1)
    }
}
pub(super) const MAX: usize = 64;
const DT: f64 = 1. / 120.;
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Cut {
    pub core: [V; 6],
    pub sides: usize,
    pub rounding: f64,
    pub reach: f64,
    pub hue: usize,
    pub back: bool,
    pub dichroic: bool,
}
impl Cut {
    fn area(&self) -> f64 {
        match self.sides {
            1 => PI * self.rounding.powi(2),
            2 => 4. * self.core[1].0 * self.rounding + PI * self.rounding.powi(2),
            n => {
                (0..n)
                    .map(|j| self.core[j].cross(self.core[(j + 1) % n]))
                    .sum::<f64>()
                    * 0.5
            }
        }
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Body {
    pub pos: V,
    pub angle: f64,
    pub cut: Cut,
    before: V,
    vel: V,
    spin: f64,
    inv_mass: f64,
    inv_inertia: f64,
    friction: f64,
    offsets: [V; 6],
    normals: [V; 6],
    own_extrema: [[u8; 2]; 6],
    own_extrema_ready: bool,
    quiet: u16,
    touched: bool,
}
impl Body {
    pub fn contains_optical(&self, point: V) -> bool {
        let p = point - self.pos;
        if self.cut.sides == 1 {
            return p.norm2() <= (0.93 * self.cut.rounding).powi(2);
        }
        if self.cut.sides == 2 {
            let a = self.offsets[0] * 0.93;
            let edge = (self.offsets[1] - self.offsets[0]) * 0.93;
            let along = ((p - a).dot(edge) / edge.norm2()).clamp(0., 1.);
            return (p - a - edge * along).norm2() <= (0.93 * self.cut.rounding).powi(2);
        }
        (0..self.cut.sides)
            .all(|i| (p - self.offsets[i] * 0.93).dot(self.normals[i]) <= 0.93 * self.cut.rounding)
    }
    fn orient(&mut self) {
        self.own_extrema_ready = false;
        let rotation = self.angle.sin_cos();
        for (dest, src) in self.offsets[..self.cut.sides]
            .iter_mut()
            .zip(&self.cut.core)
        {
            *dest = src.rotate(rotation);
        }
        for i in 0..self.cut.sides {
            let axis = (self.offsets[i] - self.offsets[(i + 1) % self.cut.sides]).perp();
            self.normals[i] = axis * (1. / axis.norm2().max(1e-18).sqrt());
        }
    }
    // Orientation is fixed throughout the contact relaxation sweeps. Cache
    // the original strict-tie scan's vertex for each signed own-face normal.
    // Positions and impulses can change between sweeps; these indices cannot.
    fn prepare_own_extrema(&mut self) {
        if self.own_extrema_ready {
            return;
        }
        let count = match self.cut.sides {
            1 => 0,
            2 => 1,
            n => n,
        };
        for j in 0..count {
            for sign in 0..2 {
                let n = if sign == 0 {
                    self.normals[j]
                } else {
                    -self.normals[j]
                };
                let mut best = 0;
                let mut height = self.offsets[0].dot(n);
                for i in 1..self.cut.sides {
                    let h = self.offsets[i].dot(n);
                    if h > height {
                        height = h;
                        best = i;
                    }
                }
                self.own_extrema[j][sign] = best as u8;
            }
        }
        self.own_extrema_ready = true;
    }
    fn own_support(&self, axis: usize, negative: bool, n: V) -> V {
        self.offsets[usize::from(self.own_extrema[axis][usize::from(negative)])]
            + n * self.cut.rounding
    }
    pub fn reflection_score(&self, point: V, light: V) -> f64 {
        let relative = point - self.pos;
        let mut distance = f64::NEG_INFINITY;
        let mut alignment = 0.;
        // Reuse the solver's oriented face normals and vertices for the inset optical cut.
        for j in 0..self.cut.sides {
            let normal = self.normals[j];
            let d = (relative - self.offsets[j] * 0.93).dot(normal);
            if d > distance {
                distance = d;
                alignment = normal.dot(light);
            }
        }
        -distance.abs() + 0.022 * alignment + 0.015 * self.cut.reach
    }
    fn support(&self, n: V) -> V {
        let mut best = self.offsets[0];
        let mut height = best.dot(n);
        for &p in &self.offsets[1..self.cut.sides] {
            let h = p.dot(n);
            if h > height {
                height = h;
                best = p;
            }
        }
        best + n * self.cut.rounding
    }
    fn kick(&mut self, impulse: V, r: V) {
        self.vel = self.vel + impulse * self.inv_mass;
        self.spin += r.cross(impulse) * self.inv_inertia;
    }
    fn inverse_at(&self, r: V, n: V) -> f64 {
        self.inv_mass + self.inv_inertia * r.cross(n).powi(2)
    }
    fn wall(&mut self) {
        if self.pos.norm2() < (1. - self.cut.reach).powi(2) {
            return;
        }
        let far = *self.offsets[..self.cut.sides]
            .iter()
            .max_by(|a, b| {
                (self.pos + **a)
                    .norm2()
                    .total_cmp(&(self.pos + **b).norm2())
            })
            .unwrap();
        let end = self.pos + far;
        let distance = end.norm2().sqrt();
        let penetration = distance + self.cut.rounding - 1.;
        if penetration < 0. {
            return;
        }
        let n = end * (1. / distance);
        let r = far + n * self.cut.rounding;
        // Contract toward the cell centre. Every vertex is inside at
        // position zero, so convexity bounds all simultaneous wall contacts.
        let clearance = 1. - self.cut.reach;
        self.pos = self.pos * (clearance / (clearance + penetration));
        let v = self.vel + r.perp() * self.spin;
        let impulse = (v.dot(n) * 1.12 / self.inverse_at(r, n)).max(0.);
        let t = n.perp();
        let friction = (v.dot(t) / self.inverse_at(r, t))
            .clamp(-self.friction * impulse, self.friction * impulse);
        self.kick(-n * impulse - t * friction, r);
        self.touched = true;
    }
}

fn contact(a: &Body, b: &Body, delta: V) -> Option<(V, f64, [V; 2])> {
    // Face-normal separation is exact for two polygons. For rounded cores
    // also test vertex pairs to catch end caps and polygon corners.
    let rounding = a.cut.rounding + b.cut.rounding;
    let mut depth = f64::MAX;
    let mut normal = V(0., 1.);
    let mut supports = None;
    let mut test = |axis: V, owner: usize, index: usize| {
        let negative = delta.dot(axis) < 0.;
        let n = if negative { -axis } else { axis };
        let sa = if owner == 0 && a.own_extrema_ready {
            a.own_support(index, negative, n)
        } else {
            a.support(n)
        };
        let sb = if owner == 1 && b.own_extrema_ready {
            b.own_support(index, !negative, -n)
        } else {
            b.support(-n)
        };
        let overlap = (sa - sb - delta).dot(n);
        if overlap <= 0. {
            return false;
        }
        if overlap < depth {
            depth = overlap;
            normal = n;
            supports = Some([sa, sb]);
        }
        true
    };
    for (owner, body) in [a, b].into_iter().enumerate() {
        let n = match body.cut.sides {
            1 => 0,
            2 => 1,
            n => n,
        };
        if !body.normals[..n]
            .iter()
            .enumerate()
            .all(|(index, &axis)| test(axis, owner, index))
        {
            return None;
        }
    }
    if rounding > 0. {
        // Only the nearest endpoints can form an exposed rounded corner.
        let mut shortest = f64::MAX;
        let mut closest = delta;
        for &p in &a.offsets[..a.cut.sides] {
            for &q in &b.offsets[..b.cut.sides] {
                let axis = delta + q - p;
                let d = axis.norm2();
                if d < shortest {
                    shortest = d;
                    closest = axis;
                }
            }
        }
        if shortest > 1e-16 && !test(closest * (1. / shortest.sqrt()), 2, 0) {
            return None;
        }
    }
    // Keep the supports of the selected axis instead of scanning them again
    // in collide. Degenerate coincident rounded points still use the original
    // default normal and support calls.
    Some((
        normal,
        depth,
        supports.unwrap_or_else(|| [a.support(normal), b.support(-normal)]),
    ))
}
const SLEEP: u16 = 54;
fn collide(a: &mut Body, b: &mut Body, resolve_sleeping: bool) {
    if !resolve_sleeping && a.quiet >= SLEEP && b.quiet >= SLEEP {
        return;
    }
    let d = b.pos - a.pos;
    if d.norm2() >= (a.cut.reach + b.cut.reach).powi(2) {
        return;
    }
    let Some((n, depth, [r1, r2])) = contact(a, b, d) else {
        return;
    };
    a.touched = true;
    b.touched = true;
    // Use one shared contact point, so tangential impulses conserve torque.
    let arm_a = (r1 + d + r2) * 0.5;
    let arm_b = arm_a - d;
    let relative = (b.vel + arm_b.perp() * b.spin) - (a.vel + arm_a.perp() * a.spin);
    if relative.dot(n) < -0.28 || depth > 0.025 {
        a.quiet = 0;
        b.quiet = 0;
    }
    let sum = a.inv_mass + b.inv_mass;
    let shift = (depth - 0.0004).max(0.) * 0.83 / sum;
    a.pos = a.pos - n * (shift * a.inv_mass);
    b.pos = b.pos + n * (shift * b.inv_mass);
    let vn = relative.dot(n);
    if vn >= 0. {
        return;
    }
    let j = -vn * (if vn < -0.15 { 1.18 } else { 1. })
        / (a.inverse_at(arm_a, n) + b.inverse_at(arm_b, n));
    let t = n.perp();
    let f = j * (a.friction + b.friction) * 0.5;
    let jt = (-relative.dot(t) / (a.inverse_at(arm_a, t) + b.inverse_at(arm_b, t))).clamp(-f, f);
    let push = n * j + t * jt;
    a.kick(-push, arm_a);
    b.kick(push, arm_b);
}

pub(super) struct Chamber {
    pub bodies: Vec<Body>,
    pub transport: V,
    split: usize,
    sleep_angle: f64,
    asleep: bool,
    stable_steps: u16,
    refined: bool,
}
impl Chamber {
    pub fn new(rng: &mut Pcg32, n: usize, glass: u32, recipe: u32, palette: u32) -> Self {
        let mut bodies = Vec::with_capacity(MAX);
        let split = n.div_ceil(2);
        let emphasis = rng.bounded(6).unwrap() as usize;
        let phase = range(rng, 0., TAU);
        for i in 0..n {
            let draw = rng.unit();
            let mode = if glass == 0 { recipe } else { glass };
            let sides = match mode {
                1 => {
                    // Even a bead-heavy mixed Ember cell carries large garnet
                    // shards. An explicitly selected bead recipe keeps its bias.
                    let beads = if glass == 0 && palette == 2 {
                        0.53
                    } else {
                        0.85
                    };
                    if draw < beads { 1 } else { 4 }
                }
                2 => {
                    if draw < 0.10 {
                        1
                    } else {
                        3 + rng.bounded(4).unwrap() as usize
                    }
                }
                3 => {
                    if draw < 0.78 {
                        2
                    } else {
                        1
                    }
                }
                _ => {
                    if draw < 0.32 {
                        1
                    } else if draw < 0.50 {
                        2
                    } else {
                        3 + rng.bounded(4).unwrap() as usize
                    }
                }
            };
            let r = range(rng, 0.65, 1.35) * if mode == 4 && i % 6 == 0 { 1.65 } else { 1. };
            let mut cut = Cut {
                sides,
                back: i < split,
                dichroic: sides >= 3 && rng.unit() < 0.58,
                ..Cut::default()
            };
            cut.hue = match palette {
                2 => {
                    if sides == 1 {
                        [1, 1, 2][rng.bounded(3).unwrap() as usize]
                    } else {
                        [4, 3, 4, 3][rng.bounded(4).unwrap() as usize]
                    }
                }
                0 | 3 | 4 => {
                    if sides == 1 {
                        [1, 2, 5][rng.bounded(3).unwrap() as usize]
                    } else {
                        [2, 3, 4][rng.bounded(3).unwrap() as usize]
                    }
                }
                _ => (emphasis + rng.bounded(4).unwrap() as usize * 5) % 6,
            };
            match sides {
                1 => cut.rounding = r * 0.80,
                2 => {
                    cut.rounding = r * range(rng, 0.17, 0.31);
                    cut.core[0] = V(-r, 0.);
                    cut.core[1] = V(r, 0.);
                }
                _ => {
                    let aspect = range(rng, 0.52, 1.05);
                    for j in 0..sides {
                        let a = TAU * (j as f64 + range(rng, -0.15, 0.15)) / sides as f64;
                        cut.core[j] = V(a.cos() * r, a.sin() * r * aspect);
                    }
                }
            }
            let j = if i < split { i } else { i - split };
            let size = if i < split { split } else { n - split };
            let r = 0.87 * ((j as f64 + 0.5) / size as f64).sqrt();
            let theta = phase + j as f64 * 2.3999632297 + if i < split { 0. } else { 1.85 };
            bodies.push(Body {
                pos: V(r * theta.cos(), r * theta.sin()),
                angle: range(rng, -PI, PI),
                friction: if sides == 1 { 0.25 } else { 0.65 },
                cut,
                ..Body::default()
            });
        }
        let area = bodies.iter().map(|b| b.cut.area()).sum::<f64>();
        let size = (PI * range(rng, 0.73, 0.84) / area).sqrt();
        for b in &mut bodies {
            for p in &mut b.cut.core {
                *p = *p * size;
            }
            b.cut.rounding *= size;
            b.cut.reach = b.cut.core[..b.cut.sides]
                .iter()
                .map(|p| p.norm2())
                .fold(0., f64::max)
                .sqrt()
                + b.cut.rounding;
            b.inv_mass = 1. / b.cut.area();
            b.inv_inertia = b.inv_mass / (0.4 * b.cut.reach.powi(2));
            b.orient();
        }
        Self {
            bodies,
            transport: V::default(),
            split,
            sleep_angle: 0.,
            asleep: false,
            stable_steps: 0,
            refined: false,
        }
    }
    pub fn step(&mut self, angle: f64) {
        self.step_refined(angle, false);
    }
    pub fn step_refined(&mut self, angle: f64, refined: bool) {
        if refined && !self.refined {
            self.asleep = false;
            for b in &mut self.bodies {
                b.quiet = 0;
            }
        }
        self.refined = refined;
        if (angle - self.sleep_angle).abs() > 0.038 {
            self.sleep_angle = angle;
            self.stable_steps = 0;
            self.asleep = false;
            for b in &mut self.bodies {
                b.quiet = 0;
            }
        }
        if self.asleep {
            return;
        }
        let (s, c) = angle.sin_cos();
        let gravity = V(s, c) * (3.6 * DT);
        for b in &mut self.bodies {
            b.before = b.pos;
            if b.quiet >= SLEEP {
                continue;
            }
            // Rolling resistance acts at contact, including in a glass jam.
            if b.touched {
                b.spin *= 0.94;
            }
            b.touched = false;
            b.vel = (b.vel + gravity) * 0.997;
            b.spin = (b.spin * 0.989).clamp(-7., 7.);
            b.pos = b.pos + b.vel * DT;
            b.angle += b.spin * DT;
            b.orient();
            b.wall();
        }
        if refined {
            for b in &mut self.bodies {
                b.prepare_own_extrema();
            }
        }
        // Freely falling glass needs one more contact pass than the former
        // speed-limited heap, especially while a twist overlaps a pour.
        for _ in 0..if refined { 5 } else { 1 } {
            let (back, front) = self.bodies.split_at_mut(self.split);
            for layer in [back, front] {
                for j in 1..layer.len() {
                    let (earlier, rest) = layer.split_at_mut(j);
                    for a in earlier {
                        collide(a, &mut rest[0], refined);
                    }
                }
            }
            if refined {
                for b in &mut self.bodies {
                    b.wall();
                }
            }
        }
        self.asleep = true;
        let mut supported = true;
        let mut energy = 0.;
        for b in &mut self.bodies {
            b.wall();
            if b.touched {
                // Feed constrained travel back into velocity. Deep stacks
                // otherwise carry invisible downward energy during a Hold.
                let travel = (b.pos - b.before) * (1. / DT);
                b.vel = b.vel * 0.40 + travel * 0.60;
            }
            supported &= b.touched;
            energy += b.vel.norm2() + (b.spin * b.cut.reach).powi(2);
            if b.touched && b.vel.norm2() < 0.0018 && b.spin.abs() * b.cut.reach < 0.045 {
                b.quiet = b.quiet.saturating_add(1);
            } else {
                b.quiet = 0;
            }
            if b.quiet >= SLEEP {
                b.vel = V::default();
                b.spin = 0.;
            } else {
                self.asleep = false;
            }
        }
        // A supported island can sleep together. This removes the residual
        // sliding of light pieces between two much heavier, stationary faces.
        if supported && energy < 0.0025 * self.bodies.len() as f64 {
            self.stable_steps += 1;
            if self.stable_steps >= 60 {
                self.asleep = true;
                for b in &mut self.bodies {
                    b.quiet = SLEEP;
                    b.vel = V::default();
                    b.spin = 0.;
                }
            }
        } else {
            self.stable_steps = 0;
        }
    }
}

// Integral of a trapezoidal velocity profile: continuous starts and stops.
#[cfg(test)]
pub(super) fn transfer_speed(time: f64, speed: f64, distance: f64) -> f64 {
    const RAMP: f64 = 0.25;
    let duration = distance / speed + RAMP;
    if time <= 0. || time >= duration {
        0.
    } else {
        speed * (time / RAMP).min((duration - time) / RAMP).min(1.)
    }
}
pub(super) fn transfer_distance(time: f64, speed: f64, distance: f64) -> f64 {
    const RAMP: f64 = 0.25;
    let duration = distance / speed + RAMP;
    let t = time.clamp(0., duration);
    if t < RAMP {
        speed * t * t / (2. * RAMP)
    } else if t > duration - RAMP {
        distance - speed * (duration - t).powi(2) / (2. * RAMP)
    } else {
        speed * (t - RAMP / 2.)
    }
}

#[cfg(test)]
impl Chamber {
    pub fn max_penetration(&self) -> f64 {
        self.max_penetration_at_scale(1.)
    }
    pub fn max_optical_penetration(&self) -> f64 {
        self.max_penetration_at_scale(0.93)
    }
    fn max_penetration_at_scale(&self, scale: f64) -> f64 {
        let bodies: Vec<_> = self
            .bodies
            .iter()
            .map(|b| {
                let mut b = *b;
                for v in &mut b.cut.core {
                    *v = *v * scale;
                }
                b.cut.rounding *= scale;
                b.cut.reach *= scale;
                b.orient();
                b
            })
            .collect();
        let mut worst: f64 = 0.;
        for layer in [&bodies[..self.split], &bodies[self.split..]] {
            for j in 1..layer.len() {
                for a in &layer[..j] {
                    if let Some((_, depth, _)) = contact(a, &layer[j], layer[j].pos - a.pos) {
                        worst = worst.max(depth);
                    }
                }
            }
        }
        worst
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed_ember_keeps_garnet_in_the_bead_recipe() {
        let beads = |glass| {
            (0..32)
                .map(|seed| {
                    let mut rng = Pcg32::new(seed, 51);
                    Chamber::new(&mut rng, 53, glass, 1, 2)
                        .bodies
                        .iter()
                        .filter(|body| body.cut.sides == 1)
                        .count()
                })
                .sum::<usize>() as f64
                / (32. * 53.)
        };
        assert!((0.48..0.58).contains(&beads(0)));
        assert!((0.80..0.90).contains(&beads(1)));
    }
    #[test]
    fn bead_and_wire_contact_does_not_tunnel() {
        let mut rng = Pcg32::new(27, 51);
        let mut c = Chamber::new(&mut rng, 30, 3, 0, 1);
        let mut bead = *c.bodies.iter().find(|p| p.cut.sides == 1).unwrap();
        let mut wire = *c.bodies.iter().find(|p| p.cut.sides == 2).unwrap();
        bead.pos = V(0., -0.32);
        bead.vel = V(0., 1.4);
        bead.angle = 0.;
        bead.orient();
        wire.pos = V(0., 0.);
        wire.vel = V(0., 0.);
        wire.angle = 0.;
        wire.orient();
        c.bodies = vec![bead, wire];
        c.split = 2;
        for _ in 0..70 {
            // The maximum physical speed runs exactly two 120 Hz steps.
            c.step(0.);
            c.step(0.);
            let (bead, wire) = (&c.bodies[0], &c.bodies[1]);
            assert!(bead.pos.1 < wire.pos.1);
        }
    }
    #[test]
    fn hold_dissipates_energy_and_sleeps() {
        let mut rng = Pcg32::new(49, 3);
        let mut chamber = Chamber::new(&mut rng, 36, 0, 0, 1);
        for _ in 0..120 * 25 {
            chamber.step(0.);
        }
        let quiet = chamber.bodies.iter().filter(|p| p.quiet >= SLEEP).count();
        let energy = chamber
            .bodies
            .iter()
            .map(|p| p.vel.norm2() + p.spin.powi(2) * p.cut.reach.powi(2))
            .sum::<f64>();
        println!("settled chamber: sleeping {quiet}/36 kinetic proxy {energy:.5}");
        assert!(quiet >= 24 && energy < 0.02);
        let centers: Vec<_> = chamber.bodies.iter().map(|p| p.pos).collect();
        for _ in 0..240 {
            chamber.step(1.1);
        }
        assert!(
            chamber
                .bodies
                .iter()
                .zip(centers)
                .any(|(p, c)| (p.pos - c).norm2() > 0.01)
        );
    }
}

#[cfg(test)]
mod support_cache_tests {
    use super::*;
    #[test]
    fn cached_support_matches_original_scan_after_rotations() {
        let mut rng = Pcg32::new(11, 5);
        let mut chamber = Chamber::new(&mut rng, 64, 0, 0, 0);
        let mut checks = 0;
        for b in &mut chamber.bodies {
            for i in 0..64 {
                b.angle = f64::from(i) * 0.17320508075688773 - 3.;
                b.orient();
                b.prepare_own_extrema();
                let count = match b.cut.sides {
                    1 => 0,
                    2 => 1,
                    n => n,
                };
                for j in 0..count {
                    for negative in [false, true] {
                        let n = if negative {
                            -b.normals[j]
                        } else {
                            b.normals[j]
                        };
                        let old = b.support(n);
                        let cached = b.own_support(j, negative, n);
                        assert_eq!(
                            [old.0.to_bits(), old.1.to_bits()],
                            [cached.0.to_bits(), cached.1.to_bits()],
                            "stale support after rotation {i}, face {j}"
                        );
                        checks += 1;
                    }
                }
            }
        }
        assert!(checks > 1000);
    }
}
