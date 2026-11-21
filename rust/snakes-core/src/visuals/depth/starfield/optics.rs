// SPDX-License-Identifier: GPL-3.0-or-later
use super::super::{
    ffi::{Lease, View},
    world::MonitorMode,
};
use super::{
    Art, Frame, noise,
    sky::{self, Sprite},
    space::*,
    universe::{GROUP, ROWS, SHELL, Star},
    voyage::Pose,
};
use std::f64::consts::PI;

// A mild positional lens keeps the spool contraction under 5% across the cockpit.
const ABERRATION: f64 = 0.029;
pub struct Glass {
    pub f: f64,
    pub anchor: [f64; 2],
    pub cylinder: bool,
    pub width: f64,
    pub height: f64,
    pub h: f64,
}
impl Glass {
    pub fn new(a: &Art, v: &View) -> Self {
        let cylinder = a.input.mode == MonitorMode::Spanning;
        let rect = v.viewport;
        let h = if cylinder {
            a.input.layout.bounds.height
        } else {
            rect.height
        };
        let f = h / (2. * 25f64.to_radians().tan());
        let (f, anchor) = if cylinder {
            (
                f.max(a.input.layout.bounds.width / PI),
                [a.anchor[0] - rect.x, a.anchor[1] - rect.y],
            )
        } else {
            (f, [rect.width / 2., rect.height / 2.])
        };
        Self {
            f,
            anchor,
            cylinder,
            width: rect.width,
            height: rect.height,
            h,
        }
    }
    pub fn project(&self, delta: V, pose: Pose) -> Option<[f64; 2]> {
        let mut v = pose.attitude.apply(delta);
        let beta = ABERRATION * pose.warp;
        let k = (1. - beta * beta).sqrt();
        let axis = pose.attitude.apply(pose.direction);
        v = v * k + axis * ((1. - k) * v.dot(axis) + beta * v.length());
        let (x, y) = if self.cylinder {
            (v.0.atan2(v.2), v.1 / v.0.hypot(v.2).max(0.00001))
        } else {
            if v.2 <= 0.00001 {
                return None;
            }
            (v.0 / v.2, v.1 / v.2)
        };
        Some([self.anchor[0] + self.f * x, self.anchor[1] - self.f * y])
    }
}

