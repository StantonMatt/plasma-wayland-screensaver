// SPDX-License-Identifier: GPL-3.0-or-later
use super::Point;
use crate::visuals::settings::Config;
#[derive(Clone, Copy, Debug)]
pub struct Shape {
    pub circles: [Point; 2],
    pub radii: [f64; 2],
    pub curve: [Point; 3],
    pub width: f64,
    pub alpha: f64,
    pub color: usize,
    pub angle: f64,
}
pub fn shapes(c: &Config, phase: f64, w: f64, h: f64, mut emit: impl FnMut(Shape)) {
    let sectors = (5.0 + c.density / 9.0).round().max(5.0) as usize;
    let layers = (3.0 + c.density / 18.0).round().max(3.0) as usize;
    let radius = w.min(h) * 0.46 * c.scale;
    for sector in 0..sectors {
        let angle = phase * c.speed * 0.32 + sector as f64 * std::f64::consts::TAU / sectors as f64;
        for layer in 0..layers {
            let radial = radius * (0.18 + layer as f64 / layers as f64 * 0.82);
            let wobble =
                (phase * c.speed + layer as f64 * 1.7 + c.seed as f64).sin() * radius * 0.09;
            let size = (radius * (0.035 + c.trails * 0.045)).max(3.0);
            emit(Shape {
                circles: [
                    Point::new(radial, wobble),
                    Point::new(radial * 0.72, -wobble * 1.3),
                ],
                radii: [size, size * 0.62],
                curve: [
                    Point::new(radial * 0.28, 0.0),
                    Point::new(radial * 0.62, wobble * 1.8),
                    Point::new(radial, wobble),
                ],
                width: 1.0 + c.trails * 3.0,
                alpha: 0.22 + c.trails * 0.34,
                color: sector + layer,
                angle,
            });
        }
    }
}
