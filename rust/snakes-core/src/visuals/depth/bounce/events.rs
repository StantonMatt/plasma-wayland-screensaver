use super::{BounceWorld, arena::*, body::*, spawn, world::Event};
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Kind {
    Tilt,
    Float,
    Drop,
}
#[derive(Clone, Copy)]
pub(super) struct Moment {
    pub kind: Kind,
    pub start: f64,
    pub duration: f64,
    pub direction: f64,
    pub anchor: V,
    pub room: usize,
    pub fired: bool,
}
fn ease(x: f64) -> f64 {
    let u = x.clamp(0., 1.);
    u * u * (3. - 2. * u)
}
impl BounceWorld {
    fn begin(&mut self, kind: Kind, t: f64, forced: Option<Event>) {
        let direction = forced.map_or_else(
            || if self.rng.unit() < 0.5 { -1. } else { 1. },
            |e| if e.strength < 0. { -1. } else { 1. },
        );
        let (duration, angle) = match kind {
            Kind::Tilt => (
                if let Some(e) = forced {
                    24. * e.strength.abs().clamp(0.4, 1.)
                } else {
                    between(&mut self.rng, 20., 45.)
                },
                between(&mut self.rng, 30., 55.).to_radians(),
            ),
            Kind::Float => (
                if let Some(e) = forced {
                    (30. * e.strength.abs()).clamp(25., 40.)
                } else {
                    between(&mut self.rng, 25., 40.)
                },
                1.,
            ),
            Kind::Drop => (between(&mut self.rng, 42., 60.), 1.),
        };
        let room = forced
            .and_then(|e| {
                self.rooms.iter().position(|r| {
                    r.outputs
                        .iter()
                        .any(|o| o.contains(v(e.anchor[0], e.anchor[1]).into()))
                })
            })
            .unwrap_or_else(|| self.rng.bounded(self.rooms.len() as u32).unwrap() as usize);
        let r = self.heavy_radius(room);
        let mut anchor = spawn::top_point(&self.rooms[room], r, &mut self.rng);
        if let Some(e) = forced {
            let top = self.rooms[room]
                .walls
                .iter()
                .filter(|w| w.inward.y > 0. && e.anchor[0] >= w.a.x + r && e.anchor[0] <= w.b.x - r)
                .min_by(|a, b| a.a.y.total_cmp(&b.a.y));
            if let Some(w) = top {
                anchor = v(e.anchor[0], w.a.y + r + 0.01);
            }
        }
        self.moment = Some(Moment {
            kind,
            start: t,
            duration,
            direction: direction * angle,
            anchor,
            room,
            fired: false,
        });
        self.quiet_until = t + duration + 45.;
    }
    fn heavy_radius(&self, room: usize) -> f64 {
        let short = self.rooms[room]
            .outputs
            .iter()
            .map(|o| o.width.min(o.height))
            .fold(0., f64::max);
        (2.4 * self.medians[room]).min(short * 0.30)
    }
    pub(super) fn event_step(&mut self, t: f64) {
        while self
            .script
            .first()
            .is_some_and(|e| e.at_ns <= self.rate_ns())
        {
            let e = self.script.remove(0);
            match e.id {
                1 => self.begin(Kind::Tilt, t, Some(e)),
                2 => self.begin(Kind::Float, t, Some(e)),
                3 => self.begin(Kind::Drop, t, Some(e)),
                4 => self.material_study(t, e.strength == 0.),
                5 => self.trigger_cascade(t),
                6 => {
                    for b in &mut self.balls {
                        let room = &self.rooms[b.room];
                        let Some(w) = room
                            .walls
                            .iter()
                            .filter(|wall| {
                                wall.inward.y < 0. && b.p.x >= wall.a.x && b.p.x <= wall.b.x
                            })
                            .min_by(|left, right| {
                                (left.a.y - b.p.y)
                                    .abs()
                                    .total_cmp(&(right.a.y - b.p.y).abs())
                            })
                        else {
                            continue;
                        };
                        let q = w.nearest(b.p);
                        let contact = q + w.inward * (b.r + 0.01);
                        if (contact - b.p).len2() > b.r * b.r {
                            // A forced study pose has no physical path to expose.
                            b.trail = Trail::default();
                        }
                        b.p = contact;
                        b.v = -w.inward * self.height * 1.10;
                        b.wake();
                    }
                }
                7 => {
                    // Capture a straight free flight against the production
                    // material and trail, with a known silhouette and speed.
                    let h = self.height;
                    let ball = &mut self.balls[0];
                    *ball = Ball::new(&mut self.rng, 0, 0.055 * h, Material::Rubber, 0);
                    ball.p = v(e.anchor[0], e.anchor[1]);
                    self.rooms[0].project(&mut ball.p, ball.r);
                    ball.v = v(0., h * e.strength.clamp(0., 1.));
                    ball.spin = 0.;
                    ball.birth = t - 2.;
                }
                8 => {
                    // Photometric control: identical pose and spin on the two
                    // sides of an isolated lighting impulse, without squash.
                    for ball in &mut self.balls {
                        ball.start_impact_light(e.strength.clamp(0., 1.));
                    }
                }
                _ => {}
            }
        }
        if self.moment.is_none()
            && self.params.worldEvents
            && t >= self.quiet_until
            && let Some(i) = (0..3)
                .filter(|&i| t >= self.due[i])
                .min_by(|&a, &b| self.due[a].total_cmp(&self.due[b]))
        {
            let (kind, lo, hi) = [
                (Kind::Tilt, 240., 480.),
                (Kind::Float, 360., 720.),
                (Kind::Drop, 600., 1200.),
            ][i];
            self.due[i] = t + between(&mut self.rng, lo, hi);
            self.begin(kind, t, None);
        }
        let Some(m) = self.moment else { return };
        let age = t - m.start;
        if m.kind == Kind::Drop && age >= 1.5 && !m.fired {
            let r = self.heavy_radius(m.room);
            let mut ball = Ball::new(&mut self.rng, m.room, r, Material::Metal, 1);
            ball.birth = self.sim_time;
            ball.p = m.anchor;
            ball.heavy = true;
            ball.v = v(0., self.height * 1.10);
            ball.alpha = 0.01;
            self.balls.truncate(self.params.count as usize);
            self.balls.push(ball);
            self.moment.as_mut().unwrap().fired = true;
        }
        if m.kind == Kind::Float && age >= 2.7 && !m.fired {
            self.moment.as_mut().unwrap().fired = true;
            let up = if self.base_g < 0. { 1. } else { -1. };
            for b in &mut self.balls {
                b.wake();
                let angle = between(&mut self.rng, -1.047, 1.047);
                let speed = between(&mut self.rng, 140., 280.);
                b.v = v(angle.sin(), up * angle.cos()) * speed + b.v.limit(95.) * 0.35;
                b.spin += between(&mut self.rng, -2., 2.);
            }
        }
        if m.kind == Kind::Float
            && self.base_g != 0.
            && age >= 3.
            && age < m.duration - 1.5
            && self.tick.is_multiple_of(30)
            && self
                .balls
                .iter()
                .filter(|b| b.v.len2() < 120_f64.powi(2))
                .count()
                * 2
                >= self.balls.len()
        {
            // A quiet billiards table recovers its drift gradually. Scatter
            // sleepy directions rather than aiming the whole room at one wall.
            for b in self
                .balls
                .iter_mut()
                .filter(|b| b.v.len2() < 120_f64.powi(2))
            {
                let speed = b.v.len();
                let angle = if speed > 12. {
                    b.v.y.atan2(b.v.x) + between(&mut self.rng, -0.25, 0.25)
                } else {
                    between(&mut self.rng, 0., std::f64::consts::TAU)
                };
                let mut direction = v(angle.cos(), angle.sin());
                let (wall, _, distance) = self.rooms[b.room].nearest(b.p);
                if distance - b.r < 16. && direction.dot(wall.inward) < 0. {
                    direction -= wall.inward * (2. * direction.dot(wall.inward));
                }
                b.v = direction * (speed + 48.).min(158.);
                b.wake();
            }
        }
        if m.kind == Kind::Tilt && !m.fired {
            self.moment.as_mut().unwrap().fired = true;
            for b in &mut self.balls {
                b.wake();
            }
        }
        if m.kind == Kind::Drop && self.balls.len() > self.params.count as usize {
            self.balls.last_mut().unwrap().alpha =
                ease((m.duration - age) / 1.2).min(ease((age - 1.5) / 0.16));
        }
        if age >= m.duration {
            if m.kind == Kind::Drop {
                self.balls.truncate(self.params.count as usize);
            }
            self.moment = None;
        }
    }
    // Capture cue: launch the lower member of the closest pile towards its
    // neighbour. All subsequent transfers use the normal contact solver.
    fn trigger_cascade(&mut self, t: f64) {
        let g = self.gravity(t);
        let up = if g.len2() > 4. {
            -g / g.len()
        } else {
            v(0., -1.)
        };
        let mut best = f64::INFINITY;
        let mut trigger = None;
        for (i, lower) in self.balls.iter().enumerate() {
            for upper in &self.balls {
                let delta = upper.p - lower.p;
                if lower.room != upper.room || delta.dot(up) <= 0. {
                    continue;
                }
                let distance = delta.len();
                let direction = delta / distance;
                let score = distance / (lower.r + upper.r) + 1.
                    - direction.dot(up)
                    - if upper.sleeping { 0.4 } else { 0. };
                if score < best {
                    best = score;
                    trigger = Some((i, direction));
                }
            }
        }
        for b in &mut self.balls {
            if b.sleeping {
                b.charge = b.period * 0.94;
            }
        }
        if let Some((i, direction)) = trigger {
            let b = &mut self.balls[i];
            b.hop(g, self.height - 2. * b.r, &mut self.rng);
            b.v = direction * b.v.len();
        }
    }
    fn material_study(&mut self, t: f64, still: bool) {
        let w = self.input.layout.bounds;
        for (i, b) in self.balls.iter_mut().take(3).enumerate() {
            let r = (w.width * 0.105).min(w.height * 0.21);
            // The stationary palette sheet includes deep ember and jade
            // enamel so the black-background colour lift is visible.
            let pigment = if still && i == 0 {
                match self.input.palette {
                    0 => 1,
                    2 => 4,
                    3 => 3,
                    _ => 0,
                }
            } else {
                i
            };
            *b = Ball::new(
                &mut self.rng,
                0,
                r,
                [Material::Rubber, Material::Glass, Material::Metal][i],
                pigment,
            );
            b.p = v(w.x + w.width * (i as f64 + 0.5) / 3., w.y + w.height * 0.48);
            b.v = if still {
                V::default()
            } else {
                v(0., (w.bottom() - r - b.p.y) / 3.)
            };
            b.spin = 0.28 * (i as f64 + 1.);
            b.birth = t - 2.;
        }
    }
    pub(super) fn gravity(&self, t: f64) -> V {
        let g = self.base_g;
        let Some(m) = self.moment else {
            return v(0., g);
        };
        let age = t - m.start;
        match m.kind {
            Kind::Tilt => {
                let a = m.direction * ease(age / 4.) * ease((m.duration - age) / 4.);
                v(g.abs() * a.sin(), g * a.cos())
            }
            Kind::Float => {
                let strength = ease(age / 3.) * ease((m.duration - age) / 1.5);
                if g == 0. {
                    v(0.145 * self.height * strength * m.direction, 0.)
                } else {
                    v(0., g * (1. - strength))
                }
            }
            Kind::Drop => v(0., g),
        }
    }
}
