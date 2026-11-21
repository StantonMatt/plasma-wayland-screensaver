use super::*;
use crate::visuals::depth::ffi::{Lease, View};

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Record {
    pub body: [f32; 4],
    pub lobe: [f32; 4],
    pub surface: [f32; 4],
}
const _: () = assert!(size_of::<Record>() == 48);
#[derive(Clone, Copy, Default)]
struct Entry {
    id: u32,
    z: f32,
    record: Record,
}
#[derive(Default)]
pub(super) struct Frame {
    entries: Vec<Entry>,
    order: Vec<usize>,
    pub clock: Vec<Clock>,
    look: u32,
    palette: u32,
    glow: f32,
    still: bool,
    mode: u32,
    speed: f32,
    time: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Style {
    pub look: f32,
    pub glow: f32,
    pub phase: f32,
    pub blend: f32,
    pub palette: f32,
}
impl Frame {
    fn put(&mut self, slot: usize, id: u32, z: f64, record: Record) {
        if record.body[3] <= 0.00001 || record.body[2] <= 0.01 {
            return;
        }
        self.entries[slot] = Entry {
            id,
            z: z as f32,
            record,
        };
        self.order.push(slot);
    }
}
fn record(pos: Vec2, r: f64, intensity: f64, kind: u32, color: f32, alt: f32, blur: f64) -> Record {
    Record {
        body: [pos.x as f32, pos.y as f32, r as f32, intensity as f32],
        lobe: [0., 0., 0., kind as f32],
        surface: [color, alt, blur as f32, 0.],
    }
}
fn incandescent(color: f32, heat: f64) -> f32 {
    let rgb = color as u32;
    let channel = |shift: u32| {
        let c = f64::from((rgb >> shift) & 255);
        (c + (255. - c) * (0.88 * heat)).round() as u32
    };
    ((channel(16) << 16) | (channel(8) << 8) | channel(0)) as f32
}
fn packed_wobble(o: Orb) -> f32 {
    let (s, c) = (2. * o.angle).sin_cos();
    let enc = |v: f64| ((v.clamp(-0.25, 0.25) * 500.).round() as i32 + 128) as u32;
    ((enc(o.wobble[0] * c) << 16) | (enc(o.wobble[0] * s) << 8) | enc(o.wobble[1])) as f32
}
impl Colony {
    fn foreground_transmission(&self, pos: Vec2, radius: f64) -> f64 {
        self.clock
            .iter()
            .map(|c| {
                let q = p(
                    pos.x.clamp(c.rect.x, c.rect.right()),
                    pos.y.clamp(c.rect.y, c.rect.bottom()),
                );
                0.15 + 0.85 * ease(((pos - q).length() - radius) / (radius.max(1.) * 0.8))
            })
            .fold(1., f64::min)
    }
    pub(super) fn frame(&self, f: &mut Frame) {
        f.palette = self.input.palette;
        let dense = self.input.density > 70. || self.input.scale > 1.5;
        f.glow = self.input.trails.min(if dense { 0.35 } else { 1. }) as f32;
        f.look = match self.params.look {
            "glass" => 1,
            "bubbles" => 2,
            _ => 0,
        };
        f.still = self.input.reduced_motion;
        f.mode = self.input.mode as u32;
        f.speed = self.input.speed as f32;
        f.time = self.time;
        f.clock.clone_from(&self.clock);
        f.order.clear();
        let n = self.orbs.len();
        let far = n * 2 + self.motes.len();
        let nr = far + self.plankton.field.len();
        f.entries.clear();
        f.entries
            .resize(nr + self.rings.len() + 2, Entry::default());
        let surf = self.surface();
        let depth = self.params.depth as f64 / 100.;
        let focus = 0.55 - 0.52 * surf;
        for (i, o) in self.orbs.iter().copied().enumerate() {
            if !o.alive() {
                continue;
            }
            let z = (o.z + 0.2 * surf).min(if o.z > 0.85 { 1. } else { 0.85 });
            let breath = if f.still { 0. } else { o.phase.sin() };
            let mut radius = self.luminous_radius(o, z) * (1. + 0.02 * breath) * ease(o.age / 1.2);
            if o.stream
                && let Some(event) = &self.event
            {
                let joined = ease(event.age / 5.) * ease((o.release_at - event.age) / 3.);
                radius *= 1. + 0.025 * joined;
            }
            let palette = self.input.palette;
            let h = Ramp::pigment(palette, o.hue, z, o.energy);
            let mut color = self.art.at(if f.look == 2 && palette == 5 {
                h - 0.16
            } else {
                h
            });
            let second = if f.look == 2 {
                h + if palette == 5 { 0.16 } else { 0.36 }
            } else {
                match palette {
                    0 => {
                        if h < 0.55 {
                            0.48
                        } else {
                            0.78
                        }
                    }
                    1 if (0.12..0.48).contains(&h) => 0.025,
                    2 => 0.13,
                    _ => h + 0.05,
                }
            };
            let alt = self.art.at(second);
            let mut light = (0.84 + 0.16 * o.energy) * (0.74 + 0.26 * z) * (1. + 0.033 * breath);
            if palette == 1 {
                light *= 0.80;
            }
            light *= 1. + 0.20 * o.flare.min(1.);
            if let Some(e) = &self.event
                && e.kind == events::Kind::Dandelion
                && !e.bloomed
                && i == e.heart
            {
                let pulse = 0.5 + 0.5 * (e.age * TAU * 0.25).sin();
                color = incandescent(color, ease(e.age / 3.));
                light *= 1. + ease(e.age / 3.) * (0.35 + 0.30 * pulse);
                radius = radius.max(self.height * 0.17 * ease(e.age / 33.)) * (1. + 0.035 * pulse);
            }
            if o.bloom_travel > 0. {
                let fresh = ease(o.bloom_travel / 5.);
                color = incandescent(color, fresh * 0.21);
                light *= 1. + 0.30 * fresh;
                radius *= 1. + 0.18 * fresh;
            }
            if z > 0.85 {
                light *= self.foreground_transmission(o.pos, radius);
            }
            if !f.still && self.time < 2.5 {
                light *= 0.10 + 0.90 * ease((self.time - 0.55 * z) / 1.95);
            }
            let blur = 0.026 + depth * 0.35 * (z - focus).abs() / 0.45;
            let mut r = record(o.pos, radius, light, 0, color, alt, blur);
            if f.look == 1 && palette == 2 {
                // Projected aperture grows as the near glass passes the lens.
                let aperture = (31. * ease((z - 0.55) / 0.30)).round();
                r.surface[2] += (64. * aperture) as f32;
            }
            if f.look == 0 {
                // Store halo attenuation and core energy together in the unused
                // high bits of the defocus field, including on paired primitives.
                let mut neighbours = 0;
                if dense {
                    self.grid.visit(o.pos, self.base * 5., |j| {
                        let other = self.orbs[j];
                        let delta = other.pos - o.pos;
                        if i != j && delta.dot(delta) < (1.6 * (radius + other.radius)).powi(2) {
                            neighbours += 1;
                        }
                    });
                }
                let halo = 1. / (1. + 0.5 * f64::from(neighbours));
                let pigment = (o.energy * 31.).round() as u32;
                let optics = pigment * 32 + (halo * 31.).round() as u32;
                r.surface[2] += 2. * optics as f32;
            }
            r.surface[3] = packed_wobble(o);
            if o.split > 0. {
                let t = ease(o.split / 2.6);
                let dir = axis(o.angle) * o.radius * t;
                radius *= 1. - t * 0.24;
                r.body[0] -= dir.x as f32 * 0.5;
                r.body[1] -= dir.y as f32 * 0.5;
                r.body[2] = radius as f32;
                r.lobe = [dir.x as f32, dir.y as f32, r.body[2], 1.];
            }
            if o.shiver > 0. {
                r.surface[3] = packed_wobble(Orb {
                    wobble: [o.wobble[0], 0.045 * (o.shiver * 13.).sin()],
                    ..o
                });
            }
            f.put(i, o.id, z, r);
            if self.input.trails >= 0.6 && !f.still && o.vel.length() > 27. {
                let offset = o.vel * (-0.40);
                let wake = record(
                    o.pos + offset,
                    radius * 0.72,
                    light * (o.vel.length() / 110.).min(1.) * 0.18,
                    4,
                    color,
                    alt,
                    0.3,
                );
                f.put(n + i, o.id, z - 0.002, wake);
            }
        }
        for l in &self.links {
            if !matches!(l.kind, LinkKind::Merge | LinkKind::Engulf) {
                continue;
            }
            let (a, b) = (self.orbs[l.a], self.orbs[l.b]);
            if [a.id, b.id] != l.ids || !a.alive() || !b.alive() {
                continue;
            }
            let (i, j) = if a.mass >= b.mass {
                (l.a, l.b)
            } else {
                (l.b, l.a)
            };
            let little = f.entries[j].record;
            let big = &mut f.entries[i].record;
            let t = ease(l.age / l.duration);
            if l.kind == LinkKind::Merge {
                let dx = (little.body[0] - big.body[0]) * (1. - t as f32 * 0.82);
                let dy = (little.body[1] - big.body[1]) * (1. - t as f32 * 0.82);
                let total = (big.body[2].powi(2) + little.body[2].powi(2)).sqrt();
                let w = (self.orbs[j].mass / (a.mass + b.mass)) as f32;
                big.body[0] += (little.body[0] - big.body[0]) * w * t as f32;
                big.body[1] += (little.body[1] - big.body[1]) * w * t as f32;
                big.body[2] += (total - big.body[2]) * t as f32;
                big.lobe = [dx, dy, little.body[2] * (1. - 0.68 * t as f32), 1.];
                f.entries[j].id = 0;
            } else {
                let host = *big;
                let z = f.entries[i].z;
                let child = &mut f.entries[j];
                child.z = z + 0.001;
                let rec = &mut child.record;
                rec.body[0] += (host.body[0] - rec.body[0]) * t as f32 * 0.86;
                rec.body[1] += (host.body[1] - rec.body[1]) * t as f32 * 0.86;
                rec.body[2] *= 1. - 0.85 * t as f32;
                rec.body[3] *= 1. - t as f32 * 0.9;
            }
        }
        for (i, m) in self.motes.iter().enumerate() {
            if m.life > 0. {
                let fade = ease(m.life / 7.) * ease((m.max_life - m.life + 0.2) / 1.);
                let c = self
                    .art
                    .at(Ramp::pigment(self.input.palette, m.hue, m.z, 0.8));
                let r = self.height * (if m.cue { 0.007 } else { 0.0011 + 0.0017 * m.z });
                let age = m.max_life - m.life;
                let centre = m.pos + m.ejecta_slip * (age * (1. - ease(age / 6.)));
                f.put(
                    2 * n + i,
                    m.id,
                    m.z,
                    record(
                        centre,
                        r,
                        (if m.cue { 1.10 } else { 0.41 + 0.48 * m.z }) * fade,
                        2,
                        c,
                        c,
                        0.1,
                    ),
                );
            }
        }
        for (i, r) in self.rings.iter().enumerate() {
            let t = r.age / 0.8;
            let c = self.art.at(r.hue);
            f.put(
                nr + i,
                r.id,
                r.z,
                record(
                    r.pos,
                    r.r * (1. + 0.5 * ease(t)),
                    (1. - ease(t)) * (0.83 + 0.15 * self.input.trails),
                    3,
                    c,
                    c,
                    t,
                ),
            );
        }
        if let Some(s) = self.pass {
            let c = self.art.at(s.hue);
            f.put(
                nr + self.rings.len(),
                s.id,
                1.,
                record(
                    s.pos,
                    s.r,
                    0.56 * ease(s.age / 3.)
                        * ease((s.life - s.age) / 4.)
                        * self.foreground_transmission(s.pos, s.r),
                    0,
                    c,
                    self.art.at(s.hue + 0.12),
                    0.43,
                ),
            );
        }
        if let Some(e) = &self.event
            && let Some(h) = e.calyx
        {
            // As the daughters unfold, the emptied seed case contracts to a
            // point. It stays rooted at the bloom centre for the full release.
            let remaining = 1. - ease((e.age - 40.) / 6.);
            let colour = incandescent(self.art.at(h.hue), 1.);
            f.put(
                nr + self.rings.len() + 1,
                h.id,
                h.z,
                record(
                    h.pos,
                    self.height * 0.17 * remaining,
                    remaining,
                    0,
                    colour,
                    self.art.at(h.hue),
                    0.06,
                ),
            );
        }
        for (index, s) in self.plankton.field.iter().enumerate() {
            let h = Ramp::pigment(self.input.palette, s.hue, 0.10, 0.40);
            let c = self.art.at(h);
            let blur = 0.30 - 0.26 * surf;
            let r = record(
                s.pos,
                s.r,
                s.light * s.opacity() * (1. + surf),
                2,
                c,
                c,
                blur,
            );
            f.put(
                far + index,
                0x80000000 + index as u32,
                0.11 + 0.15 * surf,
                r,
            );
        }
        f.order.retain(|&i| f.entries[i].id != 0);
        // A local raft can concentrate much more light than the world average.
        // Use one scale for all viewports, retaining continuity at an open seam.
        let budget = if self.input.density <= 50. && self.input.scale <= 1. {
            1.9
        } else {
            3.25
        };
        let area = f.quad_area(&self.input.layout);
        if area > budget {
            let mut lo = 0.01;
            let mut hi = 1.;
            for _ in 0..8 {
                let probe = (hi + lo) * 0.5;
                if f.area_at(&self.input.layout, probe) > budget {
                    hi = probe
                } else {
                    lo = probe
                }
            }
            for &i in &f.order {
                let r = &mut f.entries[i].record;
                r.body[2] *= lo as f32;
                for k in 0..3 {
                    r.lobe[k] *= lo as f32;
                }
            }
        }
        #[cfg(feature = "visual-capture")]
        if let Some(parity) = self.capture_parity {
            f.order.retain(|&i| f.entries[i].id % 2 == parity);
        }
        f.order
            .sort_unstable_by(|&i, &j| f.entries[i].z.total_cmp(&f.entries[j].z).then(i.cmp(&j)));
    }
}
fn endpoints(l: &Lease) -> Option<(&Frame, &Frame)> {
    Some((
        l.sample.lower.typed.as_ref()?.downcast_ref()?,
        l.sample.upper.typed.as_ref()?.downcast_ref()?,
    ))
}
fn visible(r: Record, v: &View, glow: f32) -> bool {
    let x = (f64::from(r.body[0]) - v.viewport.x) * v.uniform_scale + v.offset_x;
    let y = (f64::from(r.body[1]) - v.viewport.y) * v.uniform_scale + v.offset_y;
    let dx = f64::from(r.lobe[0]) * v.uniform_scale;
    let dy = f64::from(r.lobe[1]) * v.uniform_scale;
    let radius = f64::from(r.body[2].max(r.lobe[2]) * (1.75 + 1.12 * glow)) * v.uniform_scale + 3.;
    x + dx.max(0.) + radius >= 0.
        && x + dx.min(0.) - radius <= v.viewport.width
        && y + dy.max(0.) + radius >= 0.
        && y + dy.min(0.) - radius <= v.viewport.height
}
/// # Safety
/// Live immutable lease; dst holds two records per declared row. Returns the
/// number of rows needed even if capacity is smaller. Calling with zero is safe.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_orbs_records(
    lease: *const Lease,
    dst: *mut Record,
    capacity: usize,
) -> usize {
    let Some(l) = (unsafe { lease.as_ref() }) else {
        return 0;
    };
    let Some((a, b)) = endpoints(l) else { return 0 };
    let mut n = 0;
    let mut emit = |lo: Record, hi: Record| {
        if !visible(lo, &l.view, a.glow) && !visible(hi, &l.view, a.glow) {
            return;
        }
        if !dst.is_null() && n < capacity {
            unsafe {
                dst.add(n * 2).write(lo);
                dst.add(n * 2 + 1).write(hi);
            }
        }
        n += 1;
    };
    // Merge the two depth-sorted lists. Births, deaths and reused slots fade
    // at their own location; a new occupant never flies from a retired one.
    let mut j = 0;
    let birth = |i: usize| a.still || a.entries.get(i).is_none_or(|t| t.id != b.entries[i].id);
    let dark = |mut r: Record| {
        r.body[3] = 0.;
        r
    };
    for &i in &a.order {
        let e = a.entries[i];
        while j < b.order.len() && b.entries[b.order[j]].z <= e.z {
            let k = b.order[j];
            if birth(k) {
                let r = b.entries[k].record;
                emit(dark(r), r);
            }
            j += 1;
        }
        let next = b
            .entries
            .get(i)
            .filter(|t| !a.still && t.id == e.id)
            .map_or(dark(e.record), |t| t.record);
        emit(e.record, next);
    }
    for &i in &b.order[j..] {
        if birth(i) {
            let r = b.entries[i].record;
            emit(dark(r), r);
        }
    }
    n
}
/// # Safety
/// Caller provides a live lease and a writable style.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_orbs_style(lease: *const Lease, out: *mut Style) -> bool {
    let Some(l) = (unsafe { lease.as_ref() }) else {
        return false;
    };
    let Some((a, _)) = endpoints(l) else {
        return false;
    };
    let Some(s) = (unsafe { out.as_mut() }) else {
        return false;
    };
    let blend = if a.still {
        ease((l.sample.alpha * 368. - 360.) / 8.)
    } else {
        l.sample.alpha
    };
    *s = Style {
        look: a.look as f32,
        glow: a.glow,
        phase: if a.still {
            0.
        } else {
            ((a.time + l.sample.alpha / 30. * f64::from(a.speed)) * 0.15).rem_euclid(TAU) as f32
        },
        blend: blend as f32,
        palette: a.palette as f32,
    };
    true
}
/// # Safety
/// Caller provides a live lease and two writable doubles. Each Mirrored output consumes its own
/// obstacle after projection from the common canonical world.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_orbs_clock(lease: *const Lease, out: *mut f64) -> bool {
    let Some(l) = (unsafe { lease.as_ref() }) else {
        return false;
    };
    let Some((a, b)) = endpoints(l) else {
        return false;
    };
    if out.is_null() {
        return false;
    }
    let mut it =
        a.clock.iter().enumerate().filter(|(_, c)| {
            c.v != Vec2::default() && (a.mode == 2 || c.output == l.view.output_id)
        });
    let Some((i, c)) = it.next() else {
        return false;
    };
    let q = b.clock.get(i).unwrap_or(c);
    let t = l.sample.alpha;
    let v = l.view;
    unsafe {
        out.write(
            (c.rect.x + (q.rect.x - c.rect.x) * t - v.viewport.x) * v.uniform_scale + v.offset_x,
        );
        out.add(1).write(
            (c.rect.y + (q.rect.y - c.rect.y) * t - v.viewport.y) * v.uniform_scale + v.offset_y,
        );
    }
    true
}

