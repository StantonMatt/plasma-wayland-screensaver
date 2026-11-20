// SPDX-License-Identifier: GPL-3.0-or-later
//! Allocation-free f64 path construction and tessellation. Independent additive
//! ABI extension: existing visual and Snakes structs, versions and symbols stay frozen.
pub mod kaleidoscope;
pub mod mesh;
mod raster;
pub mod ribbons;
use super::{Request, palette, settings::Config};
use std::ops::{Add, Mul, Sub};
pub const RIBBONS: u32 = 4;
pub const KALEIDOSCOPE: u32 = 5;
pub const STROKES_ABI_VERSION: u32 = 1;
#[derive(Clone, Copy, Default, Debug)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    pub fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y
    }
    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }
    pub fn unit(self) -> Self {
        let d = self.length();
        if d > 1e-12 {
            self * (1.0 / d)
        } else {
            Self::default()
        }
    }
    pub fn transform(self, angle: f64, origin: Self) -> Self {
        let (s, c) = angle.sin_cos();
        Self::new(
            origin.x + self.x * c - self.y * s,
            origin.y + self.x * s + self.y * c,
        )
    }
}
impl Add for Point {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        Self::new(self.x + b.x, self.y + b.y)
    }
}
impl Sub for Point {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        Self::new(self.x - b.x, self.y - b.y)
    }
}
impl Mul<f64> for Point {
    type Output = Self;
    fn mul(self, b: f64) -> Self {
        Self::new(self.x * b, self.y * b)
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Vertex {
    pub position: [f32; 2],
    pub shape: [f32; 4],
    pub parameters: [f32; 2],
    pub color: [f32; 4],
}
pub struct Core {
    config: Config,
    phase: f64,
}
fn valid(c: &Config) -> bool {
    matches!(c.visual, RIBBONS | KALEIDOSCOPE) && Config { visual: 1, ..*c }.valid()
}
fn valid_request(r: &Request) -> bool {
    r.width.is_finite()
        && r.height.is_finite()
        && r.width > 0.0
        && r.height > 0.0
        && r.width <= 65536.0
        && r.height <= 65536.0
        && r.advance <= 1
        && r.reserved == 0
        && r.world_bounds
            .iter()
            .chain(r.viewport_origin.iter())
            .all(|v| v.is_finite())
        && r.world_bounds[2] >= 0.0
        && r.world_bounds[3] >= 0.0
}
pub fn vertex_count(c: &Config, r: &Request) -> usize {
    let steps = mesh::steps(c.visual, r.width, r.height, c.scale);
    if c.visual == RIBBONS {
        (3.0 + c.density / 12.0).round() as usize
            * if c.trails > 0.25 { 2 } else { 1 }
            * 2
            * steps
            * 6
    } else {
        (5.0 + c.density / 9.0).round() as usize
            * (3.0 + c.density / 18.0).round() as usize
            * (6 + steps * 6)
    }
}
fn color(c: &Config, index: usize, alpha: f64) -> [f32; 4] {
    let mut rgb = palette::color(c.palette, index);
    rgb[3] = alpha as f32;
    rgb
}
pub fn frame(c: &Config, phase: f64, r: &Request, vertices: &mut [Vertex]) -> usize {
    let steps = mesh::steps(c.visual, r.width, r.height, c.scale);
    let mut writer = mesh::Writer { vertices, count: 0 };
    let mut points = [Point::default(); mesh::MAX_STEPS * 2 + 1];
    if c.visual == RIBBONS {
        ribbons::paths(c, phase, r.width, r.height, |path| {
            for (i, p) in points[..steps * 2 + 1].iter_mut().enumerate() {
                *p = if i <= steps {
                    mesh::cubic(&path.points[..4], i as f64 / steps as f64)
                } else {
                    mesh::cubic(&path.points[3..], (i - steps) as f64 / steps as f64)
                };
            }
            writer.stroke(
                &points[..steps * 2 + 1],
                path.width,
                color(c, path.color, path.alpha),
            );
        });
    } else {
        let origin = Point::new(r.width * 0.5, r.height * 0.5);
        kaleidoscope::shapes(c, phase, r.width, r.height, |shape| {
            writer.circles(
                shape.circles.map(|p| p.transform(shape.angle, origin)),
                shape.radii,
                color(c, shape.color, shape.alpha),
            );
            for (i, p) in points[..steps + 1].iter_mut().enumerate() {
                *p = mesh::quadratic(&shape.curve, i as f64 / steps as f64)
                    .transform(shape.angle, origin);
            }
            writer.stroke(
                &points[..steps + 1],
                shape.width,
                color(c, shape.color + 2, shape.alpha),
            );
        });
    }
    writer.count
}
#[unsafe(no_mangle)]
pub extern "C" fn visuals_strokes_abi_version() -> u32 {
    STROKES_ABI_VERSION
}
/// # Safety
/// Config must be readable. Visual IDs 4/5; existing v1 config layout.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_strokes_create(config: *const Config) -> *mut Core {
    let Some(c) = (unsafe { config.as_ref() }) else {
        return std::ptr::null_mut();
    };
    if !valid(c) {
        return std::ptr::null_mut();
    }
    Box::into_raw(Box::new(Core {
        config: *c,
        phase: 0.0,
    }))
}
/// # Safety
/// Live exclusive handle, readable config; no pointers retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_strokes_configure(core: *mut Core, config: *const Config) -> i32 {
    let (Some(core), Some(c)) = (unsafe { core.as_mut() }, unsafe { config.as_ref() }) else {
        return -1;
    };
    if !valid(c) {
        return -1;
    }
    // Canvas phase survives setting/seed changes, and freezes on reduced motion.
    core.config = *c;
    0
}
/// # Safety
/// Destroy a live exclusive handle exactly once; null accepted.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_strokes_destroy(core: *mut Core) {
    if !core.is_null() {
        drop(unsafe { Box::from_raw(core) })
    }
}
/// # Safety
/// Exclusive handle; readable request; returns required vertices, zero on invalid input.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_strokes_vertex_count(
    core: *const Core,
    request: *const Request,
) -> usize {
    let (Some(core), Some(r)) = (unsafe { core.as_ref() }, unsafe { request.as_ref() }) else {
        return 0;
    };
    if !valid_request(r) {
        return 0;
    }
    vertex_count(&core.config, r)
}
/// # Safety
/// Exclusive live handle, readable request. Output is caller-owned, aligned,
/// initialised and writable for capacity vertices. Failure (-1) changes neither phase nor output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_strokes_frame(
    core: *mut Core,
    request: *const Request,
    out: *mut Vertex,
    capacity: usize,
) -> i32 {
    let (Some(core), Some(r)) = (unsafe { core.as_mut() }, unsafe { request.as_ref() }) else {
        return -1;
    };
    if !valid_request(r) || out.is_null() || capacity < vertex_count(&core.config, r) {
        return -1;
    }
    if r.advance != 0 && core.config.reduced_motion == 0 {
        let elapsed = (r.presentation_ns as i128 - core.config.epoch_ns as i128) as f64 * 1e-9;
        core.phase = elapsed
            * if core.config.visual == RIBBONS {
                0.55
            } else {
                1.0
            };
    }
    frame(&core.config, core.phase, r, unsafe {
        std::slice::from_raw_parts_mut(out, vertex_count(&core.config, r))
    }) as i32
}
/// # Safety
/// Vertices readable, RGBA8 output initialised and writable for capacity bytes, nonoverlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_strokes_raster(
    vertices: *const Vertex,
    count: usize,
    logical_width: f64,
    logical_height: f64,
    width: u32,
    height: u32,
    pixels: *mut u8,
    capacity: usize,
) -> i32 {
    let Some(bytes) = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
    else {
        return -1;
    };
    if vertices.is_null()
        || pixels.is_null()
        || count % 3 != 0
        || count > 500000
        || bytes > capacity
        || width == 0
        || height == 0
        || !logical_width.is_finite()
        || !logical_height.is_finite()
        || logical_width <= 0.0
        || logical_height <= 0.0
    {
        return -1;
    }
    let vertices = unsafe { std::slice::from_raw_parts(vertices, count) };
    if vertices.iter().any(|v| {
        v.position
            .iter()
            .chain(v.shape.iter())
            .chain(v.parameters.iter())
            .chain(v.color.iter())
            .any(|x| !x.is_finite())
    }) {
        return -1;
    }
    raster::raster(
        vertices,
        logical_width,
        logical_height,
        width as usize,
        height as usize,
        unsafe { std::slice::from_raw_parts_mut(pixels, bytes) },
    );
    0
}
const _: () = {
    assert!(size_of::<Vertex>() == 48);
    assert!(std::mem::offset_of!(Vertex, color) == 32);
};
#[cfg(test)]
mod tests;
