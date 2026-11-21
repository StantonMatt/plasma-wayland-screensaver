use super::{Association, ConstellationWorld, FIELD, Star, space::*};
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum Kind {
    Meteor = 1,
    Satellite,
    Birth,
    Nova,
    Comet,
    Shower,
    Gathering,
    Still,
}
pub fn shock_radius(age: f32) -> f32 {
    let u = ((age - 4.) / 3.).clamp(0., 1.);
    320. * (1. - (1. - u).powi(2))
}
impl Kind {
    pub const ALL: [Self; 8] = [
        Self::Meteor,
        Self::Satellite,
        Self::Birth,
        Self::Nova,
        Self::Comet,
        Self::Shower,
        Self::Gathering,
        Self::Still,
    ];
    pub fn period(self) -> (f32, f32) {
        match self {
            Self::Meteor => (25., 90.),
            Self::Satellite => (360., 900.),
            Self::Birth => (300., 720.),
            Self::Nova => (480., 1200.),
            Self::Comet => (600., 1500.),
            Self::Shower => (1500., 2700.),
            _ => (1800., 3600.),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Moment {
    pub kind: Kind,
    pub start: f32,
    pub duration: f32,
    pub p: Point,
    pub delta: Point,
    pub variant: f32,
    pub tail: f32,
    pub shell: [f32; 4],
    pub count: usize,
    pub emitted: usize,
    pub next_ignition: f32,
    pub star: Option<usize>,
}
// One packed start/sweep pair per shell arc, in integer degrees. Keep every
// gap open even when four long arcs are drawn. The phase rotates with the lobe
// ellipse and the extra draws never affect event timing.
fn remnant_arcs(variant: f32) -> [f32; 4] {
    let mut rng = super::Pcg32::new(variant.to_bits() as u64, 0xa2c);
    let n = 2 + rng.bounded(3).unwrap() as usize;
    let mut widths = [0.; 4];
    let mut weights = [0.; 4];
    for i in 0..n {
        widths[i] = rand(&mut rng, 0., 70.);
        weights[i] = rand(&mut rng, 0.2, 1.);
    }
    let budget = 360. - n as f32 * (40. + 9.);
    let scale = (budget / widths.iter().sum::<f32>().max(1.)).min(1.);
    for w in &mut widths[..n] {
        *w = (40. + *w * scale).round();
    }
    let free = 360. - widths.iter().sum::<f32>();
    let total = weights.iter().sum::<f32>();
    let mut arcs = [0.; 4];
    let mut start = rand(&mut rng, 0., 360.);
    for i in 0..n {
        arcs[i] = (start.round() % 360.) * 128. + widths[i];
        start += widths[i] + 5. + (free - 5. * n as f32) * weights[i] / total;
    }
    arcs
}
impl Moment {
    pub fn position(self, t: f32) -> Point {
        let u = ((t - self.start) / self.duration).clamp(0., 1.);
        self.p
            + self.delta
                * if self.kind == Kind::Meteor {
                    u * (1.13 - 0.13 * u)
                } else {
                    u
                }
    }
}
impl ConstellationWorld {
    pub(super) fn anchor(&mut self) -> Point {
        let v = self
            .input
            .layout
            .choose_visible_anchor(&mut self.rng, 0.)
            .unwrap();
        pt(v.x as f32, v.y as f32)
    }
    pub(super) fn nursery(&mut self) -> Point {
        self.choose_nursery(false)
    }
    pub(super) fn choose_nursery(&mut self, field: bool) -> Point {
        let mut best = self.anchor();
        let mut highest = -1.;
        let b = self.input.layout.bounds;
        for _ in 0..48 {
            let p = self.anchor();
            if field {
                // Replenish passive tracers in open sky; repeatedly placing
                // them in the same bright-figure nursery creates a dot pile.
                let clearance = self
                    .grid
                    .near(p, 160.)
                    .filter(|&i| self.stars[i].mag >= 4.)
                    .map(|i| (p - self.stars[i].p).len2())
                    .fold(25600., f32::min);
                if clearance > highest {
                    highest = clearance;
                    best = p;
                }
                continue;
            }
            let empty = self
                .groups
                .iter()
                .filter(|a| a.active)
                .map(|a| (a.p - p).len())
                .fold(1400., f32::min);
            let edge = (p.x - b.x as f32)
                .min(b.right() as f32 - p.x)
                .min(p.y - b.y as f32)
                .min(b.bottom() as f32 - p.y);
            let density = self
                .grid
                .near(p, 180.)
                .filter(|&i| self.stars[i].linkable() && (self.stars[i].p - p).len2() < 32400.)
                .count();
            let score = empty * smooth(edge / 210.)
                / (1. + density as f32 * 0.30 + self.occupancy[self.grid.cell(p)] * 0.5)
                * if p.y < (b.y + b.height * 0.67) as f32 {
                    1.12
                } else {
                    1.
                };
            if score > highest {
                highest = score;
                best = p;
            }
        }
        best
    }
    pub(super) fn spawn(&mut self, kind: Kind, anchor: Option<Point>, time: f32) {
        if self.input.reduced_motion
            || (matches!(kind, Kind::Nova | Kind::Birth) && !self.params.lifeCycle)
            || (!matches!(kind, Kind::Nova | Kind::Birth) && !self.params.skyEvents)
        {
            return;
        }
        let b = self.input.layout.bounds;
        let mut p = anchor.unwrap_or_else(|| self.anchor());
        let mut e = Moment {
            kind,
            start: time,
            duration: 1.,
            p,
            delta: pt(0., 0.),
            variant: self.rng.unit() as f32,
            shell: [0.; 4],
            tail: rand(&mut self.rng, 150., 420.),
            count: 0,
            emitted: 0,
            next_ignition: time,
            star: None,
        };
        match kind {
            Kind::Meteor => {
                e.duration = rand(&mut self.rng, 0.5, 1.1);
                let dir = Point::polar(rand(&mut self.rng, 0.18, 0.57));
                e.delta = dir * rand(&mut self.rng, 750., 1340.);
                // A scripted anchor denotes the crossing at +0.30 seconds,
                // halfway along the luminous tail, independent of its random lifetime.
                let u = 0.30 / e.duration;
                let travel = e.delta * (u * (1.13 - 0.13 * u));
                let tail = e.delta.unit() * e.tail * (1. - 0.48 * u);
                e.p = if anchor.is_some() {
                    p - travel + tail * 0.5
                } else {
                    p - e.delta * 0.35
                };
                let floor = (b.y + b.height * 0.8) as f32;
                e.p.y -= (e.p.y + e.delta.y - floor).max(0.);
            }
            Kind::Satellite | Kind::Comet => {
                e.duration = if kind == Kind::Comet {
                    rand(&mut self.rng, 50., 90.)
                } else {
                    rand(&mut self.rng, 25., 45.)
                };
                if anchor.is_none() {
                    p.x = b.x as f32 - 70.;
                    p.y = b.y as f32 + rand(&mut self.rng, 0.12, 0.63) * b.height as f32;
                }
                e.p = p;
                e.delta = pt(
                    b.width as f32 + 150.,
                    rand(&mut self.rng, -0.12, 0.18) * b.height as f32,
                );
                if anchor.is_some() && kind == Kind::Satellite {
                    e.variant = 0.;
                    e.duration = 32.;
                    e.p = p - e.delta * 0.5;
                }
            }
            Kind::Nova => {
                e.shell = remnant_arcs(e.variant);
                e.duration = rand(&mut self.rng, 244., 364.);
                let star = self
                    .stars
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| {
                        s.alive && !s.nova && s.retire_at.is_infinite() && s.age > s.life * 0.8
                    })
                    .min_by(|(_, a), (_, b)| (a.p - p).len2().total_cmp(&(b.p - p).len2()))
                    .map(|(i, _)| i);
                if let Some(i) = star {
                    e.star = Some(i);
                    e.p = self.stars[i].p;
                    self.stars[i].nova = true;
                }
            }
            Kind::Birth => {
                if anchor.is_none()
                    && self.groups.iter().filter(|a| a.active).count() > self.target_groups
                {
                    return;
                }
                e.count = rand(&mut self.rng, 4., 9.) as usize;
                // An earlier nursery can still own the reserve. Let a few
                // faint field stars fade quietly before reusing their slots.
                let pending: usize = self
                    .events
                    .iter()
                    .filter(|m| m.kind == Kind::Birth)
                    .map(|m| m.count - m.emitted)
                    .sum();
                let free = self
                    .stars
                    .iter()
                    .filter(|s| !s.alive || s.retire_at.is_finite())
                    .count();
                for _ in 0..(e.count + pending).saturating_sub(free) {
                    if let Some(star) = self
                        .stars
                        .iter_mut()
                        .filter(|s| s.alive && !s.nova && s.retire_at.is_infinite())
                        .max_by(|a, b| a.mag.total_cmp(&b.mag))
                    {
                        star.retire_at = time + 6.;
                    }
                }
                e.p = anchor.unwrap_or_else(|| self.nursery());
                e.tail = rand(&mut self.rng, 2.5, 3.0);
                e.duration = e.tail * (e.count - 1) as f32 + 61.5;
                let mut a = Association::new(e.p, e.count, self.graph.reach * 0.9, &mut self.rng);
                // Turn the walk inside the nursery until its first strokes
                // have clear sight lines. A forced nursery can land in a busy
                // part of the atlas, especially on an ultrawide output.
                let base = a.angle;
                let mut best = (f32::NEG_INFINITY, base);
                for trial in 0..24 {
                    let angle = base + trial as f32 * std::f32::consts::TAU / 24.;
                    let axis = Point::polar(angle);
                    let place = |offset: Point| {
                        e.p + axis * (offset.x * 1.15) + pt(-axis.y, axis.x) * (offset.y * 1.30)
                    };
                    let mut score = 0.;
                    for member in &a.members[1..e.count] {
                        let p = place(a.members[member.parent].offset);
                        let q = place(member.offset);
                        let d = q - p;
                        let nearest = self
                            .grid
                            .near(p.mix(q, 0.5), self.graph.reach * 1.4)
                            .filter(|&j| self.stars[j].alive && self.stars[j].mag < 4.5)
                            .map(|j| {
                                let v = self.stars[j].p - p;
                                (v - d * (v.dot(d) / d.len2()).clamp(0., 1.)).len()
                            })
                            .fold(95., f32::min);
                        score += nearest.min(70.) - 3. * (40. - nearest).max(0.);
                    }
                    if score > best.0 {
                        best = (score, angle);
                    }
                }
                a.angle = best.1;
                a.update_basis(self.params.gravity as f32 / 100., 1.);
                let slot = self
                    .groups
                    .iter()
                    .position(|g| !g.active)
                    .unwrap_or(self.groups.len());
                if slot == self.groups.len() {
                    self.groups.push(a);
                } else {
                    self.groups[slot] = a;
                }
                e.star = Some(slot);
            }
            Kind::Shower => {
                e.duration = rand(&mut self.rng, 40., 70.);
                e.count = rand(&mut self.rng, 15., 41.) as usize;
                if anchor.is_none() {
                    let r = self
                        .input
                        .layout
                        .outputs
                        .iter()
                        .max_by(|a, b| a.rect.width.total_cmp(&b.rect.width))
                        .unwrap()
                        .rect;
                    e.p = pt(
                        (r.x + r.width * 0.55) as f32,
                        (r.y + r.height * 0.23) as f32,
                    )
                }
            }
            Kind::Gathering => {
                e.duration = 90.;
                for a in &mut self.groups {
                    a.gather_partner = FIELD;
                    a.gather_offset = Point::default();
                }
                if let Some(first) = self
                    .groups
                    .iter()
                    .enumerate()
                    .filter(|(_, g)| g.active)
                    .min_by(|(_, a), (_, b)| (a.p - p).len2().total_cmp(&(b.p - p).len2()))
                    .map(|(i, _)| i)
                {
                    let center = self.groups[first].p;
                    if let Some(second) = self
                        .groups
                        .iter()
                        .enumerate()
                        .filter(|(i, g)| *i != first && g.active)
                        .min_by(|(_, a), (_, b)| {
                            ((a.p - center).len() - 560.)
                                .abs()
                                .total_cmp(&((b.p - center).len() - 560.).abs())
                        })
                        .map(|(i, _)| i)
                    {
                        self.groups[first].gather_partner = second;
                        self.groups[second].gather_partner = first;
                    }
                }
            }
            Kind::Still => e.duration = rand(&mut self.rng, 120., 180.),
        }
        self.events.push(e);
    }
    // Two close co-natal stars can capture using the same encounter test as
    // unrelated visitors. The reserve lets the secondary fade in naturally.
    fn companion(&mut self, primary: usize, t: f32) -> bool {
        if self.stars.iter().filter(|s| s.alive && s.companion).count()
            >= self.area.round().max(1.) as usize
        {
            return false;
        }
        let Some(j) = self.stars.iter().position(|s| !s.alive) else {
            return false;
        };
        let primary_star = self.stars[primary];
        let p = primary_star.p + Point::polar(rand(&mut self.rng, 0., std::f32::consts::TAU)) * 10.;
        let mut secondary = Star::new(p, &mut self.rng, true);
        secondary.companion = true;
        secondary.group = primary_star.group;
        secondary.offset = primary_star.offset;
        secondary.v = primary_star.v;
        secondary.target = primary_star.target;
        secondary.born = t;
        secondary.age = 0.;
        self.stars[j] = secondary;
        true
    }
    fn ignite(&mut self, group: usize, member: usize, t: f32) -> bool {
        let Some(i) = self.stars.iter().position(|s| !s.alive) else {
            return false;
        };
        let a = &mut self.groups[group];
        let m = a.members[member];
        let mut s = Star::new(a.position(m.offset), &mut self.rng, true);
        s.group = group;
        s.parent = if m.parent == FIELD {
            FIELD
        } else {
            a.members[m.parent].star
        };
        s.offset = m.offset;
        s.v = a.v * self.drift;
        s.set_magnitude(if member == 0 {
            rand(&mut self.rng, 1.5, 1.92)
        } else {
            rand(&mut self.rng, 1.5, 3.8)
        });
        s.life = a.life * rand(&mut self.rng, 0.94, 1.48);
        s.age = 0.;
        s.born = t;
        a.members[member].star = i;
        self.stars[i] = s;
        // A resolved nursery member has a small chance of being a double.
        let reserved: usize = self
            .events
            .iter()
            .filter(|e| e.kind == Kind::Birth)
            .map(|e| e.count - e.emitted)
            .sum();
        if self.rng.unit() < 0.055 && self.stars.iter().filter(|s| !s.alive).count() >= reserved {
            self.companion(i, t);
        }
        true
    }
    pub(super) fn events_step(&mut self, t: f32, dt: f32) {
        for k in Kind::ALL {
            let i = k as usize - 1;
            if t >= self.next[i] {
                let (a, b) = k.period();
                self.next[i] =
                    t + rand(&mut self.rng, a, b) / (self.area * self.pace.min(2.)).max(0.1);
                // Stellar deaths trigger the nova clock, so no independent flashes are scheduled.
                if k != Kind::Nova {
                    self.spawn(k, None, t)
                }
            }
        }
        let ns = (t as f64 * 1e9).round() as i64;
        while self.script.first().is_some_and(|e| e.at_ns <= ns) {
            let e = self.script.remove(0);
            if e.id == 10 {
                self.graph
                    .open_figure(pt(e.anchor[0] as f32, e.anchor[1] as f32), t, &self.stars);
                continue;
            }
            if e.id == 9 && !self.params.lifeCycle {
                continue;
            }
            if e.id == 9 {
                let p = pt(e.anchor[0] as f32, e.anchor[1] as f32);
                if let Some((a, _)) = self
                    .stars
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.linkable())
                    .min_by(|(_, a), (_, b)| (a.p - p).len2().total_cmp(&(b.p - p).len2()))
                {
                    self.companion(a, t);
                }
                continue;
            }
            let k = Kind::ALL[(e.id - 1) as usize];
            self.spawn(
                k,
                Some(pt(e.anchor[0] as f32, e.anchor[1] as f32)),
                e.at_ns as f32 * 1e-9,
            );
        }
        self.calm = 0.;
        self.gather = 0.;
        for j in 0..self.events.len() {
            let e = self.events[j];
            let age = t - e.start;
            if age < 0. {
                continue;
            }
            let u = (age / e.duration).clamp(0., 1.);
            match e.kind {
                Kind::Nova => {
                    if age < 4.
                        && let Some(i) = e.star
                    {
                        self.events[j].p = self.stars[i].p;
                    }
                    if age >= 4.
                        && let Some(i) = e.star
                    {
                        self.extinguish(i, t);
                        self.events[j].star = None;
                    }
                    if (4. ..7.).contains(&age) {
                        let radius = shock_radius(age);
                        let last = shock_radius(age - dt);
                        for i in 0..self.stars.len() {
                            let d = self.stars[i].p - e.p;
                            let dist = d.len();
                            if dist >= last && dist <= radius && self.stars[i].alive {
                                self.stars[i].v += d.unit() * 14.;
                                self.graph.sever(i, t, &self.stars);
                                self.binaries.retain(|pair| pair.a != i && pair.b != i);
                            }
                        }
                    }
                    if age >= e.duration && e.emitted == 0 {
                        self.spawn(Kind::Birth, Some(e.p), t);
                        self.events[j].emitted = 1;
                    }
                }
                Kind::Birth => {
                    if e.emitted < e.count
                        && t >= e.next_ignition
                        && self.ignite(e.star.unwrap(), e.emitted, t)
                    {
                        self.events[j].emitted += 1;
                        self.events[j].next_ignition = t + e.tail;
                        self.events[j].duration =
                            (t - e.start) + (e.count - e.emitted - 1) as f32 * e.tail + 61.5;
                    }
                }
                Kind::Comet => {
                    let head = e.position(t);
                    for i in self.grid.near(head, 210.) {
                        let d = head - self.stars[i].p;
                        let r = d.len2();
                        if r < 210. * 210. {
                            self.stars[i].v += d * (dt * 1500. / (r + 900.));
                            if r < 45. * 45. {
                                self.binaries.retain(|b| b.a != i && b.b != i);
                            }
                        }
                    }
                }
                Kind::Shower => {
                    if e.emitted < e.count && smooth(u) * e.count as f32 >= e.emitted as f32 + 0.5 {
                        let p = self.anchor();
                        let dir = (p - e.p).unit();
                        self.spawn(Kind::Meteor, Some(p), t);
                        let m = self.events.last_mut().unwrap();
                        let limit = (self.input.layout.bounds.y
                            + self.input.layout.bounds.height * 0.8)
                            as f32;
                        let distance = m.delta.len();
                        let start = if dir.y > 0.001 {
                            ((limit - e.p.y) / dir.y - distance)
                                .max(40.)
                                .min((p - e.p).len())
                        } else {
                            (p - e.p).len()
                        };
                        m.p = e.p + dir * start;
                        let available = if dir.y > 0.001 {
                            ((limit - m.p.y) / dir.y).clamp(0., distance)
                        } else {
                            distance
                        };
                        m.delta = dir * available;
                        self.events[j].emitted += 1;
                    }
                }
                Kind::Gathering => {
                    self.gather = smooth(age / 25.) * smooth((e.duration - age) / 25.)
                }
                Kind::Still => self.calm = smooth((age / 5.).min((e.duration - age) / 5.)),
                _ => {}
            }
        }
        self.events.retain(|e| {
            t < e.start
                + e.duration
                + if e.kind == Kind::Meteor && e.variant < 0.2 {
                    2.
                } else {
                    0.
                }
        });
    }
}

#[cfg(test)]
mod shell_tests {
    use super::*;
    #[test]
    fn remnants_have_separated_arcs_with_varied_spans() {
        let mut counts = [false; 3];
        for i in 0..100 {
            let arcs = remnant_arcs(i as f32 / 100.);
            let mut pieces: Vec<_> = arcs
                .into_iter()
                .filter(|&a| a > 0.)
                .map(|a| ((a / 128.).floor(), a % 128.))
                .collect();
            counts[pieces.len() - 2] = true;
            pieces.sort_by(|a, b| a.0.total_cmp(&b.0));
            for j in 0..pieces.len() {
                let (start, width) = pieces[j];
                let next = pieces[(j + 1) % pieces.len()].0;
                assert!((40. ..=110.).contains(&width));
                assert!((next - start).rem_euclid(360.) - width >= 4.);
            }
        }
        assert!(counts.into_iter().all(|seen| seen));
    }
}