impl Frame {
    #[cfg(test)]
    pub(super) fn radius_for_slot(&self, i: usize) -> f64 {
        f64::from(self.entries[i].record.body[2])
    }
    #[cfg(test)]
    pub(super) fn records_for_test(&self) -> Vec<Record> {
        self.order.iter().map(|i| self.entries[*i].record).collect()
    }
    pub(super) fn quad_area(&self, l: &Layout) -> f64 {
        self.area_at(l, 1.)
    }
    fn area_at(&self, l: &Layout, scale: f64) -> f64 {
        l.outputs
            .iter()
            .map(|o| {
                let r = o.rect;
                self.order
                    .iter()
                    .map(|i| {
                        let q = self.entries[*i].record;
                        let [x, y, rad, _] = q.body.map(f64::from);
                        let [dx, dy, r2, _] = q.lobe.map(f64::from);
                        let (dx, dy) = (dx * scale, dy * scale);
                        let halo = match q.lobe[3] as i32 {
                            2 => 1. + (3. / rad).min(0.7),
                            4 => 1.5,

                            3 => 1.38,
                            _ => f64::from(
                                1.16 + q.surface[2] % 2.
                                    + 1.05 * self.glow
                                    + 0.13 * self.glow.min(0.35) / 0.35,
                            ),
                        };
                        let reach = rad.max(r2) * scale * halo + 1.5;
                        let width = ((x + dx.max(0.) + reach).min(r.right())
                            - (x + dx.min(0.) - reach).max(r.x))
                        .max(0.);
                        let height = ((y + dy.max(0.) + reach).min(r.bottom())
                            - (y + dy.min(0.) - reach).max(r.y))
                        .max(0.);
                        width * height
                    })
                    .sum::<f64>()
                    / r.area()
            })
            .fold(0., f64::max)
    }
}
