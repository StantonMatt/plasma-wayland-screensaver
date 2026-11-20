// SPDX-License-Identifier: GPL-3.0-or-later
//! Ordered compact analytic primitives. Seed constants are cached; frame building
//! uses fixed stack arrays and caller storage, with no per-frame allocation.
use super::{
    Request, palette,
    random::{positive_modulo, random},
    settings::Config,
};
pub const MAX_RECORDS: usize = 3828 + 88;
pub fn capacity(visual: u32) -> usize {
    if visual == 2 { MAX_RECORDS } else { 356 }
}
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Primitive {
    /// Line endpoints, or disc centre in xy (zw unused).
    pub geometry: [f32; 4],
    /// Straight RGB and opacity, source-over in record order.
    pub color: [f32; 4],
    /// Half width/radius, kind (0 butt line, 1 solid disc, 2 radial disc), reserved.
    pub shape: [f32; 4],
}
pub struct Seeds {
    points: [[f64; 2]; 88],
    stars: [[f64; 4]; 355],
}
impl Seeds {
    pub fn new(seed: i32) -> Self {
        let seed = seed as f64;
        Self {
            points: std::array::from_fn(|i| {
                [
                    random((i * 5 + 1) as f64, seed),
                    random((i * 5 + 2) as f64, seed),
                ]
            }),
            stars: std::array::from_fn(|i| {
                let angle = random((i * 3 + 1) as f64, seed) * std::f64::consts::PI * 2.0;
                [
                    angle.cos(),
                    angle.sin(),
                    0.05 + random((i * 3 + 2) as f64, seed) * 0.95,
                    random((i * 3 + 3) as f64, seed),
                ]
            }),
        }
    }
}
fn record(
    geometry: [f64; 4],
    palette_id: u32,
    index: usize,
    alpha: f64,
    radius: f64,
    kind: f32,
) -> Primitive {
    let mut color = palette::color(palette_id, index);
    color[3] = alpha as f32;
    Primitive {
        geometry: geometry.map(|x| x as f32),
        color,
        shape: [radius as f32, kind, 0.0, 0.0],
    }
}
pub fn point(config: &Config, seeds: &Seeds, phase: f64, request: &Request, i: usize) -> [f64; 2] {
    [
        positive_modulo(
            seeds.points[i][0] * request.width
                + (phase * config.speed + i as f64).sin() * request.width * 0.025,
            request.width,
        ),
        positive_modulo(
            seeds.points[i][1] * request.height
                + (phase * config.speed * 0.73 + i as f64).cos() * request.height * 0.035,
            request.height,
        ),
    ]
}
pub fn star(
    config: &Config,
    seeds: &Seeds,
    phase: f64,
    request: &Request,
    i: usize,
) -> ([f64; 4], f64, f64) {
    let [cos, sin, lane, offset] = seeds.stars[i];
    let travel = positive_modulo(phase * config.speed + offset, 1.0);
    let distance = travel.powf(1.65) * request.width.hypot(request.height) * lane;
    let x = request.width / 2.0 + cos * distance;
    let y = request.height / 2.0 + sin * distance;
    let tail = 2.0 + travel * 34.0 * config.trails * config.speed;
    (
        [x - cos * tail, y - sin * tail, x, y],
        (travel * 5.2 * config.scale).max(0.8),
        0.20 + travel * 0.80,
    )
}
pub fn frame(
    config: &Config,
    seeds: &Seeds,
    phase: f64,
    request: &Request,
    out: &mut [Primitive],
) -> usize {
    let mut n = 0;
    if config.visual == 2 {
        let count = (16.0 + config.density * 0.72).round() as usize;
        let mut points = [[0.0; 2]; 88];
        for (i, p) in points[..count].iter_mut().enumerate() {
            *p = point(config, seeds, phase, request, i);
        }
        let threshold = request.width.min(request.height) * (0.09 + config.density * 0.0008);
        for i in 0..count {
            for j in i + 1..count {
                let dx = points[i][0] - points[j][0];
                let dy = points[i][1] - points[j][1];
                // Match Math.sqrt(dx*dx+dy*dy), rather than hypot's rounding.
                let distance = (dx * dx + dy * dy).sqrt();
                if distance < threshold {
                    out[n] = record(
                        [points[i][0], points[i][1], points[j][0], points[j][1]],
                        config.palette,
                        i + j,
                        (1.0 - distance / threshold) * (0.12 + config.trails * 0.42),
                        config.scale.max(0.5) / 2.0,
                        0.0,
                    );
                    n += 1;
                }
            }
        }
        for (i, p) in points[..count].iter().enumerate() {
            out[n] = record(
                [p[0], p[1], 0.0, 0.0],
                config.palette,
                i,
                0.65 + (phase * 2.0 + i as f64).sin() * 0.25,
                (1.5 + (i % 3) as f64) * config.scale,
                1.0,
            );
            n += 1;
        }
    } else {
        let count = (45.0 + config.density * 3.1).round() as usize;
        for i in 0..count {
            let (g, width, alpha) = star(config, seeds, phase, request, i);
            out[n] = record(g, config.palette, i, alpha, width / 2.0, 0.0);
            n += 1;
        }
        out[n] = record(
            [request.width / 2.0, request.height / 2.0, 0.0, 0.0],
            config.palette,
            0,
            0.5,
            request.width.min(request.height) * 0.13,
            2.0,
        );
        n += 1;
    }
    n
}
/// Plain-geometry fallback uses exactly the same analytic fragment coverage.
/// Position, local position, color, shape(half width, half length, kind, unused).
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct Vertex {
    pub position: [f32; 2],
    pub local: [f32; 2],
    pub color: [f32; 4],
    pub shape: [f32; 4],
}
pub fn vertices(records: &[Primitive], fringe: f32, out: &mut [Vertex]) {
    const CORNERS: [[f32; 2]; 6] = [
        [-1.0, -1.0],
        [1.0, -1.0],
        [-1.0, 1.0],
        [-1.0, 1.0],
        [1.0, -1.0],
        [1.0, 1.0],
    ];
    for (p, quad) in records.iter().zip(out.chunks_exact_mut(6)) {
        let [x, y, x2, y2] = p.geometry;
        let radius = p.shape[0];
        let kind = p.shape[1];
        let (center, axis, half) = if kind == 0.0 {
            let dx = x2 - x;
            let dy = y2 - y;
            let len = dx.hypot(dy);
            (
                [(x + x2) * 0.5, (y + y2) * 0.5],
                if len > 0.0 {
                    [dx / len, dy / len]
                } else {
                    [1.0, 0.0]
                },
                len * 0.5,
            )
        } else {
            ([x, y], [1.0, 0.0], radius)
        };
        for (v, c) in quad.iter_mut().zip(CORNERS) {
            let local = [c[0] * (half + fringe), c[1] * (radius + fringe)];
            *v = Vertex {
                position: [
                    center[0] + axis[0] * local[0] - axis[1] * local[1],
                    center[1] + axis[1] * local[0] + axis[0] * local[1],
                ],
                local,
                color: p.color,
                shape: [radius, half, kind, 0.0],
            };
        }
    }
}
/// Software scene graph fallback; bounded primitive raster, not a full image
/// evaluation per primitive. Pixels are premultiplied RGBA8.
pub fn raster(
    records: &[Primitive],
    viewport: [f32; 2],
    pixels: &mut [u8],
    width: usize,
    height: usize,
) {
    pixels.fill(0);
    let sx = width as f32 / viewport[0];
    let sy = height as f32 / viewport[1];
    let aa = 1.0 / sx.min(sy);
    for p in records {
        let mut quad = [Vertex::default(); 6];
        vertices(std::slice::from_ref(p), aa, &mut quad);
        let minx = quad
            .iter()
            .map(|v| v.position[0])
            .fold(f32::INFINITY, f32::min);
        let maxx = quad
            .iter()
            .map(|v| v.position[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let miny = quad
            .iter()
            .map(|v| v.position[1])
            .fold(f32::INFINITY, f32::min);
        let maxy = quad
            .iter()
            .map(|v| v.position[1])
            .fold(f32::NEG_INFINITY, f32::max);
        let [x1, y1, x2, y2] = p.geometry;
        let dx = x2 - x1;
        let dy = y2 - y1;
        let len = dx.hypot(dy);
        for y in ((miny * sy).floor().max(0.0) as usize)
            ..((maxy * sy).ceil().max(0.0) as usize).min(height)
        {
            for x in ((minx * sx).floor().max(0.0) as usize)
                ..((maxx * sx).ceil().max(0.0) as usize).min(width)
            {
                let px = (x as f32 + 0.5) / sx - x1;
                let py = (y as f32 + 0.5) / sy - y1;
                let r = p.shape[0];
                let coverage = if p.shape[1] == 0.0 {
                    if len == 0.0 {
                        0.0
                    } else {
                        let along = (px * dx + py * dy) / len;
                        ((r - (px * dy - py * dx).abs() / len) / aa + 0.5).clamp(0.0, 1.0)
                            * ((along.min(len - along)) / aa + 0.5).clamp(0.0, 1.0)
                    }
                } else if p.shape[1] == 1.0 {
                    ((r - px.hypot(py)) / aa + 0.5).clamp(0.0, 1.0)
                } else {
                    (1.0 - px.hypot(py) / r).clamp(0.0, 1.0)
                };
                let alpha = p.color[3] * coverage;
                let dest = &mut pixels[(y * width + x) * 4..][..4];
                for k in 0..4 {
                    let src = if k == 3 { 1.0 } else { p.color[k] };
                    dest[k] = (src * alpha * 255.0 + dest[k] as f32 * (1.0 - alpha)).round() as u8;
                }
            }
        }
    }
}
const _: () = {
    assert!(size_of::<Primitive>() == 48);
    assert!(size_of::<Vertex>() == 48);
};
