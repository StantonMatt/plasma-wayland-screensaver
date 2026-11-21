//! Seeded living star atlas; one simulation per coordinator world.
mod art;
mod association;
use association::{Association, FIELD};
mod events;
mod graph;
mod render;
mod space;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_lattice;
use super::{Descriptor, MirroredPolicy, schema::Parameter};
use super::{
    common::{
        clock::Rate,
        layout::Rect,
        rng::{Pcg32, hash64},
    },
    world::*,
};
use art::Art;
use events::{Kind, Moment};
use graph::Graph;
use space::*;
use std::{cell::RefCell, sync::Arc};
#[allow(non_snake_case)]
#[derive(Clone, Debug)]
pub struct Params {
    pub gravity: i32,
    pub twinkle: i32,
    pub milkyWay: i32,
    pub lifeCycle: bool,
    pub skyEvents: bool,
}
impl Default for Params {
    fn default() -> Self {
        Self {
            gravity: 40,
            twinkle: 50,
            milkyWay: 40,
            lifeCycle: true,
            skyEvents: true,
        }
    }
}
pub const PARAMETERS: &[Parameter] = &[
    Parameter::integer(1, c"gravity", 0., 100., 40.),
    Parameter::integer(2, c"twinkle", 0., 100., 50.),
    Parameter::integer(3, c"milkyWay", 0., 100., 40.),
    Parameter::boolean(4, c"lifeCycle", true),
    Parameter::boolean(5, c"skyEvents", true),
];
pub const DESCRIPTOR: Descriptor = Descriptor {
    id: 2,
    key: "constellation",
    available: true,
    rate: 60,
    mirrored: MirroredPolicy::SharedCanonical,
    fit: super::MirroredFit::Cover,
    parameters: PARAMETERS,
    create: |input| Some(Box::new(ConstellationWorld::new(input))),
};
#[derive(Clone, Copy, Debug)]
pub struct Star {
    p: Point,
    v: Point,
    target: Point,
    wander: Point,
    group: usize,
    parent: usize,
    flare: f32,
    fading: bool,
    fade_end: f32,
    offset: Point,
    linked: bool,
    crowded: bool,
    companion: bool,
    mag: f32,
    flux: f32,
    core_light: f32,
    temp: f32,
    figure: usize,
    phase: f32,
    age: f32,
    life: f32,
    born: f32,
    retire_at: f32,
    mass: f32,
    nova: bool,
    alive: bool,
}
impl Star {
    fn new(p: Point, r: &mut Pcg32, near: bool) -> Self {
        let u = r.unit() as f32;
        let mag = if near {
            if u < 0.085 {
                u / 0.085
            } else {
                1. + (1. + (u - 0.085) / 0.915 * 10.22).log10() / 0.35
            }
        } else {
            4. + (1. + u * 4.012).log10() / 0.35
        };
        let life = rand(r, 480., 1800.);
        let flux = 10f32.powf(-0.4 * mag);
        Self {
            p,
            group: FIELD,
            parent: FIELD,
            flare: f32::NEG_INFINITY,
            fading: false,
            fade_end: f32::INFINITY,
            offset: Point::default(),
            linked: false,
            crowded: false,
            companion: false,
            v: pt(0., 0.),
            target: pt(0., 0.),
            wander: Point::default(),
            mag,
            flux,
            core_light: 0.16 + 0.84 * flux.powf(0.28),
            figure: 0,
            temp: rand(r, 0., 1.),
            phase: rand(r, 0., 8192.),
            age: rand(r, 0., life * 0.96),
            life,
            born: -1000.,
            retire_at: f32::INFINITY,
            mass: 0.35 + 3. * flux.sqrt(),
            nova: false,
            alive: true,
        }
    }
    fn set_magnitude(&mut self, mag: f32) {
        self.mag = mag;
        self.flux = 10f32.powf(-0.4 * mag);
        self.mass = 0.35 + 3. * self.flux.sqrt();
        self.core_light = 0.16 + 0.84 * self.flux.powf(0.28);
    }
    fn linkable(&self) -> bool {
        self.alive && self.mag < 4. && !self.nova && !self.fading
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Binary {
    a: usize,
    b: usize,
    turn: Point,
    born: f32,
    r: f32,
}
pub struct ConstellationWorld {
    input: WorldInput,
    params: Params,
    rng: Pcg32,
    field_rng: Pcg32,
    tick: u64,
    stars: Vec<Star>,
    graph: Graph,
    grid: Grid,
    contacts: Grid,
    groups: Vec<Association>,
    target_groups: usize,
    occupancy: Vec<f32>,
    spent: Vec<(Point, f32)>,
    binaries: Vec<Binary>,
    events: Vec<Moment>,
    script: Vec<Event>,
    next: [f32; 8],
    next_nova: f32,
    area: f32,
    pace: f32,
    drift: f32,
    calm: f32,
    gather: f32,
    target_count: usize,
    art: Arc<Art>,
    empty: Snapshot,
    bodies: Vec<(Point, f32)>,

    #[cfg(test)]
    prewarm_time: std::time::Duration,
    frames: RefCell<Vec<Arc<render::Frame>>>,
}
impl ConstellationWorld {
    pub fn new(input: WorldInput) -> Self {
        let mut params = Params::default();
        for v in &input.parameters {
            match v.id {
                1 => params.gravity = v.payload as i32,
                2 => params.twinkle = v.payload as i32,
                3 => params.milkyWay = v.payload as i32,
                4 => params.lifeCycle = v.payload != 0,
                5 => params.skyEvents = v.payload != 0,
                _ => {}
            }
        }
        let mut rng = Pcg32::new(input.seed, 0x57a2);
        let area = (input.layout.visible_area / (2560. * 1440.)) as f32;
        let n = (((40. + (input.density - 10.) * 200. / 90.) as f32 * area).round() as usize)
            .clamp(18, 3000);
        let b = input.layout.bounds;
        let size = b.height.min(b.width) as f32;
        let reach =
            (size * 0.11 * (1. + 0.35 * ((input.trails as f32 - 0.35) / 0.65).clamp(0., 1.)))
                .clamp(32., 300.);
        let art = Arc::new(Art::new(&input, &params, area));
        let mut stars = Vec::with_capacity(n + 8);
        let near_count = (n as f32 * 0.35).round() as usize;
        let target_groups =
            ((near_count as f32 / 7.5).round().min((5.8 * area).round()) as usize).max(1);
        let mut groups: Vec<Association> = Vec::new();
        let associated = near_count.min(10 * target_groups);
        for i in 0..n + 8 {
            let v = input.layout.choose_visible_anchor(&mut rng, 0.).unwrap();
            let mut p = pt(v.x as f32, v.y as f32);
            let mut group = FIELD;
            let mut member = 0;
            if i < associated {
                group = i * target_groups / associated;
                let first = (group * associated).div_ceil(target_groups);
                member = i - first;
                if member == 0 {
                    let mut best = -1.;
                    for _ in 0..60 {
                        let a = input.layout.choose_visible_anchor(&mut rng, 0.).unwrap();
                        let q = pt(a.x as f32, a.y as f32);
                        let edge = (q.x - b.x as f32)
                            .min(b.right() as f32 - q.x)
                            .min(q.y - b.y as f32)
                            .min(b.bottom() as f32 - q.y);
                        let distance = groups
                            .iter()
                            .map(|c| (q - c.p).len())
                            .fold(10000., f32::min);
                        let score = distance.min(6. * reach) * smooth(edge / (1.45 * reach));
                        if score > best {
                            best = score;
                            p = q;
                        }
                    }
                    let count = ((group + 1) * associated).div_ceil(target_groups) - first;
                    let mut a = Association::new(p, count, reach, &mut rng);
                    a.age = rand(&mut rng, 0., a.life * 0.60);
                    groups.push(a);
                }
                groups[group].members[member].star = i;
                p = groups[group].position(groups[group].members[member].offset);
            }
            let mut star = Star::new(p, &mut rng, i < near_count);
            if group != FIELD {
                let a = &groups[group];
                star.group = group;
                star.offset = a.members[member].offset;
                star.age = (star.life - (a.life - a.age) * rand(&mut rng, 0.85, 1.45)).max(0.);
                if member == 0 && star.mag >= 2. {
                    star.set_magnitude(rand(&mut rng, 0.8, 1.8));
                }
            }
            star.alive = i < n;
            stars.push(star);
        }
        let pace = input.speed as f32;
        let grid = Grid::new(b, reach * 1.35);
        let occupancy = vec![0.; grid.cells.len()];
        let mut sky = Self {
            graph: Graph::new(b, stars.len(), reach, input.trails as f32, area),
            grid,
            occupancy,
            spent: Vec::new(),
            contacts: Grid::new(b, 65.),
            stars,
            groups,
            target_groups,
            binaries: Vec::new(),
            events: Vec::with_capacity(20),
            tick: 0,
            art,
            empty: Snapshot::default(),
            bodies: vec![(Point::default(), 0.); n + 8],

            #[cfg(test)]
            prewarm_time: std::time::Duration::ZERO,
            frames: RefCell::new(Vec::new()),
            area,
            pace,
            drift: pace.powf(0.8),
            calm: 0.,
            gather: 0.,
            next: [0.; 8],
            next_nova: 0.,
            script: Vec::new(),
            target_count: n,
            field_rng: Pcg32::new(input.seed, 0xf13d),
            input,
            params,
            rng,
        };
        sky.graph.scale = sky.art.scale;
        sky.graph.spectrum = sky.art.spectrum;
        // Mature the drift and atlas through a coarse 180-second pre-roll. No observer
        // exists before zero, so forces and graph opportunities can be batched.
        #[cfg(test)]
        let start = std::time::Instant::now();
        const WARM_STEP: f32 = 7.5;
        // Large panoramas use another force lane for each 140 stars. All
        // stars integrate every coarse step, and receive a full initial sample.
        let warm_lanes = n.div_ceil(140).clamp(3, 8);
        sky.regrid();
        for k in 0..24 {
            let t = -180. + WARM_STEP * (k + 1) as f32;
            if !sky.input.reduced_motion {
                let stride = if k == 0 { 1 } else { warm_lanes };
                sky.move_stars(t, WARM_STEP, k % stride, stride, true);
            }
            sky.separate(true);
            sky.graph
                .update(&sky.stars, &sky.grid, t, WARM_STEP, 1., false);
        }
        sky.graph.recognise(&mut sky.stars, 0.);
        for a in &mut sky.groups {
            a.nursery = a.p;
        }
        sky.record_occupancy();
        #[cfg(test)]
        {
            sky.prewarm_time = start.elapsed();
        }
        for k in Kind::ALL {
            let (a, b) = k.period();
            sky.next[k as usize - 1] = rand(&mut sky.rng, a, b) / (area * pace.min(2.)).max(0.1)
        }
        sky
    }
    fn record_occupancy(&mut self) {
        // The gas feeding a new association has a memory of the regions the
        // preceding figures traversed. Replenish quieter regions first.
        for heat in &mut self.occupancy {
            *heat *= 0.933;
        }
        let r = pt(150., 150.);
        for s in self.stars.iter().filter(|s| s.linkable()) {
            self.occupancy[self.grid.cell(s.p)] += 0.4;
        }
        for a in self.groups.iter().filter(|a| a.active) {
            for cell in self.grid.rect(a.p - r, a.p + r) {
                self.occupancy[cell] += 1.;
            }
        }
    }
    fn regrid(&mut self) {
        self.grid.clear();
        for (body, s) in self.bodies.iter_mut().zip(&self.stars) {
            *body = (s.p, if s.mag < 4. { s.mass } else { 0. });
        }
        for pair in &self.binaries {
            let (a, b) = (self.stars[pair.a], self.stars[pair.b]);
            let mass = a.mass + b.mass;
            self.bodies[pair.a] = (a.p.mix(b.p, b.mass / mass), mass);
            self.bodies[pair.b].1 = 0.;
        }
        for (i, s) in self.stars.iter().enumerate() {
            if s.alive {
                self.grid.insert(s.p, i)
            }
        }
    }
    fn move_stars(&mut self, t: f32, dt: f32, phase: usize, stride: usize, warm: bool) {
        let b = self.input.layout.bounds;
        let min = pt(b.x as f32, b.y as f32);
        let max = pt(b.right() as f32, b.bottom() as f32);
        self.move_groups(t, dt, warm);
        let gravity = self.params.gravity as f32 * 0.01;
        let drag = 1. - (-dt * self.pace * 0.173287).exp();
        let phase_time = t * self.drift;
        let sample_dt = dt * stride as f32 * self.pace;
        let memory = (-sample_dt / 20.).exp();
        let innovation = 0.6 * (1. - memory * memory).sqrt();
        for i in (phase..self.stars.len()).step_by(stride) {
            let s = self.stars[i];
            if !s.alive {
                continue;
            }
            if s.mag >= 4. {
                // Exact OU transition between force samples. Box-Muller uses
                // a separate stream so seeing cannot consume an event draw.
                let z = (-2. * (1. - self.field_rng.unit()).ln()).sqrt() as f32;
                let angle = rand(&mut self.field_rng, 0., std::f32::consts::TAU);
                let wander = s.wander * memory + Point::polar(angle) * (innovation * z);
                let current = field_current(s.p, phase_time, self.input.seed, min, max);
                // Start relieving pressure before the sixth neighbour enters
                // the disk. The buffer covers disks centred between stars.
                let mut away = Point::default();
                let mut nearby = 0;
                for j in self.grid.near(s.p, 120.) {
                    let d = s.p - self.stars[j].p;
                    if j != i && self.stars[j].mag >= 4. && d.len2() < 14400. {
                        nearby += 1;
                        away += d;
                    }
                }
                let relief = away.unit() * (0.5 * smooth((nearby as f32 - 3.) / 3.));
                let speed = self.drift * (1. - 0.8 * self.calm);
                self.stars[i].wander = wander;
                let flow = current * 0.8 + wander;
                self.stars[i].target = flow * (speed * 9. / flow.len().max(9.)) + relief;
                continue;
            }
            let current = curl(s.p, phase_time, self.input.seed);
            let axis = current.unit();
            let mut target = current * 0.8;
            if let Some(a) = self.groups.get(s.group) {
                let offset = a.position(s.offset) - a.p;
                let tied = a.v
                    + pt(-offset.y, offset.x) * a.spin
                    + (a.p + offset - s.p)
                        * (0.055 + 0.16 * smooth((gravity - 0.5) * 2.) + 0.25 * self.gather)
                    + current * 0.14;
                target = target.mix(tied, a.spring);
            } else if gravity > 0.5
                && s.linkable()
                && let Some(a) = self
                    .groups
                    .iter()
                    .filter(|a| a.active)
                    .min_by(|a, b| (a.p - s.p).len2().total_cmp(&(b.p - s.p).len2()))
            {
                target += (a.p - s.p).unit() * (3. * gravity);
            }
            let mut pull = Point::default();
            let mut pressure = Point::default();
            let mut crowded = 0;
            let mut neighbors = 0;
            for j in self.grid.near(s.p, 260.) {
                if i == j {
                    continue;
                }
                let (center, mass) = self.bodies[j];
                let d = center - s.p;
                let r2 = d.len2();
                if !(0.0001..67600.).contains(&r2) || mass == 0. {
                    continue;
                }
                let r = r2.sqrt();
                let dir = d / r;
                pull += dir * (mass * 17500. / (r2 + 8100.));
                let linkable = s.linkable() && self.stars[j].linkable();
                let gap = if linkable && !s.companion && !self.stars[j].companion {
                    44. * self.art.scale.sqrt()
                        * (0.7 + 0.30 * (s.phase.fract() + self.stars[j].phase.fract()))
                } else if s.companion || self.stars[j].companion {
                    10.
                } else {
                    16.
                };
                if r < gap {
                    target += dir * (-(gap - r) * 0.37);
                }
                if r < 126. && linkable {
                    neighbors += 1;
                }
                if r < 84. && linkable {
                    crowded += 1;
                    pressure += dir * (-1.);
                }
            }
            self.stars[i].crowded = neighbors >= 4;
            target += pull * (gravity * (1. + self.gather * 1.5) * 2.15 / pull.len().max(2.15));
            if crowded >= 4 {
                target += pressure.unit() * (0.65 * (crowded - 2) as f32);
            }
            // The edge potential turns the current gradually. Its boundary
            // follows the patch and has no fixed column of resting positions.
            let edge = |x: f32, lo: f32, hi: f32| {
                let margin = ((hi - lo) * 0.12).min(160.);
                13. * (smooth((lo + margin - x) / margin) - smooth((x - hi + margin) / margin))
            };
            target += pt(edge(s.p.x, min.x, max.x), edge(s.p.y, min.y, max.y));
            let len = target.len();
            let ceiling = 9.
                + if self
                    .groups
                    .get(s.group)
                    .is_some_and(|a| a.gather_partner != FIELD)
                {
                    12. * self.gather
                } else {
                    0.
                };
            if len > ceiling {
                target = target * (ceiling / len);
            } else if len < 0.45 {
                target = if len > 0.0001 {
                    target * (0.45 / len)
                } else {
                    axis * 0.45
                };
            }
            self.stars[i].target = target * (self.drift * (1. - 0.8 * self.calm));
        }
        let integral = drag / (self.pace * 0.173287);
        for s in &mut self.stars {
            if !s.alive {
                continue;
            }
            let v0 = s.v;
            s.v = v0.mix(s.target, drag);
            let floor = 0.65 * self.drift * (1. - 0.8 * self.calm);
            if s.mag < 4. && s.v.len2() < floor * floor && !warm {
                let dir = if s.v.len2() > 0.001 {
                    s.v.unit()
                } else {
                    s.target.unit()
                };
                s.v = dir * floor;
            }
            // Integrate the exponential relaxation exactly for a long pre-roll
            // interval. Live steps use their small semi-implicit increment.
            s.p += if warm {
                s.target * dt + (v0 - s.target) * integral
            } else {
                s.v * dt
            };
            s.p.x = s.p.x.clamp(min.x, max.x);
            s.p.y = s.p.y.clamp(min.y, max.y);
            if !warm && self.params.lifeCycle {
                s.age += dt * self.pace;
            }
        }
    }
    fn separate(&mut self, warm: bool) {
        // Coarse force refresh is adequate for the 1100-px current. Close
        // encounters need a per-tick constraint or two bright stars can pass
        // through one another between force samples and repeatedly flare.
        self.contacts.clear();
        for (i, s) in self.stars.iter().enumerate().filter(|(_, s)| s.alive) {
            self.contacts.insert(s.p, i);
        }
        for i in 0..self.stars.len() {
            if !self.stars[i].alive {
                continue;
            }
            let bright = self.stars[i].linkable();
            let root = self.art.scale.sqrt();
            for j in self.contacts.near(
                self.stars[i].p,
                if bright { (62. * root).max(68.) } else { 16. },
            ) {
                if j <= i {
                    continue;
                }
                let d = self.stars[j].p - self.stars[i].p;
                let r2 = d.len2();
                let bright_pair = bright && self.stars[j].linkable();
                let contact = if self.stars[i].mag >= 4. && self.stars[j].mag >= 4. {
                    5.
                } else if bright_pair {
                    13.
                } else {
                    16.
                };
                let widest = if bright_pair {
                    (62. * root).max(68.)
                } else {
                    contact
                };
                if r2 >= widest * widest {
                    continue;
                }
                let a = self.stars[i];
                let b = self.stars[j];
                let co_natal =
                    (a.companion || b.companion) && a.group == b.group && a.offset == b.offset;
                let encounter = co_natal
                    || (self.binaries.is_empty()
                        && (a.phase.to_bits() ^ b.phase.to_bits()).is_multiple_of(997));
                let gap = if bright_pair && !encounter {
                    let preferred = (40. + 14. * (a.phase.fract() + b.phase.fract() - 0.5)) * root;
                    if a.crowded || b.crowded {
                        preferred.max(62. + 6. * a.phase.fract())
                    } else {
                        preferred
                    }
                } else {
                    contact
                };
                if r2 >= gap * gap || self.binaries.iter().any(|b| b.a == i && b.b == j) {
                    continue;
                }
                let r = r2.sqrt();
                let dir = if r > 0.001 {
                    d / r
                } else {
                    Point::polar((i * 71 + j * 113) as f32)
                };
                let correction = if warm { gap - r } else { (gap - r).min(0.65) };
                let total = self.stars[i].mass + self.stars[j].mass;
                // Passive specks yield to a figure crossing their path.
                // A crowd of tracers must not carry a massive star with it.
                let fraction = match (a.mag >= 4., b.mag >= 4.) {
                    (true, false) => 1.,
                    (false, true) => 0.,
                    _ => self.stars[j].mass / total,
                };
                self.stars[i].p += dir * (-correction * fraction);
                self.stars[j].p += dir * (correction * (1. - fraction));
                let approach = (self.stars[j].v - self.stars[i].v).dot(dir).min(0.);
                self.stars[i].v += dir * (approach * fraction);
                self.stars[j].v += dir * (-approach * (1. - fraction));
            }
        }
        let bounds = self.input.layout.bounds;
        for s in &mut self.stars {
            s.p.x = s.p.x.clamp(bounds.x as f32, bounds.right() as f32);
            s.p.y = s.p.y.clamp(bounds.y as f32, bounds.bottom() as f32);
        }
        self.regrid();
    }
    fn binaries(&mut self) {
        if self.tick.is_multiple_of(60) {
            for (a, s) in self.stars.iter().enumerate() {
                if !s.linkable() || self.binaries.iter().any(|p| p.a == a || p.b == a) {
                    continue;
                }
                for b in self.grid.near(s.p, 14.) {
                    if b <= a {
                        continue;
                    }
                    let q = self.stars[b];
                    if q.linkable()
                        && (q.p - s.p).len2() < 196.
                        && (q.v - s.v).len2() < 9.
                        && !self.binaries.iter().any(|p| p.a == b || p.b == b)
                        && self.rng.unit() < 0.3
                    {
                        let d = q.p - s.p;
                        let freq = std::f32::consts::TAU / rand(&mut self.rng, 6., 14.);
                        self.binaries.push(Binary {
                            a,
                            b,
                            turn: Point::polar(freq / 60.),
                            born: self.tick as f32 / 60.,
                            r: d.len().clamp(6., 12.),
                        });
                        break;
                    }
                }
            }
        }
        self.binaries.retain(|p| {
            // A close encounter or figure constraint can pull a pair apart.
            (self.stars[p.a].p - self.stars[p.b].p).len2() < 16. * 16.
        });
        for p in &self.binaries {
            let (a, b) = (self.stars[p.a], self.stars[p.b]);
            let f = b.mass / (a.mass + b.mass);
            let center = a.p.mix(b.p, f);
            let v = a.v.mix(b.v, f);
            // Rotate from the actual previous separation. A figure or contact
            // constraint may have paused the orbit for several ticks.
            let axis = b.p - a.p;
            let separation = axis.len();
            let dir = axis.unit();
            let orbit = pt(
                dir.x * p.turn.x - dir.y * p.turn.y,
                dir.x * p.turn.y + dir.y * p.turn.x,
            );
            let d = orbit * (separation + (p.r - separation).clamp(-0.08, 0.08));
            self.stars[p.a].p = center - d * f;
            self.stars[p.b].p = center + d * (1. - f);
            self.stars[p.a].v = v;
            self.stars[p.b].v = v;
        }
    }
    fn extinguish(&mut self, id: usize, time: f32) {
        // A nursery may reuse this slot in the same tick. Detach everything
        // that refers to the old star before publishing it as vacant.
        self.graph.sever(id, time, &self.stars);
        self.binaries.retain(|pair| pair.a != id && pair.b != id);
        self.stars[id].alive = false;
    }
    fn lifecycle(&mut self, t: f32) {
        if !self.params.lifeCycle {
            return;
        }
        for i in 0..self.stars.len() {
            let s = self.stars[i];
            if !s.alive || s.nova {
                continue;
            }
            if t >= s.retire_at {
                self.extinguish(i, t);
                continue;
            }
            if !s.fading && (s.age >= s.life - 6. * self.pace || s.retire_at - t <= 6.) {
                self.stars[i].fading = true;
                self.stars[i].fade_end = (t + (s.life - s.age) / self.pace).min(s.retire_at);
            }
            if s.age < s.life - 6. * self.pace {
                continue;
            }
            if s.age < s.life - 6. * self.pace + self.pace / 60.
                && t >= self.next_nova
                && self.rng.unit() < 0.20
            {
                self.spawn(Kind::Nova, Some(s.p), t);
                self.next_nova = t + rand(&mut self.rng, 480., 1200.) / self.pace.min(2.);
                if self.stars[i].nova {
                    continue;
                }
            }
            if s.age < s.life {
                continue;
            }
            self.extinguish(i, t);
        }
        // Births replenish associations as their members disperse. A lone
        // death in the field is replenished by a faint star at another nursery.
        if self.tick.is_multiple_of(60) {
            let active = self
                .groups
                .iter()
                .filter(|a| a.active && a.life - a.age > 45.)
                .count();
            if active < self.target_groups {
                self.spawn(Kind::Birth, None, t);
            }
            let count = self.stars.iter().filter(|s| s.alive).count();
            if count < self.target_count
                && !self
                    .events
                    .iter()
                    .any(|e| e.kind == Kind::Birth && e.emitted < e.count)
            {
                let p = self.choose_nursery(true);
                if let Some(slot) = self.stars.iter_mut().find(|s| !s.alive) {
                    *slot = Star::new(p, &mut self.rng, false);
                    slot.age = 0.;
                    slot.born = t;
                }
            }
        }
    }
}
impl LivingWorld for ConstellationWorld {
    fn rate(&self) -> Rate {
        Rate::new(60, 1).unwrap()
    }
    fn step(&mut self, tick: u64) {
        self.tick = tick;
        if self.input.reduced_motion {
            return;
        }
        let t = tick as f32 / 60.;
        if tick.is_multiple_of(5400) {
            self.spent.retain(|&(_, until)| until > t);
            self.record_occupancy();
        }
        self.events_step(t, 1. / 60.);
        let still = self.calm > 0.;
        self.move_stars(t, 1. / 60., tick as usize % 24, 24, false);
        self.lifecycle(t);
        self.binaries();
        self.separate(false);
        self.graph
            .update(&self.stars, &self.grid, t, 1. / 60., self.pace, still);
        if tick.is_multiple_of(15) {
            self.graph.recognise(&mut self.stars, t);
        }
    }
    fn snapshot(&self) -> Snapshot {
        let mut pool = self.frames.borrow_mut();
        let slot = pool
            .iter()
            .position(|f| Arc::strong_count(f) == 1)
            .unwrap_or_else(|| {
                pool.push(Arc::new(render::Frame::new(self.art.clone())));
                pool.len() - 1
            });
        let f = Arc::get_mut(&mut pool[slot]).unwrap();
        f.stars.clone_from(&self.stars);
        self.graph.mark_linked(&mut f.stars);
        f.edges.clone_from(&self.graph.edges);
        f.events.clone_from(&self.events);
        f.binaries.clone_from(&self.binaries);
        f.calm = self.calm;
        f.still = self.input.reduced_motion;
        f.life = self.params.lifeCycle;
        Snapshot {
            tick: self.tick,
            time_ns: self.rate().tick_ns(self.tick),
            generations: Generations {
                layout: self.input.layout.generation,
                topology: self.graph.changes,
                data: 1,
                sample: self.tick,
            },
            typed: Some(pool[slot].clone()),
            texture: self.art.band.clone(),
            emission_estimate: self.emission_estimate(),
            ..self.empty.clone()
        }
    }
    fn emission_estimate(&self) -> f64 {
        0.003 + self.events.len().min(20) as f64 * 0.0001
    }
    fn inject(&mut self, e: Event) -> Result<(), &'static str> {
        if !(1..=10).contains(&e.id)
            || e.at_ns < 0
            || !e.anchor.iter().all(|v| v.is_finite())
            || !e.strength.is_finite()
        {
            return Err("invalid constellation event");
        }
        self.script.push(e);
        self.script.sort_by_key(|e| e.at_ns);
        Ok(())
    }
    fn state_hash(&self) -> u64 {
        let mut h = hash64(self.input.seed ^ self.tick);
        for s in &self.stars {
            for x in [s.p.x, s.p.y, s.v.x, s.v.y, s.mag, s.temp, s.age] {
                h = hash64(h ^ u64::from(x.to_bits()))
            }
        }
        for e in &self.graph.edges {
            h = hash64(h ^ e.a as u64 ^ ((e.b as u64) << 16) ^ u64::from(e.born.to_bits()))
        }
        h
    }
}
