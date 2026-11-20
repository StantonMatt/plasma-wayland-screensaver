// SPDX-License-Identifier: GPL-3.0-or-later
use super::{Point, Vertex};

pub const MAX_STEPS: usize = 512;
/// Fixed topology for a given viewport. Conservative curve curvature bounds keep
/// chord error below 0.25 logical pixels, without phase-dependent allocations.
pub fn steps(visual: u32, width: f64, height: f64, scale: f64) -> usize {
    let bound = if visual == super::RIBBONS {
        width.max(height) * 4.0
    } else {
        width.min(height) * 0.46 * scale * 2.0
    };
    (bound.sqrt().ceil() as usize).clamp(8, MAX_STEPS)
}
pub fn cubic(p: &[Point], t: f64) -> Point {
    let u = 1.0 - t;
    p[0] * (u * u * u) + p[1] * (3.0 * u * u * t) + p[2] * (3.0 * u * t * t) + p[3] * (t * t * t)
}
pub fn quadratic(p: &[Point], t: f64) -> Point {
    let u = 1.0 - t;
    p[0] * (u * u) + p[1] * (2.0 * u * t) + p[2] * (t * t)
}
pub struct Writer<'a> {
    pub vertices: &'a mut [Vertex],
    pub count: usize,
}
impl Writer<'_> {
    fn triangle(&mut self, a: Vertex, b: Vertex, c: Vertex) {
        self.vertices[self.count..self.count + 3].copy_from_slice(&[a, b, c]);
        self.count += 3;
    }
    pub fn quad(&mut self, v: [Vertex; 4]) {
        self.triangle(v[0], v[1], v[2]);
        self.triangle(v[2], v[1], v[3]);
    }
    /// A continuous miter strip (Canvas's default join), including an AA fringe
    /// and butt caps. Wide inner offsets can fold over nearby pieces: renderers
    /// must union coverage within this stroke before additive compositing.
    pub fn stroke(&mut self, points: &[Point], width: f64, color: [f32; 4]) {
        let half = width * 0.5;
        let extent = half + 1.0;
        let count = points.len();
        let mut previous = [Vertex::default(); 2];
        for i in 0..count {
            let before = (points[i] - points[i.saturating_sub(1)]).unit();
            let after = (points[(i + 1).min(count - 1)] - points[i]).unit();
            let tangent = if i == 0 {
                after
            } else if i == count - 1 {
                before
            } else {
                (before + after).unit()
            };
            let normal = Point::new(-tangent.y, tangent.x);
            let miter = if i == 0 || i == count - 1 {
                1.0
            } else {
                1.0 / tangent.dot(after).max(0.1)
            };
            let cap = if i == 0 || i == count - 1 {
                -1.0
            } else {
                65536.0
            };
            let center = if i == 0 {
                points[i] - tangent
            } else if i == count - 1 {
                points[i] + tangent
            } else {
                points[i]
            };
            // Interpolating actual arc distance gives a one-pixel butt fringe.
            let cap = if i == 1 {
                (points[1] - points[0]).length()
            } else if i == count - 2 {
                (points[count - 1] - points[count - 2]).length()
            } else {
                cap
            };
            let pair = [-1.0, 1.0].map(|side| {
                let p = center + normal * (extent * miter * side);
                Vertex {
                    position: [p.x as f32, p.y as f32],
                    shape: [(extent * side) as f32, cap as f32, 0.0, 0.0],
                    parameters: [half as f32, -1.0],
                    color,
                }
            });
            if i != 0 {
                self.quad([previous[0], previous[1], pair[0], pair[1]]);
            }
            previous = pair;
        }
    }
    pub fn circles(&mut self, centers: [Point; 2], radii: [f64; 2], color: [f32; 4]) {
        let left = (centers[0].x - radii[0]).min(centers[1].x - radii[1]) - 1.0;
        let right = (centers[0].x + radii[0]).max(centers[1].x + radii[1]) + 1.0;
        let top = (centers[0].y - radii[0]).min(centers[1].y - radii[1]) - 1.0;
        let bottom = (centers[0].y + radii[0]).max(centers[1].y + radii[1]) + 1.0;
        self.quad(
            [[left, top], [left, bottom], [right, top], [right, bottom]].map(|p| Vertex {
                position: [p[0] as f32, p[1] as f32],
                shape: [
                    (p[0] - centers[0].x) as f32,
                    (p[1] - centers[0].y) as f32,
                    (p[0] - centers[1].x) as f32,
                    (p[1] - centers[1].y) as f32,
                ],
                parameters: radii.map(|x| x as f32),
                color,
            }),
        );
    }
}
