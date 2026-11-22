use super::common::{
    flow::{FlowField, p},
    layout::{Rect, Vec2},
    rng::Pcg32,
};
use std::f64::consts::{FRAC_PI_2, TAU};

pub(super) const NODES: usize = 128;

pub(super) fn range(rng: &mut Pcg32, a: f64, b: f64) -> f64 {
    a + (b - a) * rng.unit()
}
pub(super) fn smooth(x: f64) -> f64 {
    let u = x.clamp(0., 1.);
    u * u * (3. - 2. * u)
}
pub(super) fn polar(angle: f64) -> Vec2 {
    let (y, x) = angle.sin_cos();
    p(x, y)
}
pub(super) fn turn(a: Vec2, b: Vec2) -> f64 {
    (a.x * b.y - a.y * b.x).atan2(a.dot(b))
}
#[derive(Clone, Copy, Default)]
pub(super) struct Node {
    pub pos: Vec2,
    pub phase: f64,
}
#[derive(Clone)]
pub(super) struct Ribbon {
    pub id: u32,
    pub layer: usize,
    pub nodes: [Node; NODES],
    pub count: usize,
    pub spacing: f64,
    pub width: f64,
    pub heading: Vec2,
    previous_velocity: Vec2,
    relaxed_phase: [f64; NODES],
    using_relaxed_phase: bool,
    pub speed: f64,
    pub temperament: f64,
    pub goal: Vec2,
    pub lane: f64,
    pub course_sign: f64,
    pub release: Vec2,
    pub release_age: f64,
    pub caught: bool,
    pub eddy_age: f64,
    turn_sum: f64,
    escape_for: f64,
    escape_axis: Vec2,
    pub knot_for: f64,
    pub knot_panel: usize,
    pub avoid: Vec2,
    pub motion_age: f64,
    pub shed_at: f64,
    pub sink: f64,
    pub course_for: f64,
    pub home: Rect,
    pub age: f64,
    pub life: f64,
    fray_duration: f64,
    pub grow: f64,
    pub spin: f64,
    pub spin_rate: f64,
    pub tilt: f64,
    velocities: [Vec2; 17],
    pressure: [Vec2; 17],
    pub pulses: [f64; 3],
    pub pulse_wait: f64,
    pub crossing_wait: f64,
    pub shed: usize,
    pub shed_limit: usize,
    pub hue: usize,
    pub light: f64,
    pub alive: bool,
    pub leaving: bool,
}
pub(super) struct Spawn {
    pub id: u32,
    pub layer: usize,
    pub home: Rect,
    pub head: Vec2,
    pub mature: bool,
    pub direction: f64,
    pub population: usize,
}
pub(super) struct Steering {
    pub desired: Vec2,
    pub curl: f64,
    pub gust: f64,
    pub gust_front: Option<(super::water::Moment, f64)>,
    pub turbulence: f64,
    pub still: f64,
    pub smooth_twist: bool,
    pub spanning: bool,
    pub braid_phase: Option<f64>,
    pub orbit: Option<super::water::Moment>,
}
impl Ribbon {
    pub fn new(rng: &mut Pcg32, spawn: Spawn, length: f64, scale: f64, twist: f64) -> Self {
        let Spawn {
            id,
            layer,
            home,
            head,
            mature,
            direction,
            population,
        } = spawn;
        let heading = p(direction, range(rng, -0.45, 0.45)).unit();
        let total = ((0.30 + 1.5 * length) * home.width * range(rng, 0.75, 1.25)).clamp(48., 5000.);
        // Fine transverse detail and spline subdivision live on the GPU.
        // A distant band does not need as many simulated bends as a near scarf.
        // Each independent screen retains full detail. Large shared worlds
        // distribute a fixed knot budget across their target population.
        let full = [64, 96, NODES][layer];
        let capacity = (24 * full / population).clamp(32, full);
        let count = ((total / 18.).ceil() as usize + 1).clamp(32, capacity);
        let spacing = total / (count - 1) as f64;
        let layer_scale = [0.45, 1., 1.8][layer];
        let width = 16. * scale * layer_scale * range(rng, 0.84, 1.16);
        let mut nodes = [Node::default(); NODES];
        let phase = range(rng, -3., 3.);
        let radius_scale = range(rng, 0.80, 1.35);
        let bend = range(rng, 0., TAU);
        let mut pos = head;
        // Lay out a mature, arclength-parameterized strand at activation.
        // Afterwards its shape comes from the integrated head and local current.
        for (i, node) in nodes[..count].iter_mut().enumerate() {
            let arc = i as f64 * spacing;
            *node = Node {
                pos,
                phase: twist * (phase + arc / (140. * radius_scale)),
            };
            let a = 0.50 * (arc / (265. * radius_scale) + bend).sin()
                + 0.16 * (arc / (148. * radius_scale) - bend).sin();
            let tangent = p(
                heading.x.signum(),
                a + ((pos.y - head.y) / home.height) * 1.9,
            )
            .unit();
            pos = pos - tangent * spacing;
        }
        let life = range(rng, 120., 360.);
        let age = if mature {
            range(rng, 32., life - 8.)
        } else {
            0.
        };
        Self {
            id,
            layer,
            nodes,
            count,
            spacing,
            width,
            heading,
            previous_velocity: Vec2::default(),
            relaxed_phase: [0.; NODES],
            using_relaxed_phase: false,
            speed: 90.,
            temperament: rng.unit(),
            goal: head,
            lane: head.y,
            course_sign: direction,
            release: Vec2::default(),
            release_age: 0.,
            caught: false,
            eddy_age: 0.,
            turn_sum: 0.,
            escape_for: 0.,
            escape_axis: Vec2::default(),
            knot_for: 0.,
            knot_panel: 0,
            avoid: Vec2::default(),
            motion_age: age,
            shed_at: 0.,
            sink: 0.,
            course_for: 0.,
            home,
            age,
            life,
            fray_duration: 12.0_f64.max(2.25 * (3. * total / 200.).sqrt()),
            grow: range(rng, 15., 30.),
            spin: phase,
            spin_rate: range(rng, 0.71, 1.12) * if rng.unit() < 0.5 { -1. } else { 1. },
            tilt: 0.,
            velocities: [Vec2::default(); 17],
            pressure: [Vec2::default(); 17],
            pulses: [if mature { -1000. } else { 0. }, -1000., -1000.],
            pulse_wait: range(rng, 2., 28.),
            crossing_wait: 0.,
            shed: 0,
            shed_limit: range(rng, 20., 41.) as usize,
            hue: id as usize,
            light: 1.,
            alive: true,
            leaving: false,
        }
    }
    pub fn appearance_phase(&self, i: usize) -> f64 {
        if self.using_relaxed_phase {
            self.relaxed_phase[i]
        } else {
            self.nodes[i].phase
        }
    }
    pub fn fringe_strength(&self) -> f64 {
        if self.using_relaxed_phase { 0. } else { 1. }
    }
    pub fn release_orbit(&mut self, centre: Vec2) {
        let outward = (self.head() - centre).unit();
        let (sin, cos) = 20_f64.to_radians().sin_cos();
        self.release = outward.perpendicular() * cos + outward * sin;
        self.heading = self.release;
        self.speed = self.speed.max(290.);
        self.release_age = 20.;
        self.caught = false;
        self.eddy_age = 0.;
        self.turn_sum = 0.;
        self.escape_for = 0.;
    }

