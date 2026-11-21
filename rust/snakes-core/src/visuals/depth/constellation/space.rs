use super::super::common::{
    layout::Rect,
    rng::{Pcg32, hash64},
};
use std::ops::{Add, AddAssign, Div, Mul, Sub};
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}
pub fn pt(x: f32, y: f32) -> Point {
    Point { x, y }
}
impl Add for Point {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        pt(self.x + b.x, self.y + b.y)
    }
}
impl Sub for Point {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        pt(self.x - b.x, self.y - b.y)
    }
}
impl Mul<f32> for Point {
    type Output = Self;
    fn mul(self, b: f32) -> Self {
        pt(self.x * b, self.y * b)
    }
}
impl Div<f32> for Point {
    type Output = Self;
    fn div(self, b: f32) -> Self {
        pt(self.x / b, self.y / b)
    }
}
impl AddAssign for Point {
    fn add_assign(&mut self, b: Self) {
        *self = *self + b;
    }
}
impl Point {
    pub fn dot(self, b: Self) -> f32 {
        self.x * b.x + self.y * b.y
    }
    pub fn len2(self) -> f32 {
        self.dot(self)
    }
    pub fn len(self) -> f32 {
        self.len2().sqrt()
    }
    pub fn unit(self) -> Self {
        self / self.len().max(0.001)
    }
    pub fn mix(self, b: Self, t: f32) -> Self {
        self + (b - self) * t
    }
    pub fn polar(angle: f32) -> Self {
        let (s, c) = angle.sin_cos();
        pt(c, s)
    }
}
pub fn rand(r: &mut Pcg32, a: f32, b: f32) -> f32 {
    a + (b - a) * r.unit() as f32
}
pub fn smooth(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}
pub fn hash(x: i32, y: i32, z: i32, seed: u64) -> f32 {
    let v = hash64(
        seed ^ (x as u64).wrapping_mul(73856093)
            ^ (y as u64).wrapping_mul(19349663)
            ^ (z as u64).wrapping_mul(83492791),
    );
    (v >> 40) as f32 / 16777216.
}
// Analytic spatial derivatives of interpolated 3D value noise. Rotating its
// gradient by a quarter turn gives the divergence-free sky current.
pub fn curl(p: Point, t: f32, seed: u64) -> Point {
    stream(p, t, seed).0
}
fn stream(p: Point, t: f32, seed: u64) -> (Point, f32) {
    let x = (0.819 * p.x + 0.574 * p.y) / 1100.;
    let y = (-0.574 * p.x + 0.819 * p.y) / 1100.;
    let z = t / 180.;
    let (ix, iy, iz) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
    let (u, v, w) = (x - x.floor(), y - y.floor(), smooth(z - z.floor()));
    let corner = |dx, dy| {
        let a = hash(ix + dx, iy + dy, iz, seed);
        a + (hash(ix + dx, iy + dy, iz + 1, seed) - a) * w
    };
    let (a, b, c, d) = (corner(0, 0), corner(1, 0), corner(0, 1), corner(1, 1));
    let k = a - b - c + d;
    let current = pt(
        (c - a + k * smooth(u)) * 6. * v * (1. - v),
        -(b - a + k * smooth(v)) * 6. * u * (1. - u),
    );
    let gradient = pt(
        0.819 * current.x - 0.574 * current.y,
        0.574 * current.x + 0.819 * current.y,
    ) * 9.;
    (
        gradient,
        (a + (b - a) * smooth(u) + (c - a + k * smooth(u)) * smooth(v) - 0.5) * 9900.,
    )
}

// Taper the stream function, not its velocity. Its rotated gradient remains
// divergence free and runs parallel to each wall. A normal restoring force
// alone traps passive tracers in the down-current corners.
pub fn field_current(p: Point, t: f32, seed: u64, min: Point, max: Point) -> Point {
    let (current, psi) = stream(p, t, seed);
    let window = |x: f32, lo: f32, hi: f32| {
        let margin = ((hi - lo) * 0.35).clamp(1., 480.);
        let near = x - lo < hi - x;
        let u = ((x - lo).min(hi - x) / margin).clamp(0., 1.);
        (
            smooth(u),
            6. * u * (1. - u) / margin * if near { 1. } else { -1. },
        )
    };
    let (wx, dx) = window(p.x, min.x, max.x);
    let (wy, dy) = window(p.y, min.y, max.y);
    current * (wx * wy) + pt(wx * dy, -wy * dx) * psi
}
#[derive(Debug)]
pub struct Grid {
    pub cells: Vec<Vec<usize>>,
    populated: Vec<usize>,
    min: Point,
    pub side: f32,
    nx: usize,
    ny: usize,
}
impl Grid {
    pub fn new(b: Rect, side: f32) -> Self {
        // Bound the index even for extremely large, sparse synthetic desktop layouts.
        let side = side.max((b.width.max(b.height) / 256.) as f32);
        let nx = (b.width as f32 / side).ceil().max(1.) as usize;
        let ny = (b.height as f32 / side).ceil().max(1.) as usize;
        Self {
            cells: (0..nx * ny).map(|_| Vec::new()).collect(),
            populated: Vec::new(),
            min: pt(b.x as f32, b.y as f32),
            side,
            nx,
            ny,
        }
    }
    fn coord(&self, p: Point) -> (usize, usize) {
        (
            ((p.x - self.min.x) / self.side) as usize,
            ((p.y - self.min.y) / self.side) as usize,
        )
    }
    pub fn clear(&mut self) {
        for c in self.populated.drain(..) {
            self.cells[c].clear();
        }
    }
    pub fn cell(&self, p: Point) -> usize {
        let (x, y) = self.coord(p);
        x.min(self.nx - 1) + self.nx * y.min(self.ny - 1)
    }
    pub fn insert(&mut self, p: Point, i: usize) {
        let c = self.cell(p);
        self.add_to_cell(c, i);
    }
    pub fn add_to_cell(&mut self, c: usize, i: usize) {
        if self.cells[c].is_empty() {
            self.populated.push(c);
        }
        self.cells[c].push(i);
    }
    pub fn rect(&self, a: Point, b: Point) -> impl Iterator<Item = usize> + Clone + use<> {
        let (x, y) = self.coord(pt(a.x.min(b.x), a.y.min(b.y)));
        let (u, v) = self.coord(pt(a.x.max(b.x), a.y.max(b.y)));
        let n = self.nx;
        let x = x.min(n - 1);
        let u = u.min(n - 1);
        (y.min(self.ny - 1)..=v.min(self.ny - 1)).flat_map(move |j| (x..=u).map(move |i| i + j * n))
    }
    pub fn near(&self, p: Point, r: f32) -> impl Iterator<Item = usize> + '_ {
        self.rect(p - pt(r, r), p + pt(r, r))
            .flat_map(|c| self.cells[c].iter().copied())
    }
}
