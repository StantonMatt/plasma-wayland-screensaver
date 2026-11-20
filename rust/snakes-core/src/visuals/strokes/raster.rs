// SPDX-License-Identifier: GPL-3.0-or-later
use super::Vertex;
/// Length of one Canvas fill/stroke in the frozen triangle stream. Circle pairs
/// are one quad; a stroke starts with a negative butt-cap fringe, whereas every
/// subsequent quad starts at a positive arc distance.
pub fn primitive_len(vertices: &[Vertex]) -> usize {
    let mut end = 6.min(vertices.len());
    if vertices[0].parameters[1] < 0.0 {
        while end < vertices.len()
            && vertices[end].parameters[1] < 0.0
            && vertices[end].shape[1] >= 0.0
        {
            end = (end + 6).min(vertices.len());
        }
    }
    end
}
/// Explicit slow fallback only. Union maximum coverage within each Canvas
/// primitive, then add across primitives. A bounded stack tile avoids allocating
/// a full-size per-stroke mask, including at large/fractional-DPR viewports.
pub fn raster(vertices: &[Vertex], lw: f64, lh: f64, w: usize, h: usize, pixels: &mut [u8]) {
    pixels.fill(0);
    let sx = w as f64 / lw;
    let sy = h as f64 / lh;
    let mut remaining = vertices;
    while !remaining.is_empty() {
        let count = primitive_len(remaining);
        let primitive = &remaining[..count];
        remaining = &remaining[count..];
        let min_x = primitive
            .iter()
            .map(|v| v.position[0] as f64 * sx)
            .fold(f64::INFINITY, f64::min)
            .floor()
            .max(0.0)
            .min(w as f64) as usize;
        let max_x = primitive
            .iter()
            .map(|v| v.position[0] as f64 * sx)
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil()
            .max(0.0)
            .min(w as f64) as usize;
        let min_y = primitive
            .iter()
            .map(|v| v.position[1] as f64 * sy)
            .fold(f64::INFINITY, f64::min)
            .floor()
            .max(0.0)
            .min(h as f64) as usize;
        let max_y = primitive
            .iter()
            .map(|v| v.position[1] as f64 * sy)
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil()
            .max(0.0)
            .min(h as f64) as usize;
        const TILE: usize = 1024;
        for tile_x in (min_x..max_x).step_by(TILE) {
            let tile_end = (tile_x + TILE).min(max_x);
            for tile_y in min_y..max_y {
                let mut mask = [0.0f64; TILE];
                for triangle in primitive.chunks_exact(3) {
                    let mut v = [triangle[0], triangle[1], triangle[2]];
                    let mut p = v.map(|v| [v.position[0] as f64 * sx, v.position[1] as f64 * sy]);
                    let cross = |a: [f64; 2], b: [f64; 2], q: [f64; 2]| {
                        (b[0] - a[0]) * (q[1] - a[1]) - (b[1] - a[1]) * (q[0] - a[0])
                    };
                    let mut area = cross(p[0], p[1], p[2]);
                    if area.abs() < 1e-12 {
                        continue;
                    }
                    if area < 0.0 {
                        v.swap(1, 2);
                        p.swap(1, 2);
                        area = -area;
                    }
                    let left = p
                        .iter()
                        .map(|p| p[0])
                        .fold(f64::INFINITY, f64::min)
                        .floor()
                        .max(0.0) as usize;
                    let right = p
                        .iter()
                        .map(|p| p[0])
                        .fold(f64::NEG_INFINITY, f64::max)
                        .ceil()
                        .max(0.0)
                        .min(w as f64) as usize;
                    let top = p
                        .iter()
                        .map(|p| p[1])
                        .fold(f64::INFINITY, f64::min)
                        .floor()
                        .max(0.0) as usize;
                    let bottom = p
                        .iter()
                        .map(|p| p[1])
                        .fold(f64::NEG_INFINITY, f64::max)
                        .ceil()
                        .max(0.0)
                        .min(h as f64) as usize;
                    let tl =
                        |a: [f64; 2], b: [f64; 2]| b[1] < a[1] || (b[1] == a[1] && b[0] > a[0]);
                    for y in top.max(tile_y)..bottom.min(tile_y + 1) {
                        for x in left.max(tile_x)..right.min(tile_end) {
                            let q = [x as f64 + 0.5, y as f64 + 0.5];
                            let e = [
                                cross(p[1], p[2], q),
                                cross(p[2], p[0], q),
                                cross(p[0], p[1], q),
                            ];
                            if (0..3).any(|i| {
                                e[i] < 0.0 || (e[i] == 0.0 && !tl(p[(i + 1) % 3], p[(i + 2) % 3]))
                            }) {
                                continue;
                            }
                            let shape: [f64; 4] = std::array::from_fn(|j| {
                                (0..3).map(|i| e[i] / area * v[i].shape[j] as f64).sum()
                            });
                            let distance = if v[0].parameters[1] < 0.0 {
                                (v[0].parameters[0] as f64 - shape[0].abs()).min(shape[1])
                            } else {
                                (v[0].parameters[0] as f64 - shape[0].hypot(shape[1]))
                                    .max(v[0].parameters[1] as f64 - shape[2].hypot(shape[3]))
                            };
                            let coverage = (distance * sx.min(sy) + 0.5).clamp(0.0, 1.0);
                            mask[x - tile_x] = mask[x - tile_x].max(coverage);
                        }
                    }
                }
                for x in tile_x..tile_end {
                    let alpha = mask[x - tile_x] * primitive[0].color[3] as f64;
                    for k in 0..4 {
                        let value = if k == 3 {
                            alpha
                        } else {
                            alpha * primitive[0].color[k] as f64
                        };
                        let at = (tile_y * w + x) * 4 + k;
                        pixels[at] = (pixels[at] as f64 + value * 255.0).round().min(255.0) as u8;
                    }
                }
            }
        }
    }
}
