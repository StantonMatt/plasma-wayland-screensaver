use super::{
    agent::{NODES, smooth},
    common::{flow::p, layout::Vec2},
    ffi::{Lease, View},
    material::{Art, Silk},
    simulation::Sea,
};
use std::sync::Arc;

// Advance a sampled sine/cosine pair by a known angular increment.
fn rotate((s, c): (f64, f64), (ds, dc): (f64, f64)) -> (f64, f64) {
    (s * dc + c * ds, c * dc - s * ds)
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Vertex {
    pub pos: [f32; 2],
    // Direction of the shared cross section. Its length carries local curvature
    // as 1 + 50*k. Spark records use these channels for red and green.
    pub normal: [f32; 2],
    pub half_width: f32,
    pub facing: f32,
    pub pulse: f32,
    pub alpha: f32,
    pub halo_radius: f32,
    pub cap_mix: f32,
    pub tip_distance: f32,
    pub tip_radius: f32,
}
impl Vertex {
    fn mix(self, b: Self, t: f32) -> Self {
        let f = |a: f32, b: f32| a + (b - a) * t;
        Self {
            pos: [f(self.pos[0], b.pos[0]), f(self.pos[1], b.pos[1])],
            normal: [
                f(self.normal[0], b.normal[0]),
                f(self.normal[1], b.normal[1]),
            ],
            half_width: f(self.half_width, b.half_width),
            facing: f(self.facing, b.facing),
            pulse: f(self.pulse, b.pulse),
            alpha: f(self.alpha, b.alpha),
            halo_radius: f(self.halo_radius, b.halo_radius),
            cap_mix: f(self.cap_mix, b.cap_mix),
            tip_distance: f(self.tip_distance, b.tip_distance),
            tip_radius: f(self.tip_radius, b.tip_radius),
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct VertexPair {
    pub lower: Vertex,
    pub upper: Vertex,
    /// Exact small integer vertex attribute. 0xffff marks a strip break.
    pub tag: f32,
}
impl VertexPair {
    pub fn at(self, alpha: f32) -> Vertex {
        self.lower.mix(self.upper, alpha)
    }
}
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Draw {
    pub first: u32,
    pub count: u32,
    pub motes: u32,
    pub composition: u32,
    pub opacity: f32,
    pub material: u32,
    pub silk: Silk,
    pub strand: u32,
    pub shoulder_gain: [f32; 2],
}
#[derive(Clone, Copy)]
struct Strip {
    id: u32,
    offset: usize,
    count: usize,
    layer: usize,
    hue: usize,
    margin: f32,
    outline: Outline,
    cap_start: usize,
    cap_count: usize,
    fitted: f32,
    nominal: f32,
    tip_radius: f32,
    shoulder_gain: f32,
    bounds: [f32; 4],
}
#[derive(Clone, Copy)]
struct Spark {
    id: u32,
    vertex: Vertex,
}
pub(super) struct Frame {
    pub still: bool,
    art: Arc<Art>,
    strips: Vec<Strip>,
    vertices: Vec<BodyKnot>,
    caps: Vec<[f32; 2]>,
    sparks: Vec<Spark>,
}
// Use the final, flutter-displaced chord on both sides of each knot. The
// angular speed of either adjoining segment bounds its shared transverse
// section. Hermite endpoint curvature also covers an S-shaped segment whose
// two tangents happen to be parallel.
fn fit_sections(
    nodes: &mut [BodyKnot],
    active_cap_knots: usize,
    spline_tangents: bool,
    layer: usize,
    mut head_radius: f32,
) -> f32 {
    let position = |node: &BodyKnot| p(node.pos[0] as f64, node.pos[1] as f64);
    let mut curvatures = [0.0_f64; NODES];
    if !spline_tangents {
        let mut outgoing = (position(&nodes[1]) - position(&nodes[0])).unit();
        for i in 0..nodes.len() {
            let incoming = outgoing;
            if i + 1 < nodes.len() {
                outgoing = (position(&nodes[i + 1]) - position(&nodes[i])).unit();
            }
            if nodes[i].normal == [0.; 2] {
                let direction = (incoming + outgoing).unit();
                nodes[i].normal = [-direction.y as f32, direction.x as f32];
            }
        }
    }
    let tangent = |node: &BodyKnot| p(node.normal[1] as f64, -node.normal[0] as f64);
    let cross = |a: Vec2, b: Vec2| a.x * b.y - a.y * b.x;
    for (i, span) in nodes.windows(2).enumerate() {
        let delta = position(&span[1]) - position(&span[0]);
        let length = delta.length().max(1e-5);
        let chord = delta / length;
        let a = tangent(&span[0]);
        let b = tangent(&span[1]);
        // 2*tan(angle/2) conservatively bounds the tangent rotation in radians.
        let rotation = 2. * cross(a, b).abs() / (1. + a.dot(b)).max(1e-6);
        let at_start = cross(a, chord * 6. - b * 2.).abs();
        let at_end = cross(b, chord * -6. + a * 2.).abs();
        let curvature = rotation.max(at_start).max(at_end) / length;
        curvatures[i] = curvatures[i].max(curvature);
        curvatures[i + 1] = curvatures[i + 1].max(curvature);
    }
    for (index, (node, curvature)) in nodes.iter_mut().zip(curvatures).enumerate() {
        let encoded = (1. + 50. * curvature) as f32;
        node.normal[0] *= encoded;
        node.normal[1] *= encoded;
        let face = 0.12 + 0.88 * node.facing.abs();
        node.half_width = node
            .half_width
            .min(0.5 / ((curvature as f32).max(1e-6) * face));
        // A shoulder in a tight bend has less usable halo than a flat scarf.
        // Carry that same physical radius around the entire rounded cap.
        if index < active_cap_knots && curvature > 0. {
            let projected = node.half_width * face;
            let aa = match layer {
                0 => 2.03,
                2 => (0.266 * projected).max(1.2),
                _ => 1.2,
            };
            let supported = (0.85 / curvature as f32 - projected - aa) / 1.2;
            head_radius = head_radius.min(supported.max(0.));
        }
    }
    head_radius
}
const HEAD_SPANS: usize = 8;

fn flag_phase_scale(length: f64, width: f64) -> f64 {
    (std::f64::consts::TAU * 0.6 * length * 0.15 / (0.85 * 9. * width)).min(2.65)
}

#[derive(Clone, Copy)]
struct Outline {
    length: f64,
    width: f64,
    head: f64,
    lens: f64,
    body_spans: usize,
}
impl Outline {
    fn new(length: f64, width: f64, spacing: f64) -> Self {
        let head = (0.03 * length).clamp(width, 2.5 * width).min(0.5 * length);
        let body_spans =
            (((length - head) / spacing).ceil() as usize).clamp(8, NODES - HEAD_SPANS - 2);
        Self {
            length,
            width: width * (length / (8. * width)).min(1.),
            head,
            lens: 1. - smooth((length / width - 8.) / 4.),
            body_spans,
        }
    }
    fn arc(&self, i: usize) -> f64 {
        if i < HEAD_SPANS {
            // Equal angle increments follow the quarter ellipse right up to
            // its vertical tangent. The shoulder is horizontal at pi/2.
            const ANGLE_STEPS: [f64; 8] = [
                0.,
                0.019214720,
                0.076120467,
                0.168530388,
                0.292893219,
                0.444429767,
                0.617316568,
                0.804909678,
            ];
            self.head * ANGLE_STEPS[i]
        } else {
            let t = (i - HEAD_SPANS) as f64 / self.body_spans as f64;
            self.head + (self.length - self.head) * t
        }
    }
    fn half_width(&self, arc: f64) -> f64 {
        if arc <= 0. || arc >= self.length {
            return 0.;
        }
        let tip = (1. - arc / self.head).max(0.);
        let mature =
            (1. - tip * tip).sqrt() * smooth((self.length - arc) / (0.25 * self.length)).sqrt();
        let profile = if self.lens > 0. {
            let lens_shape = (std::f64::consts::PI * arc / self.length)
                .sin()
                .max(0.)
                .powf(0.7);
            mature + (lens_shape - mature) * self.lens
        } else {
            mature
        };
        // The face reaches zero at the rounded nose and at the fading tail.
        0.5 * self.width * profile
    }
}
#[cfg(test)]
#[test]
fn short_lenses_and_mature_caps_reach_both_round_ends() {
    let short = Outline::new(60., 16., 24.);
    assert_eq!(short.width, 7.5);
    let samples = HEAD_SPANS + short.body_spans;
    assert!(samples >= 16);
    assert_eq!(short.half_width(short.arc(0)), 0.);
    assert_eq!(short.half_width(short.arc(samples)), 0.);
    let mut previous = 0.;
    for i in 1..samples {
        let arc = short.arc(i);
        assert!(arc > previous && short.half_width(arc) > 0.);
        previous = arc;
    }
    let mature = Outline::new(1400., 18., 24.);
    assert!((mature.head - 42.).abs() < 1e-12);
    assert_eq!(mature.half_width(mature.arc(HEAD_SPANS)), 9.);
    assert!(mature.half_width(mature.head - 0.01) > 8.99999);
}
#[cfg(feature = "visual-capture")]
static UNFLAPPED_CONTROL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

// Only the linked native capture fixture selects this matched material control.
#[cfg(feature = "visual-capture")]
#[unsafe(no_mangle)]
pub extern "C" fn visuals_ribbons_capture_unflapped() {
    UNFLAPPED_CONTROL.store(true, std::sync::atomic::Ordering::Relaxed);
}

impl Frame {
    pub fn new(art: Arc<Art>, count: usize, still: bool) -> Self {
        Self {
            art,
            still,
            strips: Vec::with_capacity(count),
            vertices: Vec::with_capacity(count * NODES),
            caps: Vec::with_capacity(count * NODES),
            sparks: Vec::with_capacity(512 + count),
        }
    }
    pub fn fill(&mut self, sea: &Sea, twist: f64) {
        #[cfg(feature = "visual-capture")]
        let gain = if UNFLAPPED_CONTROL.load(std::sync::atomic::Ordering::Relaxed) {
            0.
        } else {
            1.
        };
        #[cfg(not(feature = "visual-capture"))]
        let gain = 1.;
        self.fill_with_wave(sea, twist, gain);
    }
    fn fill_with_wave(&mut self, sea: &Sea, twist: f64, flag_gain: f64) {
        self.vertices.clear();
        self.caps.clear();
        self.strips.clear();
        self.sparks.clear();
        let mut base = [BodyKnot::default(); NODES];
        for r in sea.agents.iter().filter(|r| r.alive) {
            let end = r.extent() * (r.count - 1) as f64;
            let count = ((end.ceil() as usize) + 1).clamp(2, r.count);
            let length = end * r.spacing;
            // The seed and ending bloom cover the subpixel part of the life cycle.
            if length < 2. {
                continue;
            }
            let smooth_shared = sea.shared && r.layer > 0;
            let outline = Outline::new(length, r.width, r.spacing);
            let samples = HEAD_SPANS + outline.body_spans + 1;
            self.strips.push(Strip {
                id: r.id,
                offset: self.vertices.len(),
                outline,
                cap_start: self.caps.len(),
                cap_count: 0,
                fitted: 0.,
                nominal: 0.,
                tip_radius: 0.,
                shoulder_gain: 0.,
                count: samples + 1,
                layer: r.layer,
                hue: r.hue % 6,
                // The shader extends the projected cross-section into the
                // halo and bows the centre by at most one third of a chord.
                // Flutter can displace each endpoint by 0.42 face widths.
                margin: 2.18
                    * r.width as f32
                    * (0.635 + 3.5 * self.art.0[r.layer][r.hue % 6].halo[3])
                    + (r.spacing + 0.84 * r.width) as f32 / 3.,
                bounds: [
                    f32::INFINITY,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                    f32::NEG_INFINITY,
                ],
            });
            let inv_end = 1. / end.max(1.);
            let moment = sea.weather.active;
            let orbit = moment.filter(|m| r.caught && m.kind == super::water::Kind::Maelstrom);
            let spoke = orbit.map(|m| {
                super::agent::polar(2.45 * m.age - r.layer as f64 * std::f64::consts::TAU / 3.)
            });
            let gust = moment.filter(|m| m.kind == super::water::Kind::Gust);
            let still = moment
                .filter(|m| m.kind == super::water::Kind::Still)
                .map_or(0., |m| m.weight());
            let gust_wave_number = 1. / (2200. * 0.4 * r.width).sqrt();
            // Logarithmic spatial phase gives successively shorter half-waves.
            // Both the full shoulder wavelength and the shortest tail wavelength
            // scale with the actual width, including on a 200% near scarf.
            let chirp = flag_phase_scale(length, r.width);
            let flag_amplitude = flag_gain
                * 0.14
                * r.width
                * twist
                * (1. - still)
                * smooth(length / (20. * r.width));
            let fringe_amplitude = 0.22 * twist * r.fringe_strength();
            let mut fringe_phase = (r.motion_age * 1.1).sin_cos();
            let fringe_step = (-0.31_f64).sin_cos();
            for i in 0..count {
                let j = i.min(r.count - 1);
                let mut pos = r.nodes[j].pos;
                let u = (i as f64 * inv_end).min(1.);
                let tangent = (r.nodes[(j + 1).min(r.count - 1)].pos
                    - r.nodes[j.saturating_sub(1)].pos)
                    .unit();
                let fringe = (u * u * u * fringe_amplitude) * fringe_phase.0;
                fringe_phase = rotate(fringe_phase, fringe_step);
                let mut facing = (r.appearance_phase(j) + fringe).cos();
                if let Some((m, axis)) = orbit.zip(spoke) {
                    let locked = (pos - m.centre).unit().dot(axis);
                    facing += (locked - facing) * m.weight();
                }

                let arc = i as f64 * r.spacing;
                let mut pulse = 0f64;
                if !self.still {
                    for distance in r.pulses.iter().filter(|d| **d >= 0.) {
                        let z = (arc - distance) / 25.;
                        if z.abs() < 3.5 {
                            pulse = pulse.max((-0.5 * z * z).exp());
                        }
                    }
                }
                let side = tangent.perpendicular();
                let flag_u = ((u - 0.4) / 0.6).clamp(0., 1.);
                let mut offset = 0.;
                if flag_u > 0. && flag_amplitude > 0. {
                    let phase = -chirp * (-0.85 * flag_u).ln_1p() - r.motion_age * 1.75;
                    offset = flag_amplitude * smooth(flag_u) * phase.sin();
                }
                if let Some(front) = gust {
                    let long_wave = (arc * gust_wave_number - r.motion_age * 0.85).sin();
                    // A gust bends the head end of every lane as well as its tail.
                    // The displacement has a 2,200 px curvature radius at its
                    // extrema, independent of width; there is no u² flat zone.
                    let residual = 0.4 * r.width * long_wave;
                    offset += (residual - offset) * front.gust(pos, sea.left);
                }
                pos += side * offset;
                if !smooth_shared && i == count - 1 && end < j as f64 {
                    // Clip the finished, displaced chord. Applying flutter
                    // after clipping can reverse an almost-unfurled tail.
                    let xy = base[j - 1].pos;
                    let prev = p(xy[0] as f64, xy[1] as f64);
                    pos = prev + (pos - prev) * (end - (j - 1) as f64).max(0.02);
                }
                base[i] = BodyKnot {
                    pos: [pos.x as f32, pos.y as f32],
                    normal: [0.; 2],
                    half_width: 0.,
                    facing: facing as f32,
                    pulse: pulse as f32,
                    alpha: 0.,
                };
            }
            let emergence = smooth(length / (r.width * 2.).max(8.));
            // The extra span ahead of the tip carries only the rounded glow.
            // Its shared edge uses the tip's fitted transverse section.
            self.vertices.push(BodyKnot::default());
            self.caps.push([-1., 0.]);
            let mut active_cap_knots = 0;
            let glow = self.art.0[r.layer][r.hue % 6].halo[3] as f64;
            let head_halo = (3.5 * outline.width * glow).min(length * 0.5);
            let tip_half = 0.5 * outline.width * (0.12 + 0.88 * base[0].facing.abs() as f64);
            // Curvature radius of the rounded material face at its apex.
            let tip_radius = (tip_half * tip_half / outline.head.max(0.001)) as f32;
            for i in 0..samples {
                let arc = outline.arc(i);
                let knot = arc / r.spacing;
                let j = (knot as usize).min(count - 2);
                let t = ((knot - j as f64) / (end - j as f64).min(1.)).clamp(0., 1.) as f32;
                let a = base[j];
                let b = base[j + 1];
                let mix = |a: f32, b: f32| a + (b - a) * t;
                let mut v = BodyKnot {
                    pos: [mix(a.pos[0], b.pos[0]), mix(a.pos[1], b.pos[1])],
                    facing: mix(a.facing, b.facing),
                    pulse: mix(a.pulse, b.pulse),
                    ..BodyKnot::default()
                };
                let use_nose_curve = r.layer > 0
                    && arc <= outline.head + 1.7 * head_halo
                    && arc < (count - 2) as f64 * r.spacing;
                if smooth_shared || use_nose_curve {
                    // Nonuniform head/shoulder samples on a piecewise-linear
                    // backbone concentrate a whole knot turn into its short
                    // neighbouring segment. Evaluate a C2 uniform cubic on
                    // the transported links; retain its actual knot tangent.
                    // Linear extrapolation pins the start and the end.
                    let point = |i: usize| p(base[i].pos[0] as f64, base[i].pos[1] as f64);
                    let a = point(j);
                    let b = point(j + 1);
                    let left = if j > 0 { point(j - 1) } else { a * 2. - b };
                    let right = if j + 2 < count {
                        point(j + 2)
                    } else {
                        b * 2. - a
                    };
                    let v0 = (b - left) * 0.5;
                    let v1 = (left + b - a * 2.) * 0.5;
                    let v2 = (right - left + (a - b) * 3.) / 6.;
                    let local = (knot - j as f64).clamp(0., 1.);
                    let pos = a + v1 / 3. + (v0 + (v1 + v2 * local) * local) * local;
                    let tangent = (v0 + (v1 * 2. + v2 * (3. * local)) * local).unit();
                    v.pos = [pos.x as f32, pos.y as f32];
                    v.normal = [-tangent.y as f32, tangent.x as f32];
                }
                v.half_width = outline.half_width(arc) as f32;
                let cap_mix = 1. - smooth((arc - outline.head) / head_halo.max(1.));
                // Publish full-precision cap samples only through the first
                // empty knot, which the transition segment still interpolates.
                if i == active_cap_knots {
                    self.caps.push([cap_mix as f32, -arc as f32]);
                    if cap_mix as f32 > 0. {
                        active_cap_knots += 1;
                    }
                }
                let u = arc / length;
                let taper_light = 1. - (1. - smooth((1. - u) / 0.12)) * (1. - outline.lens);
                // A few native pixels of constant tip radiance keep oblique
                // antialiased samples on the forward tangent at the same gain.
                let shoulder = 4_f64.min(0.1 * outline.head);
                let nose_blend = smooth((arc - shoulder) / (outline.head - shoulder));
                let tip_light = 1. + 0.15 * (1. - nose_blend);
                if arc < 2. * shoulder {
                    let pulse_blend = smooth((arc - shoulder) / shoulder) as f32;
                    v.pulse = base[0].pulse + (v.pulse - base[0].pulse) * pulse_blend;
                }
                v.alpha = (r.light * emergence * taper_light * tip_light) as f32;
                self.vertices.push(v);
            }
            let strip = self.strips.last_mut().unwrap();
            let nodes = &mut self.vertices[strip.offset..];
            let fitted_halo = fit_sections(
                &mut nodes[1..],
                active_cap_knots,
                smooth_shared,
                r.layer,
                head_halo as f32,
            );
            strip.cap_count = self.caps.len() - strip.cap_start;
            strip.fitted = fitted_halo;
            strip.nominal = head_halo as f32;
            strip.tip_radius = tip_radius;
            let mut cap = nodes[1];
            let tangent = p(cap.normal[1] as f64, -cap.normal[0] as f64).unit();
            let extension = f64::from(1.2 * fitted_halo).max(2.);
            cap.pos[0] -= (extension * tangent.x) as f32;
            cap.pos[1] -= (extension * tangent.y) as f32;
            self.caps[strip.cap_start][1] = extension as f32;
            nodes[0] = cap;
            let shoulder = nodes[1 + HEAD_SPANS];
            strip.shoulder_gain = shoulder.alpha * (1. + 0.5 * shoulder.pulse);
            for n in nodes {
                for k in 0..2 {
                    strip.bounds[k] = strip.bounds[k].min(n.pos[k]);
                    strip.bounds[k + 2] = strip.bounds[k + 2].max(n.pos[k]);
                }
            }
        }
        // Keep each depth layer contiguous in the paired GPU stream. Sorting
        // the much smaller strip descriptors leaves the node storage in place.
        self.strips.sort_unstable_by_key(|s| (s.layer, s.id));
        if !self.still {
            for r in &sea.agents {
                let birth = smooth(r.age / 0.4) * smooth((3.5 - r.age) / 2.);
                let dying = smooth((3.5 - r.life + r.age) / 2.) * smooth((r.life - r.age) / 0.5);
                let strength = birth.max(dying);
                if !r.alive || strength <= 0. {
                    continue;
                }
                let c = self.art.0[r.layer][r.hue % 6].accent;
                let pos = r.head();
                self.sparks.push(Spark {
                    id: r.id.wrapping_mul(64).wrapping_add(63),
                    vertex: Vertex {
                        pos: [pos.x as f32, pos.y as f32],
                        normal: [c[0], c[1]],
                        facing: c[2],
                        half_width: 3.,
                        pulse: 0.,
                        alpha: (strength * 1.3) as f32,
                        ..Vertex::default()
                    },
                });
            }
            for m in &sea.motes {
                let silk = self.art.0[1][m.hue % 6];
                let c =
                    std::array::from_fn::<_, 3, _>(|i| 0.3 * silk.front[i] + 0.7 * silk.accent[i]);
                let alpha = smooth(m.age / 0.4) * smooth((m.life - m.age) / 2.) * m.twinkle();
                self.sparks.push(Spark {
                    id: m.id,
                    vertex: Vertex {
                        pos: [m.pos.x as f32, m.pos.y as f32],
                        normal: [c[0], c[1]],
                        half_width: m.radius as f32,
                        facing: c[2],
                        alpha: alpha as f32,
                        ..Vertex::default()
                    },
                });
            }
            self.sparks.sort_unstable_by_key(|s| s.id);
        }
    }
}
// Only the rounded head uses the last four scalars of a logical knot.
// Both shader inputs retain native f32 precision; body and spark records
// never touch the cap table. Tag bits 16..31 address a pair of RGBA texels.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BodyKnot {
    pub pos: [f32; 2],
    pub normal: [f32; 2],
    pub half_width: f32,
    pub facing: f32,
    pub pulse: f32,
    pub alpha: f32,
}
impl From<Vertex> for BodyKnot {
    fn from(v: Vertex) -> Self {
        Self {
            pos: v.pos,
            normal: v.normal,
            half_width: v.half_width,
            facing: v.facing,
            pulse: v.pulse,
            alpha: v.alpha,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct MotionPair {
    pub lower: BodyKnot,
    pub upper: BodyKnot,
    pub tag: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CapPair {
    pub lower: [f32; 4],
    pub upper: [f32; 4],
}
fn uses_cap(v: &VertexPair) -> bool {
    v.lower.cap_mix != 0. || v.upper.cap_mix != 0.
}
trait RecordStream {
    const LOGICAL: bool;
    fn len(&self) -> usize;
    fn put(&mut self, index: usize, v: VertexPair, cap: bool);
    fn keep(&mut self, start: usize, end: usize, dest: usize);
    fn finish_strip(&mut self, index: usize);
}
impl RecordStream for &mut [VertexPair] {
    const LOGICAL: bool = true;
    fn len(&self) -> usize {
        (**self).len()
    }
    fn put(&mut self, index: usize, v: VertexPair, _: bool) {
        self[index] = v;
    }
    fn keep(&mut self, start: usize, end: usize, dest: usize) {
        self.copy_within(start..=end, dest);
    }
    fn finish_strip(&mut self, index: usize) {
        self[index].tag = 65535.;
    }
}
struct PackedStream<'a> {
    nodes: &'a mut [MotionPair],
    caps: &'a mut [CapPair],
    used: &'a mut usize,
}
impl RecordStream for PackedStream<'_> {
    const LOGICAL: bool = false;
    fn len(&self) -> usize {
        self.nodes.len()
    }
    fn put(&mut self, index: usize, v: VertexPair, cap: bool) {
        let mut tag = v.tag as u32;
        if cap {
            let fields = |q: Vertex| [q.halo_radius, q.cap_mix, q.tip_distance, q.tip_radius];
            self.caps[*self.used] = CapPair {
                lower: fields(v.lower),
                upper: fields(v.upper),
            };
            *self.used += 1;
            tag |= (*self.used as u32) << 16;
        }
        self.nodes[index] = MotionPair {
            lower: v.lower.into(),
            upper: v.upper.into(),
            tag,
        };
    }
    fn keep(&mut self, start: usize, end: usize, dest: usize) {
        self.nodes.copy_within(start..=end, dest);
    }
    fn finish_strip(&mut self, index: usize) {
        self.nodes[index].tag |= 65535;
    }
}

struct Writer<'a, N> {
    nodes: N,
    draws: &'a mut [Draw],
    n: usize,
    d: usize,
    bounds: [f32; 4],
    antialias_margin: f32,
}
impl<N: RecordStream> Writer<'_, N> {
    fn segment_visible(&self, a: VertexPair, b: VertexPair, margin: f32) -> bool {
        (0..2).all(|k| {
            a.lower.pos[k]
                .min(a.upper.pos[k])
                .min(b.lower.pos[k])
                .min(b.upper.pos[k])
                <= self.bounds[k + 2] + margin
                && a.lower.pos[k]
                    .max(a.upper.pos[k])
                    .max(b.lower.pos[k])
                    .max(b.upper.pos[k])
                    >= self.bounds[k] - margin
        })
    }
    fn emit(&mut self, a: &Frame, b: &Frame, opacity: f32, composition: u32) {
        if opacity <= 0. {
            return;
        }
        for (k, s) in a.strips.iter().enumerate() {
            let next = b
                .strips
                .get(k)
                .filter(|v| v.id == s.id)
                .or_else(|| b.strips.iter().find(|v| v.id == s.id));
            let Some(s2) = next else {
                continue;
            };
            // Reject the entire strip before touching its node records. The
            // union of the endpoint bounds also contains all interpolants.
            let margin = s.margin.max(s2.margin) + self.antialias_margin;
            if (0..2).any(|k| {
                s.bounds[k].min(s2.bounds[k]) > self.bounds[k + 2] + margin
                    || s.bounds[k + 2].max(s2.bounds[k + 2]) < self.bounds[k] - margin
            }) {
                continue;
            }
            let count = s.count.max(s2.count);
            // A growing or fraying tail can gain or lose a knot between
            // ticks. Match equal fractions of the two polylines in that case
            // so the last bracket does not connect to a collapsed, rotated tip.
            let sample = |frame: &Frame, strip: &Strip, i: usize| {
                let needs_cap =
                    N::LOGICAL || s.count != s2.count || i < s.cap_count.max(s2.cap_count);
                let node = |j: usize| {
                    let body = frame.vertices[strip.offset + j];
                    let mut v = Vertex {
                        pos: body.pos,
                        normal: body.normal,
                        half_width: body.half_width,
                        facing: body.facing,
                        pulse: body.pulse,
                        alpha: body.alpha,
                        ..Vertex::default()
                    };
                    if needs_cap {
                        v.tip_radius = strip.tip_radius;
                        if j < strip.cap_count {
                            let [mix, distance] = frame.caps[strip.cap_start + j];
                            v.cap_mix = mix;
                            v.tip_distance = distance;
                            v.halo_radius = strip.fitted;
                        } else {
                            v.tip_distance = -strip.outline.arc(j - 1) as f32;
                            v.halo_radius = strip.nominal;
                        }
                    }
                    v
                };
                if strip.count == count {
                    return node(i);
                }
                if i <= HEAD_SPANS + 1 {
                    return node(i);
                }
                let spans = count - 2 - HEAD_SPANS;
                let scaled = (i - HEAD_SPANS - 1) * (strip.count - 2 - HEAD_SPANS);
                let index = HEAD_SPANS + 1 + scaled / spans;
                let remainder = scaled % spans;
                if remainder == 0 {
                    node(index)
                } else {
                    node(index).mix(node(index + 1), remainder as f32 / spans as f32)
                }
            };
            let vertex = |i: usize| VertexPair {
                lower: sample(a, s, i),
                upper: sample(b, s2, i),
                tag: (s.layer * 6 + s.hue + 32 * (k + 40 * composition as usize)) as f32,
            };
            let mut first = None;
            let mut last = 0;
            // Save both endpoints straight to caller memory while finding
            // visible segment extents. Include both endpoints and the full halo.
            if self.n + count > self.nodes.len() || self.d >= self.draws.len() {
                return;
            }
            let mut prev = vertex(0);
            self.nodes.put(self.n, prev, uses_cap(&prev));
            for j in 1..count {
                let cur = vertex(j);
                self.nodes
                    .put(self.n + j, cur, uses_cap(&prev) || uses_cap(&cur));
                if self.segment_visible(prev, cur, margin) {
                    first.get_or_insert(j - 1);
                    last = j;
                }
                prev = cur;
            }
            if let Some(f) = first {
                self.nodes.keep(self.n + f, self.n + last, self.n);
                self.nodes.finish_strip(self.n + last - f);
                let silk = a.art.0[s.layer][s.hue];
                self.draws[self.d] = Draw {
                    first: self.n as u32,
                    count: (last + 1 - f) as u32,
                    motes: 0,
                    composition,
                    opacity,
                    material: (s.layer * 6 + s.hue) as u32,
                    silk,
                    strand: k as u32 + 40 * composition,
                    shoulder_gain: [s.shoulder_gain, s2.shoulder_gain],
                };
                self.n += last + 1 - f;
                self.d += 1;
            }
        }
        let start = self.n;
        // Live motes retain their order between generations; expiry only
        // removes from the sequence. Match IDs across those occasional holes.
        let mut j = 0;
        for s in &a.sparks {
            while j < b.sparks.len() && b.sparks[j].id < s.id {
                j += 1;
            }
            let Some(other) = b.sparks.get(j).filter(|v| v.id == s.id) else {
                continue;
            };
            j += 1;
            let v = VertexPair {
                lower: s.vertex,
                upper: other.vertex,
                tag: (6 + 32 * 128) as f32,
            };
            if self.segment_visible(v, v, 8.) && self.n < self.nodes.len() - 1 {
                self.nodes.put(self.n, v, false);
                self.n += 1;
            }
        }
        if self.n > start && self.d < self.draws.len() {
            self.draws[self.d] = Draw {
                first: start as u32,
                count: (self.n - start) as u32,
                motes: 1,
                composition,
                opacity,
                material: 6,
                silk: a.art.0[1][0],
                strand: 0,
                shoulder_gain: [0.; 2],
            };
            self.d += 1;
            // The instanced layout also binds node i+1 for ordinary strips.
            self.nodes.put(self.n, VertexPair::default(), false);
            self.n += 1;
        }
    }
}
fn visible_bounds(v: &View) -> [f32; 4] {
    let scale = v.uniform_scale;
    [
        (v.viewport.x - v.offset_x / scale) as f32,
        (v.viewport.y - v.offset_y / scale) as f32,
        (v.viewport.x + (v.viewport.width - v.offset_x) / scale) as f32,
        (v.viewport.y + (v.viewport.height - v.offset_y) / scale) as f32,
    ]
}
/// # Safety
/// Lease is immutable and retained, and the two output arrays and node count
/// are writable and disjoint. Capacities cover two compositions and their motes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_ribbons_write(
    lease: *const Lease,
    nodes: *mut VertexPair,
    node_capacity: usize,
    draws: *mut Draw,
    draw_capacity: usize,
    written: *mut usize,
) -> usize {
    let (Some(l), Some(n)) = (unsafe { lease.as_ref() }, unsafe { written.as_mut() }) else {
        return 0;
    };
    *n = 0;
    if nodes.is_null()
        || draws.is_null()
        || !(10754..=65536).contains(&node_capacity)
        || !(82..=1024).contains(&draw_capacity)
    {
        return 0;
    }
    emit(
        l,
        unsafe { std::slice::from_raw_parts_mut(nodes, node_capacity) },
        unsafe { std::slice::from_raw_parts_mut(draws, draw_capacity) },
        n,
    )
}
/// # Safety
/// Lease and disjoint output arrays obey the same lifetime and capacity
/// contract as the logical writer. The cap table holds at least 10,754 pairs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_ribbons_write_packed(
    lease: *const Lease,
    nodes: *mut MotionPair,
    node_capacity: usize,
    caps: *mut CapPair,
    cap_capacity: usize,
    cap_count: *mut usize,
    draws: *mut Draw,
    draw_capacity: usize,
    written: *mut usize,
) -> usize {
    let (Some(l), Some(n), Some(c)) = (
        unsafe { lease.as_ref() },
        unsafe { written.as_mut() },
        unsafe { cap_count.as_mut() },
    ) else {
        return 0;
    };
    *n = 0;
    *c = 0;
    if nodes.is_null()
        || draws.is_null()
        || caps.is_null()
        || !(10754..=65536).contains(&node_capacity)
        || !(10754..=65536).contains(&cap_capacity)
        || !(82..=1024).contains(&draw_capacity)
    {
        return 0;
    }
    let packed = PackedStream {
        nodes: unsafe { std::slice::from_raw_parts_mut(nodes, node_capacity) },
        caps: unsafe { std::slice::from_raw_parts_mut(caps, cap_capacity) },
        used: c,
    };
    emit(
        l,
        packed,
        unsafe { std::slice::from_raw_parts_mut(draws, draw_capacity) },
        n,
    )
}

/// Apply the production packet encoding to replayed native fixture knots.
/// # Safety
/// The three nonoverlapping arrays cover `len` records.
#[cfg(feature = "visual-capture")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_ribbons_pack_fixture(
    input: *const VertexPair,
    len: usize,
    nodes: *mut MotionPair,
    caps: *mut CapPair,
) -> usize {
    if len > 10754 || input.is_null() || nodes.is_null() || caps.is_null() {
        return 0;
    }
    let mut used = 0;
    let mut packed = PackedStream {
        nodes: unsafe { std::slice::from_raw_parts_mut(nodes, len) },
        caps: unsafe { std::slice::from_raw_parts_mut(caps, len) },
        used: &mut used,
    };
    let mut previous = false;
    for (i, v) in unsafe { std::slice::from_raw_parts(input, len) }
        .iter()
        .enumerate()
    {
        let active = uses_cap(v);
        packed.put(i, *v, previous || active);
        previous = v.tag != 65535. && active;
    }
    used
}

