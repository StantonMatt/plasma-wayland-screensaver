//! Immutable render frames. Sphere records stay four RGBA texels; the trail
//! ring is in world coordinates so it can be reused across presentations.
use super::{common::layout::Rect, world::MonitorMode, *};
use crate::visuals::{
    depth::ffi::{Lease, View},
    palette,
};
pub(super) struct Art {
    rooms: Arc<[Room]>,
    reference_radii: Vec<f64>,
    colours: [[[f32; 4]; 6]; 3],
    palette: u32,
    enamel: [[f32; 6]; 3],
    lightest: [f32; 4],
    top: [f32; 4],
    bottom: [f32; 4],
    black: bool,
    mirrored: bool,
    trail_half_width: f64,
    lifetime: f64,
    halo: f64,
}
impl Art {
    pub(super) fn new(input: &WorldInput, rooms: Arc<[Room]>, count: usize) -> Self {
        let mut colours = std::array::from_fn(|i| palette::color(input.palette, i));
        if input.palette == 4 {
            for c in &mut colours {
                let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
                *c = [y, y, y, 1.];
            }
        }
        let swatches = |values: [u32; 6]| {
            values.map(|x| {
                [
                    ((x >> 16) & 255) as f32 / 255.,
                    ((x >> 8) & 255) as f32 / 255.,
                    (x & 255) as f32 / 255.,
                    1.,
                ]
            })
        };
        let mut materials = [colours; 3];
        if input.palette == 2 {
            materials[Material::Glass as usize] =
                swatches([0xcb8e32, 0xc98a2b, 0xe0a44e, 0xd29a3a, 0xba7221, 0xd39846]);
        } else if input.palette == 3 {
            materials[Material::Rubber as usize] =
                swatches([0x459b75, 0x25876a, 0x4ea379, 0x2f8f6a, 0x378767, 0x27775e]);
            materials[Material::Glass as usize] =
                swatches([0x6bb9a4, 0x78c6ae, 0x77bda6, 0x6cae9e, 0x81c4b5, 0x76b7a3]);
        }
        let reference_radii = rooms
            .iter()
            .map(|room| {
                let short = room
                    .outputs
                    .iter()
                    .map(|o| o.width.min(o.height))
                    .fold(0., f64::max);
                (0.075 * short).clamp(24., 220.).min(0.28 * short)
            })
            .collect();
        Self {
            reference_radii,
            rooms,
            enamel: materials
                .map(|row| row.map(|c| paint::packed(c, input.environment.background == 0))),
            lightest: *colours
                .iter()
                .max_by(|a, b| {
                    let y = |c: &[f32; 4]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
                    y(a).total_cmp(&y(b))
                })
                .unwrap(),
            colours: materials,
            palette: input.palette,
            top: input.environment.top,
            bottom: input.environment.bottom,
            black: input.environment.background == 0,
            mirrored: input.mode == MonitorMode::Mirrored,
            trail_half_width: 0.52
                - 0.12
                    * ((count as f64 - 40.) / 20.).clamp(0., 1.)
                    * ((input.scale - 1.5) / 0.5).clamp(0., 1.)
                    * ((input.trails - 0.7) / 0.3).clamp(0., 1.),
            lifetime: if input.reduced_motion {
                0.
            } else {
                0.9 * input.trails.powf(0.90)
            },
            halo: match input.palette {
                5 => 0.7,
                1 => 1.3,
                _ => 1.,
            },
        }
    }
}
#[derive(Clone)]
struct Image {
    p: V,
    velocity: V,
    r: f64,
    angle: f64,
    axis: V,
    squash: f64,
    mat: Material,
    pigment: usize,
    glow: f32,
    quiet: f32,
    boost: f32,
    pattern: u16,
    flash: f32,
    ring_age: f32,
    ring_peak: f32,
    alpha: f32,
    spill: Option<Spill>,
    wall: Wall,
    gap: f64,
    trail: Trail,
}
impl Image {
    fn squash_at(&self, next: &Self, blend: f64) -> f64 {
        (self.squash + (next.squash - self.squash) * blend).clamp(-0.12, 0.23)
    }
}
pub(super) struct Frame {
    art: Arc<Art>,
    images: Vec<Image>,
    sim_time: f64,
    time: f64,
    moment: Option<Moment>,
}
impl Frame {
    pub(super) fn new(art: Arc<Art>) -> Self {
        Self {
            art,
            images: Vec::with_capacity(61),
            sim_time: 0.,
            time: 0.,
            moment: None,
        }
    }
    pub(super) fn update(&mut self, w: &BounceWorld) {
        self.images.clear();
        self.sim_time = w.sim_time;
        self.time = w.tick as f64 / 120.;
        self.moment = w.moment;
        for (i, b) in w.balls.iter().enumerate() {
            let (wall, _, distance) = w.rooms[b.room].nearest(b.p);
            let mut axis = b.axis;
            let mut squash = b.squash;
            if b.sleeping && b.charge > b.period {
                let gravity = w.gravity(self.time);
                if gravity.len2() > 0.001 {
                    axis = -gravity.unit();
                }
                squash = 0.15 * ((b.charge - b.period) / 0.18).min(1.);
            }
            if b.v.len2() > (1.2 * w.height).powi(2) && squash.abs() < 0.015 {
                axis = b.v.unit();
                squash = -0.06 * (b.v.len() / w.height - 1.2).min(1.);
            }
            if b.near_ball {
                squash = squash.clamp(-0.056, 0.056);
            }
            let glow = if w.input.reduced_motion {
                0.30
            } else {
                b.glow()
            };
            let float = if w.moment.is_some_and(|e| e.kind == events::Kind::Float) {
                1.2
            } else {
                1.
            };
            let size_gain = (self.art.reference_radii[b.room] / b.r)
                .powi(2)
                .clamp(0.3, 1.);
            let mut p = b.p;
            if let Some(face) = b.wall_anchor
                && b.squash_age < 0.075
            {
                let t = ((b.squash_age - 1. / 60.) / (0.075 - 1. / 60.)).clamp(0., 1.);
                let gap = ((p - face).dot(b.contact_normal) - b.r).clamp(0., b.r);
                p -= b.contact_normal * (gap * (1. - t * t * (3. - 2. * t)));
            }
            if axis.dot(b.contact_normal) < 0. && !b.sleeping {
                axis = -axis;
            }
            self.images.push(Image {
                p,
                velocity: b.v,
                r: if i >= w.params.count as usize
                    && w.moment
                        .is_some_and(|m| self.time - m.start > m.duration - 1.2)
                {
                    b.r * (0.25 + 0.75 * b.alpha)
                } else {
                    b.r
                },
                angle: b.angle,
                axis,
                squash,
                mat: b.mat,
                pigment: b.pigment,
                glow: glow * float,
                quiet: if w.input.reduced_motion {
                    glow
                } else {
                    b.quiet_glow() * float
                },
                boost: (0.75 * b.body_light() * size_gain) as f32 * float,
                pattern: b.glass_pattern,
                flash: (b.flash
                    * b.impact_attack()
                    * size_gain
                    * (1.
                        - (b.flash_age - if b.mat == Material::Metal { 0.12 } else { 0.06 })
                            .max(0.)
                            / 0.04)
                        .clamp(0., 1.)) as f32,
                ring_age: (b.flash_age / 0.2).clamp(0., 1.) as f32,
                ring_peak: if b.mat == Material::Glass {
                    b.flash as f32
                } else {
                    0.
                },
                alpha: b.alpha as f32,
                spill: b.impact,
                wall,
                gap: (distance - b.r).max(0.),
                trail: b.trail.clone(),
            });
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Record {
    pub geom: [f32; 4],
    pub axis: [f32; 4],
    pub colour: [f32; 4],
    pub look: [f32; 4],
}
const _: () = assert!(size_of::<Record>() == 64);
#[repr(C)]
pub struct RingView {
    pub serial: u64,
    pub(super) knots: *const Knot,
}
#[repr(C)]
pub struct BatchInfo {
    pub count: u32,
    pub trail_start: u32,
    pub sphere_start: u32,
    pub trails: u32,
    pub trail_half_width: f32,
    pub top: [f32; 4],
    pub bottom: [f32; 4],
    pub rings: [RingView; 61],
}
const HALO: f32 = 1.;
const POOL: f32 = 2.;
const SHADOW: f32 = 3.;
const RAIL: f32 = 4.;
const TRAIL: f32 = 5.;
const SPOT: f32 = 7.;
const CUE: f32 = 8.;
struct Writer<'a> {
    viewport: Rect,
    records: &'a mut [Record],
    count: usize,
}
impl Writer<'_> {
    fn intersects(&self, p: V, h: V) -> bool {
        p.x + h.x >= self.viewport.x
            && p.x - h.x <= self.viewport.right()
            && p.y + h.y >= self.viewport.y
            && p.y - h.y <= self.viewport.bottom()
    }
    fn push(&mut self, r: Record) {
        if let Some(out) = self.records.get_mut(self.count) {
            *out = r;
        }
        self.count += 1;
    }
    #[allow(clippy::too_many_arguments)]
    fn effect(
        &mut self,
        look: [f32; 4],
        p: V,
        half: V,
        dir: V,
        mut colour: [f32; 4],
        opacity: f32,
    ) {
        if opacity < 0.001 || half.x <= 0. || half.y <= 0. {
            return;
        }
        let h = v(
            dir.x.abs() * half.x + dir.y.abs() * half.y,
            dir.y.abs() * half.x + dir.x.abs() * half.y,
        );
        if !self.intersects(p, h) {
            return;
        }
        colour[3] = opacity;
        self.push(Record {
            geom: [p.x as f32, p.y as f32, half.x as f32, half.y as f32],
            axis: [dir.x as f32, dir.y as f32, 0., 0.],
            colour,
            look,
        });
    }
    #[allow(clippy::too_many_arguments)]
    fn edge(
        &mut self,
        wall: Wall,
        q: V,
        length: f64,
        width: f64,
        colour: [f32; 4],
        alpha: f32,
        kind: f32,
    ) {
        let tangent = (wall.b - wall.a).unit();
        let a = wall.nearest(q - tangent * length);
        let b = wall.nearest(q + tangent * length);
        self.effect(
            [0., 0., 0., kind],
            (a + b) * 0.5 + wall.inward * if kind == RAIL { 0. } else { 1.25 },
            v((b - a).len() * 0.5, width),
            tangent,
            colour,
            alpha,
        );
    }
}
fn draw(
    a: &Frame,
    b: &Frame,
    view: &View,
    blend: f64,
    records: &mut [Record],
    info: &mut BatchInfo,
) {
    let scale = view.uniform_scale;
    let viewport = Rect {
        x: view.viewport.x - view.offset_x / scale,
        y: view.viewport.y - view.offset_y / scale,
        width: view.viewport.width / scale,
        height: view.viewport.height / scale,
    };
    let mut out = Writer {
        viewport,
        records,
        count: 0,
    };
    let art = &a.art;
    info.top = art.top;
    info.bottom = art.bottom;
    info.trails = 0;
    info.trail_half_width = art.trail_half_width as f32;
    for r in &mut info.rings {
        r.serial = 0;
        r.knots = std::ptr::null();
    }
    if art.mirrored && (view.offset_x.abs() > 0.01 || view.offset_y.abs() > 0.01) {
        for wall in &art.rooms[0].walls {
            let mid = (wall.a + wall.b) * 0.5;
            out.edge(
                *wall,
                mid,
                (wall.b - wall.a).len(),
                12. / scale,
                art.lightest,
                1.,
                RAIL,
            );
        }
    }
    if let Some(m) = a.moment {
        let age = a.time - m.start;
        let room = &art.rooms[m.room];
        match m.kind {
            events::Kind::Drop if age < 2. => {
                let (_, q, _) = room.nearest(m.anchor);
                let radius = (m.anchor.y - q.y).abs().max(10.);
                let opacity = if age < 1.5 {
                    0.35 * (age / 1.5)
                } else {
                    0.60 * (1. - (age - 1.5) / 0.5)
                };
                out.effect(
                    [0., 0., 0., SPOT],
                    q,
                    v(radius, radius),
                    v(1., 0.),
                    art.lightest,
                    opacity as f32,
                );
            }
            events::Kind::Tilt | events::Kind::Float => {
                let sweep = (age / 1.2).clamp(0., 1.);
                let fade = ((m.duration - age) / 4.).clamp(0., 1.) as f32;
                for room in art.rooms.iter() {
                    for &wall in &room.walls {
                        let mid = (wall.a + wall.b) * 0.5;
                        let half = (wall.b - wall.a).len() * 0.5;
                        if m.kind == events::Kind::Float {
                            out.edge(
                                wall,
                                mid,
                                half,
                                12.,
                                art.lightest,
                                0.06 * (age / 3.).clamp(0., 1.) as f32 * fade,
                                CUE,
                            );
                            continue;
                        }
                        let downhill = wall.inward.x * m.direction < 0.;
                        if downhill {
                            out.edge(
                                wall,
                                mid,
                                half,
                                14.,
                                art.lightest,
                                0.08 * sweep as f32 * fade,
                                CUE,
                            );
                        }
                        if age <= 1.2 && (downhill || wall.inward.y < 0.) {
                            let u = if downhill || m.direction > 0. {
                                sweep
                            } else {
                                1. - sweep
                            };
                            out.edge(
                                wall,
                                wall.a + (wall.b - wall.a) * u,
                                80.,
                                14.,
                                art.lightest,
                                0.30 * (std::f64::consts::PI * sweep).sin() as f32,
                                POOL,
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let at = |i: usize| {
        let im = &a.images[i];
        let next = b.images.get(i).unwrap_or(im);
        (im.p + (next.p - im.p) * blend, next)
    };
    for (i, im) in a.images.iter().enumerate() {
        let (p, next) = at(i);
        let colour = art.colours[im.mat as usize][im.pigment];
        if im.gap < 2. * im.r {
            let intensity = (1. - im.gap / (2. * im.r)) as f32 * im.alpha;
            let foot = im.wall.nearest(p);
            if !art.black && im.wall.inward.y.abs() > 0.5 {
                out.effect(
                    [0., 0., 0., SHADOW],
                    foot + im.wall.inward * 2.,
                    v(im.r * 0.8, im.r * 0.125),
                    v(1., 0.),
                    [0., 0., 0., 1.],
                    intensity * 0.35,
                );
            }
            out.edge(
                im.wall,
                foot,
                im.r * 1.25,
                18.,
                colour,
                intensity * (0.08 + 0.10 * im.glow),
                POOL,
            );
        }
        if let Some(s) = im.spill {
            out.edge(
                s.wall,
                s.point,
                im.r * 1.50,
                10.,
                colour,
                s.strength as f32 * im.alpha * 0.52,
                POOL,
            );
        }
        let radius = im.r * 1.8;
        let squash = im.squash_at(next, blend);
        let compression = 1. - squash;
        out.effect(
            [im.ring_peak, im.ring_age, 0., HALO],
            p - im.axis * (im.r * squash.max(0.)),
            v(radius * compression, radius / compression),
            im.axis,
            colour,
            (0.48 * art.halo) as f32 * im.glow * im.alpha,
        );
    }
    info.trail_start = out.count as u32;
    for (i, im) in a.images.iter().enumerate() {
        if art.lifetime == 0. || im.trail.length < 1 {
            continue;
        }
        let (p, next) = at(i);
        let velocity = im.velocity + (next.velocity - im.velocity) * blend;
        let time = a.sim_time + (b.sim_time - a.sim_time) * blend;
        let mut lo = p;
        let mut hi = p;
        let mut length = 0;
        for k in 0..im.trail.length {
            let knot = im.trail.knots[(im.trail.serial as usize - 1 - k) % KNOTS];
            if time - f64::from(knot.time) > art.lifetime {
                break;
            }
            lo = v(lo.x.min(knot.x as f64), lo.y.min(knot.y as f64));
            hi = v(hi.x.max(knot.x as f64), hi.y.max(knot.y as f64));
            length += 1;
        }
        if length == 0
            || (hi - lo).len2() < 1.
            || !out.intersects((hi + lo) * 0.5, (hi - lo) * 0.5 + v(im.r, im.r))
        {
            continue;
        }
        let mut colour = art.colours[im.mat as usize][im.pigment];
        colour[3] = im.alpha * art.halo as f32;
        out.push(Record {
            geom: [p.x as f32, p.y as f32, im.r as f32, art.lifetime as f32],
            axis: [
                ((im.trail.serial - 1) % 32) as f32,
                length as f32,
                i as f32,
                time as f32,
            ],
            colour,
            look: [im.glow, velocity.x as f32, velocity.y as f32, TRAIL],
        });
        info.rings[i] = RingView {
            serial: im.trail.serial,
            knots: im.trail.knots.as_ptr(),
        };
        info.trails += length as u32;
    }
    info.sphere_start = out.count as u32;
    for (i, im) in a.images.iter().enumerate() {
        let (p, n) = at(i);
        if !out.intersects(p, v(im.r * 1.35 + 2., im.r * 1.35 + 2.)) {
            continue;
        }
        let mut colour = art.colours[im.mat as usize][im.pigment];
        colour[3] = im.alpha + (n.alpha - im.alpha) * blend as f32;
        let axis = if im.axis.len2() < 0.1 {
            v(0., 1.)
        } else {
            im.axis
        };
        out.push(Record {
            geom: [
                p.x as f32,
                p.y as f32,
                im.r as f32,
                (im.angle + (n.angle - im.angle) * blend).rem_euclid(std::f64::consts::TAU) as f32,
            ],
            axis: [
                axis.x as f32,
                axis.y as f32,
                im.squash_at(n, blend) as f32,
                (im.mat as u32 + 4 * art.palette + 32 * u32::from(im.pattern)) as f32,
            ],
            colour,
            look: [
                im.quiet + (n.quiet - im.quiet) * blend as f32,
                im.flash + (n.flash - im.flash) * blend as f32,
                if im.mat == Material::Metal {
                    match art.palette {
                        0 => 0x9fb4c8 as f32,
                        2 => {
                            if i % 2 == 0 {
                                0xd9a441 as f32
                            } else {
                                0xc8703a as f32
                            }
                        }
                        3 => 0x7d8a52 as f32,
                        4 => 0xebebf0 as f32,
                        5 => 0xe6dcef as f32,
                        _ => art.enamel[im.mat as usize][im.pigment],
                    }
                } else {
                    art.enamel[im.mat as usize][im.pigment]
                },
                -(im.boost + (n.boost - im.boost) * blend as f32),
            ],
        });
    }
    info.count = out.count as u32;
}
/// # Safety
/// The caller supplies writable records and batch info. Ring pointers borrow the
/// retained lease and may only be used before it is released.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_depth_bounce_write(
    lease: *const Lease,
    records: *mut Record,
    capacity: usize,
    info: *mut BatchInfo,
) -> bool {
    let (Some(lease), Some(info)) = (unsafe { lease.as_ref() }, unsafe { info.as_mut() }) else {
        return false;
    };
    if records.is_null() || capacity > 8192 {
        return false;
    }
    fn frame(s: &Snapshot) -> Option<&Frame> {
        s.typed.as_ref()?.downcast_ref()
    }
    let (Some(a), Some(b)) = (frame(&lease.sample.lower), frame(&lease.sample.upper)) else {
        return false;
    };
    draw(
        a,
        b,
        &lease.view,
        lease.sample.alpha,
        unsafe { std::slice::from_raw_parts_mut(records, capacity) },
        info,
    );
    info.count as usize <= capacity
}

#[cfg(test)]
pub(super) fn test_records(w: &BounceWorld, rect: Rect) -> (Vec<Record>, usize, f64) {
    let mut frame = Frame::new(w.art.clone());
    frame.update(w);
    let mut rec = vec![Record::default(); 2048];
    let mut info = BatchInfo {
        count: 0,
        trail_start: 0,
        sphere_start: 0,
        trails: 0,
        trail_half_width: 0.,
        top: [0.; 4],
        bottom: [0.; 4],
        rings: std::array::from_fn(|_| RingView {
            serial: 0,
            knots: std::ptr::null(),
        }),
    };
    let view = View {
        layout_generation: 1,
        output_id: 0,
        viewport: rect,
        timeline_ns: w.rate_ns(),
        uniform_scale: 1.,
        offset_x: 0.,
        offset_y: 0.,
        dpr: 1.,
    };
    draw(&frame, &frame, &view, 0., &mut rec, &mut info);
    rec.truncate(info.count as usize);
    // Conservative pre-clipping rectangle area for soft-light quads, and the
    // entire oriented support of each distance-field trail. Excludes opaque spheres and shadows.
    let mut area = 0.;
    for o in &rec {
        if [HALO, POOL, RAIL, SPOT, CUE].contains(&o.look[3]) {
            area += f64::from(4. * o.geom[2] * o.geom[3]);
        }
        if o.look[3] != TRAIL {
            continue;
        }
        let b = &w.balls[o.axis[2] as usize];
        let knots = &b.trail.knots;
        let node = |k: usize| knots[(b.trail.serial as usize - 1 - k) % KNOTS];
        let point = |n: Knot| v(n.x.into(), n.y.into());
        let count = o.axis[1] as usize;
        let tip = node(0);
        let head_distance = f64::from(tip.distance) + (b.p - point(tip)).len();
        let path = (head_distance - f64::from(node(count - 1).distance)).rem_euclid(65536.);
        if path < 0.85 * b.r {
            continue;
        }
        let delta = b.p - point(node(count - 1));
        let direction = if delta.len2() > 0.01 {
            delta.unit()
        } else {
            v(1., 0.)
        };
        let mut lo = V::default();
        let mut hi = V::default();
        let mut young = b.p;
        let mut velocity = b.v;
        let mut young_time = w.sim_time;
        let mut struck = false;
        for k in 0..count {
            let old = node(k);
            let chord = young - point(old);
            let dt = (young_time - f64::from(old.time)).max(0.);
            let tangent = |velocity: V| {
                if velocity.dot(chord) <= 0. {
                    chord
                } else {
                    velocity * dt.min(1.25 * chord.len() / velocity.len().max(0.001))
                }
            };
            let old_velocity = v(old.vx.into(), old.vy.into());
            let start = tangent(old_velocity);
            let end = if struck { chord } else { tangent(velocity) };
            for control in [point(old), point(old) + start / 3., young - end / 3.] {
                let offset = control - b.p;
                let q = v(offset.dot(direction), offset.dot(direction.perp()));
                lo = v(lo.x.min(q.x), lo.y.min(q.y));
                hi = v(hi.x.max(q.x), hi.y.max(q.y));
            }
            young = point(old);
            velocity = old_velocity;
            young_time = f64::from(old.time);
            struck = old.light < 0.;
        }
        let padding = 2. * (w.art.trail_half_width + 0.47) * b.r;
        area += (hi.x - lo.x + padding) * (hi.y - lo.y + padding);
    }
    (rec, info.sphere_start as usize, area / rect.area())
}
