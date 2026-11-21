//! Seeded juggling balls, shared by every output sampling this world.
mod arena;
mod body;
mod contacts;
mod events;
mod paint;
pub mod render;
mod spawn;
#[cfg(test)]
mod tests;
use super::{Descriptor, MirroredPolicy, schema::Parameter};
use super::{common, world};
use arena::*;
use body::*;
use common::{
    clock::Rate,
    rng::{Pcg32, hash64},
};
use contacts::Solver;
use events::Moment;
use std::{cell::RefCell, sync::Arc};
use world::{Event, Generations, LivingWorld, Snapshot, WorldInput};
#[allow(non_snake_case)]
#[derive(Clone, Debug)]
pub struct Params {
    pub count: i32,
    pub gravity: i32,
    pub elasticity: i32,
    pub material: &'static str,
    pub collisions: bool,
    pub worldEvents: bool,
}
impl Default for Params {
    fn default() -> Self {
        Self {
            count: 12,
            gravity: 35,
            elasticity: 90,
            material: "mixed",
            collisions: true,
            worldEvents: true,
        }
    }
}
pub const PARAMETERS: &[Parameter] = &[
    Parameter::integer(1, c"count", 1., 60., 12.),
    Parameter::integer(2, c"gravity", -100., 100., 35.),
    Parameter::integer(3, c"elasticity", 50., 100., 90.),
    Parameter::enumeration(4, c"material", c"mixed|rubber|glass|metal", 3., 0.),
    Parameter::boolean(5, c"collisions", true),
    Parameter::boolean(6, c"worldEvents", true),
];
pub const DESCRIPTOR: Descriptor = Descriptor {
    id: 7,
    key: "bounce",
    available: true,
    rate: 120,
    mirrored: MirroredPolicy::SharedCanonical,
    fit: super::MirroredFit::Contain,
    parameters: PARAMETERS,
    create: |input| Some(Box::new(BounceWorld::new(input))),
};
impl Params {
    fn read(input: &WorldInput) -> Self {
        let mut p = Self::default();
        for value in &input.parameters {
            let x = value.payload as i64 as i32;
            match value.id {
                1 => p.count = x,
                2 => p.gravity = x,
                3 => p.elasticity = x,
                4 => p.material = ["mixed", "rubber", "glass", "metal"][x as usize],
                5 => p.collisions = x != 0,
                6 => p.worldEvents = x != 0,
                _ => {}
            }
        }
        p
    }
}
pub struct BounceWorld {
    input: WorldInput,
    params: Params,
    rooms: Arc<[Room]>,
    impact_fields: Vec<ImpactField>,
    medians: Vec<f64>,
    balls: Vec<Ball>,
    solver: Solver,
    rng: Pcg32,
    tick: u64,
    sim_time: f64,
    height: f64,
    base_g: f64,
    substeps: u32,
    dt: f64,
    wall_dt: f64,
    decay: Relaxation,
    kinetic_limit: f64,
    due: [f64; 3],
    quiet_until: f64,
    moment: Option<Moment>,
    script: Vec<Event>,
    frames: RefCell<Vec<Arc<render::Frame>>>,
    empty: Snapshot,
    art: Arc<render::Art>,
}
impl BounceWorld {
    pub fn new(input: WorldInput) -> Self {
        let params = Params::read(&input);
        let mut rng = Pcg32::new(input.seed, 0x626f756e6365);
        let rooms: Arc<[Room]> = Room::from_layout(&input.layout).into();
        let impact_fields = rooms.iter().map(ImpactField::new).collect();
        let counts = spawn::population(&rooms, params.count as usize);
        let height = input.layout.bounds.height;
        let mut balls = Vec::with_capacity(61);
        let mut medians = Vec::new();
        for (i, room) in rooms.iter().enumerate() {
            let short = room
                .outputs
                .iter()
                .map(|o| o.width.min(o.height))
                .fold(0., f64::max);
            let nominal = (0.075 * short * input.scale)
                .clamp(24., 220.)
                .min(short * 0.28);
            let radii: Vec<f64> = (0..counts[i])
                .map(|_| spawn::radius(nominal, &mut rng).min(short * 0.28))
                .collect();
            // Preserve relative sizes when the requested balls physically cannot
            // fit. Reserve open floor for rolling even at sixty balls, Scale 200.
            let area = radii
                .iter()
                .map(|r| std::f64::consts::PI * r * r)
                .sum::<f64>();
            let fit = (0.21 * room.area / area.max(1e-12)).sqrt().min(1.);
            medians.push(nominal * fit);
            for radius in radii {
                let mat = spawn::material(params.material, &mut rng);
                let pigment = rng.bounded(6).unwrap() as usize;
                let mut b = Ball::new(&mut rng, i, radius * fit, mat, pigment);
                b.p = spawn::initial(&b, room, &balls, &mut rng);
                b.v = v(
                    between(&mut rng, -0.19, 0.19),
                    between(&mut rng, -0.06, 0.10),
                ) * short;
                if input.reduced_motion {
                    b.p = spawn::still_position(&b, room, &balls, params.gravity, &mut rng);
                    b.v = V::default();
                    b.spin = 0.;
                    b.sleeping = true;
                } else {
                    b.birth = -between(&mut rng, 0.04, 0.95);
                    b.alpha = (-b.birth / 0.9).min(1.);
                }
                balls.push(b);
            }
        }
        if params.material == "mixed" {
            spawn::balance_materials(&mut balls, input.seed);
        }
        let base_g = if params.gravity.abs() < 2 {
            0.
        } else {
            height * 0.018 * params.gravity as f64
        };
        let substeps = input.speed.ceil() as u32;
        let dt = input.speed / (120. * substeps as f64);
        let wall_dt = 1. / (120. * substeps as f64);
        let decay = Relaxation::new(wall_dt);
        let mass: f64 = balls.iter().map(|b| 1. / b.inv_mass).sum();
        let kinetic_limit = mass
            * height
            * base_g.abs().max(0.09 * height)
            * (0.27 + params.elasticity as f64 / 300.);
        let due = [
            between(&mut rng, 150., 480.),
            between(&mut rng, 270., 720.),
            between(&mut rng, 475., 1200.),
        ];
        let art = Arc::new(render::Art::new(
            &input,
            rooms.clone(),
            params.count as usize,
        ));
        Self {
            input,
            params,
            rooms,
            impact_fields,
            medians,
            balls,
            solver: Solver::new(),
            rng,
            tick: 0,
            sim_time: 0.,
            height,
            base_g,
            substeps,
            dt,
            wall_dt,
            decay,
            kinetic_limit,
            due,
            quiet_until: 90.,
            moment: None,
            script: Vec::new(),
            frames: RefCell::new(vec![]),
            empty: Snapshot::default(),
            art,
        }
    }
    fn rate_ns(&self) -> i64 {
        self.rate().tick_ns(self.tick)
    }
    // In an accelerated, large-scale pile, several fully lit silhouettes can
    // repeatedly sweep across the same field even with every impact dark.
    // Diameter times wall speed estimates swept area per second. Share it
    // between neighbours, fading their contribution as they leave the field.
    fn update_flow_limits(&mut self) {
        let max = 2.2 * self.height;
        let mix =
            (self.input.speed - 1.).clamp(0., 2.) * 0.5 * (self.input.scale - 1.).clamp(0., 1.);
        if mix == 0. || self.balls.len() < 2 {
            return;
        }
        let mut limits = [max; 61];
        for (i, ball) in self.balls.iter().enumerate() {
            let field = self.impact_fields[ball.room].extent;
            let mut breadth = 0.;
            for (j, other) in self.balls.iter().enumerate() {
                if j == i || other.room != ball.room {
                    continue;
                }
                let delta = other.p - ball.p;
                let x = (1. - (delta.x.abs() - 0.6 * other.r).max(0.) / field.x).max(0.);
                let y = (1. - (delta.y.abs() - 0.6 * other.r).max(0.) / field.y).max(0.);
                breadth += 2. * other.r * x * y;
            }
            let swept = self.input.speed * breadth;
            let inverse_limit = swept / (1.5 * field.x * field.y);
            limits[i] = 1. / ((1. - mix) / max + mix * inverse_limit.max(1. / max));
        }
        for (ball, limit) in self.balls.iter_mut().zip(limits) {
            ball.flow_limit = limit;
        }
    }
    fn integrate(&mut self, g: V) {
        // Bound relative travel by the smallest diameter, including at Speed
        // 300 / Scale 25. Speed-only substeps can let two small balls swap
        // sides before the contact solver sees either overlap.
        let radius = self.balls.iter().map(|b| b.r).fold(f64::INFINITY, f64::min);
        let slices = if self.params.collisions {
            (self.dt * 2.2 * self.height / radius).ceil().max(1.) as u32
        } else {
            1
        };
        let dt = self.dt / f64::from(slices);
        let wall_dt = self.wall_dt / f64::from(slices);
        let decay = if slices == 1 {
            self.decay
        } else {
            Relaxation::new(wall_dt)
        };
        for _ in 0..slices {
            self.integrate_substep(g, dt, wall_dt, decay);
        }
    }
    fn integrate_substep(&mut self, g: V, dt: f64, wall_dt: f64, decay: Relaxation) {
        self.sim_time += dt;
        let down = g.unit();
        let cap = 2.2 * self.height;
        let elastic = self.params.elasticity as f64 * 0.01;
        let zero = g.len2() < 4.;
        let mut energy = 0.;
        for (i, b) in self.balls.iter_mut().enumerate() {
            b.supported = false;
            b.floating = zero;
            b.near_ball = false;
            b.knot_impulse = 0.;
            if i < self.params.count as usize {
                b.alpha = ((self.sim_time - b.birth) / 0.9).clamp(0., 1.);
            }
            b.relax_light(wall_dt, decay);
            if b.sleeping {
                continue;
            }
            let magnus = (b.v.perp() * ((if zero { 0.085 } else { 0.0016 }) * b.spin))
                .limit(0.04 * self.base_g.abs().max(0.15 * self.height));
            b.v = (b.v + (g + magnus) * dt).limit(cap.min(b.flow_limit));
            let spin_limit = (b.flow_limit / b.r).min(38.);
            b.spin = b.spin.clamp(-spin_limit, spin_limit);
            b.angle += b.spin * dt;
            b.drift(dt, &self.rooms[b.room], elastic, self.height, down);
            energy += (b.v.len2() + 0.5 * (b.spin * b.r).powi(2)) / b.inv_mass;
        }
        self.solver.solve(
            &mut self.balls,
            &self.rooms,
            down,
            g,
            wall_dt,
            self.height,
            elastic,
            self.params.collisions,
            true,
            &mut self.rng,
        );
        // A long tilt on the 7920 px floor can keep doing work for several
        // seconds. Keep the gentle drag at ordinary peaks and increase it
        // continuously when the whole room is moving well above its band.
        let excess = (energy / self.kinetic_limit - 2.).max(0.);
        let drag = if excess > 0. {
            (1. - dt * 0.23) / (1. + dt * 0.22 * (excess - 1.).max(0.))
        } else {
            1.
        };
        for b in &mut self.balls {
            let room = &self.rooms[b.room];
            let lively = elastic == 1. && b.mat == Material::Rubber;
            if b.sleeping && !zero && !b.supported {
                b.wake();
            }
            if b.sleeping {
                b.charge += dt;
                if b.charge >= b.period + 0.18 {
                    let up = -down;
                    let free = room
                        .sweep(b.p, up * self.height * 3., b.r)
                        .map_or(self.height, |(u, _, _)| 3. * self.height * u);
                    b.hop(g, free, &mut self.rng);
                }
            } else {
                let drag = if lively && b.v.len2() <= 100. / (drag * drag) {
                    1.
                } else {
                    drag
                };
                b.v = b.v.limit(cap.min(b.flow_limit)) * drag;
                b.spin *= drag;
                if b.supported && !lively {
                    // Rolling resistance removes the final horizontal drift after
                    // static friction has coupled the surface speed and spin.
                    let tangent = down.perp();
                    let speed = b.v.dot(tangent);
                    let resistance = dt
                        * (9.
                            + 380. * (0.9 - elastic).max(0.)
                            + 864. * (0.75 - elastic).max(0.)
                            + 0.06 * speed.abs());
                    b.v -= tangent * speed.clamp(-resistance, resistance);
                    b.spin *= 1. - 0.45 * dt;
                }
                if (b.supported && b.v.len2() < 100.) || (zero && b.v.len2() < 64.) {
                    b.rest += dt;
                } else {
                    b.rest = 0.;
                }
                if !lively
                    && b.rest
                        >= if zero {
                            2.
                        } else {
                            0.28 + 0.52 * ((elastic - 0.50) / 0.40).clamp(0., 1.)
                        }
                {
                    b.sleeping = true;
                    b.v = V::default();
                    b.spin = 0.;
                    b.charge = 0.;
                }
            }
            b.emit_contact_light(&mut self.impact_fields[b.room], self.tick as f64 / 120.);
            if self.input.trails > 0. {
                b.trail.record(
                    b.p,
                    b.v,
                    self.sim_time,
                    b.r,
                    if b.knot_impulse > 0.12 {
                        -b.knot_impulse
                    } else {
                        f64::from(b.glow())
                    },
                    b.knot_impulse > 0.12,
                );
            }
        }
    }
}
impl LivingWorld for BounceWorld {
    fn rate(&self) -> Rate {
        Rate::new(120, 1).unwrap()
    }
    fn step(&mut self, tick: u64) {
        self.tick = tick;
        if self.input.reduced_motion {
            return;
        }
        let t = tick as f64 / 120.;
        self.event_step(t);
        if tick % 4 == 1 {
            self.update_flow_limits();
        }
        let g = self.gravity(t);
        for _ in 0..self.substeps {
            self.integrate(g);
        }
    }
    fn snapshot(&self) -> Snapshot {
        let mut frames = self.frames.borrow_mut();
        let i = frames
            .iter()
            .position(|f| Arc::strong_count(f) == 1)
            .unwrap_or_else(|| {
                frames.push(Arc::new(render::Frame::new(self.art.clone())));
                frames.len() - 1
            });
        let f = Arc::get_mut(&mut frames[i]).unwrap();
        f.update(self);
        Snapshot {
            tick: self.tick,
            time_ns: self.rate_ns(),
            generations: Generations {
                layout: self.input.layout.generation,
                topology: self.balls.len() as u64,
                data: 1,
                sample: self.tick,
            },
            typed: Some(frames[i].clone()),
            emission_estimate: self.emission_estimate(),
            ..self.empty.clone()
        }
    }
    fn emission_estimate(&self) -> f64 {
        self.balls
            .iter()
            .map(|b| b.r * b.r * (0.25 + 0.3 * b.light) * b.alpha)
            .sum::<f64>()
            * std::f64::consts::PI
            / self.input.layout.visible_area
    }
    fn inject(&mut self, e: Event) -> Result<(), &'static str> {
        if !(1..=8).contains(&e.id)
            || e.at_ns < 0
            || !e.anchor.iter().all(|v| v.is_finite())
            || !e.strength.is_finite()
        {
            return Err("invalid bounce event");
        }
        if !self.input.reduced_motion {
            self.script.push(e);
            self.script.sort_by_key(|e| e.at_ns);
        }
        Ok(())
    }
    fn state_hash(&self) -> u64 {
        let mut h = hash64(self.input.seed);
        for b in &self.balls {
            for x in [
                b.p.x, b.p.y, b.v.x, b.v.y, b.spin, b.angle, b.squash, b.light, b.charge, b.alpha,
            ] {
                h = hash64(h ^ x.to_bits());
            }
        }
        h
    }
}
