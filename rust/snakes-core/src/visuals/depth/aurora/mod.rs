//! A seeded northern sky. The 20 Hz world owns the weather and all material coordinates.
use super::{Descriptor, MirroredPolicy, schema::Parameter};
#[allow(non_snake_case)]
#[derive(Clone, Debug)]
pub struct Params {
    pub activity: i32,
    pub horizon: &'static str,
    pub stars: bool,
    pub skyEvents: bool,
}
impl Default for Params {
    fn default() -> Self {
        Self {
            activity: 40,
            horizon: "forest",
            stars: true,
            skyEvents: true,
        }
    }
}
pub const PARAMETERS: &[Parameter] = &[
    Parameter::integer(1, c"activity", 0., 100., 40.),
    Parameter::enumeration(2, c"horizon", c"none|forest|mountains", 2., 1.),
    Parameter::boolean(3, c"stars", true),
    Parameter::boolean(4, c"skyEvents", true),
];
pub const DESCRIPTOR: Descriptor = Descriptor {
    id: 1,
    key: "aurora",
    available: true,
    rate: 20,
    mirrored: MirroredPolicy::SharedCanonical,
    fit: super::MirroredFit::Cover,
    parameters: PARAMETERS,
    create: |input| Some(Box::new(Aurora::new(input))),
};
mod curtain;
mod landscape;
pub mod mesh;
pub mod render;
#[cfg(test)]
mod tests;
mod weather;
use super::{
    common::{
        clock::Rate,
        rng::{Pcg32, hash64},
    },
    world::{Event, Generations, LivingWorld, Snapshot, WorldInput},
};
use curtain::{Curtain, Formation};
use landscape::Sky;
use std::{cell::RefCell, sync::Arc};
use weather::{CORONA, FIREBALL, METEOR, SUBSTORM, Weather};

fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
fn smooth(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn range(r: &mut Pcg32, lo: f32, hi: f32) -> f32 {
    mix(lo, hi, r.unit() as f32)
}
fn unit(i: i32, seed: u64) -> f32 {
    (hash64(seed ^ (i as u64).wrapping_mul(0x9e3779b9)) >> 40) as f32 / 16777216.
}
fn noise(x: f32, seed: u64) -> f32 {
    let i = x.floor() as i32;
    mix(
        unit(i, seed),
        unit(i.wrapping_add(1), seed),
        smooth(x - x.floor()),
    ) * 2.
        - 1.
}
const STILL_PERIOD: i64 = 600_000_000_000;
fn still_transition(seed: u64, ns: i64) -> (f32, i64) {
    let cycle = ns / STILL_PERIOD;
    let start = |cycle: i64| {
        // First exposure starts near ten minutes. Subsequent holds vary by up
        // to a minute either side of the nominal interval.
        let jitter = hash64(seed ^ cycle as u64)
            % if cycle == 1 {
                5_000_000_000
            } else {
                60_000_000_000
            };
        cycle
            .saturating_mul(STILL_PERIOD)
            .saturating_add(jitter as i64)
    };
    let first = start(cycle);
    if cycle == 0 {
        return (0., start(1));
    }
    if ns < first {
        return (0., first);
    }
    let end = first.saturating_add(20_000_000_000);
    if ns >= end {
        return (1., start(cycle + 1));
    }
    // Captures, repaints and a late timer all select the same exposure. Between
    // two sparse wakes, an unrelated scene-graph update cannot move the sky.
    let slot = (ns - first) / 500_000_000;
    (
        smooth(slot as f32 / 40.),
        first.saturating_add((slot + 1) * 500_000_000).min(end),
    )
}
pub struct Aurora {
    input: WorldInput,
    params: Params,
    rng: Pcg32,
    arcs: Vec<Curtain>,
    sky: Arc<Sky>,
    weather: Weather,
    tick: u64,
    next_id: u32,
    capacity: usize,
    normal: usize,
    next_encounter: f64,
    script: Vec<Event>,
    frames: RefCell<Vec<Arc<render::Frame>>>,
    empty: Snapshot,
}
impl Aurora {
    pub fn new(input: WorldInput) -> Self {
        let mut params = Params::default();
        for p in &input.parameters {
            match p.id {
                1 => params.activity = p.payload as i32,
                2 => params.horizon = ["none", "forest", "mountains"][p.payload.min(2) as usize],
                3 => params.stars = p.payload != 0,
                4 => params.skyEvents = p.payload != 0,
                _ => {}
            }
        }
        let sky = Arc::new(Sky::new(&input, &params));
        let mut rng = Pcg32::new(input.seed, 0xa1c);
        let weather = Weather::new(&mut rng, params.activity as f32 / 100., input.speed as f32);
        let capacity = (1. + input.density * 0.063).round() as usize;
        let normal = if input.reduced_motion {
            capacity.min(2)
        } else {
            (capacity - 1).max(1)
        };
        let mut arcs = Vec::with_capacity(7);
        for i in 0..normal {
            let distance = if normal == 1 {
                range(&mut rng, 115., 210.)
            } else {
                let u = i as f32 / (normal - 1) as f32;
                90. + u.powf(1.25) * 365. + range(&mut rng, -20., 20.)
            };
            arcs.push(Curtain::new(
                &mut rng,
                sky.bounds.width as f32,
                distance,
                input.density as f32,
                params.activity as f32 / 100.,
                i as u32,
                Formation::Mature,
            ));
            arcs[i].ground_hem = i + 2 >= normal;
        }
        let next_encounter = f64::from(range(&mut rng, 320., 740.));
        Self {
            input,
            params,
            rng,
            arcs,
            sky,
            weather,
            tick: 0,
            next_id: normal as u32,
            capacity,
            normal,
            next_encounter,
            script: vec![],
            frames: RefCell::new(vec![]),
            empty: Snapshot::default(),
        }
    }
    fn split(&mut self, parent: usize) {
        if self.arcs.len() >= self.capacity {
            return;
        }
        let a = &self.arcs[parent];
        let mut child = Curtain::new(
            &mut self.rng,
            self.sky.bounds.width as f32,
            a.mean,
            self.input.density as f32,
            self.weather.activity,
            self.next_id,
            Formation::Birth,
        );
        // A daughter curtain opens from the same folded sheet; its target separates
        // it in depth while its opacity grows from zero.
        child.nodes.clone_from(&a.nodes);
        child.sheet.clone_from(&a.sheet);
        child.partial = a.partial;
        child.fade_to(0.42);
        child.mean = a.mean + 17.;
        child.target = (a.mean + range(&mut self.rng, 72., 139.)).min(810.);
        self.next_id = self.next_id.wrapping_add(1);
        self.arcs[parent].fade_to(0.74);
        child.ground_hem = self.arcs.iter().filter(|a| a.ground_hem).count() < 2;
        self.arcs.push(child);
    }
    fn nearest(&self) -> usize {
        self.arcs
            .iter()
            .enumerate()
            .filter(|(_, a)| !a.retiring())
            .min_by(|(_, a), (_, b)| a.mean.total_cmp(&b.mean))
            .map(|v| v.0)
            .unwrap_or(0)
    }
    fn storm(&mut self, x: Option<f32>, power: f32) {
        if self.params.activity == 0 {
            return;
        }
        // Measure the presented hem, including its eased folds, on the actual
        // outputs. A forced point sets the azimuth only. For gaps and partial
        // curtains, look for the closest safe position on another visible arc.
        let preferred = x.unwrap_or_else(|| {
            self.input
                .layout
                .choose_visible_anchor(&mut self.rng, 0.)
                .unwrap_or_else(|| self.sky.bounds.centre())
                .x as f32
        });
        let shot = self.snapshot();
        let frame = shot
            .typed
            .as_ref()
            .unwrap()
            .downcast_ref::<render::Frame>()
            .unwrap();
        let Some((id, x, _)) = frame.flare_site(&self.input.layout.outputs, preferred) else {
            return;
        };
        let mut coverage = frame.lit_share();
        let near = self.arcs.iter().position(|a| a.id == id).unwrap();
        self.weather.onset(
            self.arcs[near].id,
            x,
            [self.sky.bounds.width as f32, self.sky.bounds.height as f32],
            power,
            self.input.density as f32,
            &mut self.rng,
        );
        let storm = self.weather.storm.unwrap();
        self.arcs[near].excite(&mut self.rng, self.weather.activity);
        let room = self.capacity.saturating_sub(self.arcs.len()).max(1);
        let births = room.min(7 - self.arcs.len()).min(3);
        for layer in 0..births {
            if coverage + 0.13 > 0.65 {
                break;
            }
            let transmission = (1. - 1.2 * (coverage - 0.45).max(0.)).max(0.3);
            // Reserve sky for this birth before considering the next poleward band.
            coverage += 0.13 * transmission;
            let mut curtain = Curtain::new(
                &mut self.rng,
                self.sky.bounds.width as f32,
                67. + 22. * layer as f32,
                self.input.density as f32,
                self.weather.activity,
                self.next_id,
                Formation::Expansion,
            );
            curtain.gain = 1.65 * transmission;
            curtain.gain_target = curtain.gain;
            curtain.gain_baseline = curtain.gain;
            curtain.expansion = Some((
                10. + 15. * layer as f32,
                storm.expansion + storm.recovery,
                storm.recovery,
            ));
            self.arcs.push(curtain);
            self.next_id = self.next_id.wrapping_add(1);
        }
    }
    fn enact(&mut self, id: u32, anchor: Option<[f64; 2]>, power: f32) {
        let b = self.sky.bounds;
        match id {
            SUBSTORM => self.storm(anchor.map(|p| p[0] as f32), power),
            CORONA if self.params.skyEvents => self.weather.corona(
                self.arcs[self.nearest()].id,
                anchor.map_or(self.sky.zenith, |p| p[0] as f32),
                power,
                &mut self.rng,
            ),
            METEOR | FIREBALL if id == METEOR || self.params.skyEvents => {
                let a = anchor.unwrap_or([b.centre().x, b.centre().y]);
                self.weather.launch_meteor(
                    a[0] as f32,
                    a[1] as f32,
                    id == FIREBALL,
                    power,
                    &mut self.rng,
                );
                if !self.params.stars {
                    self.weather.meteor = None;
                }
            }
            _ => {}
        }
    }
    fn inherit_dying_sheet(&mut self) {
        let Some(dying) = self
            .arcs
            .iter()
            .position(|a| a.expansion.is_none() && a.retiring() && a.merge_into.is_none())
        else {
            return;
        };
        let heir = if self.arcs.len() < 7 {
            let next = self.arcs[dying].successor(
                &mut self.rng,
                self.sky.bounds.width as f32,
                self.input.density as f32,
                self.weather.activity,
                self.next_id,
            );
            let id = next.id;
            self.next_id = self.next_id.wrapping_add(1);
            self.arcs.push(next);
            Some(id)
        } else {
            let mean = self.arcs[dying].mean;
            self.arcs
                .iter()
                .filter(|a| !a.retiring() && a.expansion.is_none())
                .min_by(|a, b| (a.mean - mean).abs().total_cmp(&(b.mean - mean).abs()))
                .map(|a| a.id)
        };
        self.arcs[dying].merge_into = heir;
    }
    fn encounters(&mut self) {
        if self.weather.time >= self.next_encounter {
            self.next_encounter = self.weather.time + f64::from(range(&mut self.rng, 380., 930.));
            let n = self.arcs.len();
            if n >= 2 {
                let a = self.rng.bounded(n as u32).unwrap() as usize;
                let b = (a + 1) % n;
                // Solar-wind compression drives two mean shells through their
                // usual repulsion. Upon contact one layer fades into the survivor.
                self.arcs[b].target = self.arcs[a].mean;
            }
            if n < self.capacity {
                let bright = self
                    .arcs
                    .iter()
                    .enumerate()
                    .max_by(|a, b| (a.1.light * a.1.gain).total_cmp(&(b.1.light * b.1.gain)))
                    .map(|p| p.0)
                    .unwrap_or(0);
                self.split(bright);
            }
        }
        for i in 0..self.arcs.len() {
            for j in 0..i {
                let (low, high) = self.arcs.split_at_mut(i);
                let (a, b) = (&mut low[j], &mut high[0]);
                if a.retiring() || b.retiring() || a.expansion.is_some() || b.expansion.is_some() {
                    continue;
                }
                let d = b.mean - a.mean;
                if d.abs() < 15. && a.age.min(b.age) > 80. {
                    b.merge_into = Some(a.id);
                    b.retire();
                } else if d.abs() < 40. && (a.target - b.target).abs() > 20. {
                    let push = d.signum() * (40. - d.abs()) * 0.015;
                    a.target = (a.target - push).max(55.);
                    b.target = (b.target + push).min(850.);
                }
            }
        }
        self.arcs.retain(|a| !a.retiring() || a.light > 0.006);
        if self.arcs.len() < self.normal {
            let mean = range(&mut self.rng, 90., 550.);
            let ground_hem = self.arcs.iter().filter(|a| a.ground_hem).count() < 2;
            self.arcs.push(Curtain::new(
                &mut self.rng,
                self.sky.bounds.width as f32,
                mean,
                self.input.density as f32,
                self.weather.activity,
                self.next_id,
                Formation::Birth,
            ));
            self.arcs.last_mut().unwrap().ground_hem = ground_hem;
            self.next_id = self.next_id.wrapping_add(1);
        }
    }
}
impl LivingWorld for Aurora {
    fn rate(&self) -> Rate {
        if self.input.reduced_motion {
            Rate::new(1, 600).unwrap()
        } else {
            Rate::new(20, 1).unwrap()
        }
    }
    fn next_wake_ns(&self, ns: i64) -> Option<i64> {
        self.input
            .reduced_motion
            .then(|| still_transition(self.input.seed, ns).1)
    }
    fn step(&mut self, tick: u64) {
        self.tick = tick;
        if self.input.reduced_motion {
            if tick > 1 {
                let mut input = self.input.clone();
                input.seed = hash64(input.seed ^ (tick - 1).wrapping_mul(0xd1b54a32d192ed03));
                let mut next = Self::new(input);
                Arc::get_mut(&mut next.sky).unwrap().still_seed = Some(self.input.seed);
                self.sky = next.sky;
                self.arcs = next.arcs;
                self.frames.borrow_mut().clear();
            }
            return;
        }
        let dt = self.input.speed as f32 * 0.05;
        let ns = self.rate().tick_ns(tick);
        let forecast = self
            .script
            .iter()
            .find(|e| e.id == SUBSTORM && self.params.activity > 0)
            .map_or(0., |e| {
                smooth(1. - ((e.at_ns - ns) as f64 * 1e-9 * self.input.speed / 240.) as f32)
            });
        self.weather.advance(dt, forecast, &mut self.rng);
        let b = self.sky.bounds;
        // Draw a visible random anchor only when a meteor is due. Layout holes
        // remain real space along its subsequent trajectory.
        let p = if self.weather.meteor_due() {
            self.input
                .layout
                .choose_visible_anchor(&mut self.rng, 0.)
                .unwrap_or_else(|| b.centre())
        } else {
            b.centre()
        };
        let events = self.weather.natural_events(
            self.params.stars,
            self.params.skyEvents,
            p.x as f32,
            b.y as f32 + 0.66 * (p.y - b.y) as f32,
            &mut self.rng,
        );
        for id in events {
            if id != 0 {
                self.enact(id, None, 1.);
            }
        }
        while self.script.first().is_some_and(|e| e.at_ns <= ns) {
            let e = self.script.remove(0);
            self.enact(e.id, Some(e.anchor), e.strength.clamp(0., 1.) as f32);
        }
        let mut transfer = [(0_u32, 0_f32); 7];
        for (a, amount) in self.arcs.iter_mut().zip(&mut transfer) {
            let before = a.light;
            a.step(
                dt,
                self.weather.time as f32,
                self.weather.activity,
                self.weather.expansion,
                &mut self.rng,
            );
            if self.input.density >= 70. && a.id == 0 {
                a.target = a.target.clamp(110., 180.);
            }
            if let Some(id) = a.merge_into {
                *amount = (id, (before - a.light).max(0.) * a.gain);
            }
            a.redistribute();
        }
        self.inherit_dying_sheet();
        for (id, power) in transfer.into_iter().filter(|(_, p)| *p > 0.) {
            if let Some(survivor) = self.arcs.iter_mut().find(|s| s.id == id) {
                let gain = power / survivor.light.max(0.01);
                survivor.gain += gain;
                survivor.gain_target += gain;
            }
        }
        // As the optical weight is transferred, displace the vanishing sheet
        // gently toward its partner without reassigning either material grid.
        for index in 0..self.arcs.len() {
            if let Some(id) = self.arcs[index].merge_into {
                let (earlier, later) = self.arcs.split_at_mut(index);
                if let Some(survivor) = earlier.iter().find(|a| a.id == id) {
                    let dying = &mut later[0];
                    for (node, target) in dying.nodes.iter_mut().zip(&survivor.nodes) {
                        node.z += (target.z - node.z).clamp(-18., 18.) * (dt * 0.55);
                    }
                }
            }
        }
        if tick.is_multiple_of(20) {
            self.encounters();
        }
    }
    fn snapshot(&self) -> Snapshot {
        let mut frames = self.frames.borrow_mut();
        let slot = frames
            .iter()
            .position(|f| Arc::strong_count(f) == 1)
            .unwrap_or_else(|| {
                frames.push(Arc::new(render::Frame::new(self.sky.clone())));
                frames.len() - 1
            });
        Arc::get_mut(&mut frames[slot]).unwrap().update(
            &self.arcs,
            &self.weather,
            self.input.speed as f32,
        );
        Snapshot {
            tick: self.tick,
            time_ns: self.rate().tick_ns(self.tick),
            generations: Generations {
                layout: self.input.layout.generation,
                topology: u64::from(self.next_id),
                data: 1,
                sample: self.tick,
            },
            typed: Some(frames[slot].clone()),
            texture: Some(self.sky.texture.clone()),
            emission_estimate: self.emission_estimate(),
            ..self.empty.clone()
        }
    }
    fn emission_estimate(&self) -> f64 {
        0.012 + 0.018 * f64::from(self.weather.expansion) + 0.004 * f64::from(self.weather.recovery)
    }
    fn inject(&mut self, e: Event) -> Result<(), &'static str> {
        if !Weather::validate(&e) {
            return Err("invalid Aurora sky event");
        }
        self.script.push(e);
        self.script.sort_by_key(|e| e.at_ns);
        Ok(())
    }
    fn state_hash(&self) -> u64 {
        let mut h = hash64(self.input.seed ^ self.tick);
        for a in &self.arcs {
            for n in &a.nodes {
                for v in [n.x, n.z, n.memory_z] {
                    h = hash64(h ^ u64::from(v.to_bits()))
                }
            }
        }
        hash64(h ^ self.weather.time.to_bits())
    }
}
