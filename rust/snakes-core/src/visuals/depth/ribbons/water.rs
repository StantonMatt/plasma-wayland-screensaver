use super::agent::{Ribbon, range, smooth};
use super::common::{
    flow::{Eddy, p},
    layout::{Layout, Vec2},
    rng::Pcg32,
};
use super::world::Event;

pub(super) const EVENT_ONSET_WALL_SECONDS: f64 = 1.25;
pub(super) const GUST_FORCE_PIXELS_PER_SECOND: f64 = 150.;
pub(super) const MAELSTROM_MOTOR_PIXELS_PER_SECOND: f64 = 350.;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Kind {
    Gust,
    Maelstrom,
    Still,
}
#[derive(Clone, Copy)]
pub(super) struct Moment {
    pub kind: Kind,
    pub centre: Vec2,
    pub age: f64,
    duration: f64,
    radius: f64,
    strength: f64,
    event_pace: f64,
}
impl Moment {
    fn onset_span(self, nominal: f64) -> f64 {
        nominal.max(EVENT_ONSET_WALL_SECONDS * self.event_pace)
    }
    pub fn weight(self) -> f64 {
        smooth(self.age / self.onset_span(5.))
            * smooth((self.duration - self.age) / 6.)
            * self.strength
            * (1. - self.release_weight())
    }
    pub fn gust(self, pos: Vec2, left: f64) -> f64 {
        if self.kind != Kind::Gust {
            return 0.;
        }
        let lag = self.age - (pos.x - left) / 1200.;
        let rise = self.onset_span(0.75);
        // Preserve the full-strength hold and fall while slowing the front's
        // arrival in wall time, including on accelerated event clocks.
        smooth(lag / rise) * smooth((rise + 5.25 - lag) / 3.) * self.strength
    }
    pub fn gust_steering(self) -> f64 {
        if self.kind != Kind::Gust {
            return 0.;
        }
        let span = self.onset_span(0.75);
        smooth(self.age / span) * smooth((self.duration - self.age) / span) * self.strength
    }
    pub fn orbit_radius(self, layer: usize) -> f64 {
        self.radius * [0.55, 0.80, 1.05][layer]
    }
    // Cache the rotating ellipse once per strand, rather than evaluating
    // trigonometry at every transported body node.
    pub fn orbit_contour(self, layer: usize) -> OrbitContour {
        let phase = layer as f64 * std::f64::consts::TAU / 3.;
        let breathe = 1.
            + 0.06 * (self.age * std::f64::consts::TAU * [0.08, 0.10, 0.12][layer] + phase).sin();
        OrbitContour {
            radius: self.orbit_radius(layer) * breathe,
            axis: super::agent::polar(self.age * 0.09 + phase * 0.25),
        }
    }
    pub fn unwinding(self) -> f64 {
        let span = self.onset_span(2.);
        // Establish tail tension before the head's launch ramp. A bounded
        // body needs preparation time instead of an instantaneous projection.
        smooth((4.5 + span - (self.duration - self.age)) / span)
    }
    pub fn release_weight(self) -> f64 {
        if self.kind != Kind::Maelstrom {
            return 0.;
        }
        let span = (EVENT_ONSET_WALL_SECONDS * self.event_pace).max(1e-6);
        // Finish handing orbital force and facing back to free silk before
        // the captured flag changes, instead of dropping them in one tick.
        smooth((span + 2.5 - (self.duration - self.age)) / span)
    }
    pub fn collapsing(self) -> bool {
        self.kind == Kind::Maelstrom && self.duration - self.age <= 2.5
    }
    pub fn swirl(self, pos: Vec2, layer: usize) -> (Vec2, f64) {
        let radial = pos - self.centre;
        let outward = radial.unit();
        let eddy = Eddy {
            centre: self.centre,
            core_radius: self.radius * 0.7,
            circulation: self.radius * 235.,
        }
        .sample(pos);
        let pull = (self.orbit_contour(layer).radius(outward) - radial.length()).clamp(-260., 220.);
        let drive = outward * (pull * 1.6) + outward.perpendicular() * 205. + eddy.velocity;
        (drive * self.weight(), eddy.vorticity * self.weight())
    }
}
#[derive(Clone, Copy)]
pub(super) struct OrbitContour {
    radius: f64,
    axis: Vec2,
}
impl OrbitContour {
    pub fn radius(self, outward: Vec2) -> f64 {
        let cos = outward.dot(self.axis);
        let sin = outward.dot(self.axis.perpendicular());
        self.radius * (1.07 * 0.93) / ((0.93 * cos).powi(2) + (1.07 * sin).powi(2)).sqrt()
    }
}
pub(super) struct Weather {
    pub active: Option<Moment>,
    next: [f64; 3],
    quiet: f64,
    pub script: Vec<Event>,
}
impl Weather {
    pub fn new(rng: &mut Pcg32) -> Self {
        Self {
            active: None,
            next: [
                range(rng, 180., 360.),
                range(rng, 360., 600.),
                range(rng, 480., 840.),
            ],
            quiet: 0.,
            script: vec![],
        }
    }
    pub fn inject(&mut self, e: Event) -> Result<(), &'static str> {
        if !(1..=3).contains(&e.id)
            || e.at_ns < 0
            || !e.anchor.iter().all(|x| x.is_finite())
            || !e.strength.is_finite()
        {
            return Err("invalid ribbon event");
        }
        self.script.push(e);
        self.script.sort_by_key(|e| e.at_ns);
        Ok(())
    }
    pub fn step(
        &mut self,
        layout: &Layout,
        rng: &mut Pcg32,
        dt: f64,
        event_pace: f64,
        ns: i64,
        enabled: bool,
    ) -> Option<Moment> {
        let mut ended = None;
        if let Some(m) = self.active.as_mut() {
            m.age += dt;
            if m.age >= m.duration {
                ended = self.active.take();
                self.quiet = 90.;
            }
        }
        self.quiet -= dt;
        for t in &mut self.next {
            *t -= dt;
        }
        let request = if self.script.first().is_some_and(|e| e.at_ns <= ns) {
            let e = self.script.remove(0);
            Some((
                (e.id - 1) as usize,
                p(e.anchor[0], e.anchor[1]),
                e.strength.clamp(0., 1.),
            ))
        } else if enabled && self.active.is_none() && self.quiet <= 0. {
            self.next.iter().position(|t| *t <= 0.).map(|i| {
                (
                    i,
                    layout
                        .choose_visible_anchor(rng, 10.)
                        .unwrap_or(layout.bounds.centre()),
                    1.,
                )
            })
        } else {
            None
        };
        if let Some((i, centre, strength)) = request {
            let (kind, duration, next) = match i {
                0 => (
                    Kind::Gust,
                    layout.bounds.width / 1200.
                        + 0.75_f64.max(EVENT_ONSET_WALL_SECONDS * event_pace)
                        + 5.25,
                    range(rng, 180., 360.),
                ),
                1 => (
                    Kind::Maelstrom,
                    range(rng, 27.5, 29.),
                    range(rng, 360., 600.),
                ),
                _ => (Kind::Still, range(rng, 30., 45.), range(rng, 480., 840.)),
            };
            self.next[i] = next;
            self.active = Some(Moment {
                kind,
                centre,
                age: 0.,
                duration,
                radius: layout.bounds.height * 0.40,
                strength,
                event_pace,
            });
        }
        ended
    }
}
// Broad lateral displacement from occupied lanes. A strand arriving upstream
// displaces neighbouring water gradually, including the tails in that water.
pub(super) struct Lanes {
    shifts: [[f64; 13]; 2],
    pub(super) bounds: super::common::layout::Rect,
}
impl Lanes {
    pub fn new(bounds: super::common::layout::Rect) -> Self {
        Self {
            shifts: [[0.; 13]; 2],
            bounds,
        }
    }
    pub fn rebuild(&mut self, ribbons: &[Ribbon], layout: &Layout) {
        let mut bins = [[0.; 12]; 2];
        let b = self.bounds;
        for r in ribbons.iter().filter(|r| r.alive) {
            let end = (r.extent() * (r.count - 1) as f64) as usize;
            for n in r.nodes[..=end]
                .iter()
                .step_by(4)
                .filter(|n| layout.contains(n.pos))
            {
                let weight = r.width * r.spacing * (0.25 + 0.75 * n.phase.cos().abs());
                let x = (((n.pos.x - b.x) * 12. / b.width) as usize).min(11);
                let y = (((n.pos.y - b.y) * 12. / b.height) as usize).min(11);
                bins[0][x] += weight;
                bins[1][y] += weight;
            }
        }
        for (axis, (row, shifts)) in bins.iter().zip(&mut self.shifts).enumerate() {
            let total = row.iter().sum::<f64>().max(1.);
            let size = if axis == 0 { b.width } else { b.height };
            let mut cumulative = 0.;
            for i in 1..12 {
                cumulative += row[i - 1] / total;
                let displacement = (cumulative - i as f64 / 12.) * size;
                shifts[i] = (displacement * 0.92).clamp(-145., 145.);
            }
        }
    }
    pub fn at(&self, pos: Vec2) -> Vec2 {
        let sample = |axis: usize, v: f64| {
            let u = v.clamp(0., 1.) * 12.;
            let i = (u as usize).min(11);
            let s = &self.shifts[axis];
            s[i] + (s[i + 1] - s[i]) * (u - i as f64)
        };
        p(
            sample(0, (pos.x - self.bounds.x) / self.bounds.width) * 0.68,
            sample(1, (pos.y - self.bounds.y) / self.bounds.height),
        )
    }
}
#[derive(Clone, Copy)]
struct Entry {
    pos: Vec2,
    ribbon: usize,
    node: usize,
    next: i16,
}
pub(super) struct Neighbours {
    heads: [i16; 1024],
    data: Vec<Entry>,
}
impl Neighbours {
    pub fn new() -> Self {
        Self {
            heads: [-1; 1024],
            data: Vec::with_capacity(40 * 32),
        }
    }
    fn key(x: i32, y: i32) -> usize {
        (x.wrapping_mul(73856093) ^ y.wrapping_mul(19349663)) as usize & 1023
    }
    pub fn rebuild(&mut self, ribbons: &[Ribbon]) {
        self.heads.fill(-1);
        self.data.clear();
        for (ribbon, r) in ribbons.iter().enumerate().filter(|(_, r)| r.alive) {
            let end = (r.extent() * (r.count - 1) as f64) as usize;
            for node in (0..=end).step_by(4) {
                let pos = r.nodes[node].pos;
                let k = Self::key((pos.x / 128.).floor() as i32, (pos.y / 128.).floor() as i32);
                let next = self.heads[k];
                self.heads[k] = self.data.len() as i16;
                self.data.push(Entry {
                    pos,
                    ribbon,
                    node,
                    next,
                });
            }
        }
    }
    pub fn steer(&self, ribbon: usize, p: Vec2) -> (Vec2, bool) {
        let mut push = Vec2::default();
        let mut crossing = false;
        let x = (p.x / 128.).floor() as i32;
        let y = (p.y / 128.).floor() as i32;
        for cy in y - 1..=y + 1 {
            for cx in x - 1..=x + 1 {
                let mut j = self.heads[Self::key(cx, cy)];
                while j >= 0 {
                    let e = self.data[j as usize];
                    j = e.next;
                    let d = p - e.pos;
                    let d2 = d.dot(d);
                    if (e.ribbon != ribbon || e.node > 18) && d2 < 28. * 28. {
                        crossing = true;
                    }
                    if e.ribbon != ribbon && d2 > 0.1 && d2 < 150. * 150. {
                        let len = d2.sqrt();
                        push += d * ((1. - len / 150.) * 51. / len);
                    }
                }
            }
        }
        (push.limited(66.), crossing)
    }
}
