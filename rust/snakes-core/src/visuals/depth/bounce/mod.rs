//! Seeded juggling balls, shared by every output sampling this world.
mod arena;
mod body;
mod contacts;
mod events;
#[cfg(test)]
mod flight_audit;
#[cfg(test)]
mod height_tests;
#[cfg(test)]
mod longevity;
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
// Calibrated from native two-seed G films; capture can override the pace.
const CROWDED_PACE_AT_300: f64 = 1.;
// Native L3 G sweep calibration, seeds 7/19/31, every physical output.
const OUTPUT_SWEEP_PACE_AT_300: f64 = 0.9;
// Measured unfitted-cast ceiling. Release smoothly below the smallest native
// calibration size, so ordinary tiny balls can still use the full slider.
const UNFITTED_SWEEP_PACE_AT_300: f64 = 2.0;

fn paced_speed(requested: f64, cap: f64) -> f64 {
    // Keep Speed 100 useful when a large-field sweep budget is below one.
    // Both branches remain strictly increasing, including the lowest cap.
    let low = if cap < 1. {
        0.6_f64.min(0.75 * cap)
    } else {
        0.6 * cap
    };
    let scaled = if requested <= 1. {
        low * requested
    } else {
        low + (cap - low) * (requested - 1.) / 2.
    };
    requested.min(scaled)
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
    /// World y of the layout's vertical midpoint (origins may be negative).
    mid_y: f64,
    base_g: f64,
    pace: f64,
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
    #[cfg(test)]
    flight_audit: flight_audit::FlightAudit,
}
impl BounceWorld {
    pub fn new(input: WorldInput) -> Self {
        let params = Params::read(&input);
        let mut rng = Pcg32::new(input.seed, 0x626f756e6365);
        let rooms: Arc<[Room]> = Room::from_layout(&input.layout).into();
        let impact_fields = rooms.iter().map(ImpactField::new).collect();
        let counts = spawn::population(&rooms, params.count as usize);
        let height = input.layout.bounds.height;
        let mid_y = input.layout.bounds.y + height * 0.5;
        let mut balls = Vec::with_capacity(61);
        let mut medians = Vec::new();
        let mut fitting_areas = Vec::with_capacity(rooms.len());
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
            let mut output_areas: Vec<f64> = room.outputs.iter().map(|o| o.area()).collect();
            output_areas.sort_by(f64::total_cmp);
            let fitting_area = room.area.min(output_areas[output_areas.len() / 2]);
            fitting_areas.push(fitting_area);
            let fit = (0.21 * fitting_area / area.max(1e-12)).sqrt().min(1.);
            medians.push(nominal * fit);
            for radius in radii {
                let mat = spawn::material(params.material, &mut rng);
                let pigment = rng.bounded(6).unwrap() as usize;
                let mut b = Ball::new(&mut rng, i, radius * fit, mat, pigment);
                b.high_arc = input.scale > 1.5 && params.gravity.abs() >= 2;
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
        // Preserve the spatial arc while sharing a wall-time sweep budget.
        // Clamping velocity alone lowers the apex; pacing gravity and motion
        // together makes large, crowded rooms calmer without flattening them.
        let pace = if base_g.abs() > 2. {
            // Actual fitted bodies determine the sweep risk. Scale can produce
            // identical radii on both sides of a fit threshold, and a sparse
            // spanning room can still have a large body on its smallest output.
            let (low_speed_budget, high_speed_budget) = rooms
                .iter()
                .enumerate()
                .map(|(i, room)| {
                    let area: f64 = balls
                        .iter()
                        .filter(|b| b.room == i)
                        .map(|b| std::f64::consts::PI * b.r * b.r)
                        .sum();
                    let radius = balls
                        .iter()
                        .filter(|b| b.room == i)
                        .map(|b| b.r)
                        .fold(0., f64::max);
                    let short = room
                        .outputs
                        .iter()
                        .map(|o| o.width.min(o.height))
                        .fold(f64::INFINITY, f64::min);
                    // Retain the proven room-wide sweep budget, and bound a
                    // fitted cast by one representative output as well. The
                    // shoulder preserves the approved, unfitted default pace.
                    let room_coverage_cap = (0.11 * room.area / area.max(1e-12)).clamp(1., 3.);
                    let output_coverage_cap = (0.21 * fitting_areas[i] / area.max(1e-12))
                        .powi(4)
                        .clamp(1., 3.);
                    let coverage_cap = room_coverage_cap.min(output_coverage_cap);
                    let radius_cap = (0.23 * short / radius).powi(3).clamp(0.6, 3.);
                    let rms_radius =
                        (area / (std::f64::consts::PI * counts[i].max(1) as f64)).sqrt();
                    // A sweep occupies more of a smaller physical output.
                    // The shoulder keeps ordinary unfitted casts at their
                    // original clock, while fitted large casts use the native
                    // flash calibration. Quantize the empirical budget to
                    // avoid seed-dependent floating-point clock differences.
                    let sweep_cap = (OUTPUT_SWEEP_PACE_AT_300
                        * (0.13268 * short / rms_radius.max(1e-12)).powi(10)
                        * 1000.)
                        .round()
                        / 1000.;
                    let low_speed_budget = CROWDED_PACE_AT_300
                        * coverage_cap.min(radius_cap).min(sweep_cap.clamp(0.6, 3.));
                    // Native L3 fitting gives RMS 143.30 px on a 1080 px
                    // output. A cubic shoulder bounds the pace ratio of casts
                    // within 10% RMS size to 1.1^3, even across different seeds.
                    // Total coverage has the same radius exponent; a single
                    // large hero ball cannot introduce a second pace cliff.
                    let reference_radius = (143.2997117146043 / 1080.) * short;
                    let cubic = OUTPUT_SWEEP_PACE_AT_300
                        * (reference_radius / rms_radius.max(1e-12)).powi(3);
                    let cubic = (cubic * 1e9).round() / 1e9;
                    let output_coverage = (0.21 * fitting_areas[i] / area.max(1e-12))
                        .powf(1.5)
                        .clamp(1., 3.);
                    let small_reference = (96.68942336409975 / 1080.) * short;
                    let measured = UNFITTED_SWEEP_PACE_AT_300
                        * (small_reference / rms_radius.max(1e-12)).max(1.).powi(3);
                    let measured = (measured * 1e9).round() / 1e9;
                    let high_speed_budget = CROWDED_PACE_AT_300
                        * room_coverage_cap
                            .min(output_coverage)
                            .min(cubic.min(measured).clamp(0.6, 3.));
                    (low_speed_budget, high_speed_budget)
                })
                .fold((3_f64, 3_f64), |(low, high), (a, b)| {
                    (low.min(a), high.min(b))
                });
            if input.speed <= 1. {
                // Keep approved low-Speed trajectories bit-for-bit, including
                // default Spanning. Only the fast end needs a smooth shoulder.
                paced_speed(input.speed, low_speed_budget)
            } else {
                let low = paced_speed(1., low_speed_budget);
                input
                    .speed
                    .min(low + (high_speed_budget - low) * (input.speed - 1.) / 2.)
            }
        } else {
            input.speed
        };
        let substeps = pace.ceil() as u32;
        let dt = pace / (120. * substeps as f64);
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
            mid_y,
            base_g,
            pace,
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
            #[cfg(test)]
            flight_audit: flight_audit::FlightAudit::default(),
        }
    }
    fn rate_ns(&self) -> i64 {
        self.rate().tick_ns(self.tick)
    }
    #[cfg(any(test, feature = "visual-capture"))]
    fn set_capture_pace(&mut self, pace: f64) {
        self.pace = pace.clamp(0.1, 3.);
        self.substeps = self.pace.ceil() as u32;
        self.dt = self.pace / (120. * f64::from(self.substeps));
        self.wall_dt = 1. / (120. * f64::from(self.substeps));
        self.decay = Relaxation::new(self.wall_dt);
    }
    // In an accelerated, large-scale pile, several fully lit silhouettes can
    // repeatedly sweep across the same field even with every impact dark.
    // Diameter times wall speed estimates swept area per second. Share it
    // between neighbours, fading their contribution as they leave the field.
    fn update_flow_limits(&mut self) {
        let max = 2.2 * self.height;
        let speed_mix = if self.balls.iter().any(|b| b.high_arc) {
            1.
        } else {
            (self.input.speed - 1.).clamp(0., 2.) * 0.5
        };
        let mix = speed_mix * (self.input.scale - 1.).clamp(0., 1.);
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
            let swept = self.pace * breadth;
            let inverse_limit = swept / (1.5 * field.x * field.y);
            let flow = 1. / ((1. - mix) / max + mix * inverse_limit.max(1. / max));
            // Large-ball gravity rooms preserve vertical energy separately.
            // Keep the cooperative budget for lateral sweeps; a floor on the
            // whole speed was neither a height guarantee nor a light budget.
            limits[i] = if ball.high_arc {
                flow
            } else {
                flow.max((0.12 * self.height * self.base_g.abs()).sqrt())
            };
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
        #[cfg(test)]
        let mut expected_free = [None; 61];
        for (i, b) in self.balls.iter_mut().enumerate() {
            b.supported = false;
            b.contact = false;
            b.loft_retry_delay = (b.loft_retry_delay - dt).max(0.);
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
            #[cfg(test)]
            {
                expected_free[i] = Some(b.v + (g + magnus) * dt);
            }
            b.v += (g + magnus) * dt;
            b.limit_motion(cap, down);
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
        for (ball_index, b) in self.balls.iter_mut().enumerate() {
            #[cfg(not(test))]
            let _ = ball_index;
            let room = &self.rooms[b.room];
            let lively = elastic == 1.;
            if b.sleeping && !zero && !b.supported {
                b.wake();
            }
            // The minimum possible bottom edge is 2r. A larger body cannot
            // clear mid-height, so it must not pursue an impossible loft.
            if b.high_arc && self.base_g > 2. && 4. * b.r < self.height {
                if b.p.y + b.r < self.mid_y {
                    b.below_mid_time = 0.;
                    if b.loft_due {
                        b.charge = 0.;
                    }
                } else {
                    b.below_mid_time += wall_dt;
                }
                // A tilted pile or horizontal Zero-G drift can strand one
                // ball below the others. Its seeded patience releases a loft
                // at contact, or a gentle rising current in an interlude.
                let patience = 12. + 8. * f64::from(b.glass_pattern) / 65535.;
                b.loft_due = b.below_mid_time > patience;
                if zero && b.loft_due {
                    b.wake();
                    b.v.y -= self.height * 0.06 * dt;
                }
            }
            // Charge in wall time, including during flight. At the next low
            // rebound, release the stored energy through contact instead of
            // letting the room decay into a long sleeping pile. Seeded periods
            // and launch heights keep the rebounds staggered. Positions never
            // change here; swept integration handles the ensuing flight.
            // A flow-limited crowd has much shorter flights. Match its
            // recharge to that physical flight scale, so the light-safety
            // speed guard cannot turn an accelerated pile into a still life.
            let charge_rate = if b.high_arc {
                // A Speed-300 flight lasts a third as long on screen. Keep
                // recharge in step with that flight instead of spending most
                // of each short cycle resting after a tall landing.
                self.pace
            } else if zero {
                1.
            } else {
                (cap / b.flow_limit).clamp(1., 24.)
            };
            b.charge = (b.charge + wall_dt * charge_rate).min(b.period + 0.18);
            let rebound = if b.loft_due { b.v.y } else { b.v.dot(down) };
            let low_rebound = !zero && rebound.powi(2) < 0.60 * g.len() * self.height;
            // An incidental contact near the top should exchange momentum,
            // not spend the next launch on a short hop into the ceiling.
            let launch_zone = !b.high_arc || self.base_g <= 2. || b.p.y >= self.mid_y || b.loft_due;
            let launch_contact = b.supported || (b.loft_due && b.contact);
            if launch_contact
                && low_rebound
                && launch_zone
                && b.charge >= b.period + 0.18
                && b.loft_retry_delay == 0.
            {
                let launch_up = if b.loft_due { v(0., -1.) } else { -down };
                let free = room
                    .sweep(b.p, launch_up * self.height * 3., b.r)
                    .map_or(self.height, |(u, _, _)| 3. * self.height * u);
                b.hop(g, free, &mut self.rng);
            }
            if b.sleeping {
                if b.charge >= b.period + 0.18 && b.loft_retry_delay == 0. {
                    let up = if b.loft_due { v(0., -1.) } else { -down };
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
                b.limit_motion(cap, down);
                b.v = b.v * drag;
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
            #[cfg(test)]
            self.flight_audit.observe(
                ball_index,
                b,
                expected_free[ball_index],
                cap,
                drag,
                elastic,
                self.pace,
                wall_dt,
                self.tick as f64 / 120.,
                g.y,
                !zero
                    && !self
                        .moment
                        .is_some_and(|m| matches!(m.kind, events::Kind::Float)),
            );
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
        let last_event = if cfg!(feature = "visual-capture") {
            9
        } else {
            8
        };
        if !(1..=last_event).contains(&e.id)
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
                b.p.x,
                b.p.y,
                b.v.x,
                b.v.y,
                b.spin,
                b.angle,
                b.squash,
                b.light,
                b.charge,
                b.alpha,
                b.below_mid_time,
                b.loft_retry_delay,
            ] {
                h = hash64(h ^ x.to_bits());
            }
        }
        h
    }
}
