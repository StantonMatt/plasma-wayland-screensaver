// SPDX-License-Identifier: GPL-3.0-or-later
use super::super::common::rng::{Pcg32, hash64};
use std::ops::{Add, Mul, Sub};

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct V(pub f64, pub f64, pub f64);
impl Add for V {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        V(self.0 + b.0, self.1 + b.1, self.2 + b.2)
    }
}
impl Sub for V {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        V(self.0 - b.0, self.1 - b.1, self.2 - b.2)
    }
}
impl Mul<f64> for V {
    type Output = Self;
    fn mul(self, k: f64) -> Self {
        V(self.0 * k, self.1 * k, self.2 * k)
    }
}
impl V {
    pub fn dot(self, b: Self) -> f64 {
        self.0 * b.0 + self.1 * b.1 + self.2 * b.2
    }
    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }
    pub fn unit(self) -> Self {
        self * (1. / self.length().max(1e-12))
    }
    pub fn four(self, w: f64) -> [f32; 4] {
        [self.0 as f32, self.1 as f32, self.2 as f32, w as f32]
    }
}
pub fn smooth(x: f64) -> f64 {
    let x = x.clamp(0., 1.);
    x * x * (3. - 2. * x)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Cell(pub i64, pub i64, pub i64);
impl Cell {
    pub fn at(p: V, size: f64) -> Self {
        Self(
            (p.0 / size).floor() as i64,
            (p.1 / size).floor() as i64,
            (p.2 / size).floor() as i64,
        )
    }
    pub fn base(self) -> V {
        V(self.0 as f64, self.1 as f64, self.2 as f64)
    }
    pub fn offset(self, x: i64, y: i64, z: i64) -> Self {
        Self(self.0 + x, self.1 + y, self.2 + z)
    }
    pub fn hash(self, seed: u64) -> u64 {
        hash64(hash64(hash64(seed ^ self.0 as u64) ^ self.1 as u64) ^ self.2 as u64)
    }
    pub fn slot(self) -> usize {
        (self.0.rem_euclid(5) + 5 * self.1.rem_euclid(5) + 25 * self.2.rem_euclid(5)) as usize
    }
}
pub fn region(p: V, seed: u64) -> f64 {
    let phase = (seed & 65535) as f64 * 0.00009587;
    // Logarithmic spiral arms on a gently corrugated galactic disk.
    let x = p.0 + 157.;
    let z = p.2 + 108.;
    let arm = 3. * (z.atan2(x) - 2.6 * (x.hypot(z).max(2.) / 180.).ln())
        + phase
        + 0.25 * (p.1 * 0.01).sin();
    (0.50 + 0.38 * arm.cos() + 0.12 * (p.2 * 0.018 - p.1 * 0.007 + phase).sin()).clamp(0., 1.)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Sun,
    Binary,
    Open,
    Globular,
    Cloud,
}
#[derive(Clone, Copy, Debug)]
pub struct Mark {
    pub p: V,
    pub kind: Kind,
    pub flux: f64,
    pub temp: f64,
    pub id: u64,
}
pub const DISTRICT: f64 = 3.;
pub fn landmark(seed: u64, c: Cell) -> Mark {
    let id = c.hash(seed ^ 0x51a3);
    let mut r = Pcg32::new(id, 13);
    let p = (c.base()
        + V(
            0.15 + 0.7 * r.unit(),
            0.15 + 0.7 * r.unit(),
            0.15 + 0.7 * r.unit(),
        ))
        * DISTRICT;
    let field = region(p, seed);
    let k = r.unit();
    let kind = if k < 0.025 {
        Kind::Globular
    } else if k < 0.12 + 0.14 * field {
        Kind::Open
    } else if k < 0.25 + 0.23 * field {
        Kind::Cloud
    } else if k < 0.62 {
        Kind::Binary
    } else {
        Kind::Sun
    };
    Mark {
        p,
        kind,
        flux: 0.6 + 0.4 * r.unit(),
        temp: 0.14 + 0.54 * field + 0.30 * r.unit(),
        id,
    }
}
/// Open clusters form in a chain along their natal gas complex. These knots
/// and the gas renderer use the same world coordinates across cell boundaries.
pub fn nursery(mark: Mark) -> [V; 4] {
    let mut r = Pcg32::new(mark.id, 109);
    [
        V(-0.68, -0.12, 0.10),
        V(0.16, 0.14, 0.94),
        V(0.44, 0.25, 1.80),
        V(0.20, 0.13, 2.78),
    ]
    .map(|p| mark.p + p + V(r.unit() - 0.5, r.unit() - 0.5, r.unit() - 0.5) * 0.10)
}
/// World-to-eye unit quaternion. The camera looks along +Z; +Y points up.
#[derive(Clone, Copy, Debug)]
pub struct Attitude(pub V, pub f64);
impl Attitude {
    pub fn new(yaw: f64, pitch: f64, roll: f64) -> Self {
        let (sy, cy) = (yaw * 0.5).sin_cos();
        let (sp, cp) = (-pitch * 0.5).sin_cos();
        let (sr, cr) = (roll * 0.5).sin_cos();
        // Inverse of Y(yaw) X(-pitch) Z(roll).
        Self(
            V(
                -(cy * sp * cr + sy * cp * sr),
                -(sy * cp * cr - cy * sp * sr),
                -(cy * cp * sr - sy * sp * cr),
            ),
            cy * cp * cr + sy * sp * sr,
        )
    }
    pub fn apply(self, p: V) -> V {
        let cross = |a: V, b: V| {
            V(
                a.1 * b.2 - a.2 * b.1,
                a.2 * b.0 - a.0 * b.2,
                a.0 * b.1 - a.1 * b.0,
            )
        };
        p + cross(self.0, cross(self.0, p) + p * self.1) * 2.
    }
    pub fn inverse(self) -> Self {
        Self(self.0 * (-1.), self.1)
    }
}