// Support of a cell against one frustum plane after relativistic aberration.
// The linear part has an exact box support. The radial term is Lipschitz
// with constant |beta * dot(n, direction)| over the cell's half-diagonal.
#[derive(Clone, Copy)]
struct Plane {
    normal: V,
    radial: f64,
    support: f64,
}
impl Plane {
    fn new(n: V, p: Pose) -> Self {
        let n = p.attitude.inverse().apply(n.unit());
        let beta = ABERRATION * p.warp;
        let k = (1. - beta * beta).sqrt();
        let along = n.dot(p.direction);
        let normal = n * k + p.direction * ((1. - k) * along);
        let radial = beta * along;
        Self {
            normal,
            radial,
            support: 0.5 * (normal.0.abs() + normal.1.abs() + normal.2.abs())
                + 0.867 * radial.abs(),
        }
    }
    fn upper(self, delta: V, length: f64) -> f64 {
        self.normal.dot(delta) + self.radial * length + self.support
    }
}
struct Frustum {
    sides: [Plane; 4],
    vertical: [f64; 2],
    cylinder: bool,
    full_yaw: bool,
    pose: Pose,
    angular: f64,
    travel: f64,
}
impl Frustum {
    fn new(g: &Glass, p: Pose, old: Pose, star_size: f64, trail: f64) -> Self {
        let yaw = |x: f64| {
            let v = (x - g.anchor[0]) / g.f;
            if g.cylinder { v } else { v.atan() }
        };
        let guard = g.h * p.exposure.abs() * shutter_extent(trail) * 1.06;
        let (sl, cl) = yaw(-guard).sin_cos();
        let (sr, cr) = yaw(g.width + guard).sin_cos();
        let top = (g.anchor[1] + guard) / g.f;
        let bottom = (g.anchor[1] - g.height - guard) / g.f;
        let rotation = (p.attitude.0.dot(old.attitude.0) + p.attitude.1 * old.attitude.1)
            .abs()
            .min(1.);
        let sweep = 2. * rotation.acos() + 0.25 * (p.warp - old.warp).abs();
        let sides = [
            V(cl, 0., -sl),
            V(-cr, 0., sr),
            V(0., -1., top),
            V(0., 1., -bottom),
        ]
        .map(|n| Plane::new(n, p));
        Self {
            sides,
            vertical: [top, bottom],
            cylinder: g.cylinder,
            full_yaw: g.cylinder && (g.width + 2. * guard) / g.f >= PI,
            pose: p,
            angular: 2.9 * (sweep + 24. * star_size / g.f),
            travel: (p.p - old.p).length() * 1.2,
        }
    }
    fn contains(&self, d: V, scale: f64) -> bool {
        let nearest = |x: f64| (x.abs() - 0.5).max(0.);
        let q = V(nearest(d.0), nearest(d.1), nearest(d.2));
        if q.dot(q) >= 4. {
            return false;
        }
        let l = d.length();
        let fringe = self.travel / scale + self.angular;
        let sides = if self.full_yaw {
            &self.sides[..0]
        } else if self.cylinder {
            &self.sides[..2]
        } else {
            &self.sides[..]
        };
        if sides.iter().any(|p| p.upper(d, l) < -fringe) {
            return false;
        }
        if self.cylinder {
            let beta = ABERRATION * self.pose.warp;
            let k = (1. - beta * beta).sqrt();
            let boosted =
                d * k + self.pose.direction * ((1. - k) * d.dot(self.pose.direction) + beta * l);
            let v = self.pose.attitude.apply(boosted);
            let horizon = v.0.hypot(v.2);
            let r = 0.867 * (1. + beta) + fringe;
            let [top, bottom] = self.vertical;
            if v.1 - top * horizon > r * (1. + top * top).sqrt()
                || bottom * horizon - v.1 > r * (1. + bottom * bottom).sqrt()
            {
                return false;
            }
        }
        true
    }
}
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct Uniforms {
    pub projection: [f32; 4],
    pub viewport: [f32; 4],
    pub rotation: [f32; 4],
    pub old_rotation: [f32; 4],
    pub eye: [[f32; 4]; 2],
    pub shift: [f32; 4],
    pub optics: [f32; 4],
    pub surface: [f32; 4],
    pub field: [f32; 4],
    pub axis: [f32; 4],
    pub old_axis: [f32; 4],
    pub destination: [f32; 4],
    pub palette: [[f32; 4]; 6],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Upload {
    pub stars: *const Star,
    pub stamp: u64,
    pub row: u32,
    pub count: u32,
}
#[repr(C)]
pub struct Packet {
    pub uniforms: Uniforms,
    pub groups: [u32; 512],
    pub sprites: [Sprite; 16],
    pub group_count: u32,
    pub sprite_count: u32,
    pub updates: u32,
    pub debris: u32,
    pub row_capacity: u32,
    pub near_group_count: u32,
    pub upload: [Upload; ROWS],
}
impl Packet {
    pub fn sprite(&mut self, s: Sprite) {
        if let Some(out) = self.sprites.get_mut(self.sprite_count as usize) {
            *out = s;
            self.sprite_count += 1;
        }
    }
}
// Maximum optical trail as a fraction of the shared reference height.
fn shutter_extent(trails: f64) -> f64 {
    if trails <= 0.35 {
        0.0024 + (0.0815 - 0.0024) * trails / 0.35
    } else {
        0.0815 + (0.399 - 0.0815) * (trails - 0.35) / 0.65
    }
}
pub fn export(f: &Frame, v: &View, known: &[u64; ROWS], out: &mut Packet) {
    let a = &f.art;
    let still = a.input.reduced_motion;
    let t = if still {
        0.
    } else {
        v.timeline_ns as f64 * 1e-9
    };
    let now = f.pose(t);
    let before = if still {
        now
    } else {
        f.pose((t - a.shutter).max(0.))
    };
    let glass = Glass::new(a, v);
    out.row_capacity = super::universe::row_capacity(a.input.density) as u32;
    out.group_count = 0;
    out.near_group_count = 0;
    out.sprite_count = 0;
    out.updates = 0;
    out.debris = 0;
    let folded = |s: f64| {
        let p = now.p * (1. / s);
        V(p.0.rem_euclid(5.), p.1.rem_euclid(5.), p.2.rem_euclid(5.)).four(s)
    };
    let vanishing = glass.project(now.direction, now).unwrap_or(glass.anchor);
    let extent = shutter_extent(a.input.trails);
    // At a long shutter the spool is still a small anticipation. At punch the
    // exposure opens continuously onto the full setting-dependent travel.
    let exposure = if now.bow == 0. && now.warp > 0. && now.exposure > 0. {
        let spool = (0.0195 / extent).min(0.23);
        now.exposure - (0.23 - spool) * now.warp * (1. - now.surge)
    } else {
        now.exposure
    };
    out.uniforms = Uniforms {
        axis: now.attitude.apply(now.direction).four(exposure),
        old_axis: before.attitude.apply(before.direction).four(extent),
        projection: [
            glass.f as f32,
            glass.anchor[0] as f32,
            glass.anchor[1] as f32,
            f32::from(glass.cylinder),
        ],
        viewport: [
            glass.width as f32,
            glass.height as f32,
            glass.h as f32,
            (1. / v.dpr) as f32,
        ],
        rotation: now.attitude.0.four(now.attitude.1),
        old_rotation: before.attitude.0.four(before.attitude.1),
        eye: SHELL.map(folded),
        shift: (now.p - before.p).four(before.warp),
        optics: [
            now.warp as f32,
            t as f32,
            a.input.scale as f32,
            now.surge as f32,
        ],
        surface: [
            a.params.nebulae as f32 * 0.01,
            a.input.environment.background as f32,
            if still {
                1.
            } else {
                f.leg_at(t).visibility(t) as f32
            },
            a.input.palette as f32,
        ],
        field: [
            (a.input.seed % 4093) as f32 / 4093.,
            now.bow as f32,
            vanishing[0] as f32,
            vanishing[1] as f32,
        ],
        destination: ((if still {
            a.vista
        } else {
            f.leg_at(t).destination
        })
        .p - now.p)
            .four(if still {
                0.
            } else {
                smooth((t - f.leg_at(t).arrival_start()) / 11.) * f.leg_at(t).visibility(t)
            }),
        palette: a.palette,
    };
    let frustum = Frustum::new(&glass, now, before, a.input.scale, a.input.trails);
    for (slot, row) in f.atlas.iter().enumerate() {
        if slot == 125 {
            out.near_group_count = out.group_count;
        }
        let layer = slot / 125;
        let delta = row.cell.base() + V(0.5, 0.5, 0.5) - now.p * (1. / SHELL[layer]);
        if !frustum.contains(delta, SHELL[layer]) {
            continue;
        }
        if known[slot] != row.stamp {
            out.upload[out.updates as usize] = Upload {
                stars: row.stars.as_ptr(),
                stamp: row.stamp,
                row: slot as u32,
                count: row.stars.len() as u32,
            };
            out.updates += 1;
        }
        let n = row.stars.len();
        for group in 0..n.div_ceil(GROUP) {
            let index = out.group_count as usize;
            let Some(pair) = out.groups.get_mut(index / 2) else {
                break;
            };
            let used = (n - group * GROUP).min(GROUP).div_ceil(4);
            let packed =
                (slot * (super::universe::WIDTH / GROUP) + group + ((used - 1) << 11)) as u32;
            if index.is_multiple_of(2) {
                *pair = packed
            } else {
                *pair |= packed << 16
            }
            out.group_count += 1;
        }
    }
    sky::compose(f, now, before, &glass, t, out);
}
/// # Safety
/// The retained sample owns all returned row pointers. The render thread owns
/// the 250 resident stamps and writable packet for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_starfield_sample(
    lease: *const Lease,
    known: *const [u64; ROWS],
    out: *mut Packet,
) -> bool {
    let (Some(l), Some(known), Some(out)) = (
        unsafe { lease.as_ref() },
        unsafe { known.as_ref() },
        unsafe { out.as_mut() },
    ) else {
        return false;
    };
    let Some(frame) = l
        .sample
        .upper
        .typed
        .as_ref()
        .and_then(|p| p.downcast_ref::<Frame>())
    else {
        return false;
    };
    export(frame, &l.view, known, out);
    true
}
#[unsafe(no_mangle)]
pub extern "C" fn visuals_starfield_noise() -> *const u8 {
    noise::pixels().as_ptr()
}
const _: () = {
    assert!(size_of::<Uniforms>() == 304);
    assert!(size_of::<Star>() == 32);
    assert!(size_of::<Sprite>() == 48);
};

