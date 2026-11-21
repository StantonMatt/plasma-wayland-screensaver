//! A colony of liquid spheres, integrated on the world's fixed clock.
mod color;
mod events;
mod life;
mod plankton;
mod render;
mod space;
pub use render::{Record as OrbRecord, visuals_orbs_records};
#[cfg(test)]
mod tests;
use super::common::{
    clock::Rate,
    flow::{FlowField, FlowSettings, p},
    layout::{Layout, Rect, Vec2},
    rng::{Pcg32, hash64},
};
use super::world::*;
use super::{Descriptor, MirroredPolicy, schema::Parameter};
use color::Ramp;
use space::{Clock, Grid};
use std::{cell::RefCell, f64::consts::TAU, sync::Arc};
#[allow(non_snake_case)]
#[derive(Clone, Debug)]
pub struct Params {
    pub look: &'static str,
    pub flow: &'static str,
    pub merging: i32,
    pub depth: i32,
    pub worldEvents: bool,
}
impl Default for Params {
    fn default() -> Self {
        Self {
            look: "glow",
            flow: "drift",
            merging: 50,
            depth: 60,
            worldEvents: true,
        }
    }
}
pub const PARAMETERS: &[Parameter] = &[
    Parameter::enumeration(1, c"look", c"glow|glass|bubbles", 2., 0.),
    Parameter::enumeration(2, c"flow", c"drift|rise|swirl", 2., 0.),
    Parameter::integer(3, c"merging", 0., 100., 50.),
    Parameter::integer(4, c"depth", 0., 100., 60.),
    Parameter::boolean(5, c"worldEvents", true),
];
pub const DESCRIPTOR: Descriptor = Descriptor {
    id: 8,
    key: "orbs",
    available: true,
    rate: 30,
    mirrored: MirroredPolicy::SharedCanonical,
    fit: super::MirroredFit::Cover,
    parameters: PARAMETERS,
    create: |input| Some(Box::new(Colony::new(input))),
};
fn range(r: &mut Pcg32, lo: f64, hi: f64) -> f64 {
    lo + (hi - lo) * r.unit()
}
fn axis(angle: f64) -> Vec2 {
    let (s, c) = angle.sin_cos();
    p(c, s)
}
fn hue_delta(a: f64, b: f64) -> f64 {
    (b - a + 0.5).rem_euclid(1.) - 0.5
}
fn ease(t: f64) -> f64 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}
#[derive(Clone, Copy, Debug, Default)]
struct Orb {
    id: u32,
    pos: Vec2,
    home: Vec2,
    vel: Vec2,
    wander: Vec2,
    z: f64,
    mass: f64,
    hue: f64,
    energy: f64,
    temperature: f64,
    heating: bool,
    age: f64,
    half_life: f64,
    phase: f64,
    angle: f64,
    wobble: [f64; 2],
    swing: [f64; 2],
    radius: f64,
    split: f64,
    shiver: f64,
    cooldown: f64,
    bloom_travel: f64,
    stream: bool,
    train: usize,
    stream_station: f64,
    release_at: f64,
    flare: f64,
    links: u8,
    neighbours: [u32; 4],
    partner: Option<usize>,
}
impl Orb {
    fn alive(self) -> bool {
        self.mass > 0.
    }
    fn recalc(&mut self, base: f64) {
        self.radius = base * self.mass.sqrt() * (0.45 + 1.15 * self.z);
    }
}
#[derive(Clone, Copy, Default)]
struct Mote {
    id: u32,
    pos: Vec2,
    vel: Vec2,
    z: f64,
    hue: f64,
    mass: f64,
    life: f64,
    max_life: f64,
    current: Vec2,
    ejecta_slip: Vec2,
    cue: bool,
}
#[derive(Clone, Copy)]
struct Ring {
    id: u32,
    pos: Vec2,
    z: f64,
    hue: f64,
    r: f64,
    age: f64,
}
#[derive(Clone, Copy, PartialEq, Debug)]
enum LinkKind {
    Kiss,
    Merge,
    Engulf,
    Raft,
}
#[derive(Clone, Copy)]
struct Link {
    a: usize,
    b: usize,
    ids: [u32; 2],
    kind: LinkKind,
    age: f64,
    duration: f64,
}
#[derive(Clone, Copy)]
struct Pass {
    pos: Vec2,
    v: Vec2,
    r: f64,
    age: f64,
    life: f64,
    hue: f64,
    id: u32,
}
pub struct Colony {
    input: WorldInput,
    params: Params,
    rng: Pcg32,
    flow: FlowField,
    plankton: plankton::Plankton,
    clock: Vec<Clock>,
    base: f64,
    height: f64,
    target: usize,
    target_mass: f64,
    orbs: Vec<Orb>,
    motes: Vec<Mote>,
    rings: Vec<Ring>,
    links: Vec<Link>,
    grid: Grid,
    mote_grid: Grid,
    forces: Vec<Vec2>,
    coupling: Vec<f64>,
    births: Vec<(usize, usize)>,
    tick: u64,
    time: f64,
    serial: u32,
    thermostat: f64,
    next_pass: f64,
    next_fission: f64,
    next_feeding_ground: f64,
    pass: Option<Pass>,
    next_event: f64,
    event: Option<events::Moment>,
    script: Vec<Event>,
    art: Arc<Ramp>,
    frames: RefCell<Vec<Arc<render::Frame>>>,
    empty: Snapshot,
    stats: [u64; 6],
    #[cfg(feature = "visual-capture")]
    capture_parity: Option<u32>,
}
impl Colony {
    pub fn new(input: WorldInput) -> Self {
        let mut params = Params::default();
        for v in &input.parameters {
            match v.id {
                1 => params.look = ["glow", "glass", "bubbles"][v.payload as usize],
                2 => params.flow = ["drift", "rise", "swirl"][v.payload as usize],
                3 => params.merging = v.payload as i32,
                4 => params.depth = v.payload as i32,
                5 => params.worldEvents = v.payload != 0,
                _ => {}
            }
        }
        let mut heights: Vec<_> = input.layout.outputs.iter().map(|o| o.rect.height).collect();
        heights.sort_by(f64::total_cmp);
        let height = heights[heights.len() / 2].clamp(32., 4320.);
        let area = input.layout.visible_area / (2560. * 1440.);
        let n = (if input.density <= 50. {
            8. + (input.density - 10.) * 0.35
        } else {
            22. + (input.density - 50.) * 0.66
        }) * area;
        let target = (n.round() as usize).clamp(4, 200);
        // Compress the very largest sizes in a fully stocked tank. Contact
        // geometry and the visible bodies keep the same physical radius.
        let crowd = ((input.density - 45.) / 55.).clamp(0., 1.);
        let body_scale = 0.071 - 0.035 * ease((input.density - 50.) / 50.);
        let base =
            height * body_scale * input.scale / (1. + 0.57 * crowd * (input.scale - 1.).max(0.));
        let count = target * 3 / 2 + 5;
        let motes = (target * 4 + 36).min(720);
        let art = Arc::new(Ramp::new(input.palette));
        let mut world = Self {
            plankton: plankton::Plankton::new(&input, params.depth, height),
            flow: FlowField::with_wall_clearance(
                input.seed,
                &input.layout,
                FlowSettings::orbs(params.flow == "swirl", area, height / 1440.),
                if params.flow == "swirl" { 1.5 } else { 0. },
            ),
            rng: Pcg32::new(input.seed, 0x0bb5),
            clock: Clock::new(&input),
            base,
            height,
            target,
            target_mass: 0.,
            params,
            orbs: vec![Orb::default(); count],
            forces: vec![Vec2::default(); count],
            coupling: vec![0.; count],
            grid: Grid::new(count, base * 3.8),
            mote_grid: Grid::new(motes, 40.),
            births: Vec::with_capacity(count),
            links: Vec::with_capacity(count * 2),
            motes: vec![Mote::default(); motes],
            rings: Vec::with_capacity(16),
            frames: RefCell::new(Vec::new()),
            empty: Snapshot::default(),
            art,
            input,
            tick: 0,
            time: 0.,
            serial: 0,
            thermostat: 4.,
            next_pass: 260.,
            next_fission: 160. / area.max(0.25),
            next_feeding_ground: 18.,
            pass: None,
            next_event: 360.,
            event: None,
            script: Vec::new(),
            stats: [0; 6],
            #[cfg(feature = "visual-capture")]
            capture_parity: None,
        };
        world.populate();
        world
    }
    fn luminous_radius(&self, orb: Orb, depth: f64) -> f64 {
        let outer = orb
            .radius
            .min(self.height * if depth > 0.85 { 0.18 } else { 0.155 });
        // In Glow, the emitting body sits just inside the soft contact layer.
        // Its halo extends through that layer and illuminates an approaching neighbour.
        outer * if self.params.look == "glow" { 0.94 } else { 1. }
    }
    fn point(&mut self) -> Vec2 {
        self.input
            .layout
            .choose_visible_anchor(&mut self.rng, 0.)
            .unwrap()
    }
    fn id(&mut self) -> u32 {
        self.serial = self.serial.wrapping_add(1).max(1);
        self.serial
    }
    fn colony_anchor(&mut self, screens: &[Rect], group: usize, groups: usize) -> Vec2 {
        // Stratification prevents an initial empty monitor. Jitter stays inside
        // each broad band; the fluid moves freely across their boundaries.
        let target =
            (group as f64 + 0.5) / groups as f64 * screens.iter().map(|r| r.area()).sum::<f64>();
        let mut sum = 0.;
        let mut screen = *screens.last().unwrap();
        let mut local = 0.5;
        for &r in screens {
            if target < sum + r.area() {
                screen = r;
                local = (target - sum) / screen.area();
                break;
            }
            sum += r.area();
        }
        let x = (local + range(&mut self.rng, -0.07, 0.07)).clamp(0.07, 0.93);
        let y = (0.12 + 0.76 * ((group as f64 * 0.618034 + self.rng.unit() * 0.32).fract()))
            .clamp(0.1, 0.9);
        p(screen.x + screen.width * x, screen.y + screen.height * y)
    }
    fn make_orb(&mut self, pos: Vec2, z: f64, hue: f64, mass: f64) -> Orb {
        let mut o = Orb {
            id: self.id(),
            pos,
            home: pos,
            z,
            hue,
            mass,
            energy: range(&mut self.rng, 0.52, 1.),
            phase: self.rng.unit() * TAU,
            half_life: range(&mut self.rng, 180., 360.),
            angle: self.rng.unit() * TAU,
            cooldown: 6.,
            ..Default::default()
        };
        // The initial suspension has both hot and cold bodies, including at
        // the same depth. A new seed retains the thermal history of its lane.
        o.temperature = 0.5 + 0.38 * o.phase.sin();
        o.heating = o.phase.cos() >= 0.;
        o.recalc(self.base);
        o
    }
    fn populate(&mut self) {
        let depth = self.params.depth as f64 / 100.;
        // Feeding grounds follow physical screen order, independent of the
        // connector hashes or the order the compositor announced outputs.
        let mut screens: Vec<_> = self.input.layout.outputs.iter().map(|o| o.rect).collect();
        screens.sort_by(|a, b| {
            a.x.total_cmp(&b.x)
                .then(a.y.total_cmp(&b.y))
                .then(a.width.total_cmp(&b.width))
                .then(a.height.total_cmp(&b.height))
        });
        let mut centre = self.point();
        let mut z = 0.5;
        let mut hue = 0.4;
        let mut near = vec![false; screens.len()];
        let group_size = if self.params.merging < 20 { 6 } else { 3 };
        let cluster_span = if self.params.merging == 0 { 2.45 } else { 3.8 };
        for i in 0..self.target {
            if i % group_size == 0 {
                centre =
                    self.colony_anchor(&screens, i / group_size, self.target.div_ceil(group_size));
                hue = self.rng.unit();
                let draw = self.rng.unit();
                z = if draw < 0.28 {
                    range(&mut self.rng, 0.01, 0.22)
                } else {
                    range(&mut self.rng, 0.42, 0.78)
                };
            }
            let screen = screens.iter().position(|r| r.contains(centre)).unwrap_or(0);
            let giant = i % 5 == 1 && !near[screen];
            let slab = if giant {
                near[screen] = true;
                0.96
            } else {
                (z + range(&mut self.rng, -0.048, 0.048)).clamp(0.01, 0.84)
            };
            let true_z = (0.55 + (slab - 0.55) * (depth / 0.65).min(1.15)).clamp(0.01, 0.97);
            let pos = if i % group_size == 0 {
                centre
            } else {
                self.input.layout.nearest_visible(
                    centre
                        + axis(self.rng.unit() * TAU)
                            * range(&mut self.rng, self.base * 1.2, self.base * cluster_span),
                )
            };
            let mass = if giant {
                range(&mut self.rng, 3.7, 6.3)
            } else {
                0.26 + 2.8 * self.rng.unit().powi(2)
            };
            let h = (hue + range(&mut self.rng, -0.09, 0.09)).rem_euclid(1.);
            let mut o = self.make_orb(pos, true_z, h, mass);
            o.age = range(&mut self.rng, 25., 360.);
            o.vel = self.flow.sample(pos).velocity;
            o.energy = range(&mut self.rng, 0.13, 0.98);
            self.orbs[i] = o;
        }
        // Distribute the initial stock of luminous area by habitat size. The
        // screen union is open after seeding and every radius follows live mass.
        let area: f64 = self.orbs.iter().map(|o| o.radius * o.radius).sum();
        for screen in &screens {
            let local: f64 = self
                .orbs
                .iter()
                .filter(|o| o.alive() && screen.contains(o.pos))
                .map(|o| o.radius * o.radius)
                .sum();
            if local <= 0. {
                continue;
            }
            let growth =
                (screen.area() / self.input.layout.visible_area * area / local).clamp(0.70, 1.45);
            for orb in self
                .orbs
                .iter_mut()
                .filter(|o| o.alive() && screen.contains(o.pos))
            {
                orb.mass *= growth;
                orb.recalc(self.base);
            }
        }
        if screens.len() > 1 {
            // A seeded group can draw several large bodies on the same side.
            // Balance the initial column of water without moving an edge.
            for r in &screens {
                let mut mass = 0.;
                let mut moment = 0.;
                let mut mobility = 0.;
                for o in self.orbs.iter().filter(|o| o.alive() && r.contains(o.pos)) {
                    let y = (o.pos.y - r.y) / r.height;
                    let w = o.radius * o.radius;
                    mass += w;
                    moment += y * w;
                    mobility += 4. * y * (1. - y) * w;
                }
                let shift = ((0.5 * mass - moment) / mobility.max(1.)).clamp(-0.23, 0.23);
                for o in self
                    .orbs
                    .iter_mut()
                    .filter(|o| o.alive() && r.contains(o.pos))
                {
                    let y = (o.pos.y - r.y) / r.height;
                    o.pos.y += shift * 4. * y * (1. - y) * r.height;
                    o.home = o.pos;
                }
            }
        }
        self.target_mass = self.orbs.iter().map(|o| o.mass).sum();
        for i in 0..self.target * 2 {
            let pos = self.point();
            let z = self.rng.unit();
            let hue = self.rng.unit();
            self.seed_mote(i, pos, z, hue, 0.025, Vec2::default());
        }
        self.next_event = range(&mut self.rng, 300., 600.);
        self.next_pass = range(&mut self.rng, 240., 480.);
        // Settle a mature field before the first presentation. Coarse steps are
        // safe here: the same contact and containment solvers cap displacement.
        let settle = if self.input.reduced_motion { 90. } else { 8. };
        for _ in 0..(settle * 4.) as usize {
            self.update(0.25, true);
        }
        self.time = 0.;
        self.thermostat = 3.;
        self.stats = [0; 6];
    }
    fn seed_mote(&mut self, i: usize, pos: Vec2, z: f64, hue: f64, mass: f64, vel: Vec2) {
        let life = range(&mut self.rng, 30., 60.);
        self.motes[i] = Mote {
            id: self.id(),
            pos,
            z,
            hue,
            mass,
            vel,
            life,
            max_life: life,
            current: space::tracer_current(&self.input.layout, &self.flow, pos).velocity,
            ejecta_slip: Vec2::default(),
            cue: false,
        };
    }
    fn update(&mut self, dt: f64, warming: bool) {
        self.time += dt;
        self.flow.advance(&self.input.layout, dt);
        self.plankton.advance(&self.input.layout, &self.flow, dt);
        self.forces.fill(Vec2::default());
        self.coupling.fill(0.);
        self.grid.clear();
        for (i, o) in self.orbs.iter().enumerate() {
            if o.alive() {
                self.grid.add(i, o.pos)
            }
        }
        if !warming {
            self.update_events(dt);
        }
        self.interactions(dt);
        self.update_links(dt);
        self.update_motes(dt);
        let height = self.height;
        let spanning = self.input.layout.outputs.len() > 1;
        let rising = self.params.flow == "rise";
        let mut displacement = 0.;
        let vertical_return = if spanning || rising {
            let mut moment = 0.;
            let mut total = 0.;
            let mid = self.input.layout.bounds.centre().y;
            for o in self.orbs.iter().filter(|o| o.alive()) {
                let area = o.radius * o.radius;
                moment += (mid - o.pos.y) * area;
                total += area;
            }
            // In the closed vessel, a rising concentration displaces water
            // downward. Both ascending and sinking parcels feel that current.
            displacement = moment / total.max(1.) / height;
            if spanning {
                (0.22 * moment / total.max(1.)).clamp(-55., 55.)
            } else {
                0.
            }
        } else {
            0.
        };
        for (i, o) in self.orbs.iter_mut().enumerate() {
            if !o.alive() {
                continue;
            }
            let local = self.flow.sample(o.pos).velocity;
            let drag = if o.bloom_travel > 0. {
                5.8
            } else {
                1.2 * (o.radius / self.base).sqrt()
            };
            let k = dt / (drag + dt);
            o.wander += p(self.rng.unit() - 0.5, self.rng.unit() - 0.5) * (8. * dt.sqrt())
                - o.wander * (dt * 0.32);
            let mut flow = local * (0.5 + 0.7 * o.z);
            if rising {
                if let Some(interval) = self
                    .input
                    .layout
                    .vertical_intervals(o.pos.x)
                    .iter()
                    .find(|v| o.pos.y >= v.start && o.pos.y < v.end)
                {
                    let y = (o.pos.y - interval.start) / (interval.end - interval.start);
                    // A parcel keeps its thermal history between the hot floor
                    // and cool ceiling. Neighbours at the ceiling exchange heat
                    // faster. Fusion energy has no effect on buoyancy.
                    if y < 0.35 {
                        o.heating = false;
                    } else if y > 0.78 {
                        o.heating = true;
                    }
                    let (target, tau) = if o.heating {
                        (0.96, 40.)
                    } else {
                        (0.04, 48. / (1. + 0.16 * f64::from(o.links)))
                    };
                    o.temperature += (target - o.temperature) * dt / tau;
                    if self.input.palette == 2 {
                        let warmth = ease((o.temperature - 0.31 + 0.14 * (0.60 - y)) / 0.28);
                        let warmth = warmth * (0.44 + 0.56 * ease((0.90 - y) / 0.42));
                        let pigment = 0.036 + 0.405 * warmth;
                        let branch = if o.hue < 0.5 { pigment } else { 1. - pigment };
                        o.hue = (o.hue + hue_delta(o.hue, branch) * dt * 0.09).rem_euclid(1.);
                    }
                }
                flow.y =
                    0.15 * flow.y + (0.50 - o.temperature + 1.8 * displacement) * height * 0.22;
            }
            let wall = space::wall(
                &self.input.layout,
                o.pos,
                (o.radius * 1.45 + height * 0.055).max(1.),
            );
            // A basin has a return current. Counter the prevailing flow away
            // from each feeding ground so a six-hour drift cannot deposit the
            // entire colony against the downstream wall. Events open the basin.
            let offset = o.home - o.pos;
            let mut return_current = (offset * 0.093).limited(height * 0.115);
            if rising {
                return_current.y *= 0.24;
            }
            return_current.y += vertical_return;
            let open = o.stream
                || o.bloom_travel > 0.
                || self
                    .event
                    .as_ref()
                    .is_some_and(|e| e.kind == events::Kind::Foam && e.screen.contains(o.pos));
            if !open {
                flow += return_current;
            }
            if spanning {
                flow = space::shore_flow(&self.input.layout, o.pos, flow, 300. * height / 1440.);
            } else {
                flow = space::slip(&self.input.layout, o.pos, flow, 300. * height / 1440.);
            }
            flow += wall * (height * 0.48);
            for c in &self.clock {
                // Anticipate the moving label over the particle's drag time.
                let clock = Rect {
                    x: c.rect.x + c.v.x * 0.65,
                    y: c.rect.y + c.v.y * 0.65,
                    ..c.rect
                };
                let away = space::repel(clock, o.pos, 1.85 * o.radius + 0.42 * c.rect.height);
                flow += away * (0.4 * height);
            }
            let incoming_velocity = o.vel;
            o.vel += ((flow + o.wander - o.vel) * k * if o.stream { 0.10 } else { 1. }
                + self.forces[i] * dt)
                .limited(
                    (if o.stream {
                        height * 0.18
                    } else {
                        height * 0.14
                    }) * dt,
                );
            let mut next = o.pos + o.vel.limited(height * if o.stream { 0.30 } else { 0.18 }) * dt;
            if let Some(e) = &self.event
                && e.kind == events::Kind::Dandelion
                && i == e.heart
                && !e.bloomed
            {
                // The heart roots in the current. Its tether follows a damped
                // spring in the interior of the anchor screen, even through fusion.
                let limit = height / 12.;
                let offset = (o.pos - e.anchor).limited(limit);
                o.vel = (incoming_velocity * (1. - 2.5 * dt) - offset * (3.2 * dt))
                    .limited(height * 0.12);
                next = e.anchor + (offset + o.vel * dt).limited(limit);
                next.x = next.x.clamp(
                    e.screen.x + 0.25 * e.screen.width,
                    e.screen.x + 0.75 * e.screen.width,
                );
                next.y = next.y.clamp(
                    e.screen.y + 0.25 * e.screen.height,
                    e.screen.y + 0.75 * e.screen.height,
                );
                o.energy = (o.energy + dt * 0.4).min(1.);
            }
            if o.bloom_travel > 0.
                && let Some(e) = &self.event
                && e.kind == events::Kind::Dandelion
                && e.bloomed
            {
                // The expanding pulse carries the daughters in separate
                // directions. It dissolves into the local water near the end.
                let age = 18. - o.bloom_travel;
                let ray = (o.home - e.anchor).unit();
                let margin = (e.screen.right() - e.anchor.x)
                    .min(e.anchor.x - e.screen.x)
                    .min(e.screen.bottom() - e.anchor.y)
                    .min(e.anchor.y - e.screen.y);
                let reach = (height * 0.46).min(margin * 0.91);
                let decay = (-age / 3.6).exp();
                let target = e.anchor + ray * (reach * (1. - 0.83 * decay));
                let drift = ray * (reach * 0.83 * decay / 3.6) + (target - o.pos) * 1.1;
                o.vel += (drift - o.vel) * (dt * 2.6 * ease(o.bloom_travel / 3.)).min(1.);
                next = o.pos + o.vel * dt;
            }
            if self.input.layout.contains(next) {
                o.pos = next
            } else {
                let fixed = self.input.layout.nearest_visible(next);
                let n = (fixed - next).unit();
                o.vel += n * (-1.4 * o.vel.dot(n).min(0.));
                o.pos = fixed;
            }
            o.phase = (o.phase
                + dt * (TAU / (7. + (o.radius / self.base * 2.).min(6.))
                    + self.coupling[i].clamp(-0.25, 0.25)))
            .rem_euclid(TAU);
            for (j, w) in o.wobble.iter_mut().enumerate() {
                let freq = 5. + j as f64 * 1.2;
                o.swing[j] += (-freq * freq * *w - 2. * o.swing[j]) * dt.min(0.12);
                *w = (*w + o.swing[j] * dt.min(0.12)).clamp(-0.18, 0.18);
            }
            o.age += dt;
            o.bloom_travel = (o.bloom_travel - dt).max(0.);
            o.flare = (o.flare - dt).max(0.);
            o.cooldown = (o.cooldown - dt).max(0.);
            o.energy *= 1. - dt * std::f64::consts::LN_2 / o.half_life;
            // Younger cells draw fuel from the water. Their older counterparts
            // gradually cool and make room for the next generation.
            if o.age < o.half_life * 1.7 {
                o.energy += (1. - o.energy) * dt * 0.008;
            }
        }
        // At most one ordinary foreground sphere in either half of an output.
        // Moving, splitting and accreting at a bezel can all change occupancy.
        let mut near_halves = [0u8; 64];
        for o in self.orbs.iter_mut().filter(|o| o.alive() && o.z > 0.85) {
            if let Some((i, screen)) = self
                .input
                .layout
                .outputs
                .iter()
                .enumerate()
                .find(|(_, s)| s.rect.contains(o.pos))
            {
                let half = 1 << usize::from(o.pos.x > screen.rect.centre().x);
                if near_halves[i] & half != 0 {
                    o.z = 0.845;
                    o.recalc(self.base);
                }
                near_halves[i] |= half;
            }
        }
        self.life_cycle(dt, warming);
        for r in &mut self.rings {
            r.age += dt
        }
        self.rings.retain(|r| r.age < 0.8);
    }
}
impl LivingWorld for Colony {
    fn rate(&self) -> Rate {
        if self.input.reduced_motion {
            Rate::new(1, 368).unwrap()
        } else {
            Rate::new(30, 1).unwrap()
        }
    }
    fn next_wake_ns(&self, ns: i64) -> Option<i64> {
        if !self.input.reduced_motion {
            return None;
        }
        let period = 368_000_000_000;
        let base = ns - ns.rem_euclid(period);
        let start = base + 360_000_000_000;
        Some(if ns < start {
            start
        } else {
            (ns + 66_666_667).min(base + period)
        })
    }
    fn step(&mut self, tick: u64) {
        if self.input.reduced_motion {
            let mut next = self.input.clone();
            next.seed = hash64(next.seed ^ tick);
            let mut composition = Self::new(next);
            composition.input.seed = self.input.seed;
            composition.tick = tick;
            *self = composition;
            return;
        }
        self.tick = tick;
        for c in &mut self.clock {
            c.advance(&self.input.layout, 1. / 30.);
        }
        let n = if self.input.speed >= 2. { 2 } else { 1 };
        for _ in 0..n {
            self.update(self.input.speed / (30. * f64::from(n)), false);
        }
    }
    fn snapshot(&self) -> Snapshot {
        let mut frames = self.frames.borrow_mut();
        let index = frames
            .iter()
            .position(|f| Arc::strong_count(f) == 1)
            .unwrap_or(frames.len());
        if index == frames.len() {
            frames.push(Arc::new(render::Frame::default()))
        }
        self.frame(Arc::get_mut(&mut frames[index]).unwrap());
        Snapshot {
            tick: self.tick,
            time_ns: self.rate().tick_ns(self.tick),
            generations: Generations {
                layout: self.input.layout.generation,
                data: self.tick + 1,
                sample: self.tick,
                topology: 0,
            },
            emission_estimate: self.emission_estimate(),
            typed: Some(frames[index].clone()),
            ..self.empty.clone()
        }
    }
    fn state_hash(&self) -> u64 {
        self.orbs
            .iter()
            .filter(|o| o.alive())
            .fold(self.tick, |h, o| {
                hash64(
                    h ^ u64::from(o.id)
                        ^ o.pos.x.to_bits()
                        ^ o.pos.y.to_bits()
                        ^ o.hue.to_bits()
                        ^ o.mass.to_bits(),
                )
            })
    }
    fn inject(&mut self, e: Event) -> Result<(), &'static str> {
        // Optical controls draw complementary halves of the same settled
        // world. They must not consume a random value or affect the fill budget.
        #[cfg(feature = "visual-capture")]
        if matches!(e.id, 7 | 8) && e.at_ns == 0 {
            self.capture_parity = Some(e.id - 7);
            return Ok(());
        }
        if events::Kind::from_id(e.id).is_none() && !matches!(e.id, 5 | 6) {
            return Err("unknown Orbs event");
        }
        self.script.push(e);
        self.script.sort_by_key(|e| e.at_ns);
        Ok(())
    }
    fn emission_estimate(&self) -> f64 {
        self.orbs
            .iter()
            .filter(|o| o.alive())
            .map(|o| o.radius.powi(2) * (0.5 + o.energy))
            .sum::<f64>()
            / self.input.layout.visible_area
    }
}
