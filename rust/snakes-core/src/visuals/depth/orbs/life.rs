use super::*;

const MAX_DUOS: usize = 16;

fn combined(a: Orb, b: Orb) -> (Vec2, Vec2, f64) {
    let t = b.mass / (a.mass + b.mass);
    let (sa, ca) = (a.hue * TAU).sin_cos();
    let (sb, cb) = (b.hue * TAU).sin_cos();
    (
        a.pos + (b.pos - a.pos) * t,
        a.vel + (b.vel - a.vel) * t,
        ((sa * a.mass + sb * b.mass).atan2(ca * a.mass + cb * b.mass) / TAU).rem_euclid(1.),
    )
}
impl Colony {
    pub(super) fn interactions(&mut self, _dt: f64) {
        self.births.clear();
        for (i, a) in self.orbs.iter().copied().enumerate() {
            if !a.alive() {
                continue;
            }
            self.grid.visit(a.pos, a.radius * 3.2, |j| {
                let b = self.orbs[j];
                if b.radius > a.radius
                    || (b.radius == a.radius && j >= i)
                    || (a.z - b.z).abs() > 0.08
                {
                    return;
                }
                let d = b.pos - a.pos;
                let len2 = d.dot(d);
                let rest = a.radius + b.radius;
                if len2 > rest * rest * 2.56 {
                    return;
                }
                let len = len2.sqrt().max(0.01);
                let n = d / len;
                let kin = 1. - 2. * hue_delta(a.hue, b.hue).abs();
                let overlap = rest - len;
                let mut pull = (kin - 0.12) * (1. - len / (1.6 * rest)).max(0.) * 32.;
                if self.params.merging == 0 {
                    pull *= 1.8;
                }
                if overlap > 0. {
                    pull -= (overlap - 0.10 * b.radius).max(0.) * 0.85;
                    pull += (b.vel - a.vel).dot(n) * 0.32;
                }
                let force = n * pull;
                let ratio = b.mass / (a.mass + b.mass);
                self.forces[i] += force * ratio;
                self.forces[j] += force * (ratio - 1.);
                let coupling = (b.phase - a.phase).sin() * 0.024;
                self.coupling[i] += coupling;
                self.coupling[j] -= coupling;
                if overlap > -0.04 * b.radius
                    && a.links < 4
                    && b.links < 4
                    && a.cooldown == 0.
                    && b.cooldown == 0.
                    && self.births.len() < self.orbs.len() * 2
                {
                    self.births.push((i, j));
                }
            });
        }
        for k in 0..self.births.len() {
            let (i, j) = self.births[k];
            let (a, b) = (self.orbs[i], self.orbs[j]);
            if a.links >= 4
                || b.links >= 4
                || a.partner.is_some()
                || b.partner.is_some()
                || a.neighbours.contains(&b.id)
                || a.split > 0.
                || b.split > 0.
            {
                continue;
            }
            let kin = 1. - hue_delta(a.hue, b.hue).abs() * 2.;
            let duration = (2. - 1.7 * self.params.merging as f64 / 100.) * (1.2 - 0.4 * kin);
            self.links.push(Link {
                a: i,
                b: j,
                ids: [a.id, b.id],
                kind: LinkKind::Kiss,
                age: 0.,
                duration,
            });
            for (slot, peer) in [(i, b.id), (j, a.id)] {
                let o = &mut self.orbs[slot];
                let free = o
                    .neighbours
                    .iter_mut()
                    .find(|n| **n == 0)
                    .expect("four contact slots");
                *free = peer;
                o.links += 1;
            }
        }
    }
    pub(super) fn unlink(&mut self, k: usize) {
        let l = self.links.swap_remove(k);
        for (i, id) in [(l.a, l.ids[0]), (l.b, l.ids[1])] {
            let o = &mut self.orbs[i];
            if o.id == id {
                let peer = if i == l.a { l.ids[1] } else { l.ids[0] };
                if let Some(slot) = o.neighbours.iter_mut().find(|n| **n == peer) {
                    *slot = 0;
                    o.links -= 1;
                }
                if o.partner == Some(if i == l.a { l.b } else { l.a }) {
                    o.partner = None
                }
            }
        }
    }
    fn fuse(&mut self, i: usize, j: usize) {
        let (a, b) = (self.orbs[i], self.orbs[j]);
        let (pos, vel, hue) = combined(a, b);
        let o = &mut self.orbs[i];
        o.home = a.home + (b.home - a.home) * (b.mass / (a.mass + b.mass));
        o.pos = pos;
        o.vel = vel;
        o.hue = hue;
        o.mass = a.mass + b.mass;
        o.temperature = (a.temperature * a.mass + b.temperature * b.mass) / o.mass;
        o.energy = ((a.energy * a.mass + b.energy * b.mass) / o.mass + 0.1).min(1.);
        o.wobble[0] = 0.12;
        o.flare = 1.8;
        o.swing[1] = 0.28;
        o.cooldown = 3.;
        o.recalc(self.base);
        self.orbs[j].mass = 0.;
        self.orbs[j].partner = None;
        self.orbs[i].partner = None;
    }
    pub(super) fn update_links(&mut self, dt: f64) {
        let foam = self
            .event
            .as_ref()
            .is_some_and(|e| e.kind == events::Kind::Foam && e.age < 39.);
        let dandelion = self
            .event
            .as_ref()
            .filter(|e| e.kind == events::Kind::Dandelion && !e.bloomed)
            .map(|e| e.heart);
        let splitting = self
            .orbs
            .iter()
            .filter(|orb| orb.alive() && orb.split > 0.)
            .count();
        let mut duos = self
            .links
            .iter()
            .filter(|l| matches!(l.kind, LinkKind::Merge | LinkKind::Engulf))
            .count();
        let mut count = self.orbs.iter().filter(|o| o.alive()).count();
        let mut k = 0;
        while k < self.links.len() {
            let mut l = self.links[k];
            let (a, b) = (self.orbs[l.a], self.orbs[l.b]);
            l.age += dt;
            if !a.alive() || !b.alive() || [a.id, b.id] != l.ids {
                self.unlink(k);
                continue;
            }
            let delta = b.pos - a.pos;
            let len = delta.length().max(0.01);
            let rest = a.radius + b.radius;
            let n = delta / len;
            let heart = dandelion == Some(l.a) || dandelion == Some(l.b);
            let spring = match l.kind {
                LinkKind::Kiss => {
                    if len > 1.21 * rest || a.partner.is_some() || b.partner.is_some() {
                        self.unlink(k);
                        continue;
                    }
                    if l.age >= l.duration {
                        let ready = self.params.merging > 0
                            && !foam
                            && duos + splitting < MAX_DUOS
                            && count.saturating_sub(duos) > self.target * 7 / 10;
                        let ratio = a.mass.max(b.mass) / a.mass.min(b.mass);
                        l.kind = if ready && ratio > 5. && !heart {
                            LinkKind::Engulf
                        } else if ready
                            && (heart
                                || ((self.params.merging > 50
                                    || hue_delta(a.hue, b.hue).abs() < 0.19)
                                    && a.mass + b.mass < 9.1
                                    && self.rng.unit()
                                        < (self.params.merging as f64 / 100.)
                                            * (1.2 - hue_delta(a.hue, b.hue).abs())))
                        {
                            LinkKind::Merge
                        } else {
                            LinkKind::Raft
                        };
                        l.age = 0.;
                        l.duration = match l.kind {
                            LinkKind::Engulf => range(&mut self.rng, 2., 4.),
                            LinkKind::Raft if self.params.merging == 0 => {
                                range(&mut self.rng, 120., 240.)
                            }
                            LinkKind::Raft => range(&mut self.rng, 60., 160.),
                            _ => 0.65,
                        };
                        if l.kind != LinkKind::Raft {
                            self.orbs[l.a].partner = Some(l.b);
                            self.orbs[l.b].partner = Some(l.a);
                            duos += 1;
                        } else {
                            self.stats[2] += 1;
                        }
                    }
                    0.2 * (len - rest)
                }
                LinkKind::Raft => {
                    if l.age > l.duration
                        || len > rest * (if self.params.merging == 0 { 1.85 } else { 1.55 })
                        || a.partner.is_some()
                        || b.partner.is_some()
                    {
                        self.orbs[l.a].cooldown = 1.;
                        self.unlink(k);
                        continue;
                    }
                    let f = (b.phase - a.phase).sin() * 0.13;
                    self.coupling[l.a] += f;
                    self.coupling[l.b] -= f;
                    (len - rest * 0.93) * 2.4 + (b.vel - a.vel).dot(n) * 1.6
                }
                LinkKind::Merge | LinkKind::Engulf => {
                    if l.age >= l.duration {
                        let (i, j) = if dandelion == Some(l.a)
                            || (dandelion != Some(l.b) && a.mass >= b.mass)
                        {
                            (l.a, l.b)
                        } else {
                            (l.b, l.a)
                        };
                        self.fuse(i, j);
                        count -= 1;
                        self.stats[usize::from(l.kind == LinkKind::Engulf)] += 1;
                        self.unlink(k);
                        continue;
                    }
                    (len - rest * (1. - 0.84 * ease(l.age / l.duration))) * 2.4
                }
            };
            let ratio = b.mass / (a.mass + b.mass);
            self.forces[l.a] += n * (spring * ratio);
            self.forces[l.b] += n * (spring * (ratio - 1.));
            self.links[k] = l;
            k += 1;
        }
    }
    pub(super) fn divide(&mut self, i: usize, desired: usize) {
        let live = self.orbs.iter().filter(|o| o.alive()).count();
        let desired = desired.min((self.target * 13 / 10).saturating_sub(live) + 1);
        let old = self.orbs[i];
        let mut slots = [0; 10];
        slots[0] = i;
        let mut n = 1;
        for (j, o) in self.orbs.iter().enumerate() {
            if !o.alive() && n < desired {
                slots[n] = j;
                n += 1
            }
        }
        if n < 2 {
            return;
        }
        let mut momentum = Vec2::default();
        let mut centre = Vec2::default();
        let mut remaining = old.mass;
        for (k, j) in slots[..n].iter().copied().enumerate() {
            let a = old.angle + TAU * k as f64 / n as f64;
            let mass = if k + 1 == n {
                remaining
            } else {
                (remaining / (n - k) as f64) * range(&mut self.rng, 0.85, 1.12)
            };
            remaining -= mass;
            let dir = axis(a);
            let mut o = old;
            o.id = self.id();
            o.mass = mass;
            // Only the first daughter inherits a parent's foreground depth.
            if k > 0 && o.z > 0.85 {
                o.z = 0.79;
            }
            o.age = 2.;
            o.links = 0;
            o.neighbours = [0; 4];
            o.partner = None;
            o.cooldown = if n > 3 { 32. } else { 8. };
            o.bloom_travel = if n > 3 { 18. } else { 0. };
            o.stream = false;
            o.split = 0.;
            o.shiver = 0.;
            o.flare = 1.;
            o.energy = if n > 3 {
                0.96
            } else {
                (old.energy + 0.06).min(1.)
            };
            let shift = if n > 3 {
                range(&mut self.rng, 0.045, 0.085) * if k % 2 == 0 { 1. } else { -1. }
            } else {
                range(&mut self.rng, -0.04, 0.04)
            };
            o.hue = if n > 3 {
                (old.hue + k as f64 / n as f64 + shift * 0.4).rem_euclid(1.)
            } else {
                (old.hue + shift).rem_euclid(1.)
            };
            o.recalc(self.base);
            o.pos += dir * (old.radius * (if n > 3 { 1.05 } else { 0.58 }));
            o.vel += dir * (if n > 3 { 155. } else { 19. });
            o.wobble[0] = 0.085;
            o.phase += k as f64 * 0.32;
            momentum += o.vel * mass;
            centre += o.pos * mass;
            self.orbs[j] = o;
        }
        for j in &slots[..n] {
            self.orbs[*j].vel += old.vel - momentum / old.mass;
            self.orbs[*j].pos = self
                .input
                .layout
                .nearest_visible(self.orbs[*j].pos + old.pos - centre / old.mass);
        }
        if n > 3 {
            for &slot in &slots[..n] {
                let child = self.orbs[slot];
                self.orbs[slot].home = self
                    .input
                    .layout
                    .nearest_visible(child.pos + child.vel * 4.);
                for k in 1..=3 {
                    let Some(j) = self.motes.iter().position(|m| m.life <= 0.) else {
                        break;
                    };
                    let pos = self
                        .input
                        .layout
                        .nearest_visible(child.pos - child.vel.unit() * k as f64 * 12.);
                    self.seed_mote(
                        j,
                        pos,
                        (child.z - 0.12).max(0.),
                        child.hue,
                        0.,
                        child.vel * (0.85 - 0.17 * k as f64),
                    );
                    self.motes[j].cue = true;
                }
            }
        }
        self.stats[3] += 1;
    }
    pub(super) fn begin_lifecycle(&mut self, id: u32, anchor: Vec2) {
        if id == 5
            && self.orbs.iter().filter(|o| o.split > 0.).count()
                + self
                    .links
                    .iter()
                    .filter(|l| matches!(l.kind, LinkKind::Merge | LinkKind::Engulf))
                    .count()
                >= MAX_DUOS
        {
            return;
        }
        let heart = self
            .event
            .as_ref()
            .filter(|e| e.kind == events::Kind::Dandelion)
            .map(|e| e.heart);
        let candidate = self
            .orbs
            .iter()
            .enumerate()
            .filter(|(i, o)| {
                Some(*i) != heart
                    && o.alive()
                    && o.z >= 0.25
                    && o.cooldown == 0.
                    && o.partner.is_none()
                    && o.shiver == 0.
                    && o.split == 0.
            })
            .min_by(|(_, a), (_, b)| {
                let cost = |o: &Orb| (o.pos - anchor).length() - 2. * o.radius;
                cost(a).total_cmp(&cost(b))
            })
            .map(|(i, _)| i);
        if let Some(i) = candidate {
            if id == 5 {
                self.orbs[i].split = 0.001;
            } else {
                self.orbs[i].shiver = 0.001;
            }
        }
    }
    pub(super) fn release_foam_pair(&mut self, screen: Rect) {
        let pair = self
            .links
            .iter()
            .enumerate()
            .filter(|(_, l)| {
                matches!(l.kind, LinkKind::Raft | LinkKind::Kiss)
                    && screen.contains(self.orbs[l.a].pos)
                    && [l.a, l.b].iter().all(|&i| {
                        self.orbs[i].alive()
                            && self.orbs[i].partner.is_none()
                            && self.orbs[i].split == 0.
                    })
            })
            .min_by(|(_, a), (_, b)| {
                let gap = |l: &Link| (self.orbs[l.a].pos - self.orbs[l.b].pos).length();
                gap(a).total_cmp(&gap(b))
            })
            .map(|(k, _)| k);
        let used = self.orbs.iter().filter(|o| o.split > 0.).count()
            + self
                .links
                .iter()
                .filter(|l| matches!(l.kind, LinkKind::Merge | LinkKind::Engulf))
                .count();
        if let Some(k) = pair
            && used < MAX_DUOS
        {
            let l = &mut self.links[k];
            l.kind = LinkKind::Merge;
            l.duration = 1.2;
            l.age = 0.;
            self.orbs[l.a].partner = Some(l.b);
            self.orbs[l.b].partner = Some(l.a);
            self.orbs[l.a].flare = 1.7;
        }
    }
    pub(super) fn burst(&mut self, i: usize) -> bool {
        let o = self.orbs[i];
        let requested = 8 + self.rng.bounded(17).unwrap() as usize;
        let mut slots = [0usize; 24];
        let mut n = 0;
        for (j, m) in self.motes.iter().enumerate() {
            if m.life <= 0. && n < requested {
                slots[n] = j;
                n += 1
            }
        }
        if n < 8 {
            return false;
        }
        let spin = self.rng.unit() * TAU;
        let mut scatter = Pcg32::new(u64::from(o.id), self.tick);
        for (k, j) in slots[..n].iter().enumerate() {
            let angle = spin + k as f64 * TAU / n as f64;
            let dir = axis(angle);
            let speed = 22. + o.radius * 0.3;
            self.seed_mote(
                *j,
                self.input
                    .layout
                    .nearest_visible(o.pos + dir * o.radius * 0.8),
                o.z,
                o.hue,
                o.mass * 0.98 / n as f64,
                o.vel + dir * speed,
            );
            // The luminous ejecta outrun or lag their mass-carrying droplets
            // before mixing back into the water. The shell conserves momentum.
            let tilt = range(&mut scatter, -12., 12.).to_radians();
            self.motes[*j].ejecta_slip =
                (axis(angle + tilt) * range(&mut scatter, 0.70, 1.30) - dir) * speed;
        }
        if self.params.look != "bubbles" {
            let id = self.id();
            self.rings.push(Ring {
                id,
                pos: o.pos,
                z: o.z,
                hue: o.hue,
                r: o.radius,
                age: 0.,
            });
        }
        self.orbs[i].mass = 0.;
        self.stats[4] += 1;
        true
    }
    pub(super) fn update_motes(&mut self, dt: f64) {
        self.mote_grid.clear();
        for (i, m) in self.motes.iter_mut().enumerate() {
            if m.life <= 0. {
                continue;
            }
            m.life -= dt;
            let mut host = None;
            self.grid.visit(m.pos, self.base * 1.2, |j| {
                let o = self.orbs[j];
                let d = o.pos - m.pos;
                if !m.cue
                    && o.alive()
                    && (o.z - m.z).abs() < 0.08
                    && d.dot(d) < (o.radius + 12.).powi(2)
                {
                    host = Some(j)
                }
            });
            if let Some(j) = host {
                let o = &mut self.orbs[j];
                o.vel += (m.vel - o.vel) * (m.mass / (o.mass + m.mass));
                o.mass += m.mass;
                o.energy = (o.energy + 0.025).min(1.);
                o.recalc(self.base);
                m.life = 0.;
                continue;
            }
            if (self.tick as usize + i).is_multiple_of(8) {
                m.current = space::tracer_current(&self.input.layout, &self.flow, m.pos).velocity;
            }
            m.vel += (m.current - m.vel) * (dt / (1.8 + dt));
            let next = m.pos + m.vel * dt;
            if self.input.layout.contains(next) {
                m.pos = next;
            } else {
                m.vel = (self.input.layout.nearest_visible(next) - next).unit() * 35.;
            }
            self.mote_grid.add(i, m.pos);
        }
    }
    fn accrete(&mut self) -> bool {
        let Some(slot) = self.orbs.iter().position(|o| !o.alive()) else {
            return false;
        };
        for i in 0..self.motes.len() {
            let m = self.motes[i];
            if m.life <= 0. {
                continue;
            }
            let mut ids = [0; 6];
            let mut n = 0;
            self.mote_grid.visit(m.pos, 40., |j| {
                let q = self.motes[j];
                if n < 6
                    && q.life > 0.
                    && (m.z - q.z).abs() < 0.08
                    && (m.pos - q.pos).dot(m.pos - q.pos) <= 1600.
                {
                    ids[n] = j;
                    n += 1;
                }
            });
            if n == 6 {
                let mut mass = 0.;
                let mut v = Vec2::default();
                for j in ids {
                    let s = &mut self.motes[j];
                    mass += s.mass;
                    v += s.vel * s.mass;
                    s.life = 0.;
                }
                let mut o = self.make_orb(m.pos, m.z, m.hue, mass);
                o.vel = v / mass;
                self.orbs[slot] = o;
                self.stats[5] += 1;
                return true;
            }
        }
        false
    }
    pub(super) fn life_cycle(&mut self, dt: f64, warming: bool) {
        let occupied = self
            .orbs
            .iter()
            .filter(|orb| orb.alive() && orb.split > 0.)
            .count()
            + self
                .links
                .iter()
                .filter(|link| matches!(link.kind, LinkKind::Merge | LinkKind::Engulf))
                .count();
        let mut available = MAX_DUOS.saturating_sub(occupied);
        let mut count = self.orbs.iter().filter(|o| o.alive()).count();
        let heart = self
            .event
            .as_ref()
            .filter(|e| e.kind == events::Kind::Dandelion && e.age < 47.)
            .map(|e| e.heart);
        for i in 0..self.orbs.len() {
            let o = &mut self.orbs[i];
            if !o.alive() || o.partner.is_some() || Some(i) == heart || o.stream {
                continue;
            }
            let critical = if count < self.target * 9 / 10 {
                3.2
            } else {
                5.4
            };
            if (o.mass > critical || o.split > 0.)
                && o.cooldown == 0.
                && (o.split > 0. || count < self.target * 13 / 10)
                && (o.split > 0. || available > 0)
            {
                if o.split == 0. {
                    available -= 1;
                }
                o.split += dt;
                if o.split >= 2.6 {
                    let n = if self.rng.unit() < 0.08 { 3 } else { 2 };
                    self.divide(i, n);
                    count += n - 1;
                }
            } else {
                o.split = 0.
            }
            let o = &mut self.orbs[i];
            if o.shiver > 0. || (o.energy < 0.08 && o.age > 75. && count > self.target * 7 / 10) {
                o.shiver += dt;
                if o.shiver > 0.65 && self.burst(i) {
                    count -= 1;
                }
            }
        }
        if !warming && self.time >= self.next_fission {
            let pos = self.point();
            if count < self.target * 13 / 10 && available > 0 {
                self.begin_lifecycle(5, pos);
            }
            let waiting = count >= self.target * 13 / 10 || available == 0;
            let screens = self.input.layout.visible_area / (2560. * 1440.);
            self.next_fission = self.time
                + if waiting {
                    4.
                } else {
                    range(&mut self.rng, 105., 175.) / screens.max(0.25)
                };
        }
        self.thermostat -= dt;
        if self.thermostat > 0. {
            return;
        }
        self.thermostat = range(&mut self.rng, 2.5, 5.5);
        if count < self.target * 13 / 10 && self.accrete() {
            count += 1;
        }
        let mass = self.orbs.iter().map(|o| o.mass).sum::<f64>();
        let suspended = self
            .motes
            .iter()
            .filter(|m| m.life > 0.)
            .map(|m| m.mass)
            .sum::<f64>();
        let mut deficit = (self.target_mass - mass - suspended).max(0.);
        // Old light returns to the reservoir. This gives births somewhere to
        // draw from after a long quiet interval without inventing new mass.
        let recycle = if count < self.target { 0.011 } else { 0.003 };
        for o in self.orbs.iter_mut().filter(|o| o.alive() && o.age > 45.) {
            let released = o.mass * recycle;
            o.mass -= released;
            o.recalc(self.base);
            deficit += released;
        }
        self.next_feeding_ground -= self.thermostat;
        if self.next_feeding_ground <= 0. && self.event.is_none() {
            self.next_feeding_ground = range(&mut self.rng, 8., 18.);
            // Nutrients arriving in a quiet patch gradually draw a colony away
            // from a crowded eddy. All movement still passes through drag.
            let mut quiet = self.point();
            let mut best = 0.;
            for _ in 0..5 {
                let point = self.point();
                let room = self
                    .orbs
                    .iter()
                    .filter(|o| o.alive())
                    .map(|o| (o.pos - point).dot(o.pos - point))
                    .fold(f64::MAX, f64::min);
                if room > best {
                    best = room;
                    quiet = point
                }
            }
            if let Some(i) = self
                .orbs
                .iter()
                .enumerate()
                .filter(|(_, o)| o.alive() && o.partner.is_none())
                .min_by(|(_, a), (_, b)| {
                    ((a.pos - quiet).length() + a.mass * self.base)
                        .total_cmp(&((b.pos - quiet).length() + b.mass * self.base))
                })
                .map(|(i, _)| i)
            {
                self.orbs[i].home = quiet;
            }
        }
        if deficit > 0.55
            && count < self.target * 6 / 5
            && let Some(i) = self.orbs.iter().position(|o| !o.alive())
        {
            let pos = self.point();
            let z = 0.55 + (self.rng.unit() * 0.85 - 0.55) * self.params.depth as f64 / 100.;
            let h = self.rng.unit();
            let m = range(&mut self.rng, 0.55, 2.8).min(deficit);
            deficit -= m;
            self.orbs[i] = self.make_orb(pos, z, h, m);
            self.stats[5] += 1;
        }
        let mote_count = self.motes.iter().filter(|m| m.life > 0.).count();
        let needed = (self.target * 2)
            .saturating_sub(mote_count)
            .min(self.target / 5 + 1);
        for _ in 0..needed {
            if let Some(i) = self.motes.iter().position(|m| m.life <= 0.) {
                let pos = self.point();
                let z = self.rng.unit();
                let h = self.rng.unit();
                // At equilibrium motes recycle material from a mature body.
                // Blindly adding their mass at every refill grows the tank
                // over a six-hour run even when the orb count remains bounded.
                let m = 0.028;
                let recycled = (m - deficit).max(0.);
                if recycled > 0. {
                    let Some(donor) = self
                        .orbs
                        .iter_mut()
                        .filter(|o| o.mass > 0.3)
                        .max_by(|a, b| a.mass.total_cmp(&b.mass))
                    else {
                        break;
                    };
                    donor.mass -= recycled;
                    donor.recalc(self.base);
                }
                deficit = (deficit - m).max(0.);
                self.seed_mote(i, pos, z, h, m, Vec2::default());
            }
        }
        if !warming && count > self.target && mass > self.target_mass * 1.12 {
            for o in &mut self.orbs {
                o.energy *= 0.995;
            }
        }
    }
}