/// Adapt the legacy Rust-projected dashes on software surfaces. That renderer
/// has no jump phase, so its optional radial fog record is omitted.
/// # Safety
/// The caller owns the writable primitive slice for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_starfield_software_material(
    records: *mut crate::visuals::primitives::Primitive,
    count: u32,
    palette: u32,
) -> u32 {
    use crate::visuals::{depth::common::rng::hash64, palette::color};
    if records.is_null() || count == 0 || count > 356 || palette >= 6 {
        return count;
    }
    let slice = unsafe { std::slice::from_raw_parts_mut(records, count as usize) };
    let n = if slice.last().is_some_and(|p| p.shape[1] == 2.) {
        slice.len() - 1
    } else {
        slice.len()
    };
    let ramps = [
        [3, 2, 1, 0],
        [5, 1, 2, 4],
        [3, 2, 1, 0],
        [3, 2, 5, 0],
        [3, 2, 1, 0],
        [4, 0, 1, 1],
    ];
    for (i, p) in slice[..n].iter_mut().enumerate() {
        let value = ((hash64(i as u64 + 71) >> 40) as f32 / 16_777_216.) * 3.;
        let k = (value as usize).min(2);
        let ramp = ramps[palette as usize];
        let a = color(palette, ramp[k]);
        let b = color(palette, ramp[k + 1]);
        let white = if palette == 1 {
            0.55
        } else if palette == 2 {
            0.07
        } else {
            0.20
        };
        for c in 0..3 {
            let v = a[c] + (b[c] - a[c]) * (value - k as f32);
            p.color[c] = v + (1. - v) * white;
        }
    }
    n as u32
}
