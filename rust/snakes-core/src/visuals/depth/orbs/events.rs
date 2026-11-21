use super::*;
#[derive(Clone, Copy, PartialEq, Debug)]
#[repr(u32)]
pub enum Kind {
    Dandelion = 1,
    Procession,
    Foam,
    Surfacing,
}
impl Kind {
    pub fn from_id(id: u32) -> Option<Self> {
        Some(match id {
            1 => Self::Dandelion,
            2 => Self::Procession,
            3 => Self::Foam,
            4 => Self::Surfacing,
            _ => return None,
        })
    }
}
#[derive(Clone, Copy)]
struct ReturnBend {
    seam: f64,
    radius: f64,
    side: f64,
}
pub(super) struct Moment {
    pub kind: Kind,
    pub age: f64,
    pub heart: usize,
    pub anchor: Vec2,
    pub screen: Rect,
    targets: Vec<(usize, u32, Vec2)>,
    axis: f64,
    pub bloomed: bool,
    pub calyx: Option<Orb>,
    cascade: usize,
    bends: Vec<Option<ReturnBend>>,
}
impl Colony {
    fn start_event(&mut self, kind: Kind, point: Vec2) {
        if !self.params.worldEvents || (kind == Kind::Surfacing && self.params.depth == 0) {
            return;
        }
        let visible = self.input.layout.nearest_visible(point);
        let screen = self
            .input
            .layout
            .outputs
            .iter()
            .find(|o| o.rect.contains(visible))
            .unwrap_or(&self.input.layout.outputs[0])
            .rect;
        let anchor = p(
            visible.x.clamp(
                screen.x + 0.25 * screen.width,
                screen.x + 0.75 * screen.width,
            ),
            visible.y.clamp(
                screen.y + 0.25 * screen.height,
                screen.y + 0.75 * screen.height,
            ),
        );
        let mut members: Vec<_> = self
            .orbs
            .iter()
            .enumerate()
            .filter(|(_, o)| o.alive() && screen.contains(o.pos))
            .map(|(i, _)| i)
            .collect();
        members.sort_by(|&a, &b| {
            let cost = |o: Orb| (o.pos - anchor).length() + (o.z - 0.55).abs() * self.height;
            cost(self.orbs[a]).total_cmp(&cost(self.orbs[b]))
        });
        let Some(&heart) = members.first() else {
            return;
        };
        let mut targets = Vec::new();
        let axis = self.rng.unit() * TAU;
        let bounds = self.input.layout.bounds;
        for orb in &mut self.orbs {
            orb.stream = false;
        }
        let mut bends = Vec::new();
        if kind == Kind::Procession {
            bends = self.recruit_trains(axis);
            // Dust already suspended near the stream lights up first. Sparse
            // new tracer motes reveal the current between isolated pearls.
            for j in 0..48 {
                let Some(slot) = self.motes.iter().position(|m| m.life <= 0.) else {
                    break;
                };
                let x = bounds.x + bounds.width * (j as f64 + self.rng.unit()) / 48.;
                let pos = p(
                    x,
                    line(bounds, self.height, x, axis) + range(&mut self.rng, -60., 60.),
                );
                if self.input.layout.contains(pos) {
                    self.seed_mote(slot, pos, 0.60, 0.46, 0., p(7., 0.));
                    self.motes[slot].cue = true;
                }
            }
        }
        if kind == Kind::Foam {
            // Hexagonal rings provide one rest position per member of the raft.
            let mean_mass =
                members.iter().map(|&i| self.orbs[i].mass).sum::<f64>() / members.len() as f64;
            let spacing = self.base * mean_mass.sqrt() * 2.10;
            let mut cells = vec![(0, 0)];
            for radius in 1..=8 {
                let (mut x, mut y) = (radius, 0);
                for (dx, dy) in [(0, -1), (-1, 0), (-1, 1), (0, 1), (1, 0), (1, -1)] {
                    for _ in 0..radius {
                        cells.push((x, y));
                        x += dx;
                        y += dy;
                    }
                }
            }
            for (i, (x, y)) in members.into_iter().zip(cells) {
                targets.push((
                    i,
                    self.orbs[i].id,
                    p(
                        (x as f64 + y as f64 * 0.5) * spacing,
                        y as f64 * 0.866 * spacing,
                    ),
                ));
            }
        }
        self.event = Some(Moment {
            kind,
            age: 0.,
            heart,
            screen,
            anchor,
            targets,
            axis,
            cascade: 0,
            bends,
            bloomed: false,
            calyx: None,
        });
    }
    fn recruit_trains(&mut self, angle: f64) -> Vec<Option<ReturnBend>> {
        let mut bends = Vec::new();
        let bounds = self.input.layout.bounds;
        let scale = self.height / 1440.;
        let mut screens: Vec<_> = self.input.layout.outputs.iter().map(|o| o.rect).collect();
        screens.sort_by(|a, b| a.x.total_cmp(&b.x));
        // A current gathers water on both sides of each open bezel. A small
        // receiving screen therefore contributes some of the returning beads.
        let mut seams: Vec<_> = screens
            .windows(2)
            .filter(|pair| {
                (pair[0].right() - pair[1].x).abs() < 1.
                    && pair[0].y < pair[1].bottom()
                    && pair[1].y < pair[0].bottom()
            })
            .map(|pair| (pair[0], pair[1]))
            .take(3)
            .collect();
        let local_train = seams.is_empty();
        if local_train {
            let r = screens
                .iter()
                .copied()
                .max_by(|a, b| a.area().total_cmp(&b.area()))
                .unwrap();
            seams.push((
                Rect {
                    width: r.width * 0.55,
                    ..r
                },
                Rect {
                    x: r.x + r.width * 0.55,
                    width: r.width * 0.45,
                    ..r
                },
            ));
        }
        for (train, (left, right)) in seams.into_iter().enumerate() {
            let seam = right.x;
            let mut station = seam - (680. * scale).min(left.width * 0.36);
            let mut previous_radius = 0.;
            let capacity = |o: &Orb| {
                o.alive()
                    && !o.stream
                    && (0.25..=0.85).contains(&o.z)
                    && o.partner.is_none()
                    && o.split == 0.
                    && o.shiver == 0.
            };
            let local = self
                .orbs
                .iter()
                .filter(|o| capacity(o) && (left.contains(o.pos) || right.contains(o.pos)))
                .count();
            let upstream = self
                .orbs
                .iter()
                .filter(|o| capacity(o) && left.contains(o.pos))
                .count();
            let count = (local / 4).clamp(3, if upstream < 20 { 6 } else { 7 });
            let receiving = self
                .orbs
                .iter()
                .filter(|o| capacity(o) && right.contains(o.pos))
                .count();
            let contribution = (count / 2).min(receiving * 27 / 100);
            let mut spacing = 0.;
            for rank in 0..count {
                let source = if rank < contribution { right } else { left };
                let best = self
                    .orbs
                    .iter()
                    .enumerate()
                    .filter(|(_, o)| capacity(o) && source.contains(o.pos))
                    .min_by(|(_, a), (_, b)| {
                        let cost = |o: &Orb| {
                            let x = station
                                - 1.40 * (previous_radius + o.radius.min(self.height * 0.155));
                            let dy = o.pos.y - line(bounds, self.height, x, angle);
                            (o.pos.x - x).hypot(dy * 1.55)
                                + 0.7 * o.radius
                                + 110. * f64::from(o.links)
                        };
                        cost(a).total_cmp(&cost(b))
                    })
                    .map(|(i, _)| i);
                let Some(i) = best else {
                    continue;
                };
                for k in (0..self.links.len()).rev() {
                    if self.links[k].a == i || self.links[k].b == i {
                        self.unlink(k);
                    }
                }
                let radius = self.luminous_radius(self.orbs[i], self.orbs[i].z);
                let o = &mut self.orbs[i];
                if previous_radius > 0. {
                    let gap = 1.37 * (previous_radius + radius);
                    station -= gap;
                    spacing += gap;
                }
                o.stream = true;
                o.train = train;
                o.stream_station = station;
                o.release_at = 35.2 + 9.4 * rank as f64 / (count - 1) as f64;
                o.cooldown = o.release_at + 3.;
                previous_radius = radius;
            }
            // On a narrow single output, keep the tail inside its own basin.
            let inset = left.x + previous_radius - station;
            if inset > 0. {
                for o in self
                    .orbs
                    .iter_mut()
                    .filter(|o| o.stream && o.train == train)
                {
                    o.stream_station += inset;
                }
            }
            // At the downstream margin the stream turns back into the water.
            // The length of its loop reflects the receiving colony's stock.
            let terminal = local_train || (right.right() - bounds.right()).abs() < 1.;
            let mean_gap = spacing / (count - 1) as f64;
            // A short receiving screen can be smaller than the colony's
            // median-height minimum, so let its available room cap both bounds.
            let max_reach = 0.17 * right.height;
            let reach = (mean_gap * (contribution as f64 + 1.3) / std::f64::consts::PI)
                .clamp((0.07 * self.height).min(max_reach), max_reach);
            let side = if line(bounds, self.height, seam, angle) < right.centre().y {
                1.
            } else {
                -1.
            };
            bends.push(terminal.then_some(ReturnBend {
                seam,
                radius: reach,
                side,
            }));
        }
        bends
    }
    pub(super) fn surface(&self) -> f64 {
        self.event
            .as_ref()
            .filter(|e| e.kind == Kind::Surfacing)
            .map_or(0., |e| ease((e.age - 3.) / 15.) * ease((40. - e.age) / 15.))
    }
    pub(super) fn update_events(&mut self, dt: f64) {
        let ns = self.rate().tick_ns(self.tick);
        while self.script.first().is_some_and(|e| e.at_ns <= ns) {
            let e = self.script.remove(0);
            let pos = p(e.anchor[0], e.anchor[1]);
            if let Some(kind) = Kind::from_id(e.id) {
                self.start_event(kind, pos);
            } else {
                self.begin_lifecycle(e.id, pos);
            }
            self.next_event = self.time + range(&mut self.rng, 300., 600.);
        }
        if self.time >= self.next_event && self.event.is_none() {
            let kind = Kind::from_id(1 + self.rng.bounded(4).unwrap()).unwrap();
            let pos = self.point();
            self.start_event(kind, pos);
            self.next_event = self.time + range(&mut self.rng, 300., 600.);
        }
        let mut split = None;
        let mut foam_release = None;
        if let Some(e) = self.event.as_mut() {
            e.age += dt;
            match e.kind {
                Kind::Dandelion => {
                    let h = self.orbs[e.heart];
                    let height = e.screen.height;
                    let gather = ease((e.age - 2.) / 6.) * ease((28. - e.age) / 5.);
                    let clearance = ease((e.age - 32.) / 8.) * ease((58. - e.age) / 6.);
                    for (i, o) in self.orbs.iter().enumerate() {
                        if !o.alive() || !e.screen.contains(o.pos) || o.bloom_travel > 0. {
                            continue;
                        }
                        let d = o.pos - e.anchor;
                        if i == e.heart && !e.bloomed {
                            continue;
                        }
                        let donor = !e.bloomed && (o.z - h.z).abs() < 0.095;
                        if donor && gather > 0. && d.length() < (h.radius + o.radius) * 10. {
                            self.forces[i] +=
                                (-d * 0.60 - o.vel * 0.64).limited(height * 0.15) * gather;
                        }
                        // Water displaced by the swelling heart returns along
                        // its flanks. At the ceiling and floor the outgoing
                        // current turns sideways before reaching the glass.
                        let radius = (0.72 * height).min(0.40 * e.screen.width);
                        let outward = ease((radius + 2. * o.radius - d.length()) / (height * 0.18));
                        let side = if d.x < 0. { -1. } else { 1. };
                        let shore =
                            ((o.pos.y - e.screen.centre().y) / (0.5 * height)).clamp(-1., 1.);
                        let mut current = d.unit() * (height * 0.10);
                        current.x += side * height * (0.07 + 0.09 * shore.abs());
                        current.y *= 1. - shore.abs();
                        current.y -= shore.powi(3) * height * 0.27;
                        let pass = if donor {
                            ease((e.age - 25.) / 5.)
                        } else {
                            ease(e.age / 9.)
                        };
                        self.forces[i] += (current - o.vel) * outward * pass.max(clearance) * 0.9;
                        // The same return flow lifts the colony off both edges,
                        // including the part just outside the opening bloom.
                        self.forces[i].y -= shore.powi(5) * height * 0.19 * ease(e.age / 5.);
                    }
                    if e.age >= 40. && !e.bloomed && h.alive() {
                        e.bloomed = true;
                        e.calyx = Some(h);
                        split = Some(e.heart);
                    }
                }
                Kind::Procession => {
                    let b = self.input.layout.bounds;
                    let t = e.age;
                    let ramp = 1. - (-t / 2.).exp();
                    let speed = 58. * ramp;
                    let distance = 58. * (t - 2. * ramp);
                    for (i, o) in self.orbs.iter_mut().enumerate() {
                        if !o.stream || !o.alive() {
                            continue;
                        }
                        if t >= o.release_at {
                            o.stream = false;
                            o.home = o.pos + (o.home - o.pos) * 0.36;
                            continue;
                        }
                        let x = o.stream_station + distance * self.height / 1440.;
                        let path = |station: f64| {
                            let angle = e.axis - t * 0.018;
                            if let Some(ReturnBend { seam, radius, side }) = e.bends[o.train]
                                && station > seam
                            {
                                let angle_along =
                                    ((station - seam) / radius).min(std::f64::consts::PI);
                                let runout =
                                    (station - seam - std::f64::consts::PI * radius).max(0.);
                                // The current accelerates through the turn. Its longer
                                // arc keeps adjacent membranes apart on the inside.
                                let bend_radius = radius * 1.19;
                                let nx = seam + bend_radius * angle_along.sin() - runout;
                                return p(
                                    nx,
                                    line(b, self.height, nx, angle)
                                        + side * bend_radius * (1. - angle_along.cos()),
                                );
                            }
                            p(station, line(b, self.height, station, angle))
                        };
                        let turn = ease((o.release_at - t) / 3.);
                        let target = path(x);
                        let tangent = (path(x + 2.) - target) * (0.5 * speed * self.height / 1440.);
                        let desired =
                            tangent + ((target - o.pos) * 0.92).limited(self.height * 0.27);
                        self.forces[i] = (desired - o.vel) * 2.5 * ease(t / 3.5) * turn;
                    }
                    for m in &mut self.motes {
                        if !m.cue || m.life <= 0. {
                            continue;
                        }
                        let dy = line(b, self.height, m.pos.x, e.axis - e.age * 0.018) - m.pos.y;
                        m.vel += (p(64., dy * 0.8) - m.vel) * (dt * 1.3 * ease(e.age / 3.));
                    }
                }
                Kind::Foam => {
                    let strength = ease(e.age / 5.) * ease((47. - e.age) / 5.);
                    let (s, c) = (e.age * 0.024).sin_cos();
                    let phase = self.orbs[e.heart].phase;
                    let valid = e.targets.iter().filter_map(|&(i, id, _)| {
                        (self.orbs[i].alive() && self.orbs[i].id == id).then_some(self.orbs[i])
                    });
                    let mean =
                        valid.clone().map(|o| o.mass).sum::<f64>() / valid.count().max(1) as f64;
                    for &(i, id, offset) in &e.targets {
                        let o = &mut self.orbs[i];
                        if !o.alive() || o.id != id {
                            continue;
                        }
                        let target =
                            e.anchor + p(offset.x * c - offset.y * s, offset.x * s + offset.y * c);
                        self.forces[i] += ((target - o.pos) * 1.6 - o.vel * 1.55)
                            .limited(self.height * 0.14)
                            * strength;
                        o.z += (0.55 - o.z) * dt * 1.4 * ease(e.age);
                        if e.age < 39. {
                            o.mass += (mean - o.mass) * dt * 0.34;
                        }
                        o.recalc(self.base);
                        self.coupling[i] += (phase - o.phase).sin() * 0.5 * strength;
                    }
                    if e.age >= 39.3 + e.cascade as f64 * 0.57
                        && e.cascade < 6
                        && self.params.merging > 0
                    {
                        foam_release = Some(e.screen);
                        e.cascade += 1;
                    }
                }
                Kind::Surfacing => {}
            }
            let duration = match e.kind {
                Kind::Dandelion => 60.,
                Kind::Procession => 45.,
                Kind::Foam => 50.,
                Kind::Surfacing => 40.,
            };
            if e.age > duration {
                self.event = None;
                for o in &mut self.orbs {
                    if o.stream {
                        o.home = o.pos + (o.home - o.pos) * 0.48;
                    }
                    o.stream = false;
                }
            }
        }
        if let Some(i) = split {
            let n = 6 + self.rng.bounded(5).unwrap() as usize;
            self.divide(i, n);
        }
        if let Some(screen) = foam_release {
            self.release_foam_pair(screen);
        }
        if let Some(pass) = self.pass.as_mut() {
            pass.pos += pass.v * dt;
            pass.age += dt;
            if pass.age >= pass.life {
                self.pass = None
            }
        }
        if self.time >= self.next_pass {
            let point = self.point();
            let s = self
                .input
                .layout
                .outputs
                .iter()
                .find(|o| o.rect.contains(point))
                .unwrap()
                .rect;
            let life = range(&mut self.rng, 22., 39.);
            let r = self.height * (0.06 + 0.12 * self.params.depth as f64 / 100.);
            self.pass = Some(Pass {
                pos: p(s.x - r, s.y + s.height * (0.2 + 0.6 * self.rng.unit())),
                v: p((s.width + 2. * r) / life, range(&mut self.rng, -5., 5.)),
                r,
                age: 0.,
                life,
                hue: self.rng.unit(),
                id: self.id(),
            });
            self.next_pass = self.time + range(&mut self.rng, 240., 480.);
        }
    }
}

// A single world-space line continues through the bezels without changing phase.
fn line(bounds: Rect, height: f64, x: f64, angle: f64) -> f64 {
    bounds.y + bounds.height * (0.5 + 0.12 * ((x - bounds.x) / height * 2.3 + angle).sin())
}
