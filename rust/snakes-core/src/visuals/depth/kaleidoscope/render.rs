//! Frozen chamber cuts live for a cell recipe. Only transforms and the optical
//! state are interpolated per presentation, before the GPU multiplies the glass.
use super::{Chamber, Kaleidoscope, Params, WorldInput, chamber::MAX};
use crate::visuals::{
    depth::{
        common::layout::Rect,
        ffi::{Lease, View},
        world::MonitorMode,
    },
    palette,
};
use std::sync::Arc;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Template {
    pub vertices: [[f32; 2]; 6],
    pub shape: [f32; 4],
    pub material: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Pose(pub [f32; 4]);
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Lens {
    pub eye: [f32; 4],
    pub motion: [f32; 4],
    pub glass: [f32; 4],
    pub light: [f32; 4],
    pub flare: [f32; 4],
    pub colors: [[f32; 4]; 6],
    pub rim: [f32; 4],
    pub guide: [f32; 4], // chamber offset at the apex, glint radius and wedge fraction
    pub mote: [f32; 4],  // radius, wedge fraction and light
}
const _: () =
    assert!(size_of::<Template>() == 80 && size_of::<Pose>() == 16 && size_of::<Lens>() == 224);

pub(super) struct Optics {
    pub count: usize,
    pub glass: u32,
    host: Rect,
    spanning: bool,
    mirrored: bool,
    base: Lens,
}
impl Optics {
    pub fn new(input: &WorldInput, p: &Params) -> Self {
        let center = input.layout.bounds.centre();
        let host = input
            .layout
            .outputs
            .iter()
            .max_by_key(|o| (o.rect.contains(center), o.rect.area() as u64, o.primary))
            .unwrap()
            .rect;
        let colors = std::array::from_fn(|i| palette::color(input.palette, i));
        let mut rim = colors
            .iter()
            .copied()
            .max_by(|a, b| {
                let lum = |c: [f32; 4]| c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722;
                lum(*a).total_cmp(&lum(*b))
            })
            .unwrap();
        rim[3] = input.palette as f32;
        let base = Lens {
            eye: [0., 0., 1., p.points as f32],
            motion: [0., 0., 0., (p.mirrors == "lattice") as u8 as f32],
            glass: [
                input.trails as f32,
                input.scale as f32,
                (input.palette == 5) as u8 as f32,
                1.,
            ],
            colors,
            rim,
            ..Lens::default()
        };
        Self {
            count: (14. + (input.density - 10.) * 50. / 90.).round() as usize,
            glass: match p.glass {
                "beads" => 1,
                "shards" => 2,
                "rods-wire" => 3,
                _ => 0,
            },
            host,
            spanning: input.mode == MonitorMode::Spanning,
            mirrored: input.mode == MonitorMode::Mirrored,
            base,
        }
    }
    fn eye(&self, v: &View) -> [f32; 4] {
        let r = if self.spanning { self.host } else { v.viewport };
        let c = r.centre();
        [
            (c.x - v.viewport.x) as f32,
            (c.y - v.viewport.y) as f32,
            (0.47 * r.width.min(r.height)) as f32,
            self.base.eye[3],
        ]
    }
}
pub(super) struct Cell {
    pub revision: u64,
    cuts: Vec<Template>,
}
impl Cell {
    pub fn new(chamber: &Chamber, revision: u64) -> Self {
        let mut cuts = Vec::with_capacity(chamber.bodies.len() + 1);
        for b in &chamber.bodies {
            let c = &b.cut;
            cuts.push(Template {
                vertices: c.core.map(|p| [p.0 as f32, p.1 as f32]),
                shape: [
                    c.reach as f32,
                    c.rounding as f32,
                    c.sides as f32,
                    c.back as u8 as f32,
                ],
                material: [c.hue as f32, c.dichroic as u8 as f32, 0., 0.],
            });
        }
        cuts.push(Template {
            shape: [0.012, 0.012, 1., 0.],
            material: [0., 0., 1., 0.],
            ..Template::default()
        });
        Self { revision, cuts }
    }
}
pub(super) struct Frame {
    optics: Arc<Optics>,
    cell: Arc<Cell>,
    poses: [Pose; MAX + 1],
    time: f64,
    angle: f64,
    light_angle: f64,
    cloud: f64,
    aperture: f64,
    flare: [f32; 4],
    guide: [f32; 4],
    mote: [f32; 4],
    speed: f64,
}
impl Frame {
    pub fn new(optics: Arc<Optics>, cell: Arc<Cell>) -> Self {
        Self {
            optics,
            cell,
            poses: [Pose::default(); MAX + 1],
            time: 0.,
            angle: 0.,
            light_angle: 0.,
            cloud: 0.,
            aperture: 1.,
            flare: [0., 0., -20., -20.],
            guide: [0.; 4],
            mote: [0.; 4],
            speed: 1.,
        }
    }
    pub(super) fn update(&mut self, w: &Kaleidoscope) {
        self.cell.clone_from(&w.cell);
        self.time = w.moments.time;
        self.angle = w.hand.angle;
        self.light_angle = w.light_angle();
        self.cloud = w.moments.cloud();
        self.aperture = w.moments.aperture();
        self.speed = w.input.speed.min(super::PHYSICAL_SPEED_LIMIT);
        let q = w.moments.glint_point;
        let glint_age = w.moments.age(super::GLINT);
        let swap_age = w.moments.age(super::SWAP);
        self.guide = [
            w.focus.0 as f32,
            w.focus.1 as f32,
            w.moments.glint_sector.0 as f32,
            w.moments.glint_sector.1 as f32,
        ];
        self.flare = [
            q.0 as f32,
            q.1 as f32,
            glint_age.min(10.) as f32,
            swap_age.min(10.) as f32,
        ];
        let glint = w.moments.glint() as f32;
        for (i, b) in w.chamber.bodies.iter().enumerate() {
            // Slide each piece past the outer edge, with a little separation
            // between pieces. The optics stay open throughout the exchange.
            let stagger = (i % 7) as f64 * 0.027;
            let travel = if swap_age < 3.25 {
                // Drift visibly across the sector before clearing the short
                // remaining guard band beyond the illuminated outer rim.
                let slide = (swap_age - 0.44 - stagger) / 3.27;
                (slide + 0.21 * super::smooth((swap_age - 2.85) / 0.40)).clamp(0., 1.)
            } else if swap_age < 4. {
                1.
            } else if swap_age < 7. {
                let progress = ((swap_age - 4. - stagger) / (3. - stagger)).clamp(0., 1.);
                (1. - progress).powi(2)
                    - 0.035 * (progress * 10.).sin() * (1. - progress) * progress
            } else {
                0.
            };
            let exit = 1.04 + b.cut.reach;
            self.poses[i] = Pose([
                (b.pos.0 + (exit - b.pos.0) * travel) as f32,
                (b.pos.1 - 0.22 * travel) as f32,
                (b.angle + travel * (if i % 2 == 0 { 0.40 } else { -0.31 })) as f32,
                if i == w.moments.glint { glint } else { 0. },
            ]);
        }
        let dust = (w.moments.dust() * self.aperture) as f32;
        self.mote = [0.; 4];
        if dust == 0. {
            self.poses[w.chamber.bodies.len()] = Pose::default();
            return;
        }
        let age = w.moments.age(super::DUST);
        let radius = 0.58 + 0.23 * (age * 0.20).sin();
        let side = 0.50 + 0.32 * (age * 0.37).sin();
        self.mote = [radius as f32, side as f32, dust, 0.];
        let p = w.viewed_point(radius, side);
        self.poses[w.chamber.bodies.len()] = Pose([p.0 as f32, p.1 as f32, 0., dust]);
    }
    fn lens(&self, b: &Self, v: &View, t: f64) -> Lens {
        let mix = |x, y| x + (y - x) * t;
        let mut lens = self.optics.base;
        lens.eye = self.optics.eye(v);
        let time = mix(self.time, b.time) * b.speed;
        let tube = mix(self.angle, b.angle);
        lens.motion[0] = (tube * if lens.motion[3] > 0.5 { 0.08 } else { 0.30 }) as f32;
        lens.motion[1] = (time.to_radians() * 0.5 % std::f64::consts::TAU) as f32;
        // Three equilateral cells along x repeat the three vertex colours.
        lens.motion[2] = if self.optics.mirrored {
            // SharedCanonical supplies the reference output as the world layout.
            // Keep its approved 20 px/s drift, scaling pixels for each output.
            let host = self.optics.host;
            let reference_radius = f64::from((0.47 * host.width.min(host.height)) as f32);
            let phase = (time * 20.) % (3. * reference_radius * 0.68);
            (phase * (f64::from(lens.eye[2]) / reference_radius)) as f32
        } else {
            ((time * 20.) % (3. * f64::from(lens.eye[2]) * 0.68)) as f32
        };
        lens.glass[3] = mix(self.aperture, b.aperture) as f32;
        lens.flare = b.flare;
        lens.guide = b.guide;
        lens.mote = b.mote;
        for i in 0..2 {
            lens.guide[i] = self.guide[i] + (b.guide[i] - self.guide[i]) * t as f32;
        }
        if self.mote[2] > 0. && b.mote[2] > 0. {
            lens.mote =
                std::array::from_fn(|i| self.mote[i] + (b.mote[i] - self.mote[i]) * t as f32);
        }
        if (b.flare[2] - self.flare[2]).abs() < 1. {
            lens.flare[2] = self.flare[2] + (b.flare[2] - self.flare[2]) * t as f32;
        }
        let (s, c) = mix(self.light_angle, b.light_angle).sin_cos();
        lens.light = [
            c as f32,
            s as f32,
            mix(self.cloud, b.cloud) as f32,
            (0.5 + 0.5 * (time / 781.).sin()) as f32,
        ];
        lens
    }
}
fn frames(l: &Lease) -> Option<(&Frame, &Frame)> {
    Some((
        l.sample.lower.typed.as_ref()?.downcast_ref()?,
        l.sample.upper.typed.as_ref()?.downcast_ref()?,
    ))
}
/// # Safety
/// A retained depth sample. The returned static templates borrow its upper endpoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_depth_kaleidoscope_templates(
    lease: *const Lease,
    count: *mut usize,
) -> *const Template {
    let (Some(l), Some(count)) = (unsafe { lease.as_ref() }, unsafe { count.as_mut() }) else {
        return std::ptr::null();
    };
    let Some((_, b)) = frames(l) else {
        *count = 0;
        return std::ptr::null();
    };
    *count = b.cell.cuts.len();
    b.cell.cuts.as_ptr()
}
/// # Safety
/// A retained sample, writable lens and room for capacity transforms. Returns the required count.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_depth_kaleidoscope_sample(
    lease: *const Lease,
    lens: *mut Lens,
    out: *mut Pose,
    capacity: usize,
) -> usize {
    let (Some(l), Some(dest)) = (unsafe { lease.as_ref() }, unsafe { lens.as_mut() }) else {
        return 0;
    };
    let Some((a, b)) = frames(l) else { return 0 };
    let a = if a.cell.revision == b.cell.revision {
        a
    } else {
        b
    };
    *dest = a.lens(b, &l.view, l.sample.alpha);
    let count = b.cell.cuts.len();
    if out.is_null() || capacity < count {
        return count;
    }
    let slice = unsafe { std::slice::from_raw_parts_mut(out, count) };
    let t = l.sample.alpha as f32;
    for ((dst, p), q) in slice.iter_mut().zip(&a.poses).zip(&b.poses) {
        *dst = Pose(std::array::from_fn(|i| p.0[i] + (q.0[i] - p.0[i]) * t));
    }
    count
}
