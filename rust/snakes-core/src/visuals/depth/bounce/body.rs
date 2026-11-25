use super::{arena::*, common::rng::Pcg32};
use std::f64::consts::{PI, TAU};

pub(super) const KNOTS: usize = 32;
pub(super) fn between(rng: &mut Pcg32, lo: f64, hi: f64) -> f64 {
    lo + (hi - lo) * rng.unit()
}
/// Exact wall-time transition for the underdamped visual spring. Computed
/// once per world, so the response is independent of simulation substeps.
#[derive(Clone, Copy)]
pub(super) struct Relaxation {
    light: f64,
    spill: f64,
    xx: f64,
    xv: f64,
    vx: f64,
    vv: f64,
    axis: f64,
}
impl Relaxation {
    pub fn new(dt: f64) -> Self {
        let omega = TAU * 4.5;
        let damping = 0.2 * omega;
        let damped = (omega * omega - damping * damping).sqrt();
        let envelope = (-damping * dt).exp();
        let xv = envelope * (damped * dt).sin() / damped;
        let cos = envelope * (damped * dt).cos();
        Self {
            light: (-dt / 0.35).exp(),
            spill: (-dt / 0.30).exp(),
            xx: cos + damping * xv,
            xv,
            vx: -omega * omega * xv,
            vv: cos - damping * xv,
            axis: 1. - (-dt / 0.05).exp(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum Material {
    Rubber,
    Glass,
    Metal,
}
impl Material {
    pub fn density(self) -> f64 {
        match self {
            Self::Rubber => 1.,
            Self::Glass => 0.6,
            Self::Metal => 2.6,
        }
    }
    pub fn restitution(self, elasticity: f64) -> f64 {
        // The slider controls energy loss, with distinct losses per material.
        // At 100 every material is elastic; multiplying by a fixed material
        // factor made even that endpoint dissipate most of a drop in seconds.
        let loss = match self {
            Self::Rubber => 0.20,
            Self::Glass => 0.30,
            Self::Metal => 0.40,
        };
        1. - loss * (1. - elasticity)
    }
    pub fn friction(self) -> f64 {
        match self {
            Self::Rubber => 0.8,
            Self::Glass => 0.3,
            Self::Metal => 0.4,
        }
    }
    pub fn squash(self) -> f64 {
        match self {
            Self::Rubber => 0.22,
            Self::Glass => 0.,
            Self::Metal => 0.04,
        }
    }
}
#[derive(Clone, Copy, Default, Debug)]
#[repr(C)]
pub(super) struct Knot {
    pub x: f32,
    pub y: f32,
    pub time: f32,
    pub light: f32,
    pub vx: f32,
    pub vy: f32,
    pub distance: f32,
    pub reserved: f32, // fourth component of the motion texel
}
const _: () = assert!(size_of::<Knot>() == 32);
#[derive(Clone)]
pub(super) struct Trail {
    pub knots: [Knot; KNOTS],
    pub serial: u64,
    pub length: usize,
    dir: V,
}
impl Default for Trail {
    fn default() -> Self {
        Self {
            knots: [Knot::default(); KNOTS],
            serial: 0,
            length: 0,
            dir: V::default(),
        }
    }
}
impl Trail {
    pub fn record(&mut self, p: V, velocity: V, t: f64, r: f64, light: f64, hit: bool) {
        let mut distance = 0.;
        if self.length > 0 {
            let prev = self.knots[(self.serial as usize - 1) % KNOTS];
            let moved = (p - v(f64::from(prev.x), f64::from(prev.y))).len2();
            let turn = self.dir.dot(velocity);
            let bent = turn < 0. || turn * turn < 0.98063 * self.dir.len2() * velocity.len2();
            if moved < 0.01
                || (!hit
                    && moved < (0.35 * r).powi(2)
                    && !(bent && moved > (0.10 * r).powi(2))
                    && (t - f64::from(prev.time) < 0.08 || moved < 4.))
            {
                return;
            }
            // Only differences within the 32-knot history are used. Wrapping
            // keeps subpixel precision during long running sessions.
            distance = (f64::from(prev.distance) + moved.sqrt()) % 65536.;
        }
        self.knots[self.serial as usize % KNOTS] = Knot {
            x: p.x as f32,
            y: p.y as f32,
            time: t as f32,
            light: light as f32,
            vx: velocity.x as f32,
            vy: velocity.y as f32,
            distance: distance as f32,
            reserved: 0.,
        };
        self.serial += 1;
        self.length = (self.length + 1).min(KNOTS);
        self.dir = velocity;
    }
}
#[derive(Clone, Copy)]
pub(super) struct Spill {
    pub wall: Wall,
    pub point: V,
    pub strength: f64,
}
#[derive(Clone)]
pub(super) struct Ball {
    pub room: usize,
    pub p: V,
    pub v: V,
    pub r: f64,
    pub inv_mass: f64,
    pub inv_inertia: f64,
    pub mat: Material,
    pub pigment: usize,
    pub spin: f64,
    pub angle: f64,
    pub squash: f64,
    pub spring: f64,
    pub axis: V,
    pub contact_normal: V,
    pub wall_anchor: Option<V>,
    pub squash_age: f64,
    attack_from: f64,
    attack_target: f64,
    target_axis: V,
    pub near_ball: bool,
    pub contact: bool,
    pub flow_limit: f64,
    pub launch_direction: f64,
    pub near_side_wall: bool,
    pub lateral_launch_budget: f64,
    pub high_arc: bool,
    pub below_mid_time: f64,
    pub loft_due: bool,
    pub loft_retry_delay: f64,
    pub light: f64,
    pub flash: f64,
    pub flash_age: f64,
    body_from: f64,
    pub glass_pattern: u16,
    pub knot_impulse: f64,
    pub pending_light: f64,
    pub heavy: bool,
    pub landed: bool,
    pub impact: Option<Spill>,
    pub supported: bool,
    pub floating: bool,
    pub floating_wall_time: f64,
    pub sleeping: bool,
    pub rest: f64,
    pub charge: f64,
    #[cfg(test)]
    pub hop_serial: u64,
    #[cfg(test)]
    pub hop_origin: V,
    pub period: f64,
    pub phase: f64,
    pub birth: f64,
    pub alpha: f64,
    pub trail: Trail,
}
impl Ball {
    pub fn new(rng: &mut Pcg32, room: usize, r: f64, mat: Material, pigment: usize) -> Self {
        let inv_mass = 1. / (PI * r * r * mat.density());
        let spin = between(rng, -1.8, 1.8);
        let angle = between(rng, 0., TAU);
        Self {
            room,
            p: V::default(),
            v: V::default(),
            r,
            inv_mass,
            inv_inertia: 2. * inv_mass / (r * r),
            mat,
            pigment,
            spin,
            angle,
            squash: 0.,
            spring: 0.,
            axis: v(0., 1.),
            contact_normal: v(0., 1.),
            wall_anchor: None,
            squash_age: 1.,
            attack_from: 0.,
            attack_target: 0.,
            target_axis: v(0., 1.),
            near_ball: false,
            contact: false,
            flow_limit: f64::INFINITY,
            launch_direction: 0.,
            near_side_wall: false,
            lateral_launch_budget: 0.,
            high_arc: false,
            below_mid_time: 0.,
            loft_due: false,
            loft_retry_delay: 0.,
            light: 0.,
            flash: 0.,
            flash_age: 1.,
            body_from: 0.,
            glass_pattern: (angle / TAU * 65535.) as u16,
            knot_impulse: 0.,
            pending_light: 0.,
            heavy: false,
            landed: false,
            impact: None,
            supported: false,
            floating: false,
            floating_wall_time: 0.,
            sleeping: false,
            rest: 0.,
            charge: 0.,
            #[cfg(test)]
            hop_serial: 0,
            #[cfg(test)]
            hop_origin: V::default(),
            period: between(rng, 0.55, 1.45),
            phase: between(rng, 0., TAU),
            birth: 0.,
            alpha: 1.,
            trail: Trail::default(),
        }
    }
    pub fn limit_motion(&mut self, cap: f64, _down: V) {
        if self.high_arc && !self.floating && self.flow_limit < cap {
            // Crowding limits lateral sweeps, not the energy of a vertical
            // bounce. A norm clamp here was cutting tall launches back down
            // to a few percent of the room height on every integration step.
            let velocity = self.v.limit(cap);
            // Retain the launch allowance through the apex. Tying it to the
            // current vertical speed erases sideways travel on every ascent.
            let lateral_limit = self.flow_limit.max(self.lateral_launch_budget);
            self.v = v(velocity.x.clamp(-lateral_limit, lateral_limit), velocity.y);
        } else {
            self.v = self.v.limit(cap);
        }
    }
    pub fn set_material(&mut self, material: Material) {
        self.mat = material;
        self.inv_mass = 1. / (PI * self.r * self.r * material.density());
        self.inv_inertia = 2. * self.inv_mass / (self.r * self.r);
    }
    pub fn strike(&mut self, n: V, speed: f64, height: f64, pair: bool) {
        let amount = (speed / height).clamp(0., 1.);
        if amount < 0.035 {
            return;
        }
        // The first contact sets the axis before there is visible deformation.
        // Later contacts rotate an unoriented axis along its shorter arc.
        if self.squash.abs() < 0.005 && self.spring.abs() < 0.15 {
            self.axis = n;
        }
        self.target_axis = if self.axis.dot(n) < 0. { -n } else { n };
        let amplitude = self.mat.squash() * amount * if pair { 0.4 } else { 1. };
        if amplitude > self.squash.max(0.)
            && (self.squash_age >= 1. / 60. || amplitude >= self.attack_target)
        {
            self.attack_from = self.squash;
            self.attack_target = amplitude;
            self.squash_age = 0.;
            self.contact_normal = n;
            self.wall_anchor = None;
            self.spring = 0.;
        }
        self.knot_impulse = self.knot_impulse.max(amount);
        self.pending_light = self.pending_light.max(amount);
    }
    pub fn emit_contact_light(&mut self) {
        let amount = std::mem::take(&mut self.pending_light);
        if amount > 0. {
            self.start_impact_light(amount);
        }
    }
    pub fn start_impact_light(&mut self, amount: f64) {
        self.body_from = self.body_light();
        self.light = self.light.max(amount);
        self.flash = amount;
        self.flash_age = 0.;
    }
    pub fn wall_impulse(&mut self, n: V, wall: Wall, elasticity: f64, height: f64) {
        self.contact = true;
        let vn = self.v.dot(n);
        if vn >= 0. {
            return;
        }
        let settle_speed = if self.floating {
            2.
        } else {
            42. + height * 0.60 * (0.80 - elasticity).max(0.)
        };
        let restitution = if vn < -settle_speed || elasticity == 1. {
            self.mat.restitution(elasticity)
        } else {
            0.
        };
        let impulse = -(1. + restitution) * vn;
        let t = n.perp();
        let slip = self.v.dot(t) - self.spin * self.r;
        let limit = self.mat.friction() * impulse;
        let tangent = (-slip / 3.).clamp(-limit, limit);
        self.v += n * impulse + t * tangent;
        self.spin -= 2. * tangent / self.r;
        self.strike(n, -vn, height, false);
        if self.squash_age == 0. && self.contact_normal.dot(n) > 0.99 {
            self.wall_anchor = Some(wall.nearest(self.p));
        }
        if -vn > 0.06 * height {
            self.impact = Some(Spill {
                wall,
                point: wall.nearest(self.p),
                strength: (-vn / height).min(1.),
            });
        }
    }
    pub fn drift(&mut self, dt: f64, room: &Room, elasticity: f64, height: f64, down: V) {
        let mut remaining = dt;
        for _ in 0..4 {
            let travel = self.v * remaining;
            if travel.len2() < 1e-12 {
                break;
            }
            if let Some((t, n, wall)) = room.sweep(self.p, travel, self.r) {
                self.p += travel * t + n * 1e-7;
                self.supported |= n.dot(down) < -0.55;
                self.wall_impulse(n, wall, elasticity, height);
                remaining *= 1. - t;
            } else {
                self.p += travel;
                break;
            }
        }
    }
    pub fn relax_light(&mut self, wall_dt: f64, decay: Relaxation) {
        if self.sleeping {
            let charge = (self.charge / self.period).min(1.);
            self.phase += TAU * wall_dt * (0.5 + 1.5 * charge);
        }
        self.light *= decay.light;
        self.flash_age += wall_dt;
        if let Some(spill) = &mut self.impact {
            spill.strength *= decay.spill;
            if spill.strength < 0.005 {
                self.impact = None;
            }
        }
        let attack = 1. / 60.;
        let prior_age = self.squash_age;
        self.squash_age += wall_dt;
        if prior_age < attack - 1e-10 {
            let t = (self.squash_age / attack).clamp(0., 1.);
            self.squash =
                self.attack_from + (self.attack_target - self.attack_from) * t * t * (3. - 2. * t);
            self.axis = self.target_axis;
        } else if self.squash.abs() + self.spring.abs() > 0.00001 {
            self.axis = (self.axis + (self.target_axis - self.axis) * decay.axis).unit();
            (self.squash, self.spring) = (
                decay.xx * self.squash + decay.xv * self.spring,
                decay.vx * self.squash + decay.vv * self.spring,
            );
        }
    }
    pub fn wake(&mut self) {
        self.sleeping = false;
        self.rest = 0.;
        // A nudge can wake a large ball many times while it is trapped in a
        // pile. Keep its stored launch energy until it actually spends it.
        if !self.high_arc {
            self.charge = 0.;
        }
    }
    pub fn hop(&mut self, g: V, height: f64, rng: &mut Pcg32) {
        let gravity = g.len();
        self.lateral_launch_budget = 0.;
        #[cfg(test)]
        {
            self.hop_serial += 1;
            self.hop_origin = self.p;
        }
        self.wake();
        // A due loft keeps its stored charge until it reaches the upper half.
        // A collision may interrupt it; retry at contact, never in free air.
        // Repeated solver samples of the same bed contact are not separate
        // launch attempts. Let the contact respond before another retry.
        self.loft_retry_delay = if self.loft_due && gravity > 2. {
            0.12
        } else {
            0.
        };
        self.charge = if self.loft_due && gravity > 2. {
            self.period + 0.18
        } else {
            0.
        };
        if gravity > 2. {
            let up = if self.loft_due {
                v(0., -1.)
            } else {
                -g / gravity
            };
            let (low, high) = if self.high_arc {
                (1.10, 1.30)
            } else {
                (0.25, 0.70)
            };
            let speed = (2. * gravity * height.max(self.r) * between(rng, low, high)).sqrt();
            let side = if self.high_arc { 0.12 } else { 0.37 };
            let lateral = between(rng, -side, side);
            let lateral =
                if self.high_arc && (self.near_side_wall || self.flow_limit < 0.4000 * speed) {
                    let direction = if self.launch_direction == 0. {
                        lateral.signum()
                    } else {
                        self.launch_direction * up.perp().x.signum()
                    };
                    direction * (0.2000 + 0.02 * lateral.abs() / side)
                } else {
                    lateral
                };
            if self.high_arc {
                self.lateral_launch_budget = lateral.abs() * speed;
            }
            self.v = up * speed + up.perp() * lateral * speed;
            self.axis = up;
        } else {
            let angle = between(rng, 0., TAU);
            self.v = v(angle.cos(), angle.sin()) * between(rng, 0.09, 0.24) * height;
        }
        self.spin += between(rng, -1.2, 1.2);
        self.pending_light = self.pending_light.max(0.55);
    }
    pub fn impact_attack(&self) -> f64 {
        let t = (self.flash_age / 0.050).clamp(0., 1.);
        t * t * (3. - 2. * t)
    }
    pub fn body_light(&self) -> f64 {
        self.body_from + (self.light - self.body_from) * self.impact_attack()
    }
    pub fn quiet_glow(&self) -> f32 {
        if !self.sleeping {
            return 0.25;
        }
        let charge = (self.charge / self.period).min(1.);
        let anticipation = self.period.min(1.5);
        let rise = ((self.charge - self.period + anticipation) / anticipation).clamp(0., 1.);
        (0.25 + 0.35 * rise + (0.045 + 0.32 * charge) * self.phase.sin()).clamp(0.04, 1.) as f32
    }
    pub fn glow(&self) -> f32 {
        (self.quiet_glow() + 0.75 * self.light as f32).min(1.)
    }
}
