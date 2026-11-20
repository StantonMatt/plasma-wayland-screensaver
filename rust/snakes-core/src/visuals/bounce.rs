// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounce compatibility modes. Independent QML physics is deliberately distinct
//! from AnimationState shared physics. Fixed storage includes all eight trails.
use super::{Request, palette, primitives::Primitive, random::random, settings::Config};
pub const VISUAL: u32 = 7;
pub const MAX_RECORDS: usize = 180;
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub abi_version: u32,
    pub struct_size: u32,
    pub count: u32,
    pub mode: u32,
    pub gravity: f64,
    pub elasticity: f64,
    pub collisions: u32,
    pub reserved: u32,
}
impl Settings {
    fn valid(&self) -> bool {
        self.abi_version == 1
            && self.struct_size as usize == size_of::<Self>()
            && (1..=20).contains(&self.count)
            && self.mode <= 2
            && self.collisions <= 1
            && self.reserved == 0
            && self.gravity.is_finite()
            && (-100.0..=100.0).contains(&self.gravity)
            && self.elasticity.is_finite()
            && (50.0..=100.0).contains(&self.elasticity)
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ball {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub size: f64,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Body {
    pub ball: Ball,
    pub trail: [[f64; 2]; 8],
    pub trail_count: usize,
}
pub struct Bounce {
    config: Config,
    settings: Settings,
    pub bodies: [Body; 20],
    width: f64,
    height: f64,
    accumulator: f64,
    prior_size: [f64; 2],
}
fn valid(c: &Config, s: &Settings) -> bool {
    let mut common = *c;
    common.visual = 3;
    c.visual == VISUAL && common.valid() && s.valid()
}
impl Bounce {
    pub fn new(config: Config, settings: Settings) -> Self {
        Self {
            config,
            settings,
            bodies: [Body::default(); 20],
            width: 0.0,
            height: 0.0,
            accumulator: 0.0,
            prior_size: [0.0; 2],
        }
    }
    pub fn configure(&mut self, c: Config, s: Settings) {
        if self.config.seed != c.seed
            || self.config.scale != c.scale
            || self.settings.count != s.count
            || self.settings.mode != s.mode
        {
            self.prior_size = [0.0; 2];
            self.accumulator = 0.0;
        }
        if self.config.reduced_motion != c.reduced_motion {
            self.accumulator = 0.0;
        }
        self.config = c;
        self.settings = s;
    }
    pub fn initialize(&mut self, width: f64, height: f64) {
        self.width = width;
        self.height = height;
        self.prior_size = [width, height];
        self.accumulator = 0.0;
        let base = (width.min(height) * 0.075 * self.config.scale).clamp(24.0, 220.0);
        for (i, body) in self.bodies[..self.settings.count as usize]
            .iter_mut()
            .enumerate()
        {
            let size = base * (0.78 + (i * 37 % 45) as f64 / 100.0);
            *body = Body {
                ball: Ball {
                    x: random((i * 5 + 1) as f64, self.config.seed as f64)
                        * (width - size).max(1.0),
                    y: random((i * 5 + 2) as f64, self.config.seed as f64)
                        * (height - size).max(1.0),
                    vx: (if i % 2 != 0 { -1.0 } else { 1.0 }) * (118 + i * 47 % 125) as f64,
                    vy: (if i % 3 != 0 { 1.0 } else { -1.0 }) * (86 + i * 61 % 110) as f64,
                    size,
                },
                ..Body::default()
            };
        }
    }
    pub fn step(&mut self, seconds: f64) {
        let dt = seconds.min(0.05) * self.config.speed;
        let gravity = self.settings.gravity * 8.0;
        let elasticity = self.settings.elasticity / 100.0;
        let trails = (self.config.trails * 100.0 / 12.0).round() as usize;
        for body in &mut self.bodies[..self.settings.count as usize] {
            let b = &mut body.ball;
            b.vy += gravity * dt;
            if trails > 0 {
                // QML unshift + one pop: shrinking the setting retains the old
                // length until reset. Do not truncate it to the new setting.
                let n = (body.trail_count + 1).min(8);
                body.trail.copy_within(0..n - 1, 1);
                body.trail[0] = [b.x, b.y];
                body.trail_count = if body.trail_count + 1 > trails {
                    body.trail_count
                } else {
                    n
                };
            } else {
                body.trail_count = 0;
            }
            b.x += b.vx * dt;
            b.y += b.vy * dt;
            if b.x < 0.0 {
                b.x = 0.0;
                b.vx = b.vx.abs() * elasticity;
            }
            if b.x + b.size > self.width {
                b.x = (self.width - b.size).max(0.0);
                b.vx = -b.vx.abs() * elasticity;
            }
            if b.y < 0.0 {
                b.y = 0.0;
                b.vy = b.vy.abs() * elasticity;
            }
            if b.y + b.size > self.height {
                b.y = (self.height - b.size).max(0.0);
                b.vy = -b.vy.abs() * elasticity;
                if b.vy.abs() < 28.0 && gravity.abs() > 1.0 {
                    b.vy = if gravity > 0.0 { -72.0 } else { 72.0 };
                }
            }
        }
        if self.settings.collisions == 0 {
            return;
        }
        for i in 0..self.settings.count as usize {
            for j in i + 1..self.settings.count as usize {
                let (left, right) = self.bodies.split_at_mut(j);
                let a = &mut left[i].ball;
                let b = &mut right[0].ball;
                let dx = (b.x + b.size / 2.0) - (a.x + a.size / 2.0);
                let dy = (b.y + b.size / 2.0) - (a.y + a.size / 2.0);
                let distance = (dx * dx + dy * dy).sqrt();
                let minimum = (a.size + b.size) / 2.0;
                if distance <= 0.001 || distance >= minimum {
                    continue;
                }
                let nx = dx / distance;
                let ny = dy / distance;
                let relative = (b.vx - a.vx) * nx + (b.vy - a.vy) * ny;
                if relative < 0.0 {
                    let impulse = -(1.0 + elasticity) * relative / 2.0;
                    a.vx -= nx * impulse;
                    a.vy -= ny * impulse;
                    b.vx += nx * impulse;
                    b.vy += ny * impulse;
                }
                let correction = (minimum - distance) / 2.0;
                a.x -= nx * correction;
                a.y -= ny * correction;
                b.x += nx * correction;
                b.y += ny * correction;
                a.x = a.x.min(self.width - a.size).max(0.0);
                a.y = a.y.min(self.height - a.size).max(0.0);
                b.x = b.x.min(self.width - b.size).max(0.0);
                b.y = b.y.min(self.height - b.size).max(0.0);
            }
        }
    }
    pub fn frame(
        &mut self,
        r: &Request,
        delta_seconds: f64,
        shared: &[Ball],
        out: &mut [Primitive],
    ) -> usize {
        if self.settings.mode == 0 {
            if (r.width - self.prior_size[0]).abs() > 1.0
                || (r.height - self.prior_size[1]).abs() > 1.0
            {
                self.initialize(r.width, r.height);
            }
            self.width = r.width;
            self.height = r.height;
            if r.advance != 0 && self.config.reduced_motion == 0 {
                self.accumulator += delta_seconds.min(0.1);
                while self.accumulator + 1e-10 >= 1.0 / 60.0 {
                    self.step(1.0 / 60.0);
                    self.accumulator -= 1.0 / 60.0;
                }
            }
        }
        let count = if self.settings.mode == 0 {
            self.settings.count as usize
        } else {
            shared.len()
        };
        let mut n = 0;
        let scale = if self.settings.mode == 1 {
            [
                r.width / r.world_bounds[2].max(1.0),
                r.height / r.world_bounds[3].max(1.0),
            ]
        } else {
            [1.0, 1.0]
        };
        let origin = if self.settings.mode == 2 {
            r.viewport_origin
        } else {
            [0.0, 0.0]
        };
        for i in 0..count {
            let b = if self.settings.mode == 0 {
                self.bodies[i].ball
            } else {
                shared[i]
            };
            let color = palette::color(self.config.palette, i);
            let mut emit = |x: f64, y: f64, radius: f64, alpha: f64, kind: f32| {
                let mut color = color;
                color[3] = alpha as f32;
                out[n] = Primitive {
                    geometry: [
                        ((x - origin[0]) * scale[0]) as f32,
                        ((y - origin[1]) * scale[1]) as f32,
                        scale[0] as f32,
                        scale[1] as f32,
                    ],
                    color,
                    shape: [radius as f32, kind, 0.0, 0.0],
                };
                n += 1;
            };
            let velocity = (b.vx * b.vx + b.vy * b.vy).sqrt().max(1.0);
            if self.settings.mode == 2 {
                let trails = (self.config.trails * 100.0 / 20.0).round() as usize;
                for t in 0..trails {
                    let size = b.size * (0.72 - t as f64 * 0.08);
                    let distance = (t + 1) as f64 * b.size * 0.28;
                    emit(
                        b.x - b.vx / velocity * distance + size / 2.0,
                        b.y - b.vy / velocity * distance + size / 2.0,
                        size / 2.0,
                        (0.22 - t as f64 * 0.035).max(0.04),
                        1.0,
                    );
                }
                emit(
                    b.x + b.size / 2.0,
                    b.y + b.size / 2.0,
                    b.size / 2.0,
                    1.0,
                    4.0,
                );
            } else {
                let trails = if self.settings.mode == 0 {
                    self.bodies[i].trail_count
                } else {
                    (self.config.trails * 100.0 / 12.0).round() as usize
                };
                for t in (0..trails).rev() {
                    let [x, y] = if self.settings.mode == 0 {
                        self.bodies[i].trail[t]
                    } else {
                        [
                            b.x - b.vx / velocity * (t + 1) as f64 * b.size * 0.28,
                            b.y - b.vy / velocity * (t + 1) as f64 * b.size * 0.28,
                        ]
                    };
                    emit(
                        x + b.size / 2.0,
                        y + b.size / 2.0,
                        b.size * 0.42,
                        (1.0 - t as f64 / trails.max(1) as f64) * 0.16,
                        if self.settings.mode == 1 { 5.0 } else { 1.0 },
                    );
                }
                emit(
                    b.x + b.size / 2.0,
                    b.y + b.size / 2.0,
                    b.size / 2.0,
                    1.0,
                    3.0,
                );
            }
        }
        n
    }
}
/// # Safety
/// Config/settings readable. The returned single-threaded handle owns its storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_bounce_create(
    c: *const Config,
    s: *const Settings,
) -> *mut Bounce {
    let (Some(c), Some(s)) = (unsafe { c.as_ref() }, unsafe { s.as_ref() }) else {
        return std::ptr::null_mut();
    };
    if !valid(c, s) {
        return std::ptr::null_mut();
    };
    Box::into_raw(Box::new(Bounce::new(*c, *s)))
}
/// # Safety
/// Exclusive live handle; config/settings readable. Invalid input leaves state unchanged.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_bounce_configure(
    b: *mut Bounce,
    c: *const Config,
    s: *const Settings,
) -> i32 {
    let (Some(b), Some(c), Some(s)) = (unsafe { b.as_mut() }, unsafe { c.as_ref() }, unsafe {
        s.as_ref()
    }) else {
        return -1;
    };
    if !valid(c, s) {
        return -1;
    };
    b.configure(*c, *s);
    0
}
/// # Safety
/// Destroy a live handle once. Null accepted.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_bounce_destroy(b: *mut Bounce) {
    if !b.is_null() {
        drop(unsafe { Box::from_raw(b) })
    }
}
/// # Safety
/// Exclusive handle; readable request/shared; writable out capacity records and count.
/// Buffers must not overlap. Invalid input leaves output/count/state unchanged.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_bounce_frame(
    b: *mut Bounce,
    r: *const Request,
    delta_seconds: f64,
    shared: *const Ball,
    shared_count: usize,
    out: *mut Primitive,
    capacity: usize,
    count: *mut u32,
) -> i32 {
    let (Some(b), Some(r)) = (unsafe { b.as_mut() }, unsafe { r.as_ref() }) else {
        return -1;
    };
    if !delta_seconds.is_finite()
        || delta_seconds < 0.0
        || !r.valid()
        || out.is_null()
        || count.is_null()
        || capacity < MAX_RECORDS
        || shared_count > 20
        || (shared_count > 0 && shared.is_null())
    {
        return -1;
    }
    let shared = if shared_count == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(shared, shared_count) }
    };
    if shared
        .iter()
        .any(|b| ![b.x, b.y, b.vx, b.vy, b.size].iter().all(|v| v.is_finite()) || b.size <= 0.0)
    {
        return -1;
    }
    let n = b.frame(r, delta_seconds, shared, unsafe {
        std::slice::from_raw_parts_mut(out, capacity)
    });
    unsafe { count.write(n as u32) };
    0
}
#[cfg(test)]
mod tests;
const _: () = {
    assert!(size_of::<Settings>() == 40);
    assert!(size_of::<Ball>() == 40);
};
