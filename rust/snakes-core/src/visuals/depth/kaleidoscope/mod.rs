//! A dark-field object chamber shared by every mirror in the eyepiece.
use super::{Descriptor, MirroredPolicy, schema::Parameter};
#[allow(non_snake_case)]
#[derive(Clone, Debug)]
pub struct Params {
    pub mirrors: &'static str,
    pub points: i32,
    pub glass: &'static str,
    pub turning: i32,
    pub events: bool,
}
impl Default for Params {
    fn default() -> Self {
        Self {
            mirrors: "mandala",
            points: 8,
            glass: "mixed",
            turning: 40,
            events: true,
        }
    }
}
pub const PARAMETERS: &[Parameter] = &[
    Parameter::enumeration(1, c"mirrors", c"mandala|lattice", 1., 0.),
    Parameter::integer(2, c"points", 3., 16., 8.),
    Parameter::enumeration(3, c"glass", c"mixed|beads|shards|rods-wire", 3., 0.),
    Parameter::integer(4, c"turning", 0., 100., 40.),
    Parameter::boolean(5, c"events", true),
];
pub const DESCRIPTOR: Descriptor = Descriptor {
    id: 5,
    key: "kaleidoscope",
    available: true,
    rate: 120,
    mirrored: MirroredPolicy::SharedCanonical,
    fit: super::MirroredFit::Cover,
    parameters: PARAMETERS,
    create: |input| Some(Box::new(Kaleidoscope::new(input))),
};

mod chamber;
mod hand;
pub mod render;
#[cfg(test)]
mod tests;
use super::{
    common::{
        clock::Rate,
        rng::{Pcg32, hash64},
    },
    world::{Event, Generations, LivingWorld, Snapshot, WorldInput},
};
use chamber::Chamber;
use hand::{Hand, Mood, smooth};
use render::{Cell, Frame, Optics};
use std::{cell::RefCell, f64::consts::TAU, sync::Arc};

fn range(r: &mut Pcg32, a: f64, b: f64) -> f64 {
    a + (b - a) * r.unit()
}
const PHYSICAL_SPEED_LIMIT: f64 = 1.5;
const DUST: u32 = 1;
const CLOUD: u32 = 2;
const SWAP: u32 = 3;
const GLINT: u32 = 4;
const TWIST: u32 = 5;
const HOLD: u32 = 6;

