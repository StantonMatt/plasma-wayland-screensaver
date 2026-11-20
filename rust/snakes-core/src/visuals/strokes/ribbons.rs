// SPDX-License-Identifier: GPL-3.0-or-later
use super::Point;
use crate::visuals::settings::Config;

#[derive(Clone, Copy, Debug)]
pub struct Path {
    pub points: [Point; 7],
    pub width: f64,
    pub alpha: f64,
    pub color: usize,
}
/// The two cubics are one path: the join is stroked once, before the faint wider pass.
pub fn paths(c: &Config, phase: f64, w: f64, h: f64, mut emit: impl FnMut(Path)) {
    let count = (3.0 + c.density / 12.0).round() as usize;
    for i in 0..count {
        let y = h * (0.12 + i as f64 / (count - 1).max(1) as f64 * 0.76);
        let wave = phase * c.speed + i as f64 * 0.71 + c.seed as f64 * 0.01;
        let path = Path {
            points: [
                Point::new(-w * 0.05, y + wave.sin() * h * 0.08),
                Point::new(w * 0.22, y + (wave * 1.2).cos() * h * 0.28),
                Point::new(w * 0.38, y + (wave * 0.83 + 2.0).sin() * h * 0.25),
                Point::new(w * 0.55, y + (wave + 1.0).cos() * h * 0.12),
                Point::new(w * 0.72, y + (wave * 1.31).sin() * h * 0.27),
                Point::new(w * 0.88, y + (wave * 0.77 + 3.0).cos() * h * 0.24),
                Point::new(w * 1.05, y + (wave + 2.0).sin() * h * 0.08),
            ],
            width: (2.0 + (i % 3) as f64 * 2.5) * c.scale,
            alpha: 0.16 + c.trails * 0.42,
            color: i,
        };
        emit(path);
        if c.trails > 0.25 {
            emit(Path {
                width: path.width * 4.0,
                alpha: c.trails * 0.08,
                ..path
            });
        }
    }
}
