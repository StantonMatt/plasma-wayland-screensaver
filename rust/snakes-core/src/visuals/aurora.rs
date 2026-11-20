// SPDX-License-Identifier: GPL-3.0-or-later
use super::{Band, Frame, Request, palette, settings::Config};

pub fn band_geometry(config: &Config, phase: f64, width: f64, height: f64, i: usize) -> [f64; 4] {
    let i = i as f64;
    let travel = if config.reduced_motion != 0 {
        0.08
    } else {
        0.14 + config.density * 0.0012
    };
    [
        width * (0.18 + i * 0.23 + (phase * 0.55 + i * 1.7).sin() * travel),
        height * (0.35 + (phase * 0.42 + i * 1.3).cos() * 0.24),
        width.max(height) * (0.30 + i * 0.025) * config.scale,
        0.24 + config.trails * 0.44,
    ]
}
pub fn frame(config: &Config, phase: f64, request: &Request) -> Frame {
    let count = (3.0 + config.density / 18.0).round().max(3.0) as usize;
    let mut frame = Frame {
        viewport: [request.width as f32, request.height as f32],
        band_count: count as u32,
        ..Frame::default()
    };
    for i in 0..count {
        frame.bands[i] = Band {
            geometry: band_geometry(config, phase, request.width, request.height, i)
                .map(|v| v as f32),
            inner: palette::color(config.palette, i),
            middle: palette::color(config.palette, i + 2),
        };
    }
    // QML's shade inherits globalAlpha from the last band. Positions 0/.55/1.
    frame.shade = [
        48.0 / 255.0,
        0.55,
        120.0 / 255.0,
        (0.24 + config.trails * 0.44) as f32,
    ];
    frame
}

/// Explicit software-backend/pipeline-failure fallback. Premultiplied RGBA8;
/// procedural math still lives in Rust. Never used by the measured GPU path.
pub fn raster(frame: &Frame, pixels: &mut [u8], width: usize, height: usize) {
    for y in 0..height {
        for x in 0..width {
            let p = [
                (x as f32 + 0.5) * frame.viewport[0] / width as f32,
                (y as f32 + 0.5) * frame.viewport[1] / height as f32,
            ];
            let mut out = [0.0f32; 4];
            for b in &frame.bands[..frame.band_count as usize] {
                let t = ((p[0] - b.geometry[0]).hypot(p[1] - b.geometry[1]) / b.geometry[2])
                    .clamp(0.0, 1.0);
                let alpha = b.geometry[3] * (2.0 - t * 2.0).min(1.0);
                let u = (t * 2.0).min(1.0);
                for (k, out) in out.iter_mut().enumerate().take(3) {
                    let color = b.inner[k] + (b.middle[k] - b.inner[k]) * u;
                    *out = color * alpha + *out * (1.0 - alpha);
                }
                out[3] = alpha + out[3] * (1.0 - alpha);
            }
            let t = p[1] / frame.viewport[1];
            let shade = if t < frame.shade[1] {
                frame.shade[0] * (1.0 - t / frame.shade[1])
            } else {
                frame.shade[2] * (t - frame.shade[1]) / (1.0 - frame.shade[1])
            } * frame.shade[3];
            for c in out.iter_mut().take(3) {
                *c *= 1.0 - shade;
            }
            out[3] = shade + out[3] * (1.0 - shade);
            for k in 0..4 {
                pixels[(y * width + x) * 4 + k] = (out[k] * 255.0).round() as u8;
            }
        }
    }
}