    pub fn head(&self) -> Vec2 {
        self.nodes[0].pos
    }
    // At maximum lifecycle speed, the last 200 px take at least 1.5 s.
    // smoothstep(t) <= 3*t², so this also bounds a partially completed fray.
    pub fn exit_duration(&self) -> f64 {
        10.0_f64.max(2.25 * (3. * self.spacing * (self.count - 1) as f64 / 200.).sqrt())
    }
    pub fn retire_downstream(&mut self) {
        let remaining = self.life - self.age;
        let exit = self.exit_duration();
        if remaining > exit {
            // Preserve the current taper when an exiting head shortens its
            // lifetime. A fresh ten-second fray starts at full length.
            self.fray_duration *= exit / remaining.min(self.fray_duration);
            self.life = self.age + exit;
        }
    }
    pub fn extent(&self) -> f64 {
        smooth((self.age - 1.5) / self.grow) * smooth((self.life - self.age) / self.fray_duration)
    }
    // A threatened crossing is detected along the forward ray, against the
    // actual distal segments. Keep the avoidance force perpendicular to travel.
    pub fn crossing_ahead(&self) -> Option<Vec2> {
        let head = self.head();
        let horizon = 1.5 * self.speed;
        let clearance = 1.5 * self.width;
        let end = ((self.count - 1) as f64 * self.extent()) as usize;
        let side = self.heading.perpendicular();
        let mut nearest = horizon + clearance;
        let mut escape = None;
        for pair in self.nodes[end / 4..=end].windows(2) {
            let a = pair[0].pos - head;
            let b = pair[1].pos - head;
            let ya = a.dot(side);
            let yb = b.dot(side);
            if ya.min(yb) > clearance || ya.max(yb) < -clearance {
                continue;
            }
            let t = if (yb - ya).abs() < 1e-6 {
                0.5
            } else {
                (-ya / (yb - ya)).clamp(0., 1.)
            };
            let point = a + (b - a) * t;
            let x = point.dot(self.heading);
            if x > 0. && x <= horizon && x < nearest {
                nearest = x;
                escape = Some(side * -1.0_f64.copysign(0.5 * (ya + yb)));
            }
        }
        escape
    }
    pub fn pulse(&mut self) {
        if let Some(p) = self.pulses.iter_mut().find(|p| **p < 0.) {
            *p = 0.;
        }
    }
    pub fn swim(
        &mut self,
        steering: Steering,
        water: &FlowField,
        lanes: &super::water::Lanes,
        twist: f64,
        dt: f64,
        refresh: bool,
    ) {
        let Steering {
            mut desired,
            curl,
            gust,
            gust_front,
            turbulence,
            still,
            smooth_twist,
            spanning,
            braid_phase,
            orbit,
        } = steering;
        let old_nodes = self.nodes;
        let rate = [0.55, 1., 1.3][self.layer];
        if self.escape_for > 0. && orbit.is_none() {
            desired += self.escape_axis * (0.4 * self.speed);
            self.escape_for -= dt;
        }
        let calm = turbulence <= 0.15 && orbit.is_none() && gust == 0.;
        if calm {
            let tide = water.drift.unit();
            let horizontal = p(1.0_f64.copysign(tide.x), 0.);
            let towards_lane = turn(tide, horizontal);
            let lane_offset =
                turn(horizontal, desired).clamp(-16_f64.to_radians(), 16_f64.to_radians());
            let offset =
                (towards_lane + lane_offset).clamp(-25_f64.to_radians(), 25_f64.to_radians());
            desired =
                (tide * offset.cos() + tide.perpendicular() * offset.sin()) * desired.length();
        }
        let departing = self.release_age > 0.;
        let shared_lane = spanning
            && self.layer > 0
            && self.knot_for == 0.
            && orbit.is_none()
            && !departing
            && still == 0.;
        if shared_lane {
            // Even beside a bezel or a short output, the swimming direction
            // continues downstream. All steering still goes through the turn budget.
            desired.x =
                self.course_sign * (desired.x * self.course_sign).max(0.26 * desired.length());
        }
        if departing {
            desired += self.release * (300. * smooth(self.release_age / 6.));
        }
        let target_speed = (desired.length() * rate).clamp(
            38. * rate,
            if departing {
                315.
            } else if gust > 0. {
                500.
            } else {
                230.
            },
        ) * (1. - 0.70 * still);
        let target_speed = if departing {
            target_speed.max(240. * smooth(self.release_age / 8.))
        } else {
            target_speed
        };
        self.speed += (target_speed - self.speed) * (dt * 1.25).min(1.);
        let angle = turn(self.heading, desired);
        let radius = [160., 220., 300.][self.layer];
        let max_rate = if calm {
            (self.speed / 520.).min(15_f64.to_radians())
        } else {
            (self.speed / radius).min(0.88)
        };
        let max_rate = max_rate * if self.release_age > 14. { 0.30 } else { 1. };
        let delta = angle.clamp(-max_rate * dt, max_rate * dt);
        if self.turn_sum * delta < 0. {
            self.turn_sum = 0.;
        }
        if !departing {
            self.turn_sum += delta;
        }
        if self.turn_sum.abs() >= 300_f64.to_radians() {
            self.escape_axis = self.heading.perpendicular() * -self.turn_sum.signum();
            self.escape_for = 4.;
            self.turn_sum = 0.;
        }
        let (s, c) = delta.sin_cos();
        self.heading = self.heading * c + self.heading.perpendicular() * s;
        let wind_push = p(395., -10.) * [0.72, 0.90, 1.0][self.layer];
        let mut velocity = self.heading * self.speed + wind_push * gust;
        let shared_radius = spanning && self.layer > 0 && orbit.is_none() && !departing;
        if shared_radius && self.previous_velocity.dot(self.previous_velocity) > 1e-6 {
            // A travelling gust also translates the head. Bound the resultant
            // velocity, after adding the wind, in the same native units as the
            // steering limit. The shorter chord bounds the three-point radius.
            let speed = velocity.length();
            let before = self.previous_velocity.unit();
            let limit = dt * speed.min(self.previous_velocity.length()) / (1.02 * radius);
            let angle = turn(before, velocity);
            if angle.abs() > limit {
                let (sn, cs) = limit.copysign(angle).sin_cos();
                velocity = (before * cs + before.perpendicular() * sn) * speed;
            }
        }
        self.previous_velocity = velocity;
        self.nodes[0].pos += velocity * dt;
        let head_y = self.nodes[0].pos.y;
        self.spin += dt * (self.spin_rate + curl.clamp(-1.8, 1.8) * 1.6) * (twist / 0.55);
        let desired_phase = braid_phase.unwrap_or(self.spin);
        let phase = self.nodes[0].phase;
        // Tight turns fold silk toward the nearest edge-on orientation.
        let edge = FRAC_PI_2
            + ((desired_phase - FRAC_PI_2) / std::f64::consts::PI).round() * std::f64::consts::PI;
        let flat = (phase / std::f64::consts::PI).round() * std::f64::consts::PI;
        let turn_weight = (angle.abs() * twist * 0.76).clamp(0., 0.91);
        let target = desired_phase + (edge - desired_phase) * turn_weight;
        let target = target + (flat - target) * gust.max(still);
        let error = (target - phase + std::f64::consts::PI).rem_euclid(TAU) - std::f64::consts::PI;
        self.tilt += (5.5 * error - 2.85 * self.tilt) * dt;
        self.nodes[0].phase = if twist == 0. {
            0.
        } else {
            phase + self.tilt * dt
        };
        if refresh {
            for k in 0..=self.count.div_ceil(8).min(16) {
                self.pressure[k] = lanes.at(self.nodes[(8 * k).min(self.count - 1)].pos);
                self.velocities[k] = water
                    .sample(self.nodes[(8 * k).min(self.count - 1)].pos)
                    .velocity;
            }
        }
        self.sink = (self.sink + (6. * still - 16. * (1. - still)) * dt).clamp(0., 40.);
        let propagation = (self.speed + 46.) * dt / self.spacing;
        // Transport the angle independently of node positions. Read backwards
        // so a flip travels a finite distance during each step.
        for i in (1..self.count).rev() {
            let u = i as f64 / (self.count - 1) as f64;
            let prev = self.nodes[i - 1].phase;
            let node = &mut self.nodes[i];
            let wrapped = prev - node.phase + std::f64::consts::PI;
            let error = if (0.0..TAU).contains(&wrapped) {
                wrapped
            } else {
                wrapped.rem_euclid(TAU)
            } - std::f64::consts::PI;
            node.phase += error * propagation.min(0.94);
            let a = self.velocities[i / 8];
            let b = self.velocities[(i / 8 + 1).min(16)];
            let current = a + (b - a) * ((i % 8) as f64 / 8.);
            let gust = gust_front.map_or(0., |(front, left)| front.gust(node.pos, left));
            let wind = current * (1. - gust);
            let mut v = wind * ((0.15 + 0.45 * u) * (1. - 0.92 * still)) + wind_push * gust;
            v.y += (head_y - node.pos.y).clamp(-135., 135.) * gust;
            let k = i / 8;
            v += (self.pressure[k]
                + (self.pressure[(k + 1).min(16)] - self.pressure[k]) * ((i % 8) as f64 / 8.))
                * (1. - still)
                * (1. - 0.25 * gust);
            // When the water rests the slack settles below the head. Its
            // vertical drag approaches a hanging profile over several seconds.
            v.y += self.sink * u * u * still;
            // Keep loose tails in their own band as streamlines approach a shore.
            v.y += (self.lane - node.pos.y).clamp(-320., 320.) * 0.12 * u * (1. - still);
            node.pos += v * dt;
        }
        self.using_relaxed_phase = smooth_twist;
        if smooth_twist {
            // The visible cloth relaxes to a common face. Retain the transported
            // spin in the water-pressure calculation so the relaxed material
            // cannot steer the accepted hanging composition to a different lane.
            let face = (self.nodes[0].phase / std::f64::consts::PI).round() * std::f64::consts::PI;
            let mut unwrapped = self.nodes[0].phase;
            for (out, node) in self.relaxed_phase[..self.count].iter_mut().zip(&self.nodes) {
                unwrapped += (node.phase - unwrapped + std::f64::consts::PI).rem_euclid(TAU)
                    - std::f64::consts::PI;
                *out = unwrapped + (face - unwrapped) * still;
            }
            for _ in 0..3 {
                let mut left = self.relaxed_phase[0];
                for i in 1..self.count - 1 {
                    let centre = self.relaxed_phase[i];
                    self.relaxed_phase[i] +=
                        0.25 * (left + self.relaxed_phase[i + 1] - 2. * centre);
                    left = centre;
                }
            }
            // The exported outline can stride across more than one physical
            // knot near its shoulder. Reserve space for that resampling.
            let limit = 9_f64.to_radians();
            for i in 1..self.count {
                let previous = self.relaxed_phase[i - 1];
                self.relaxed_phase[i] =
                    self.relaxed_phase[i].clamp(previous - limit, previous + limit);
            }
        }
        // Smooth shear at the tail before reimposing arclength. A forward
        // projection with a turning cone cannot turn a compressed link inside
        // out. Neighbouring spans share their terminal tangent for rendering.
        let mut old = self.nodes[self.count / 2 - 1].pos;
        for i in self.count / 2..self.count - 1 {
            let point = self.nodes[i].pos;
            self.nodes[i].pos += (old + self.nodes[i + 1].pos - point * 2.) * 0.125;
            old = point;
        }
        let max_bend = if self.layer == 0 { 0.20944 } else { 0.245 };
        let calm_limit = (self.spacing / 510.).min(max_bend);
        let relaxed_limit = calm_limit + (max_bend - calm_limit) * smooth(turbulence / 0.20);
        let angle_limit =
            relaxed_limit + gust * ((self.spacing / 720.).min(relaxed_limit) - relaxed_limit);
        let bounded_body = shared_radius || departing;
        let angle_limit = if bounded_body {
            angle_limit.min(self.spacing / (if shared_radius { 1.30 } else { 0.62 } * radius))
        } else {
            angle_limit
        };
        let body_bend = angle_limit.sin_cos();
        let neck_bend = (angle_limit / (1. + still)).sin_cos();
        let drop = 190. + 70. * self.temperament;
        let length = (self.count - 1) as f64 * self.spacing;
        let sway = (self.motion_age * TAU / (6. + 4. * self.temperament) + self.id as f64).sin();
        let sigma = (0.2 * length).clamp(35., 210.);
        let bow = 0.42 * length.min(850.);
        let quiet = still * smooth(self.sink / 20.);
        let lane_axis = p(-self.course_sign, 0.);
        let curl_guard = if braid_phase.is_none() && self.knot_for <= 0. {
            // In a shared sea, different eddies pull on opposite ends of the
            // same scarf. Increase downstream tension with the space available.
            let span = (lanes.bounds.width / self.home.width - 1.).clamp(0., 1.);
            let tension = if calm { 3.8 } else { 1.1 + 3.0 * span };
            (dt * tension).min(0.65) * (1. - still)
        } else {
            0.
        };
        // Lose orbital tension as the eddy subsides. Release the coiled tail
        // before the head receives its exit pulse, while leaving the layered
        // full-strength orbit intact.
        let unwind = if departing {
            smooth(self.release_age / 5.)
        } else {
            orbit.map_or(0., |m| m.unwinding())
        };
        let mut last_old_tangent = -self.heading;
        let mut old_angle = 0.;
        let mut last_new_angle = 0.;
        let orbit_weight = orbit.map_or(0., |m| m.weight());
        // The spatial projection walks from head to tail. A strongly bent
        // body straightened by a gust must retain the previous side of each
        // turn while it relaxes; a spatial limit alone permits an entire
        // suffix to reverse within a single tick.
        let (turn_sin, turn_cos) = std::f64::consts::FRAC_PI_3.sin_cos();
        let inverse_spacing = self.spacing.recip();
        let mut dir = -self.heading;
        for i in 1..self.count {
            let prev = self.nodes[i - 1].pos;
            let delta = self.nodes[i].pos - prev;
            let mut axis = delta.unit();
            let u = i as f64 / (self.count - 1) as f64;
            if curl_guard > 0. {
                // Unsupported tails otherwise curl back on themselves even
                // when their heads are swimming in open, nearly parallel lanes.
                axis = (axis + lane_axis * (curl_guard * u)).unit();
            }
            if calm && still == 0. {
                // Set a gentle pitch on the body too. A horizontal head alone
                // does not pull a long tail clear of transverse shear.
                let pitch = axis.y.clamp(-0.38, 0.38);
                axis = p(lane_axis.x * (1. - pitch * pitch).sqrt(), pitch);
            }
            if quiet > 0. {
                // A hanging arc with a freely bending hook in the last 200 px.
                // Sway shifts the hook sideways; the tip stays below its head.
                let from_tail = (1. - u) * length;
                let hook = (-from_tail / sigma).exp();
                let baseline = -4. * bow * u * (1. - u) + drop * u;
                let hang = baseline + (0.48 * drop) * (hook - u);
                let gradient = (drop - 4. * bow * (1. - 2. * u) - 0.48 * drop) / length
                    + 0.48 * drop * hook / sigma;
                let dy = (gradient + (head_y + hang - prev.y) / 70.).clamp(-0.97, 0.97);
                let hook_x = (20. + 20. * self.temperament) * sway * hook / sigma;
                let guide = p(-self.course_sign * (1. - dy * dy).sqrt() + hook_x, dy).unit();
                let weight = (0.58 * quiet).min(0.65);
                axis = (axis * (1. - weight) + guide * weight).unit();
            }
            if let Some(m) = orbit {
                let radial = prev - m.centre;
                let distance = radial.length().max(1.);
                let outward = radial / distance;
                let correction = ((m.orbit_radius(self.layer) - distance) / 80.).clamp(-1.4, 1.4);
                let guide = (-outward.perpendicular() + outward * correction).unit();
                let weight = orbit_weight * 0.19 * (1. - unwind);
                axis = (axis * (1. - weight) + guide * weight).unit();
            }
            if let Some((front, left)) = gust_front {
                let local = front.gust(prev, left);
                let target = p(-self.course_sign, 0.);
                let weight = 0.62 * local;
                axis = (axis * (1. - weight) + target * weight).unit();
            }
            if shared_lane {
                let back = p(-self.course_sign, 0.);
                let along = axis.dot(back);
                if along < 0.12 {
                    axis = back * 0.12
                        + back.perpendicular()
                            * 0.992773891679_f64.copysign(axis.dot(back.perpendicular()));
                }
            }
            let (s, c) = if u < 0.30 { neck_bend } else { body_bend };
            let previous_dir = dir;
            dir = if axis.dot(dir) >= c {
                axis
            } else {
                let side = dir.perpendicular();
                dir * c + side * s.copysign(side.dot(axis))
            };
            let old_dir = (old_nodes[i].pos - old_nodes[i - 1].pos) * inverse_spacing;
            if dir.dot(old_dir) < turn_cos {
                let side = old_dir.perpendicular();
                dir = old_dir * turn_cos + side * turn_sin.copysign(side.dot(dir));
            }
            if bounded_body && dir.dot(previous_dir) < c {
                let side = previous_dir.perpendicular();
                dir = previous_dir * c + side * s.copysign(side.dot(dir));
            }
            if unwind > 0. {
                // Keep the winding number around the entire prior body. A
                // wrapped link can otherwise jump from +pi to -pi and turn
                // the whole suffix inside-out between the two rendered ticks.
                old_angle += turn(last_old_tangent, old_dir);
                last_old_tangent = old_dir;
                let relax = (old_angle * (1.8 * dt * unwind).min(1.)).clamp(-0.45, 0.45);
                let current = turn(old_dir, dir).clamp(-0.45, 0.45) * (1. - unwind);
                let mut next = old_angle + (current - relax).clamp(-0.55, 0.55);
                if i > 1 {
                    let lower = (last_new_angle - angle_limit).max(old_angle - 0.55);
                    let upper = (last_new_angle + angle_limit).min(old_angle + 0.55);
                    if lower <= upper {
                        next = next.clamp(lower, upper);
                    }
                }
                let (sn, cs) = (next - old_angle).sin_cos();
                dir = old_dir * cs + old_dir.perpendicular() * sn;
                last_new_angle = next;
            }
            self.nodes[i].pos = prev + dir * self.spacing;
        }
        self.motion_age += dt;
        self.light = (1. + 0.20 * gust) * (1. + 0.04 * still * (self.age * 0.32).sin());
    }
}
