use super::{Pcg32, space::*};

pub(super) const FIELD: usize = usize::MAX;
const MAX_MEMBERS: usize = 10;
const TRAIL_SAMPLES: usize = 60;

#[derive(Clone, Copy)]
pub(super) struct Member {
    pub offset: Point,
    pub parent: usize,
    pub star: usize,
}
#[derive(Clone)]
pub(super) struct Association {
    pub p: Point,
    pub nursery: Point,
    pub v: Point,
    current: Point,
    pub angle: f32,
    pub spin: f32,
    pub basis: [Point; 2],
    pub members: [Member; MAX_MEMBERS],
    pub age: f32,
    pub life: f32,
    pub spring: f32,
    pub active: bool,
    pub gather_partner: usize,
    pub gather_offset: Point,
    tide: Point,
    trail: [Point; TRAIL_SAMPLES],
    left_site: [bool; TRAIL_SAMPLES],
    trail_count: usize,
    trail_head: usize,
    pub weight: f32,
}
impl Association {
    pub fn new(p: Point, n: usize, reach: f32, rng: &mut Pcg32) -> Self {
        let empty = Member {
            offset: Point::default(),
            parent: FIELD,
            star: FIELD,
        };
        let mut members = [empty; MAX_MEMBERS];
        let n = n.clamp(1, MAX_MEMBERS);
        let mut directions = [0.; MAX_MEMBERS];
        directions[0] = rand(rng, -0.65, 0.65);
        // A short walk with occasional side branches. Grow away from older ink;
        // restart a stuck walk instead of adding a lattice point to fill a hole.
        let mut i = 1;
        let mut attempts = 0;
        let mut right_angles = 0;
        while i < n {
            attempts += 1;
            if attempts == 1024 {
                // A dense walk can exhaust the trial budget. Alternating a
                // shallow opening angle leaves a guaranteed unobstructed spine.
                for j in 1..n {
                    let side = if j % 2 == 0 { 0.34 } else { -0.34 };
                    members[j] = Member {
                        offset: members[j - 1].offset
                            + Point::polar(directions[0] + side) * reach * 0.80,
                        parent: j - 1,
                        star: FIELD,
                    };
                }
                break;
            }
            if attempts % 128 == 0 {
                i = 1;
                right_angles = 0;
            }
            let parent = if i > 2 && rng.unit() < 0.25 {
                (rng.unit() * (i - 1) as f64) as usize
            } else {
                i - 1
            };
            let turn = rand(rng, 20., 70.).to_radians() * if rng.unit() < 0.5 { -1. } else { 1. };
            let heading = directions[parent] + turn;
            let axis = Point::polar(heading);
            let a = members[parent].offset;
            let b = a + axis * (rand(rng, 0.45, 0.77) * reach);
            if members[..i]
                .iter()
                .any(|m| (b - m.offset).len2() < (reach * 0.40).powi(2))
            {
                continue;
            }
            let mut right = 0;
            let mut valid = true;
            for (j, member) in members[1..i].iter().enumerate() {
                let c = member.offset;
                let d = members[member.parent].offset;
                let other = (c - d).unit();
                let dot = other.dot(axis).abs();
                if member.parent == parent || j + 1 == parent {
                    if dot < 0.208 {
                        right += 1;
                    }
                    if member.parent == parent && other.dot(axis) > 0.70 {
                        valid = false;
                    }
                } else if dot > 0.966 && (a.mix(b, 0.5) - c.mix(d, 0.5)).len() < 1.2 * reach {
                    valid = false;
                } else {
                    let cross = |u: Point, v: Point| u.x * v.y - u.y * v.x;
                    if cross(b - a, c - a) * cross(b - a, d - a) <= 0.
                        && cross(d - c, a - c) * cross(d - c, b - c) <= 0.
                    {
                        valid = false;
                    }
                }
            }
            if !valid || right_angles + right > 1 {
                continue;
            }
            right_angles += right;
            members[i] = Member {
                offset: b,
                parent,
                star: FIELD,
            };
            directions[i] = heading;
            i += 1;
        }
        let mean = members[..n]
            .iter()
            .fold(Point::default(), |sum, m| sum + m.offset)
            / n as f32;
        // Express the walk along its longest axis. Gravity stretches along its
        // spine even if the walk happened to turn a corner at the first star.
        let (mut xx, mut xy, mut yy) = (0., 0., 0.);
        for m in &members[..n] {
            let d = m.offset - mean;
            xx += d.x * d.x;
            xy += d.x * d.y;
            yy += d.y * d.y;
        }
        let longitudinal = Point::polar(0.5 * (2. * xy).atan2(xx - yy));
        for m in &mut members[..n] {
            let d = m.offset - mean;
            m.offset = pt(
                d.dot(longitudinal),
                d.dot(pt(-longitudinal.y, longitudinal.x)),
            );
        }
        let angle = rand(rng, 0., std::f32::consts::TAU);
        Self {
            p,
            v: Point::default(),
            current: Point::default(),
            nursery: p,
            angle,
            spin: 0.,
            basis: [
                Point::polar(angle),
                Point::polar(angle + std::f32::consts::FRAC_PI_2),
            ],
            trail: [p; TRAIL_SAMPLES],
            left_site: [false; TRAIL_SAMPLES],
            trail_count: 0,
            trail_head: 0,
            weight: n as f32,
            members,
            age: 0.,
            life: rand(rng, 480., 1200.),
            spring: 1.,
            active: true,
            gather_partner: FIELD,
            gather_offset: Point::default(),
            tide: Point::default(),
        }
    }
    pub fn update_basis(&mut self, g: f32, shrink: f32) {
        let width = if g < 0.4 {
            1.55 - 0.625 * g
        } else {
            1.30 - (g - 0.4) * 0.833333
        };
        let length = if g < 0.4 {
            1. + 0.375 * g
        } else {
            1.15 + (g - 0.4) * 0.75
        };
        let axis = Point::polar(self.angle);
        self.basis = [
            axis * (length * shrink),
            pt(-axis.y, axis.x) * (width * shrink),
        ];
    }
    pub fn position(&self, offset: Point) -> Point {
        self.p + self.basis[0] * offset.x + self.basis[1] * offset.y
    }
}
impl super::ConstellationWorld {
    pub(super) fn move_groups(&mut self, t: f32, dt: f32, warm: bool) {
        let g = self.params.gravity as f32 / 100.;
        let elapsed = dt * self.drift * (1. - 0.8 * self.calm);
        let refresh = warm || self.tick % 15 == 1;
        let bounds = self.input.layout.bounds;
        for i in 0..self.groups.len() {
            if !self.groups[i].active {
                continue;
            }
            if refresh {
                let p = self.groups[i].p;
                let flow = |p| curl(p, t * self.drift, self.input.seed);
                let current = flow(p);
                let spin = ((flow(p + pt(32., 0.)).y - current.y)
                    - (flow(p + pt(0., 32.)).x - current.x))
                    / 64.;
                let mut v = current.unit() * (0.8 * current.len()).clamp(1.5, 2.6);
                let mut pull = Point::default();
                let mut mass = 0f32;
                let mut closest = f32::INFINITY;
                let mut toward = Point::default();
                for (j, group) in self.groups.iter().enumerate() {
                    if i == j || !group.active {
                        continue;
                    }
                    let d = group.p - p;
                    let r = d.len();
                    // Spacing is strongest in an unbound field. Gravity gives a
                    // neighbouring pair persistent relative velocity on the 12s
                    // drag timescale, leaving isolated associations in the void.
                    v += d.unit()
                        * (-0.4
                            * (1. - g)
                            * smooth((700. - r) / (160. + 160. * (g / 0.4).min(1.))));
                    if r < 900. {
                        let well = smooth((900. - r) / 320.) * (group.weight / 6.).powi(3);
                        pull += d.unit() * (well * ((r - 175.) / 130.).clamp(-1., 1.));
                        mass = mass.max(well);
                    }
                    if r < closest {
                        closest = r;
                        toward = d.unit();
                    }
                }
                if closest < 900. {
                    v += toward
                        * (3.6
                            * g
                            * smooth((900. - closest) / 220.)
                            * ((closest - 230.) / 100.).clamp(-1., 1.));
                }
                let bound = smooth((g - 0.45) / 0.55);
                v += pull * (bound * 4.8 / mass.max(1.));
                // The current has temporal eddies. Retain a coarse streamline
                // history so a newly reversed eddy diverts the group to fresh sky.
                let a = &self.groups[i];
                let mut avoidance = Point::default();
                for k in (8..a.trail_count).step_by(2) {
                    let site = a.trail[(a.trail_head + TRAIL_SAMPLES - 1 - k) % TRAIL_SAMPLES];
                    let distance = (p - site).len();
                    avoidance += (p - site).unit() * smooth((340. - distance) / 220.);
                }
                v += avoidance * (1.3 / avoidance.len().max(1.));
                let mut tide = Point::default();
                if let Some(partner) = self
                    .groups
                    .get(self.groups[i].gather_partner)
                    .filter(|p| p.active)
                {
                    let d = partner.p - p;
                    tide = d.unit() * (self.gather * ((d.len() - 245.) * 0.042).clamp(-5., 12.));
                    // Store the displacement due to the tide separately from
                    // advection. After the gathering the current releases it.
                    tide += self.groups[i].gather_offset * (-(1. - self.gather) / 47.);
                }
                self.groups[i].tide = tide;
                v += tide;
                let edge = |x: f32, lo: f32, size: f32| {
                    let margin = (size * 0.19).min(240.);
                    5. * (smooth((lo + margin - x) / margin)
                        - smooth((x - lo - size + margin) / margin))
                };
                v += pt(
                    edge(p.x, bounds.x as f32, bounds.width as f32),
                    edge(p.y, bounds.y as f32, bounds.height as f32),
                );
                // Spent nurseries are pressure gradients, not destinations.
                // A dispersed group leaves a local cavity until its gas has
                // mixed back into the field. Strong tides can cross it.
                for &(site, until) in &self.spent {
                    let d = p - site;
                    let r = d.len();
                    if r < 600. {
                        // A loose, zero-gravity association leaves no bound
                        // cloud to disperse. At high gravity the tide dominates.
                        let strength =
                            smooth((until - t) * self.pace / 180.) * (g / 0.4).min(1.) * (1. - g);
                        v += d.unit() * (5.6 * strength * smooth((600. - r) / 315.));
                    }
                }
                let a = &mut self.groups[i];
                for k in 0..a.trail_count {
                    let d = p - a.trail[k];
                    let r = d.len();
                    // Remember departure from this particular patch even if a
                    // reversing eddy later brings the whole path within 400px.
                    a.left_site[k] |= r > 280.;
                    if a.left_site[k] && r < 300. {
                        let normal = d.unit();
                        let escape = ((235. - r) * 0.085 - v.dot(normal)).max(0.);
                        v += normal * escape;
                    }
                }
                if g > 0.5 && self.gather == 0. {
                    let max_velocity = 2.50 + 8. * (1. - g);
                    v = v * (max_velocity / v.len().max(max_velocity));
                }
                self.groups[i].current = v;
                self.groups[i].spin = spin.clamp(-0.01, 0.01);
            }
            let a = &mut self.groups[i];
            a.v = a.v.mix(a.current, 1. - (-elapsed / 4.).exp());
            a.p += a.v * elapsed;
            a.gather_offset += a.tide * elapsed;
            if !warm && self.tick.is_multiple_of(900) {
                a.trail[a.trail_head] = a.p;
                a.left_site[a.trail_head] = false;
                a.trail_head = (a.trail_head + 1) % TRAIL_SAMPLES;
                a.trail_count = (a.trail_count + 1).min(TRAIL_SAMPLES);
            }
            a.angle += a.spin * elapsed;
            if !warm && self.params.lifeCycle {
                a.age += dt * self.pace;
            }
            a.spring = smooth((a.life - a.age) / 90.);
            let shrink = 1.
                - if a.gather_partner != FIELD {
                    0.43 * self.gather
                } else {
                    0.
                };
            a.update_basis(g, shrink);
            if a.age >= a.life {
                a.active = false;
                self.spent.push((a.nursery, t + 1500. / self.pace));
                for s in &mut self.stars {
                    if s.group == i {
                        s.group = FIELD;
                        s.parent = FIELD;
                    }
                }
            }
        }
    }
}
