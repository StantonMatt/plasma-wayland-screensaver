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
const EVENT_SURFACE_SPEED_LIMIT: f64 = 0.50;
// A 0.25 encoded mouth shadow, expressed in linear light.
const MOUTH_SHADOW_FLOOR: f64 = 0.05087608817155679;
// Keep the coloured leading glass visible for a full pour. At the protected
// transfer rate, the old 0.35R fade hid it in under one second.
const MOUTH_SHADOW_FADE_DISTANCE: f64 = 0.65;
const SWAP_VISIBLE_COVERAGE: f64 = 0.75;
const SWAP_MAX_WAIT: f64 = 8.;
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
#[derive(Clone, Copy, PartialEq, Eq)]
enum RefillGuide {
    Unchecked,
    Braking,
    Arriving,
    Returning,
    Following,
    Original,
}

// A finite aiming move with matching position, velocity and acceleration.
// These paths steer the existing eye guide only for an otherwise sparse refill.
#[derive(Clone, Copy)]
struct GuidePath {
    start: f64,
    duration: f64,
    c: [chamber::V; 6],
}
impl GuidePath {
    fn new(
        start: f64,
        duration: f64,
        p: chamber::V,
        velocity: chamber::V,
        acceleration: chamber::V,
        target: chamber::V,
    ) -> Self {
        Self::with_end(
            start,
            duration,
            (p, velocity, acceleration),
            (target, chamber::V::default(), chamber::V::default()),
        )
    }
    fn with_end(
        start: f64,
        duration: f64,
        initial: (chamber::V, chamber::V, chamber::V),
        end: (chamber::V, chamber::V, chamber::V),
    ) -> Self {
        let (p, velocity, acceleration) = initial;
        let (target, end_velocity, end_acceleration) = end;
        let d = target - p;
        let v = velocity * duration;
        let a = acceleration * (duration * duration);
        let v1 = end_velocity * duration;
        let a1 = end_acceleration * (duration * duration);
        Self {
            start,
            duration,
            c: [
                p,
                v,
                a * 0.5,
                d * 10. - v * 6. - v1 * 4. - a * 1.5 + a1 * 0.5,
                d * (-15.) + v * 8. + v1 * 7. + a * 1.5 - a1,
                d * 6. - v * 3. - v1 * 3. - a * 0.5 + a1 * 0.5,
            ],
        }
    }
    fn sample(self, time: f64) -> (chamber::V, chamber::V, chamber::V) {
        let u = ((time - self.start) / self.duration).clamp(0., 1.);
        let [c0, c1, c2, c3, c4, c5] = self.c;
        let p = c0 + c1 * u + c2 * u.powi(2) + c3 * u.powi(3) + c4 * u.powi(4) + c5 * u.powi(5);
        let v = (c1
            + c2 * (2. * u)
            + c3 * (3. * u.powi(2))
            + c4 * (4. * u.powi(3))
            + c5 * (5. * u.powi(4)))
            * (1. / self.duration);
        let a = (c2 * 2. + c3 * (6. * u) + c4 * (12. * u.powi(2)) + c5 * (20. * u.powi(3)))
            * (1. / self.duration.powi(2));
        (p, v, a)
    }
}
struct Kaleidoscope {
    focus: chamber::V,
    focus_previous: chamber::V,
    focus_velocity: chamber::V,
    refill_guide: Option<GuidePath>,
    refill_guide_phase: RefillGuide,
    physics_previous: [(chamber::V, f64); chamber::MAX],
    swap_origins: [chamber::V; chamber::MAX],
    swap_shadow_started: bool,
    swap_requested_at: Option<f64>,
    swap_queue_delay: f64,
    physical_event_until: f64,
    event_contacts: bool,
    refill_target: Option<chamber::V>,
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
        let mut physics_previous = [(chamber::V::default(), 0.); chamber::MAX];
        for (pose, body) in physics_previous.iter_mut().zip(&chamber.bodies) {
            *pose = (body.pos, body.angle);
        }
        Self {
            input,
            focus: chamber::V::default(),
            focus_previous: chamber::V::default(),
            focus_velocity: chamber::V::default(),
            refill_guide: None,
            refill_guide_phase: RefillGuide::Unchecked,
            swap_origins: [chamber::V::default(); chamber::MAX],
            swap_shadow_started: false,
            swap_requested_at: None,
            swap_queue_delay: 0.,
            physical_event_until: 0.,
            event_contacts: false,
            refill_target: None,
            physics_previous,
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
    fn magnified_mandala(&self) -> bool {
        self.params.mirrors == "mandala" && self.params.points >= 12 && self.input.scale >= 1.5
    }
    fn event_surface_speed(&self) -> f64 {
        EVENT_SURFACE_SPEED_LIMIT
    }
    // One reference radius includes the first echo's radial compression.
    fn event_chamber_speed(&self) -> f64 {
        self.event_surface_speed() * 1.67 * 0.65 / self.input.scale
    }
    fn begin_physical_event(&mut self, duration: f64) {
        self.physical_event_until = self.physical_event_until.max(self.moments.time + duration);
        self.event_contacts = true;
    }
    // A requested pour is invisible during a breath's empty view. Wait for
    // the physical heap to cover the apex, without changing the eyepiece or
    // its light, and bound the wait for sparse or exceptional arrangements.
    fn try_begin_swap(&mut self) {
        let Some(requested) = self.swap_requested_at else {
            return;
        };
        if self.moments.time - requested < SWAP_MAX_WAIT
            && self.apex_fill(self.focus) < SWAP_VISIBLE_COVERAGE
        {
            return;
        }
        self.swap_queue_delay = self.moments.time - requested;
        self.swap_requested_at = None;
        self.refill_guide_phase = RefillGuide::Unchecked;
        self.swap_shadow_started = false;
        self.begin_physical_event(10.);
        self.moments.start[(SWAP - 1) as usize] = self.moments.time;
    }
    fn transit_key(&self, index: usize) -> Option<f64> {
        let age = self.moments.age(SWAP);
        if !self.magnified_mandala() || !(0.0..7.0).contains(&age) {
            return None;
        }
        if age < 0.44 {
            return Some(1.);
        }
        if age < 3.25 {
            let p = self.chamber.bodies[index].pos + self.chamber.transport;
            let delta = p - self.swap_origins[index];
            let travel = delta.0.max(0.) * self.input.scale / (1.67 * 0.65);
            // The leading edge leaves lit. The mouth then absorbs the same
            // linear fraction of every colour channel, rather than whitening it.
            Some(
                (1. - (1. - MOUTH_SHADOW_FLOOR) * smooth(travel / MOUTH_SHADOW_FADE_DISTANCE))
                    * (1. - smooth((travel - MOUTH_SHADOW_FADE_DISTANCE - 0.10) / 0.85)),
            )
        } else if age < 4. {
            Some(0.)
        } else {
            let returned = (0.40 - self.chamber.transport.0).max(0.);
            Some(smooth(returned * self.input.scale / (1.67 * 0.65 * 0.35)))
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
                    || (!self.magnified_mandala() && self.hand.mood == Mood::Hold)
                    || self.moments.age(SWAP) < 7.
                {
                    return;
                }
                if self.magnified_mandala() {
                    self.swap_requested_at.get_or_insert(self.moments.time);
                    self.try_begin_swap();
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
                if self.magnified_mandala() {
                    self.begin_physical_event(3.);
                }
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
    fn apex_fill(&self, offset: chamber::V) -> f64 {
        let fold = |v: f64, lo: f64, hi: f64| {
            lo + (hi - lo) * (1. - (((v - lo) / (hi - lo)).rem_euclid(2.) - 1.).abs())
        };
        let mut filled = 0.;
        for ring in 0..5 {
            let radius = 0.25 * ((ring as f64 + 0.5) / 5.).sqrt();
            for petal in 0..4 {
                let point = self.viewed_point(radius, (petal as f64 + 0.5) / 4.) + offset;
                let point = chamber::V(fold(point.0, -0.98, 0.98), fold(point.1, -0.11, 1.02));
                if self
                    .chamber
                    .bodies
                    .iter()
                    .any(|b| !b.cut.back && b.contains_optical(point))
                {
                    filled += 1.;
                } else if self
                    .chamber
                    .bodies
                    .iter()
                    .any(|b| b.cut.back && b.contains_optical(point))
                {
                    filled += 0.15;
                }
            }
        }
        filled / 20.
    }
    fn filled_refill_target(&self) -> chamber::V {
        let limit = 0.65 * (self.input.scale - 1.);
        let clamp = |v: chamber::V| v * (limit / v.0.hypot(v.1).max(limit));
        let prior = self.refill_target.unwrap_or(self.focus);
        let prior_score = self.apex_fill(prior);
        if prior_score >= 0.95 {
            return prior;
        }
        let mut best = prior;
        let mut score = prior_score;
        let centre = self.viewed_point(0.17, 0.5);
        for body in self.chamber.bodies.iter().filter(|b| !b.cut.back) {
            for shift in [-0.035, 0., 0.035] {
                let candidate = clamp(body.pos - centre + chamber::V(shift, 0.));
                let quality = self.apex_fill(candidate);
                let d = candidate - prior;
                let quality = quality - 0.005 * d.0.hypot(d.1);
                if quality > score + 0.001 {
                    best = candidate;
                    score = quality;
                }
            }
        }
        // Once the core is full, don't chase equally good neighbouring cuts.
        if prior_score >= 0.9 && score < prior_score + 0.10 {
            prior
        } else {
            best
        }
    }
    // A magnified eyepiece sees a small part of the cell. Follow the nearby
    // heap on an eased arc as the hand turns, keeping the outer petals fixed.
    fn ordinary_guide_offset(&self) -> chamber::V {
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
        offset
    }
    fn guide_magnified_eye(&mut self) {
        if self.input.scale <= 1. || self.params.mirrors != "mandala" {
            return;
        }
        let age = self.moments.age(SWAP);
        // Most recipes refill with the physical Hold and the ordinary guide.
        // After the hidden settling beat, reserve a smooth aim only when the
        // ordinary guide's destination would leave the core less than 90% full.
        if self.magnified_mandala()
            && (3.9..12.).contains(&age)
            && self.refill_guide_phase != RefillGuide::Original
        {
            let now = self.moments.time;
            if self.refill_guide_phase == RefillGuide::Unchecked
                && self.apex_fill(self.ordinary_guide_offset()) >= 0.9
            {
                self.refill_guide_phase = RefillGuide::Original;
            }
            if self.refill_guide_phase == RefillGuide::Unchecked {
                let velocity = self.focus_velocity * (120. / 8.);
                self.refill_guide = Some(GuidePath::new(
                    now,
                    0.4,
                    self.focus,
                    velocity,
                    chamber::V::default(),
                    self.focus + velocity * 0.2,
                ));
                self.refill_guide_phase = RefillGuide::Braking;
            }
            if self.refill_guide_phase == RefillGuide::Braking
                && now >= self.refill_guide.unwrap().start + 0.4
            {
                let (p, v, a) = self.refill_guide.unwrap().sample(now);
                let target = self.filled_refill_target();
                self.refill_target = Some(target);
                let duration = (6.4 - age).max(0.5);
                self.refill_guide = Some(GuidePath::new(now, duration, p, v, a, target));
                self.refill_guide_phase = RefillGuide::Arriving;
            }
            if self.refill_guide_phase == RefillGuide::Braking
                || (self.refill_guide_phase == RefillGuide::Arriving && age < 9.)
            {
                self.focus = self.refill_guide.unwrap().sample(now).0;
                return;
            }
        }
        let offset = self.ordinary_guide_offset();
        if self.magnified_mandala()
            && (9.0..12.).contains(&age)
            && self.refill_guide_phase == RefillGuide::Arriving
        {
            let now = self.moments.time;
            let (p, v, a) = self.refill_guide.unwrap().sample(now);
            self.refill_guide = Some(GuidePath::new(now, 12. - age, p, v, a, offset));
            self.refill_guide_phase = RefillGuide::Returning;
        }
        if self.refill_guide_phase == RefillGuide::Returning && age < 12. {
            self.focus = self.refill_guide.unwrap().sample(self.moments.time).0;
        } else {
            // Continue with C2 segments at the original guide's endpoints.
            // This returns to ordinary aiming without a velocity/acceleration
            // discontinuity when the finite refill path ends.
            if let Some(path) = self.refill_guide {
                let now = self.moments.time;
                let (p, v, a) = path.sample(now);
                let dt = 8. / 120.;
                let target = p + (offset - p) * 0.10;
                let end_velocity = (target - p) * (1. / dt);
                self.refill_guide = Some(GuidePath::with_end(
                    now,
                    dt,
                    (p, v, a),
                    (target, end_velocity, chamber::V::default()),
                ));
                self.focus = target;
                if self.refill_guide_phase == RefillGuide::Returning {
                    self.refill_guide_phase = RefillGuide::Following;
                }
            } else {
                self.focus = self.focus + (offset - self.focus) * 0.10;
            }
        }
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
        let hand_limit = if self.magnified_mandala() {
            self.event_surface_speed() * 0.9
        } else {
            f64::INFINITY
        };
        self.hand
            .step_limited(1. / 120., self.input.speed, &mut self.rng, hand_limit);
        if self.magnified_mandala() && self.hand.mood == Mood::Twist {
            self.begin_physical_event(3.);
        }
        for i in 0..4 {
            if self.moments.time >= self.moments.due[i] {
                let id = i as u32 + 1;
                if id == SWAP
                    && (!self.magnified_mandala() || !self.params.events)
                    && self.hand.mood == Mood::Hold
                {
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
        self.try_begin_swap();
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
            // Seat the new glass while the hand is still. The handoff happens
            // in the dark beat, and a real Hold lets the refill stay settled.
            if self.magnified_mandala() {
                self.hand.hold_for(6., self.input.speed, &mut self.rng);
            }
            self.refill_target = None;
            for (pose, body) in self.physics_previous.iter_mut().zip(&self.chamber.bodies) {
                *pose = (body.pos, body.angle);
            }
        }
        let protected = self.magnified_mandala();
        if protected && (0.44..3.25).contains(&swap) && !self.swap_shadow_started {
            // The hand can still turn during the leading beat. Start mouth
            // absorption at the actual pour, not at that earlier hand motion.
            for (origin, body) in self.swap_origins.iter_mut().zip(&self.chamber.bodies) {
                *origin = body.pos;
            }
            self.swap_shadow_started = true;
        }
        let tip = if protected && swap >= 3.25 {
            0.
        } else {
            smooth(swap / 1.5) * smooth((6. - swap) / 2.)
        };
        let physical_pace = self.input.speed.min(PHYSICAL_SPEED_LIMIT);
        let chamber_speed = self.event_chamber_speed();
        let transfer_speed = chamber_speed * 0.7 / 1.01_f64.sqrt();
        let moving_transfer = if protected && (0.44..3.25).contains(&swap) {
            chamber::transfer_speed(swap - 0.44, transfer_speed, 10.)
        } else if protected && (4.0..7.0).contains(&swap) {
            chamber::transfer_speed(swap - 4., transfer_speed, 0.40)
        } else {
            0.
        };
        let limit = if protected && self.moments.time < self.physical_event_until {
            Some((chamber_speed - moving_transfer * 1.01_f64.sqrt()).max(0.) / physical_pace)
        } else if protected && self.event_contacts {
            Some(f64::INFINITY)
        } else {
            None
        };
        // One rigid transfer belongs to the physical chamber. Contacts remain
        // in its co-moving coordinates; pieces never independently chase poses.
        self.chamber.transport = if protected {
            let distance = if (0.44..3.25).contains(&swap) {
                chamber::transfer_distance(swap - 0.44, transfer_speed, 10.)
            } else if (3.25..4.).contains(&swap) {
                0.40
            } else if (4.0..7.0).contains(&swap) {
                0.40 - chamber::transfer_distance(swap - 4., transfer_speed, 0.40)
            } else {
                0.
            };
            chamber::V(distance, -0.10 * distance)
        } else {
            chamber::V::default()
        };
        // Prepare a contact-resolved incoming heap while it is unlit. Spread
        // the work over the dark beat rather than one catch-up frame.
        if protected && (3.25..4.).contains(&swap) {
            // Keep the refined solver here too: one-pass preparation can
            // leave the selected refill target moving after the dark beat.
            for _ in 0..8 {
                self.chamber
                    .step_limited(self.hand.angle, Some(f64::INFINITY));
            }
        }
        self.fraction += physical_pace;
        for _ in 0..self.fraction.floor() as usize {
            for (pose, body) in self.physics_previous.iter_mut().zip(&self.chamber.bodies) {
                *pose = (body.pos, body.angle);
            }
            self.chamber
                .step_limited(self.hand.angle + tip * 1.8, limit);
        }
        self.fraction = self.fraction.fract();
        if tick.is_multiple_of(8) {
            self.focus_previous = self.focus;
            self.guide_magnified_eye();
            self.focus_velocity = self.focus - self.focus_previous;
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
        if self.chamber.transport != chamber::V::default() {
            h = hash64(h ^ self.chamber.transport.0.to_bits());
            h = hash64(h ^ self.chamber.transport.1.to_bits());
        }
        h
    }
}
