use super::{
    Binary, Star,
    art::{Art, Rgb, mix},
    events::{Kind, Moment},
    graph::Edge,
    space::*,
};
use crate::visuals::depth::ffi::{Lease, View};
use std::sync::Arc;
#[derive(Clone)]
pub struct Frame {
    pub stars: Vec<Star>,
    pub edges: Vec<Edge>,
    pub binaries: Vec<Binary>,
    pub events: Vec<Moment>,
    pub art: Arc<Art>,
    pub calm: f32,
    pub still: bool,
    pub life: bool,
}
impl Frame {
    pub fn new(art: Arc<Art>) -> Self {
        Self {
            stars: Vec::new(),
            edges: Vec::new(),
            events: Vec::new(),
            binaries: Vec::new(),
            art,
            calm: 0.,
            still: false,
            life: true,
        }
    }
}
const LINE: u32 = 0;
const CORE: u32 = 1;
const HALO: u32 = 6;
const TAPER: u32 = 7;
const RING: u32 = 8;
const PEN: u32 = 9;
const DUST_TAIL: u32 = 10;
const PULSE: u32 = 13;
const ADD: u32 = 16;
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Primitive {
    pub ends: [f32; 4],
    pub rgba: [f32; 4],
    pub shape: [f32; 4],
}
const _: () = assert!(size_of::<Primitive>() == 48);
struct Batch<'a> {
    out: &'a mut [Primitive],
    count: usize,
    origin: Point,
    offset: Point,
    scale: f32,
    size: Point,
    margin: f32,
}
impl Batch<'_> {
    fn emit(&mut self, kind: u32, a: Point, b: Point, r: f32, c: Rgb, alpha: f32) {
        if r <= 0. || alpha <= 0.001 {
            return;
        }
        let a = (a - self.origin) * self.scale + self.offset;
        let b = (b - self.origin) * self.scale + self.offset;
        // The curved dust tail bows away from the chord encoded in its record.
        let bend = if kind & 15 == DUST_TAIL {
            (b - a).len() * (85. / 245.)
        } else {
            0.
        };
        let m = r * self.scale * if matches!(kind & 15, 11 | 12) { 2. } else { 1. }
            + 2.
            + self.margin
            + bend;
        if a.x.max(b.x) < -m
            || a.y.max(b.y) < -m
            || a.x.min(b.x) > self.size.x + m
            || a.y.min(b.y) > self.size.y + m
        {
            return;
        }
        if let Some(rec) = self.out.get_mut(self.count) {
            *rec = Primitive {
                ends: [a.x, a.y, b.x, b.y],
                rgba: [c[0], c[1], c[2], alpha.clamp(0., 1.)],
                shape: [r * self.scale, (kind & 15) as f32, (kind >> 4) as f32, 0.],
            }
        }
        self.count += 1;
    }
    fn dot(&mut self, kind: u32, p: Point, r: f32, c: Rgb, alpha: f32) {
        self.emit(kind, p, p, r, c, alpha)
    }
}
fn scintillation(t: f32, phase: f32) -> f32 {
    // Seeded frequency modulation keeps the phases independent and the
    // instantaneous frequency well inside the real-time seeing band.
    let hz = 0.52 + phase.fract() * 0.94;
    (std::f32::consts::TAU * hz * t + phase + 0.75 * (t * 0.63 + phase * 0.37).sin()).sin()
}
fn radius(s: Star, art: &Art) -> f32 {
    let base = 0.72 + 2.20 * s.flux.sqrt();
    if s.mag > 3.5 {
        (base * art.scale.sqrt()).min(1.6)
    } else {
        base * art.scale
    }
}
fn star_pos(a: &Frame, b: &Frame, i: usize, t: f32) -> Point {
    let s = a.stars[i];
    let q = b.stars[i];
    if s.born == q.born {
        s.p.mix(q.p, t)
    } else {
        s.p
    }
}
pub(super) fn draw(
    frame: &Frame,
    next: &Frame,
    v: &View,
    blend: f32,
    static_layer: bool,
    out: &mut [Primitive],
) -> usize {
    let mut g = Batch {
        out,
        count: 0,
        origin: pt(v.viewport.x as f32, v.viewport.y as f32),
        offset: pt(v.offset_x as f32, v.offset_y as f32),
        scale: v.uniform_scale as f32,
        size: pt(v.viewport.width as f32, v.viewport.height as f32),
        margin: if static_layer {
            12. * v.uniform_scale as f32
        } else {
            0.
        },
    };
    let art = &frame.art;
    let t = if frame.still {
        0.
    } else {
        v.timeline_ns as f32 * 1e-9
    };
    if static_layer {
        for &(p, r, a) in &art.dust {
            g.dot(CORE, p, r, art.stars[1], a)
        }
        return g.count;
    }
    let root = art.scale.sqrt();
    // Include a complete diffraction spike, its width and the shader fringe.
    // Check once per star before computing its twinkle, colour and event lighting.
    let reach = 30. * root + 2. / g.scale;
    let first = g.origin - g.offset / g.scale - pt(reach, reach);
    let last = g.origin + (g.size - g.offset) / g.scale + pt(reach, reach);
    let on_screen = |p: Point| p.x >= first.x && p.x <= last.x && p.y >= first.y && p.y <= last.y;
    let at = |i| star_pos(frame, next, i, blend);
    let mut nebula_lobes = 0;
    for e in &frame.events {
        if nebula_lobes >= 20 && e.kind != Kind::Nova {
            continue;
        }
        let age = t - e.start;
        if age < 0. {
            continue;
        }
        let (strength, rad) = match e.kind {
            Kind::Birth => (smooth(age / 3.) * smooth((e.duration - age) / 60.), 70.),
            Kind::Nova if age > 4. => (
                smooth((age - 4.) / 2.) * smooth((e.duration - age) / 25.),
                102.,
            ),
            _ => continue,
        };
        let before = g.count;
        let nova = e.kind == Kind::Nova;
        // Offset knots and unequal limbs keep the centre hollow. Each
        // orientation is seeded independently of its direction from the core.
        for (j, (az, dist, r, tilt, aspect)) in [
            (0.1, 0.56, 0.64, 0.5, 1.47),
            (1.26, 0.60, 0.47, 0.8, 1.85),
            (3.05, 0.52, 0.61, -0.7, 1.38),
            (4.7, 0.50, 0.50, 0.9, 1.67),
        ]
        .into_iter()
        .enumerate()
        {
            let angle = e.variant * std::f32::consts::TAU + az;
            let p = e.p + Point::polar(angle) * rad * dist;
            let c = if j % 2 == 0 {
                art.haze[0]
            } else if art.forest_on_plum {
                art.haze[1]
            } else {
                mix(art.haze[1], art.accent, 0.30)
            };
            g.emit(
                11 | ADD,
                p,
                p + Point::polar(angle + tilt) * rad * r * aspect,
                rad * r,
                if nova {
                    c.map(|v| (v * 1.65).min(1.))
                } else {
                    c
                },
                strength
                    * (if j == 0 { 0.070 } else { 0.060 })
                    * if art.forest_on_plum {
                        2.6
                    } else if nova {
                        1.
                    } else {
                        art.tone
                    },
            );
        }
        nebula_lobes += g.count - before;
        if nova {
            let axis = Point::polar(e.variant * 6.);
            for &arc in e.shell.iter().take_while(|&&arc| arc > 0.) {
                let slot = g.count;
                g.emit(
                    12 | ADD,
                    e.p,
                    e.p + axis * rad * 1.23,
                    rad * 0.86,
                    art.haze[1],
                    strength * 0.052 * art.tone,
                );
                if g.count > slot
                    && let Some(record) = g.out.get_mut(slot)
                {
                    record.shape[3] = arc;
                }
            }
        }
    }
    // The shock passes behind the chart and its stars. A star already lit by
    // the blast must not receive a second white highlight from the wavefront.
    for e in frame.events.iter().filter(|e| e.kind == Kind::Nova) {
        let age = t - e.start;
        if (4. ..7.).contains(&age) {
            let u = (age - 4.) / 3.;
            g.dot(
                RING,
                e.p,
                super::events::shock_radius(age).max(1.),
                art.accent,
                0.35 * smooth(u / 0.075) * (1. - smooth(u)),
            );
        }
    }
    for (i, &s) in frame
        .stars
        .iter()
        .enumerate()
        .filter(|(_, s)| s.alive && s.mag >= 4.)
    {
        let p = at(i);
        if on_screen(p) {
            paint_star(&mut g, i, s, p, frame, t);
        }
    }
    let first_ink = g.count;
    let mut retracting = false;
    for (edge_id, e) in frame.edges.iter().enumerate() {
        if e.end <= 0. {
            continue;
        } // Ink dropped before activation was never visible.
        if e.live()
            && frame
                .binaries
                .iter()
                .any(|p| (p.a == e.a && p.b == e.b) || (p.a == e.b && p.b == e.a))
        {
            continue;
        }
        let [a, b] = if e.live() {
            [at(e.a), at(e.b)]
        } else {
            e.cut_at
        };
        let dir = (b - a).unit();
        let ga = radius(frame.stars[e.a], art) + 3.;
        let gb = radius(frame.stars[e.b], art) + 3.;
        if (b - a).len() < ga + gb {
            continue;
        }
        let (a, b) = (a + dir * ga, b - dir * gb);
        let (hue, base_alpha) = if art.spectrum {
            (
                art.hues[frame.stars[e.a].figure % 6],
                art.hue_alpha[frame.stars[e.a].figure % 6],
            )
        } else {
            (art.lines, art.line_alpha)
        };
        let width = (0.54 * root).min(0.75);
        let wave = t - e.wave;
        let ink_luma = 0.2126 * hue[0] + 0.7152 * hue[1] + 0.0722 * hue[2];
        let alpha = base_alpha.min(0.49 / (1.43 * ink_luma.max(0.01)))
            * (1. + 0.43 * smooth((wave - 0.25) / 1.5))
            * (1. - 0.9 * visitor_glare(frame, t, a, b));
        let alpha = if e.live() {
            alpha
                * stellar_light(frame.stars[e.a], t, frame.life).min(stellar_light(
                    frame.stars[e.b],
                    t,
                    frame.life,
                ))
        } else {
            alpha
        };
        // A cut in the upper frame also constrains this interpolated sample.
        // Keep the attached ends drifting until the cut, but route the ink
        // through the usual retract clipping before it can overlap live ink.
        let cut_next = e.live()
            && next
                .edges
                .iter()
                .any(|next| !next.live() && next.a == e.a && next.b == e.b && next.born == e.born);
        if !e.live() || cut_next {
            let u = smooth((t - e.end) / 0.8);
            let m = a.mix(b, 0.5);
            let first = g.count;
            g.emit(
                LINE,
                a,
                m.mix(a, u),
                width * (1. - u),
                hue,
                alpha * (1. - u),
            );
            g.emit(
                LINE,
                b,
                m.mix(b, u),
                width * (1. - u),
                hue,
                alpha * (1. - u),
            );
            retracting |= g.count > first;
            let end = g.count.min(g.out.len());
            for record in g.out.get_mut(first..end).into_iter().flatten() {
                record.shape[3] = -((edge_id + 1) as f32);
            }
        } else {
            let trace = 1. - (1. - ((t - e.born) / 1.2).clamp(0., 1.)).powi(3);
            let tip = a.mix(b, trace);
            // The nib and ink share one coverage function. Painting a separate
            // additive disc here makes a bright pulse as the tip overtakes its line.
            let pen = if t - e.born < 1.2 { PEN } else { LINE };
            let before = g.count;
            g.emit(pen, a, tip, width, hue, alpha);
            if pen == PEN
                && g.count > before
                && let Some(record) = g.out.get_mut(before)
            {
                record.shape[3] = smooth((1.2 - (t - e.born)) / 0.15);
            }
            if (0. ..0.65).contains(&wave) {
                let (start, end) = if e.reverse { (b, a) } else { (a, b) };
                let before = g.count;
                g.emit(
                    PULSE | ADD,
                    start,
                    end,
                    5.,
                    mix(hue, [1.; 3], 0.30),
                    (0.85 - alpha).max(0.20) * (1. - visitor_glare(frame, t, a, b)),
                );
                if g.count > before
                    && let Some(record) = g.out.get_mut(before)
                {
                    record.shape[3] = wave;
                }
            }
        }
    }
    for pair in &frame.binaries {
        let (a, b) = (at(pair.a), at(pair.b));
        let center = a.mix(b, 0.5);
        let mut light = smooth((t - pair.born) / 0.8);
        for e in &frame.events {
            if e.kind == Kind::Comet {
                light *= smooth(((center - e.position(t)).len() - 45.) / 85.);
            } else if e.kind == Kind::Nova && (4. ..7.).contains(&(t - e.start)) {
                let radius = super::events::shock_radius(t - e.start);
                light *= smooth(((center - e.p).len() - radius) / 25.);
            }
        }
        g.dot(HALO | ADD, center, 10., art.lines, 0.12 * light);
    }
    if retracting {
        clip_retractions(&mut g, first_ink, frame, &at, t);
    }
    for (i, &s) in frame
        .stars
        .iter()
        .enumerate()
        .filter(|(_, s)| s.alive && s.mag < 4.)
    {
        let p = at(i);
        if on_screen(p) {
            paint_star(&mut g, i, s, p, frame, t);
        }
    }
    for &e in &frame.events {
        paint_event(&mut g, e, t, art)
    }
    g.count
}
// A retracting stroke stays where it was inked while live stars and lines
// keep drifting. Clip each half from its attached end against the presented
// geometry. Live ink has priority, followed by older retracting strokes.
fn clip_retractions(
    g: &mut Batch<'_>,
    first: usize,
    frame: &Frame,
    at: &impl Fn(usize) -> Point,
    time: f32,
) {
    let end = g.count.min(g.out.len());
    for i in first..end {
        let rec = g.out[i];
        if rec.shape[3] >= 0. {
            continue;
        }
        let edge = frame.edges[(-rec.shape[3]) as usize - 1];
        let start = pt(rec.ends[0], rec.ends[1]);
        let delta = pt(rec.ends[2], rec.ends[3]) - start;
        let length = delta.len();
        let dir = delta / length.max(0.001);
        let cross = |a: Point, b: Point| a.x * b.y - a.y * b.x;
        let mut visible = length;
        for other in &g.out[first..end] {
            if other.shape[3] < 0. || other.rgba[3] <= 0. || !matches!(other.shape[1], 0. | 9.) {
                continue;
            }
            let p = pt(other.ends[0], other.ends[1]);
            let q = pt(other.ends[2], other.ends[3]);
            let v = q - p;
            let denominator = cross(dir, v);
            let projected = dir.dot(v);
            if denominator * denominator < 0.0669873 * v.len2() && projected.abs() > 0.001 {
                let x = dir.dot(p - start);
                let y = x + projected;
                let a = x.min(y).max(0.);
                let b = x.max(y).min(visible);
                if b > a {
                    let offset = cross(dir, p - start);
                    let d0 = offset + denominator * ((a - x) / projected);
                    let d1 = offset + denominator * ((b - x) / projected);
                    let gap = 40. * g.scale;
                    if d0.abs() < gap {
                        visible = (a - 1.5 * g.scale).max(0.);
                    } else if d0 * d1 < 0. || d1.abs() < gap {
                        let entry = a + (b - a) * (d0.signum() * gap - d0) / (d1 - d0);
                        visible = (entry - 1.5 * g.scale).max(0.);
                    }
                }
            }
            // A nearby apparent node gets the same opening angle as a live
            // chart junction, even when the retiring half used another star.
            for (base, axis) in [(p, v), (q, v * (-1.))] {
                if (start - base).len2() < (20. * g.scale).powi(2)
                    && dir.dot(axis) > 0.
                    && dir.dot(axis).powi(2) > 0.6710101 * axis.len2()
                {
                    visible = 0.;
                }
            }
            if denominator.abs() < 0.0001 {
                continue;
            }
            let along = cross(p - start, v) / denominator;
            let across = cross(p - start, dir) / denominator;
            if along >= 0. && along <= visible && (0. ..=1.).contains(&across) {
                visible = (along - (1.5 * g.scale)).max(0.);
            }
        }
        for (j, s) in frame.stars.iter().enumerate() {
            if j == edge.a
                || j == edge.b
                || !s.alive
                || s.mag >= 4.5
                || stellar_light(*s, time, frame.life) < 0.6
            {
                continue;
            }
            let p = (at(j) - g.origin) * g.scale + g.offset - start;
            let along = p.dot(dir);
            let clearance = (radius(*s, &frame.art) + 3.) * g.scale;
            if along < -clearance || along > visible + clearance {
                continue;
            }
            let perpendicular = cross(dir, p);
            let chord = clearance * clearance - perpendicular * perpendicular;
            if chord > 0. {
                let half = chord.sqrt();
                if along + half >= 0. {
                    visible = visible.min((along - half).max(0.));
                }
            }
        }
        let tip = start + dir * visible;
        let record = &mut g.out[i];
        record.ends[2] = tip.x;
        record.ends[3] = tip.y;
        record.shape[3] = 0.;
        if visible < 0.5 * g.scale {
            record.rgba[3] = 0.;
        }
    }
}
// The burst raises the local background around the giant. Reduce the
// contrast of chart ink and neighbouring stars there until the shock has
// passed, so their motion cannot flash through the brilliant core.
fn visitor_glare(frame: &Frame, time: f32, a: Point, b: Point) -> f32 {
    let mut glare = 0f32;
    for e in &frame.events {
        let age = time - e.start;
        let center = match e.kind {
            Kind::Nova if (2. ..9.).contains(&age) => e.p,
            Kind::Comet if (0. ..e.duration).contains(&age) => e.position(time),
            _ => continue,
        };
        let delta = b - a;
        let along = ((center - a).dot(delta) / delta.len2().max(0.01)).clamp(0., 1.);
        let distance = (center - a - delta * along).len();
        if e.kind == Kind::Comet {
            // A passing coma briefly washes out the ink and small points
            // behind it. The head, endpoint and ion tail then give a pixel
            // one continuous exposure as they pass.
            let life = smooth(age / 2.) * smooth((e.duration - age) / 2.);
            glare = glare.max(life * smooth((150. - distance) / 80.));
            continue;
        }
        let near = smooth((90. - distance) / 54.);
        glare = glare.max(near * smooth((age - 2.) / 2.) * smooth((7. - age) / 2.));
        // Wash out fine detail as the moving front passes. A shocked star
        // changes direction, so its core must settle before it regains contrast.
        if age > 3.8 && distance < 345. {
            let arrival = 4. + 3. * (1. - (1. - (distance / 320.).min(1.)).sqrt());
            let since = age - arrival;
            glare = glare.max(smooth((since + 0.45) / 0.4) * smooth((1.7 - since) / 0.85));
        }
    }
    glare
}
fn stellar_light(s: Star, t: f32, life: bool) -> f32 {
    if life {
        (1. - (1. - ((t - s.born) / 1.5).clamp(0., 1.)).powi(3))
            * smooth((s.retire_at - t) / 6.)
            * if s.nova {
                1.
            } else {
                smooth((s.fade_end - t) / 6.)
            }
    } else {
        1.
    }
}
fn paint_star(g: &mut Batch<'_>, i: usize, s: Star, p: Point, frame: &Frame, t: f32) {
    let art = &frame.art;
    let root = art.scale.sqrt();
    let horizon = ((p.y - art.floor) / art.horizon).clamp(0., 1.);
    let old = if frame.life {
        smooth((s.age / s.life - 0.8) / 0.2)
    } else {
        0.
    };
    let r = radius(s, art) * (1. + 0.21 * old);
    let x = s.temp * 2.;
    let mut c = if x < 1. {
        mix(art.stars[0], art.stars[1], x)
    } else {
        mix(art.stars[1], art.stars[2], x - 1.)
    };
    c = mix(c, art.stars[3], old * 0.80 + horizon * 0.06);
    if art.spectrum {
        c = mix(c, [0.98, 0.98, 1.], 0.67);
    }
    let variation = if frame.still || art.twinkle == 0. {
        0.
    } else {
        scintillation(t, s.phase) * art.twinkle * (1. - frame.calm * 0.5)
    };
    let shimmer = 1. + variation * (0.10 + (0.10 + 0.02 * art.twinkle.powi(3)) * horizon);
    let warm = 0.03 * variation * smooth((horizon - 0.8) * 5.);
    c[0] = (c[0] * (1. + warm)).min(1.);
    c[2] = (c[2] * (1. - warm)).min(1.);
    if s.nova
        && let Some(e) = frame
            .events
            .iter()
            .find(|e| e.kind == Kind::Nova && e.star == Some(i))
    {
        let gain = smooth((t - e.start) / 4.);
        c = mix(c, [1.; 3], gain * 0.85);
        g.dot(HALO | ADD, p, 10. + 18. * gain, c, 0.5 * gain);
    }
    let life = stellar_light(s, t, frame.life);
    let life = if s.nova {
        life
    } else {
        life * (1. - 0.91 * visitor_glare(frame, t, p, p))
    };
    let flare = 1. + 0.35 * smooth((t - s.flare + 0.06) / 0.06) * smooth((s.flare + 0.4 - t) / 0.4);
    if s.linked {
        let luma = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        c = mix(c, [1.; 3], ((0.83 - luma) / (1.001 - luma)).clamp(0., 0.78));
    }
    let brightness = life
        * flare
        * if s.linked {
            (s.core_light * shimmer).max(0.96)
        } else {
            s.core_light * shimmer
        };
    if s.mag < 3.8 {
        let halo_color = if art.spectrum {
            art.hues[s.figure % 6]
        } else {
            c
        };
        let halo = (3. + 11. * s.flux.sqrt()) * root;
        g.dot(
            HALO | ADD,
            p,
            halo,
            halo_color,
            (0.12 + 0.40 * s.flux.sqrt())
                * life
                * shimmer
                * flare
                * (1. + 0.3 * smooth((s.born + 0.3 - t) / 0.3)),
        );
    }
    if s.mag < 1. {
        let spike = (10. + 18. * s.flux) * root;
        for d in [pt(1., 0.), pt(-1., 0.), pt(0., 1.), pt(0., -1.)] {
            g.emit(
                TAPER | ADD,
                p + d * spike,
                p,
                0.68 * root,
                c,
                0.20 * life * shimmer,
            );
        }
    }
    let r = if s.linked { r.max(1.15) } else { r };
    if art.scale > 1. || s.nova {
        g.dot(HALO, p, r * 1.85, c, brightness);
    } else {
        g.dot(CORE, p, r, c, brightness);
    }
    if s.mag < 1.8 {
        g.dot(
            CORE | ADD,
            p,
            (r * 0.44).max(0.62),
            art.stars[0],
            0.54 * life * shimmer,
        )
    }
}
fn paint_event(g: &mut Batch<'_>, e: Moment, t: f32, a: &Art) {
    let age = t - e.start;
    if age < 0. {
        return;
    }
    let u = (age / e.duration).clamp(0., 1.);
    let p = e.position(t);
    let fade = smooth(u * 12.) * smooth((1. - u) * 9.);
    match e.kind {
        Kind::Meteor => {
            let d = e.delta.unit();
            let q = p - d * e.tail * (1. - 0.48 * u);
            if age < e.duration {
                g.emit(TAPER | ADD, q, p, 1.85, a.accent, 0.85 * fade);
                g.dot(HALO | ADD, p, 9., a.accent, 0.45 * fade);
                g.dot(CORE, p, 2.1, a.accent, fade);
            }
            if e.variant < 0.2 {
                let train = if age >= e.duration {
                    smooth(1. - (age - e.duration) / 2.)
                } else {
                    smooth(u * 4.)
                };
                g.emit(
                    TAPER | ADD,
                    p - d * e.tail * 1.4,
                    p,
                    1.2,
                    a.haze[1],
                    0.16 * train,
                );
            }
        }
        Kind::Satellite => {
            let flare = if e.variant < 0.25 {
                (1. - ((age - e.duration * 0.5) / 0.75).abs()).max(0.)
            } else {
                0.
            };
            g.dot(
                CORE,
                p,
                1.35 + 1.8 * flare,
                a.accent,
                fade * (0.70 + 0.3 * flare),
            );
            g.dot(
                HALO | ADD,
                p,
                4. + 12. * flare,
                a.accent,
                fade * (0.11 + 0.5 * flare),
            );
        }
        Kind::Comet => {
            let d = (p - a.sun).unit();
            // Both tails emerge continuously from the coma. A single curved
            // profile avoids double exposure at the joints of a moving polyline.
            g.emit(TAPER | ADD, p + d * 330., p, 2.3, a.haze[0], 0.18 * fade);
            g.emit(
                DUST_TAIL | ADD,
                p,
                p + d * 510.,
                38.,
                a.haze[1],
                0.30 * fade,
            );
            g.dot(HALO | ADD, p, 32., a.accent, 0.32 * fade);
            g.dot(CORE, p, 2.8, a.accent, fade);
        }
        Kind::Nova => {
            let bloom = if age < 4. {
                smooth((age - 3.7) / 0.3)
            } else {
                (-(age - 4.) / 0.6).exp()
            };
            if bloom > 0.001 {
                g.dot(
                    HALO | ADD,
                    e.p,
                    84.,
                    mix(a.accent, [1.; 3], 0.68),
                    0.96 * bloom,
                );
                g.dot(HALO | ADD, e.p, 17., [1.; 3], 0.78 * bloom);
            }
        }
        _ => {}
    }
}
/// # Safety
/// The retained lease is immutable. The caller owns capacity writable records.
/// Returns the required count, allowing an exact retry if the buffer was small.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_constellation_primitives(
    lease: *const Lease,
    static_layer: bool,
    out: *mut Primitive,
    capacity: usize,
) -> usize {
    let Some(l) = (unsafe { lease.as_ref() }) else {
        return 0;
    };
    if out.is_null() || capacity > 1_000_000 {
        return 0;
    }
    let Some(a) = l
        .sample
        .lower
        .typed
        .as_ref()
        .and_then(|a| a.downcast_ref::<Frame>())
    else {
        return 0;
    };
    let Some(b) = l
        .sample
        .upper
        .typed
        .as_ref()
        .and_then(|a| a.downcast_ref::<Frame>())
    else {
        return 0;
    };
    draw(a, b, &l.view, l.sample.alpha as f32, static_layer, unsafe {
        std::slice::from_raw_parts_mut(out, capacity)
    })
}

/// # Safety
/// The lease is retained and the output points to two writable floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_constellation_parallax(lease: *const Lease, out: *mut [f32; 2]) {
    if let (Some(l), Some(result)) = (unsafe { lease.as_ref() }, unsafe { out.as_mut() })
        && let Some(f) = l
            .sample
            .lower
            .typed
            .as_ref()
            .and_then(|a| a.downcast_ref::<Frame>())
    {
        let t = if f.still {
            0.
        } else {
            l.view.timeline_ns as f32 * 1e-9
        };
        let p = f.art.band_shift * (t / 240.).sin() * 12. * l.view.uniform_scale as f32;
        *result = [p.x, p.y];
    }
}
