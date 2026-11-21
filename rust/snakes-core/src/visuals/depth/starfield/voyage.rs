// SPDX-License-Identifier: GPL-3.0-or-later
use super::super::{
    common::rng::{Pcg32, hash64},
    world::Event,
};
use super::{Params, space::*};

#[derive(Clone, Copy, Debug)]
pub struct Pose {
    pub direction: V,
    pub p: V,
    pub attitude: Attitude,
    pub warp: f64,
    pub surge: f64,
    pub exposure: f64,
    pub bow: f64,
}
#[derive(Clone, Copy)]
struct Segment {
    duration: f64,
    start: f64,
    end: f64,
}
impl Segment {
    // Exact integral of a smoothstep velocity transition.
    fn distance(self, t: f64) -> f64 {
        let u = (t / self.duration).clamp(0., 1.);
        self.duration * (self.start * u + (self.end - self.start) * u * u * u * (1. - 0.5 * u))
    }
}
// Store the sideways distance and bend integral against longitudinal travel.
// Interpolation in distance keeps the focus of expansion continuous through
// engine ramps, even when their speed changes within one planning interval.
#[derive(Clone, Copy, Default)]
struct CoursePoint {
    position: V,
    tangent: V,
    curvature: V,
}
#[derive(Clone)]
struct Excursion {
    leg_start: f64,
    start: f64,
    amplitude: f64,
    boost: f64,
    samples: Vec<CoursePoint>,
}
impl Excursion {
    const RAMP: f64 = 27.;
    const HOLD: f64 = 2.;
    // The flight direction sweeps about 48 degrees while the camera turns only slightly.
    const SWEEP: f64 = 0.84;
    const STEP: f64 = 0.25;
    fn fraction(t: f64) -> f64 {
        cut_progress((t / Self::RAMP).clamp(0., 1.))
            * cut_progress(((2. * Self::RAMP + Self::HOLD - t) / Self::RAMP).clamp(0., 1.))
    }
    fn extra_distance(&self, t: f64) -> f64 {
        let age = t - self.start;
        let total = Self::RAMP + Self::HOLD;
        let rise = |u: f64| {
            // Integral of cut_progress on [0,1], using symmetry at the right shoulder.
            let e = 0.15;
            let q = u.min(1. - u);
            let integral = if q < e {
                q.powi(3) / (6. * e * (1. - e))
            } else {
                (0.5 * q * (q - e) + e * e / 6.) / (1. - e)
            };
            integral + (u - 0.5).max(0.)
        };
        let distance = if age < Self::RAMP {
            Self::RAMP * rise((age / Self::RAMP).max(0.))
        } else if age < total {
            age - 0.5 * Self::RAMP
        } else {
            total - Self::RAMP * rise(((total + Self::RAMP - age) / Self::RAMP).max(0.))
        };
        self.boost * distance
    }
    fn new(
        leg_start: f64,
        centre: f64,
        amplitude: f64,
        speed: f64,
        segments: &[Segment; 7],
        slope: V,
        settle: f64,
    ) -> Self {
        let duration: f64 = segments.iter().map(|s| s.duration).sum();
        let count = (duration / Self::STEP).ceil() as usize;
        let mut route = Self {
            leg_start,
            start: (centre - Self::RAMP - Self::HOLD / 2.).max(leg_start),
            amplitude,
            boost: 4.8 * speed * amplitude,
            samples: Vec::with_capacity(count + 1),
        };
        for i in 0..=count {
            let age = (i as f64 * Self::STEP).min(duration);
            let mut remaining = age;
            let mut distance = 0.;
            for s in segments {
                distance += s.distance(remaining);
                remaining -= s.duration;
                if remaining <= 0. {
                    break;
                }
            }
            let t = leg_start + age;
            let z = distance + route.extra_distance(t);
            let weight = Self::fraction(t - route.start) * amplitude;
            let yaw = (slope.0.atan() - (Self::SWEEP + 0.012) * weight).clamp(-1.43, 1.43);
            let a = distance / settle;
            let nominal = 1. - (1. + a) * (-a).exp();
            let course = (1. - amplitude) * nominal + amplitude * smooth(age / (0.85 * duration));
            let tangent = V(yaw.tan() - slope.0, course, 1.);
            route.samples.push(CoursePoint {
                position: V(0., 0., z),
                tangent,
                curvature: V::default(),
            });
        }
        // Share one curvature between neighbouring segments. Linear heading
        // interpolation makes every distant star twitch at the same instant.
        for i in 0..=count {
            let before = route.samples[i.saturating_sub(1)];
            let after = route.samples[(i + 1).min(count)];
            let span = after.position.2 - before.position.2;
            route.samples[i].curvature = (after.tangent - before.tangent) * (1. / span.max(1e-14));
        }
        for i in 1..=count {
            let left = route.samples[i - 1];
            let right = &mut route.samples[i];
            let span = right.position.2 - left.position.2;
            // Exact integral of the cubic Hermite tangent over this interval.
            let step = (left.tangent + right.tangent) * (0.5 * span)
                + (left.curvature - right.curvature) * (span * span / 12.);
            right.position.0 = left.position.0 + step.0;
            right.position.1 = left.position.1 + step.1;
        }
        route
    }
    fn travel(&self, t: f64, distance: f64) -> (V, V) {
        let i = ((t - self.leg_start).max(0.) / Self::STEP) as usize;
        if i >= self.samples.len() - 1 {
            let last = self.samples.last().unwrap();
            return (last.position, last.tangent);
        }
        self.interpolate(i, distance + self.extra_distance(t))
    }
    fn at_distance(&self, distance: f64) -> (V, V) {
        let i = self.samples.partition_point(|p| p.position.2 <= distance);
        if i >= self.samples.len() {
            let last = self.samples.last().unwrap();
            // A slow cruise may end before reaching its landmark. Continue
            // its final course when solving the bend at that farther distance.
            return (
                last.position + last.tangent * (distance - last.position.2),
                last.tangent,
            );
        }
        self.interpolate(i.saturating_sub(1), distance)
    }
    fn interpolate(&self, i: usize, distance: f64) -> (V, V) {
        let left = self.samples[i];
        let right = self.samples[i + 1];
        let span = right.position.2 - left.position.2;
        // Collapsed knots cannot support distance interpolation. The planner
        // keeps normal spans positive; retain the left knot defensively if
        // floating-point precision ever collapses an interval.
        if !span.is_finite() || span <= 0. {
            return (left.position, left.tangent);
        }
        let step = (distance - left.position.2).clamp(0., span);
        let u = step / span.max(1e-14);
        let first = left.curvature * span;
        let last = right.curvature * span;
        let delta = right.tangent - left.tangent;
        let quadratic = delta * 3. - first * 2. - last;
        let cubic = first + last - delta * 2.;
        let tangent = left.tangent + (first + (quadratic + cubic * u) * u) * u;
        let mean =
            left.tangent + (first * 0.5 + (quadratic * (1. / 3.) + cubic * (u / 4.)) * u) * u;
        (left.position + mean * step, tangent)
    }
}
fn cut_progress(t: f64) -> f64 {
    let edge = 0.15;
    if t < edge {
        t * t / (2. * edge * (1. - edge))
    } else if t > 1. - edge {
        1. - (1. - t).powi(2) / (2. * edge * (1. - edge))
    } else {
        (t - edge / 2.) / (1. - edge)
    }
}
#[derive(Clone)]
pub struct Leg {
    pub start: f64,
    pub end: f64,
    origin: V,
    slope: V,
    bend: V,
    settle: f64,
    segments: [Segment; 7],
    pub destination: Mark,
    pub serial: u64,
    steering: f64,
    flight: &'static str,
    pub previous: [Kind; 2],
    turn: Option<f64>,
    excursion: Option<Excursion>,
}
impl Leg {
    #[allow(clippy::too_many_arguments)]
    pub fn plan(
        seed: u64,
        serial: u64,
        start: f64,
        origin: V,
        slope: V,
        incoming: f64,
        previous: [Kind; 2],
        settings: &Params,
        speed: f64,
        script: &[Event],
    ) -> Self {
        let mut r = Pcg32::new(hash64(seed ^ serial.wrapping_mul(0x86ab)), 27);
        let steering = settings.steering as f64 * 0.01;
        let pace = speed.sqrt();
        let cruise_speed = speed * (0.031 + 0.014 * (1. - region(origin, seed))) / slope.length();
        let warp_speed = 0.42 * speed.powf(0.68) / slope.length();
        let mut cruise = (40. + 80. * r.unit()) / pace;
        let mut warp = (11. + 34. * r.unit()) / pace;
        let arrival = (25. + 33. * r.unit()) / pace;
        let mut cluster = false;
        if serial == 0 {
            for e in script {
                let t = e.at_ns as f64 * 1e-9;
                if e.id == 1 {
                    cruise = t.max(0.01);
                    warp = 16.75
                }
                if e.id == 2 {
                    warp = 16.75;
                    cruise = (t - 20.6).max(0.01);
                    cluster = true
                }
            }
        }
        let flight = settings.flight;
        let high = if flight == "cruise" {
            cruise_speed
        } else {
            warp_speed
        };
        let low = if flight == "hyperspace" {
            warp_speed
        } else {
            cruise_speed
        };
        let approach = if flight == "hyperspace" {
            warp_speed
        } else {
            low * 0.61
        };
        let enter = if serial == 0 { low } else { incoming };
        let mut duration = [3f64.min(cruise * 0.4), 0., 3., 0.25, warp, 0.6, arrival];
        duration[1] = (cruise - duration[0]).max(0.0001);
        let profile = |high: f64, approach: f64| {
            let speeds = [
                enter,
                low,
                low,
                low + 0.17 * (high - low),
                high,
                high,
                approach,
                approach,
            ];
            std::array::from_fn::<_, 7, _>(|i| Segment {
                duration: duration[i],
                start: speeds[i],
                end: speeds[i + 1],
            })
        };
        let flyby = |segments: &[Segment; 7]| {
            segments[..6]
                .iter()
                .map(|s| s.distance(s.duration))
                .sum::<f64>()
                + segments[6].distance(13.)
        };
        let mut segments = profile(high, approach);
        let turn = script
            .iter()
            .find(|e| e.id == 3)
            .map(|e| e.at_ns as f64 * 1e-9);
        let centre = turn
            .map(|t| t + 10.)
            .or_else(|| (cruise > 65.).then_some(start + cruise * 0.50));
        let amplitude = smooth((steering - 0.40) / 0.60);
        let distance = flyby(&segments);
        let settle = (distance * 0.16).clamp(0.45, 9.);
        let route = |segments: &[Segment; 7]| {
            centre
                .filter(|centre| {
                    settings.panoramic
                        && amplitude > 0.
                        && *centre >= start
                        && *centre < start + cruise + 10.
                })
                .map(|centre| {
                    Excursion::new(
                        start,
                        centre,
                        amplitude,
                        cruise_speed,
                        segments,
                        slope,
                        settle,
                    )
                })
        };
        let pass_time = start + duration[..6].iter().sum::<f64>() + 13.;
        let axis = slope.unit();
        let initial_route = route(&segments);
        let extra = initial_route.as_ref().map_or(V::default(), |e| {
            let path = e.travel(pass_time, distance).0;
            slope * (path.2 - distance) + V(path.0, 0., 0.)
        });
        let aim = if initial_route.is_some() {
            100. * amplitude
        } else {
            0.
        };
        let desired = origin + slope * distance + extra;
        let area = Cell::at(desired, DISTRICT);
        let mut score = f64::NEG_INFINITY;
        let mut destination = landmark(seed, area);
        // 175 real objects, queried only at a leg boundary. Penalise turns,
        // arrival-time error and the two most recently visited object classes.
        for dz in -2..=2 {
            for dy in -2..=2 {
                for dx in -3..=3 {
                    let mark = landmark(seed, area.offset(dx, dy, dz));
                    let delta = mark.p - origin;
                    let cosine = axis.dot((delta - extra).unit());
                    if delta.2 < 0.15 || cosine < (0.60 + 0.22 * steering).cos() {
                        continue;
                    }
                    let rarity = match mark.kind {
                        Kind::Sun => 0.,
                        Kind::Binary => 0.23,
                        Kind::Open => 0.46,
                        Kind::Cloud => 0.41,
                        Kind::Globular => 0.7,
                    };
                    let repeat = previous.iter().filter(|&&k| k == mark.kind).count() as f64;
                    let value = rarity + 0.8 * mark.flux
                        - 0.48 * repeat
                        - (1. - cosine) * (9. - 5. * steering + aim)
                        - (delta.2 - extra.2 - distance).abs()
                            / (approach * arrival * 0.6).max(0.6)
                        + if cluster && mark.kind == Kind::Open {
                            50.
                        } else {
                            0.
                        };
                    if value > score {
                        score = value;
                        destination = mark
                    }
                }
            }
        }
        let approach = low
            * match destination.kind {
                Kind::Open | Kind::Globular => 0.80,
                Kind::Cloud => 0.85,
                _ => 0.98,
            };
        // The luminous centre passes one wing thirteen seconds into arrival.
        // At engine cut this offset subtends 4–8 degrees at the windshield.
        let pass = V(
            (0.105 + 0.012 * r.unit()) * 13. * approach * if r.unit() < 0.5 { -1. } else { 1. },
            13. * approach * (0.012 + 0.005 * r.unit()),
            0.,
        );
        let delta = destination.p + pass - origin;
        if flight == "voyage" {
            let zero = flyby(&profile(0., approach));
            let gain = flyby(&profile(1., approach)) - zero;
            let adjusted = ((delta.2 - extra.2 - zero) / gain).max(0.08 * high);
            segments = profile(adjusted, approach);
        }
        let excursion = route(&segments);
        let (delta, gain) = if let Some(e) = &excursion {
            let path = if flight == "voyage" {
                e.travel(pass_time, flyby(&segments)).0
            } else {
                // Unlike voyage, fixed-speed flights do not adjust their
                // travel to reach the landmark at pass_time. The lateral
                // offset and bend integral must use the same longitudinal
                // distance as the slope * delta.2 term below. Mixing actual
                // travel with a farther landmark amplifies the exit slope
                // on every leg until vector normalization overflows.
                e.at_distance(delta.2.max(0.2)).0
            };
            (delta - V(path.0, 0., 0.), path.1.max(0.1))
        } else {
            (delta, bend_integral(delta.2.max(0.2), settle).max(0.1))
        };
        let turn_scale = (steering * 5.).min(1.);
        let bend = V(
            ((delta.0 - slope.0 * delta.2) / gain) * turn_scale,
            ((delta.1 - slope.1 * delta.2) / gain) * turn_scale,
            0.,
        );
        let end = start + duration.iter().sum::<f64>();
        Self {
            start,
            end,
            origin,
            slope,
            bend,
            settle,
            segments,
            destination,
            serial,
            steering,
            flight,
            previous,
            turn,
            excursion,
        }
    }
    pub fn visibility(&self, t: f64) -> f64 {
        let enter = if self.serial == 0 {
            1.
        } else {
            smooth((t - self.start) / 1.5)
        };
        enter * smooth((self.end - t) / 1.5)
    }
    pub fn warp_start(&self) -> f64 {
        self.start + self.segments[..4].iter().map(|s| s.duration).sum::<f64>()
    }
    pub fn arrival_start(&self) -> f64 {
        self.end - self.segments[6].duration
    }
    fn distance_phase(&self, t: f64) -> (f64, usize, f64) {
        let mut age = (t - self.start).max(0.);
        let mut d = 0.;
        for (i, &s) in self.segments.iter().enumerate() {
            d += s.distance(age);
            if age < s.duration || i == 6 {
                return (d, i, (age / s.duration).min(1.));
            }
            age -= s.duration;
        }
        unreachable!()
    }
    fn tangent(&self, d: f64) -> V {
        let a = d / self.settle;
        self.slope + self.bend * (1. - (1. + a) * (-a).exp())
    }
    pub fn next(&self, seed: u64, settings: &Params, speed: f64, script: &[Event]) -> Self {
        let pose = self.pose(self.end);
        let tangent = if self.excursion.is_some() {
            pose.direction * (1. / pose.direction.2)
        } else {
            self.tangent(self.distance_phase(self.end).0)
        };
        Self::plan(
            seed,
            self.serial + 1,
            self.end,
            pose.p,
            tangent,
            self.segments[6].end,
            [self.destination.kind, self.previous[0]],
            settings,
            speed,
            script,
        )
    }
    pub fn pose(&self, t: f64) -> Pose {
        let (d, phase, u) = self.distance_phase(t);
        // Position aberration is established during spool. The shutter measures
        // translation and rotation with this lens held constant at both ends.
        let (warp, exposure, surge, bow) = match self.flight {
            "hyperspace" => (1., 1., 0., 1.),
            "voyage" => match phase {
                2 => (smooth(u), 0.23 * smooth(u), 0., 0.),
                3 => (1., 0.23 + 1.37 * smooth(u), smooth(u), 0.),
                4 => {
                    let age = u * self.segments[4].duration;
                    let settle = (1. + 10. * age) * (-10. * age).exp();
                    (1., 1. + 0.60 * settle, settle, smooth(age / 1.5))
                }
                5 => {
                    let decay = 1. - cut_progress(u);
                    (decay, 1.08 * decay - 0.08, 0., decay)
                }
                6 => {
                    let q = (u * self.segments[6].duration / 0.35).min(1.);
                    // A slight reverse in magnification moves the heads a few
                    // pixels beyond the rest position before the lens settles.
                    (
                        -0.08 * (4. * q * (1. - q)).powi(2),
                        -0.08 * (1. - smooth(q)),
                        0.,
                        0.,
                    )
                }
                _ => (0., 0., 0., 0.),
            },
            _ => (0., 0., 0., 0.),
        };
        let s = self.tangent(d);
        let rate = d / self.settle * (-d / self.settle).exp();
        let mut bank = (self.bend.0 * rate * 2.2).clamp(-1., 1.) * (0.11 + 0.24 * self.steering);
        let mut yaw = s.0.atan() + self.steering * self.bend.0 * rate * 1.65;
        let mut position = self.origin + self.slope * d + self.bend * bend_integral(d, self.settle);
        if phase == 6 {
            // Track the actual landmark as it passes the wing. Higher steering
            // permits a longer look across the panorama before returning to course.
            let to = self.destination.p - position;
            let bearing = (to.0 - s.0 * to.2).atan2(to.2.abs().max(0.04));
            let envelope = 0.14 * (std::f64::consts::PI * u).sin().powi(2);
            yaw += bearing.clamp(-1., 1.) * envelope * self.steering.powf(1.45);
        }
        let mut pitch =
            (s.1 / (1. + s.0 * s.0).sqrt()).atan() + 0.18 * self.bend.1 * rate * self.steering;
        let mut direction = s.unit();
        if let Some(excursion) = &self.excursion {
            let (path, tangent) = excursion.travel(t, d);
            let travel = |v: V| V(v.0, 0., 0.) + self.bend * v.1 + self.slope * v.2;
            position = self.origin + travel(path);
            direction = travel(tangent).unit();
            let weight = Excursion::fraction(t - excursion.start) * excursion.amplitude;
            let guided_yaw = direction.0.atan2(direction.2) + Excursion::SWEEP * weight;
            let guided_pitch = direction
                .1
                .atan2(direction.0 * guided_yaw.sin() + direction.2 * guided_yaw.cos());
            yaw += excursion.amplitude * (guided_yaw - yaw);
            pitch += excursion.amplitude * (guided_pitch - pitch);
            bank *= 1. - excursion.amplitude;
        } else if let Some(t0) = self.turn {
            let u = ((t - t0) / 20.).clamp(0., 1.);
            yaw += 0.88 * (std::f64::consts::PI * u).sin().powi(2);
        }
        Pose {
            p: position,
            direction,
            attitude: Attitude::new(yaw, pitch, bank),
            warp,
            surge,
            exposure,
            bow,
        }
    }
}
fn bend_integral(d: f64, settle: f64) -> f64 {
    let a = d / settle;
    d - settle * (2. - (2. + a) * (-a).exp())
}
pub fn observatory(seed: u64) -> (V, Mark) {
    let mut best = landmark(seed, Cell(0, 0, 0));
    'search: for z in 0..4 {
        for y in -1..=1 {
            for x in -2..=2 {
                let m = landmark(seed, Cell(x, y, z));
                if m.kind == Kind::Open {
                    best = m;
                    break 'search;
                }
            }
        }
    }
    (best.p - V(0.36, 0.15, 1.24), best)
}