fn emit<N: RecordStream>(l: &Lease, nodes: N, draws: &mut [Draw], n: &mut usize) -> usize {
    let (Some(a), Some(b)) = (
        l.sample
            .lower
            .typed
            .as_ref()
            .and_then(|f| f.downcast_ref::<Frame>()),
        l.sample
            .upper
            .typed
            .as_ref()
            .and_then(|f| f.downcast_ref::<Frame>()),
    ) else {
        return 0;
    };
    let bounds = visible_bounds(&l.view);
    let pixel_scale = (l.view.uniform_scale * l.view.dpr) as f32;
    let mut w = Writer {
        nodes,
        draws,
        n: 0,
        d: 0,
        bounds,
        antialias_margin: 2.18 * (1.2 / pixel_scale).max(2.04),
    };
    if a.still {
        let t = dissolve(l.view.timeline_ns);
        w.emit(a, a, 1. - t, 0);
        w.emit(b, b, t, 1);
    } else {
        w.emit(a, b, 1., 0);
    }
    *n = w.n;
    w.d
}
fn dissolve(ns: i64) -> f32 {
    if ns < 600_000_000_000 {
        0.
    } else {
        smooth((ns % 600_000_000_000) as f64 / 8e9) as f32
    }
}
/// # Safety
/// Any non-null handle is a retained immutable sample.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_ribbons_dissolve(lease: *const Lease) -> f32 {
    let Some(l) = (unsafe { lease.as_ref() }) else {
        return -1.;
    };
    if l.sample
        .lower
        .typed
        .as_ref()
        .and_then(|f| f.downcast_ref::<Frame>())
        .is_some_and(|f| f.still)
    {
        dissolve(l.view.timeline_ns)
    } else {
        -1.
    }
}
/// # Safety
/// Both non-null handles must remain retained throughout the comparison.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn visuals_ribbons_same_pair(a: *const Lease, b: *const Lease) -> bool {
    let (Some(a), Some(b)) = (unsafe { a.as_ref() }, unsafe { b.as_ref() }) else {
        return false;
    };
    Arc::ptr_eq(&a.sample.lower, &b.sample.lower)
        && Arc::ptr_eq(&a.sample.upper, &b.sample.upper)
        && a.view.viewport == b.view.viewport
        && a.view.uniform_scale == b.view.uniform_scale
        && a.view.offset_x == b.view.offset_x
        && a.view.offset_y == b.view.offset_y
        && a.view.dpr == b.view.dpr
}
const _: () = {
    // A dissolve can export both 40-ribbon compositions. Even before
    // visibility trimming, every cap fits in the caller-guaranteed table.
    assert!(2 * 40 * NODES <= 10754);
    assert!(size_of::<MotionPair>() == 68 && size_of::<CapPair>() == 32);
    assert!(size_of::<Vertex>() == 48);
    assert!(size_of::<Draw>() == 100);
    assert!(size_of::<VertexPair>() == 100);
};

#[cfg(test)]
#[path = "../../../../../../tests/depth/ribbons/acceptance.rs"]
mod acceptance;

#[cfg(test)]
#[path = "../../../../../../tests/depth/ribbons/polish.rs"]
mod polish;