struct Moments {
    time: f64,
    pace: f64,
    due: [f64; 4],
    start: [f64; 4],
    mote_duration: f64,
    glint: usize,
    glint_point: chamber::V,
    glint_sector: chamber::V,
}
impl Moments {
    fn interval(r: &mut Pcg32, i: usize) -> f64 {
        let (a, b) = [(240., 480.), (180., 360.), (540., 900.), (20., 60.)][i];
        range(r, a, b)
    }
    fn new(r: &mut Pcg32, speed: f64) -> Self {
        let pace = speed.min(1.5);
        Self {
            time: 0.,
            pace,
            due: std::array::from_fn(|i| Self::interval(r, i) / pace),
            start: [-1e6; 4],
            mote_duration: 30.,
            glint: 0,
            glint_point: chamber::V::default(),
            glint_sector: chamber::V(0.67, 0.5),
        }
    }
    fn age(&self, id: u32) -> f64 {
        self.time - self.start[id as usize - 1]
    }
    fn dust(&self) -> f64 {
        let u = self.age(DUST) / self.mote_duration;
        smooth(u * 14.) * smooth((1. - u) * 14.)
    }
    fn cloud(&self) -> f64 {
        let u = self.age(CLOUD) / 12.;
        smooth(u * 2.) * smooth((1. - u) * 2.)
    }
    fn glint(&self) -> f64 {
        let t = self.age(GLINT);
        smooth(t / 0.4) * smooth((1.9 - t) / 1.5)
    }
    fn aperture(&self) -> f64 {
        let t = self.age(SWAP);
        1. - smooth(t / 2.5) + smooth((t - 4.) / 3.)
    }
}
struct Kaleidoscope {
    focus: chamber::V,
    input: WorldInput,
    params: Params,
    rng: Pcg32,
    hand: Hand,
    chamber: Chamber,
    cell: Arc<Cell>,
    optics: Arc<Optics>,
    moments: Moments,
    recipe: u32,
    tick: u64,
    fraction: f64,
    script: Vec<Event>,
    frames: RefCell<Vec<Arc<Frame>>>,
    empty: Snapshot,
}
impl Kaleidoscope {
    fn new(input: WorldInput) -> Self {
        let mut params = Params::default();
        for p in &input.parameters {
            match p.id {
                1 => params.mirrors = if p.payload == 1 { "lattice" } else { "mandala" },
                2 => params.points = p.payload as i32,
                3 => params.glass = ["mixed", "beads", "shards", "rods-wire"][p.payload as usize],
                4 => params.turning = p.payload as i32,
                5 => params.events = p.payload != 0,
                _ => {}
            }
        }
        let optics = Arc::new(Optics::new(&input, &params));
        let mut rng = Pcg32::new(input.seed, 0x61a55);
        let hand = Hand::new(&mut rng, params.turning as f64 / 100.);
        let moments = Moments::new(&mut rng, input.speed);
        let mut chamber = Chamber::new(&mut rng, optics.count, optics.glass, 0, input.palette);
        for _ in 0..180 {
            chamber.step(hand.angle);
        }
        let cell = Arc::new(Cell::new(&chamber, 1));
        Self {
            input,
            focus: chamber::V::default(),
            params,
            rng,
            hand,
            chamber,
            cell,
            optics,
            moments,
            recipe: 0,
            tick: 0,
            fraction: 0.,
            script: vec![],
            frames: RefCell::new(vec![]),
            empty: Snapshot::default(),
        }
    }
    fn enact(&mut self, id: u32) {
        if !self.params.events && id <= SWAP {
            return;
        }
        match id {
            DUST => self.moments.mote_duration = range(&mut self.rng, 20., 40.),
            CLOUD => {}
            SWAP => {
                if self.moments.time < 60.
                    || self.hand.mood == Mood::Hold
                    || self.moments.age(SWAP) < 7.
                {
                    return;
                }
            }
            GLINT => {
                // Find an actual, lit edge inside the shared field of view.
                // Keeping away from a mirror seam leaves two distinct stars per petal.
                let (sin, cos) = self.light_angle().sin_cos();
                let light = chamber::V(cos, sin);
                let mut best = f64::NEG_INFINITY;
                for radius in [0.53, 0.61, 0.69, 0.77, 0.85] {
                    for side in [0.30, 0.44, 0.58, 0.70] {
                        let q = self.viewed_point(radius, side);
                        for (i, b) in self.chamber.bodies.iter().enumerate() {
                            if b.cut.sides < 3 {
                                continue;
                            }
                            let score = b.reflection_score(q, light);
                            if score > best {
                                best = score;
                                self.moments.glint = i;
                                self.moments.glint_point = q;
                                self.moments.glint_sector = chamber::V(radius, side);
                            }
                        }
                    }
                }
                if !best.is_finite() {
                    self.moments.glint_sector = chamber::V(0.67, 0.5);
                    self.moments.glint_point = self.viewed_point(0.67, 0.5);
                }
            }
            TWIST => {
                self.hand.begin(Mood::Twist, &mut self.rng);
                return;
            }
            HOLD => {
                self.hand.begin(Mood::Hold, &mut self.rng);
                return;
            }
            _ => return,
        }
        self.moments.start[id as usize - 1] = self.moments.time;
    }
    fn viewed_point(&self, radius: f64, side: f64) -> chamber::V {
        let p = if self.params.mirrors == "lattice" {
            chamber::V(-1.07 + 2.14 * radius, -0.14 + 1.15 * side * 0.50)
        } else {
            let theta = side * std::f64::consts::PI / f64::from(self.params.points) + 0.02;
            chamber::V(
                -0.82 + 1.67 * radius * theta.cos(),
                0.46 + 1.67 * radius * (theta.sin() - 0.10),
            )
        };
        let fold = |v: f64, pivot: f64, lo: f64, hi: f64| {
            let v = pivot + (v - pivot) / self.input.scale;
            lo + (hi - lo) * (1. - (((v - lo) / (hi - lo)).rem_euclid(2.) - 1.).abs())
        };
        chamber::V(fold(p.0, -0.18, -0.98, 0.98), fold(p.1, 0.5, -0.11, 1.02))
    }
    fn light_angle(&self) -> f64 {
        -2.2 + self.moments.time * TAU / 1200. - self.hand.angle
    }
    // A magnified eyepiece sees a small part of the cell. Follow the nearby
    // heap on an eased arc as the hand turns, keeping the outer petals fixed.
    fn guide_magnified_eye(&mut self) {
        if self.input.scale <= 1. || self.params.mirrors != "mandala" {
            return;
        }
        let centre = self.viewed_point(0.16, 0.5);
        let gravity = chamber::V(self.hand.angle.sin(), self.hand.angle.cos());
        let target = centre + gravity * 0.075;
        let mut best = f64::INFINITY;
        let mut offset = chamber::V::default();
        for body in &self.chamber.bodies {
            let d = body.pos - target;
            let score = d.0 * d.0 + d.1 * d.1 + if body.cut.back { 0.025 } else { 0. }
                - 0.14 * body.cut.reach
                - if body.cut.sides == 1 { 0.020 } else { 0. };
            if score < best {
                best = score;
                offset = body.pos - centre;
            }
        }
        let limit = 0.20 * (self.input.scale - 1.);
        offset = offset * (limit / offset.0.hypot(offset.1).max(limit));
        self.focus = self.focus + (offset - self.focus) * 0.10;
    }
}
impl LivingWorld for Kaleidoscope {
    fn rate(&self) -> Rate {
        Rate::new(120, 1).unwrap()
    }
    fn step(&mut self, tick: u64) {
        self.tick = tick;
        if self.input.reduced_motion {
            return;
        }
        self.moments.time = tick as f64 / 120.;
        self.hand.step(1. / 120., self.input.speed, &mut self.rng);
        for i in 0..4 {
            if self.moments.time >= self.moments.due[i] {
                let id = i as u32 + 1;
                if id == SWAP && self.hand.mood == Mood::Hold {
                    continue;
                }
                self.moments.due[i] =
                    self.moments.time + Moments::interval(&mut self.rng, i) / self.moments.pace;
                self.enact(id);
            }
        }
        let ns = self.rate().tick_ns(tick);
        while self.script.first().is_some_and(|e| e.at_ns <= ns) {
            let id = self.script.remove(0).id;
            self.enact(id);
        }
        let swap = self.moments.age(SWAP);
        if (3.25..7.).contains(&swap) && self.cell.revision != self.moments.start[2].to_bits() {
            self.recipe = (self.recipe + 1 + self.rng.bounded(4).unwrap()) % 5;
            self.chamber = Chamber::new(
                &mut self.rng,
                self.optics.count,
                self.optics.glass,
                self.recipe,
                self.input.palette,
            );
            self.cell = Arc::new(Cell::new(&self.chamber, self.moments.start[2].to_bits()));
        }
        let tip = smooth(swap / 1.5) * smooth((6. - swap) / 2.);
        self.fraction += self.input.speed.min(PHYSICAL_SPEED_LIMIT);
        for _ in 0..self.fraction.floor() as usize {
            self.chamber.step(self.hand.angle + tip * 1.8);
        }
        self.fraction = self.fraction.fract();
        if tick.is_multiple_of(8) {
            self.guide_magnified_eye();
        }
    }
    fn snapshot(&self) -> Snapshot {
        let mut pool = self.frames.borrow_mut();
        let i = pool
            .iter()
            .position(|f| Arc::strong_count(f) == 1)
            .unwrap_or_else(|| {
                pool.push(Arc::new(Frame::new(self.optics.clone(), self.cell.clone())));
                pool.len() - 1
            });
        let f = Arc::get_mut(&mut pool[i]).unwrap();
        f.update(self);
        Snapshot {
            tick: self.tick,
            time_ns: self.rate().tick_ns(self.tick),
            generations: Generations {
                layout: self.input.layout.generation,
                topology: self.cell.revision,
                data: self.cell.revision,
                sample: self.tick,
            },
            typed: Some(pool[i].clone()),
            emission_estimate: self.emission_estimate(),
            ..self.empty.clone()
        }
    }
    fn emission_estimate(&self) -> f64 {
        0.04 * self.moments.aperture() * (1. - 0.35 * self.moments.cloud())
    }
    fn inject(&mut self, e: Event) -> Result<(), &'static str> {
        if !(DUST..=HOLD).contains(&e.id)
            || e.at_ns < 0
            || !e.strength.is_finite()
            || !e.anchor.iter().all(|x| x.is_finite())
        {
            return Err("invalid chamber event");
        }
        self.script.push(e);
        self.script.sort_by_key(|e| e.at_ns);
        Ok(())
    }
    fn state_hash(&self) -> u64 {
        let mut h = hash64(self.input.seed ^ self.cell.revision ^ self.hand.angle.to_bits());
        for b in &self.chamber.bodies {
            for v in [b.pos.0, b.pos.1, b.angle] {
                h = hash64(h ^ v.to_bits())
            }
        }
        h
    }
}
