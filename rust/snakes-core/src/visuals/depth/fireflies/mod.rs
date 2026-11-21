//! A low meadow of independently coupled lanterns, advanced at 30 Hz.
mod behaviour;
mod events;
mod fly;
pub mod frame;
mod terrain;
#[cfg(test)]
mod tests;
use super::{Descriptor, MirroredPolicy, schema::Parameter};
#[allow(non_snake_case)]
#[derive(Clone, Debug)]
pub struct Params {
    pub synchrony: i32,
    pub nightCycle: bool,
    pub nightEvents: bool,
    pub hunters: bool,
    pub grass: bool,
}
impl Default for Params {
    fn default() -> Self {
        Self {
            synchrony: 55,
            nightCycle: true,
            nightEvents: true,
            hunters: true,
            grass: true,
        }
    }
}
pub const PARAMETERS: &[Parameter] = &[
    Parameter::integer(1, c"synchrony", 0., 100., 55.),
    Parameter::boolean(2, c"nightCycle", true),
    Parameter::boolean(3, c"nightEvents", true),
    Parameter::boolean(4, c"hunters", true),
    Parameter::boolean(5, c"grass", true),
];
pub const DESCRIPTOR: Descriptor = Descriptor {
    id: 9,
    key: "fireflies",
    available: true,
    rate: 30,
    mirrored: MirroredPolicy::SharedCanonical,
    fit: super::MirroredFit::Height,
    parameters: PARAMETERS,
    create: |input| Some(Box::new(World::new(input))),
};

use super::{
    common::{
        clock::Rate,
        rng::{Pcg32, derive_seed, hash64},
    },
    world::{Event, Generations, LivingWorld, MonitorMode, Snapshot, WorldInput},
};
use events::Weather;
use fly::{Fly, Glade, Species, glades};
use frame::{Art, Frame, Texel};
use std::sync::Arc;
use terrain::{Meadow, Neighbours, between};

pub struct Distant {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    z: f32,
    phase: f32,
    period: f32,
    light: f32,
    home: usize,
    roam: f32,
    altitude: f32,
    near: bool,
    age: f32,
    tint: usize,
    tree_delay: f32,
    tree_gain: f32,
    since: f32,
    last_flash: f64,
    tree: Option<[f32; 2]>,
}
pub struct World {
    seed: u64,
    layout_generation: u64,
    params: Params,
    still: bool,
    mid_count: usize,
    far_count: usize,
    near_count: usize,
    meadow: Meadow,
    grid: Neighbours,
    flies: Vec<Fly>,
    glades: Vec<Glade>,
    distant: Vec<Distant>,
    rain: Vec<[f32; 3]>,
    signals: Vec<usize>,
    #[cfg(test)]
    coupling_edges: Vec<(usize, usize)>,
    rng: Pcg32,
    night: u64,
    night_age: f32,
    night_length: f32,
    pace: f32,
    cadence: f32,
    sync_gain: f32,
    phase_coupling: f32,
    flash_interval: f32,
    noise_step: f32,
    clock: f32,
    // Shared by scripted events and all lights in this simulation step.
    flash_time: f64,
    wind: Vec<[f32; 2]>,
    wind_phase: f32,
    weather: Weather,
    next_event: f32,
    last_shower: f32,
    hunt_cooldown: f32,
    after_rain: f32,
    gathered: bool,
    tree_night: bool,
    tree_phase: f32,
    tree_since: f32,
    tree_pulses: usize,
    tree_ready: bool,
    script: Vec<Event>,
    tick: u64,
    iteration: u64,
    emission: f32,
    exposures: Vec<[f32; 11]>,
    art: Arc<Art>,
    frame: Option<Arc<Frame>>,
    history: Arc<Vec<Arc<[Texel]>>>,
    history_serial: u64,
    relocated: Vec<usize>,
    trail_count: usize,
}
impl World {
    pub fn new(input: WorldInput) -> Self {
        let mut params = Params::default();
        for v in &input.parameters {
            match v.id {
                1 => params.synchrony = v.payload.min(100) as i32,
                2 => params.nightCycle = v.payload != 0,
                3 => params.nightEvents = v.payload != 0,
                4 => params.hunters = v.payload != 0,
                5 => params.grass = v.payload != 0,
                _ => {}
            }
        }
        let mut rng = Pcg32::new(input.seed, 0xf1ea);
        let meadow = Meadow::new(&input.layout);
        let art = Arc::new(Art::new(&input, &meadow, &mut rng, params.grass));
        let mid_count = ((4. + 0.62 * input.density) * input.layout.visible_area / 1e6)
            .round()
            .clamp(10., 1200.) as usize;
        let far_count = ((mid_count as f32 * 0.6).round() as usize).min(800);
        // One foreground lens per output can drift beyond its birthplace. At most
        // one from either neighbour can overlap it during a crossing.
        let near_count = meadow.outputs.len().min(3);
        let speed = input.speed as f32;
        let sync_gain = params.synchrony as f32 / 100.;
        // Rise gradually to local entrainment at the default setting, retaining
        // the established upper-range response for closely synchronized glades.
        let coupling = if sync_gain < 0.8 {
            0.8_f32.powf(6.18) * (sync_gain / 0.8).powf(5.4)
        } else {
            sync_gain.powf(6.18)
        };
        let cadence = speed.powf(0.3);
        let pace = art.pace;
        let trail_count = if input.reduced_motion {
            60
        } else {
            (18. * (input.trails as f32).powf(1.21)).ceil() as usize
        };
        let night_length = 2160. / speed.sqrt();
        // Independent worlds get a connector-specific seed from the coordinator.
        let offset = if input.mode == MonitorMode::Independent {
            input.environment.independent_index % 3
        } else {
            0
        };
        let mut w = Self {
            seed: input.seed,
            layout_generation: input.layout.generation,
            still: input.reduced_motion,
            mid_count,
            far_count,
            near_count,
            grid: Neighbours::new(meadow.bounds, mid_count),
            wind: vec![[0.; 2]; meadow.columns.len().div_ceil(4)],
            wind_phase: 0.,
            rain: vec![],
            exposures: vec![[0.; 11]; meadow.outputs.len()],
            meadow,
            art,
            flies: vec![],
            glades: vec![],
            distant: vec![],
            signals: Vec::with_capacity(mid_count),
            #[cfg(test)]
            coupling_edges: vec![],
            rng,
            night: 0,
            night_age: night_length * offset as f32 / 3.,
            night_length,
            pace,
            cadence,
            sync_gain,
            phase_coupling: 1.65 * coupling,
            params,
            flash_interval: (0.5 / cadence).max(0.5),
            noise_step: 0.20,
            clock: 0.,
            flash_time: 0.,
            weather: Weather::default(),
            next_event: 180.,
            last_shower: -400.,
            hunt_cooldown: 60.,
            after_rain: 0.,
            gathered: false,
            tree_night: false,
            tree_phase: 0.,
            tree_since: 0.,
            tree_pulses: 0,
            tree_ready: false,
            script: vec![],
            tick: 0,
            iteration: 0,
            emission: 0.,
            frame: None,
            history: Arc::new(vec![Arc::from([]); trail_count.max(1)]),
            history_serial: 0,
            relocated: vec![],
            trail_count,
        };
        w.new_night();
        // The still uses a coarse 84 s ecological settling pass and an exact six-second exposure.
        // A moving world starts with a complete history even when first shown at time zero.
        let age = if w.still {
            0.35 * night_length
        } else {
            w.night_age
        };
        if w.still {
            w.night_age = age - 90.;
        }
        if w.still {
            for _ in 0..336 {
                w.advance(0.25, false);
            }
        }
        let steps = if w.still {
            180
        } else {
            (trail_count * 3).max(1)
        };
        for _ in 0..steps {
            w.advance(1. / 30., true);
        }
        if w.still {
            w.finish_exposure();
        }
        w.night_age = age;
        w
    }
    fn new_night(&mut self) {
        self.rng = Pcg32::new(derive_seed(self.seed, 0xf1ea, self.night, 0), 9);
        self.glades = glades(&self.meadow, &mut self.rng);
        self.wind_phase = between(&mut self.rng, 0., 100.);
        self.tree_night = self.rng.unit() < 0.45;
        self.gathered = false;
        self.weather = Weather::default();
        self.tree_ready = false;
        self.next_event = between(&mut self.rng, 185., 340.);
        let dip = between(&mut self.rng, 0.29, 0.43);
        let sync = dip + 0.40;
        let ghost = if self.params.hunters {
            between(&mut self.rng, 0.84, 0.91)
        } else {
            1.
        };
        self.flies.clear();
        let previous_distant = std::mem::take(&mut self.distant);
        self.distant.reserve(self.far_count + self.near_count);
        self.rain.clear();
        for i in 0..self.mid_count {
            // Stratified draws preserve the species mix even in sparse meadows.
            let roll = (i as f32 + self.rng.unit() as f32) / self.mid_count as f32;
            let mut species = if roll < dip {
                Species::Dipper
            } else if roll < sync {
                Species::Syncer
            } else if roll < ghost {
                Species::Ghost
            } else {
                Species::Hunter
            };
            if self.mid_count < 20 && i < 4 {
                species = [
                    Species::Dipper,
                    Species::Syncer,
                    Species::Ghost,
                    if self.params.hunters {
                        Species::Hunter
                    } else {
                        Species::Ghost
                    },
                ][i];
            }
            let mut f = Fly::spawn(
                i,
                species,
                &self.glades,
                &self.meadow,
                &mut self.rng,
                self.cadence,
            );
            f.last_flash = self.flash_time - f64::from(f.since);
            if species == Species::Hunter
                && !self.flies.iter().any(|q| q.species == Species::Hunter)
            {
                f.female = true;
                f.altitude = 0.09;
                f.state = fly::State::Perched;
            }
            if i == 0 {
                f.female = false;
                f.state = fly::State::Patrol;
                f.altitude = 0.25;
            }
            self.flies.push(f);
        }
        for i in 0..self.far_count + self.near_count {
            let home = self.rng.next_u32() as usize % self.glades.len();
            let g = self.glades[home];
            let roam = between(&mut self.rng, -g.width * 2.5, g.width * 2.5);
            let near = i >= self.far_count;
            let mut x = (g.x + roam).clamp(
                self.meadow.bounds.x as f32,
                self.meadow.bounds.right() as f32,
            );
            if near {
                let o = self.meadow.outputs[(i - self.far_count) % self.meadow.outputs.len()];
                x = o.x as f32 + between(&mut self.rng, 0.15, 0.85) * o.width as f32;
            }
            let c = self.meadow.column(x);
            let altitude = if near {
                between(&mut self.rng, 0.08, 0.48)
            } else {
                between(&mut self.rng, 0.37, 0.60)
            };
            let mut distant = Distant {
                x,
                y: c.floor - c.height * altitude,
                vx: 0.,
                vy: 0.,
                z: between(&mut self.rng, 0.08, 0.29),
                phase: between(&mut self.rng, 0., 1.),
                period: between(&mut self.rng, 4., 9.),
                age: 0.,
                tint: 0,
                tree_delay: 0.,
                tree_gain: 1.,
                since: 10.,
                last_flash: f64::NEG_INFINITY,
                light: 0.,
                home,
                roam,
                altitude,
                near,
                tree: None,
            };
            // A few lanterns on the horizon survive the hush. Their new glade
            // attracts them gradually, so the next night never teleports them.
            if let Some(old) = previous_distant.get(i) {
                distant.x = old.x;
                distant.y = old.y;
                distant.vx = old.vx;
                distant.vy = old.vy;
                distant.phase = old.phase;
                distant.since = old.since;
                distant.last_flash = old.last_flash;
                distant.light = old.light;
            }
            if near {
                distant.home = (i - self.far_count) % self.near_count;
                distant.rebirth(
                    self.meadow.outputs[(i - self.far_count) % self.near_count],
                    &mut self.rng,
                );
                distant.age = between(&mut self.rng, -8., distant.period * 0.5);
            }
            self.distant.push(distant);
        }
        for _ in 0..(self.meadow.bounds.width / 40.).ceil().clamp(1., 500.) as usize {
            let x = between(
                &mut self.rng,
                self.meadow.bounds.x as f32,
                self.meadow.bounds.right() as f32,
            );
            let c = self.meadow.column(x);
            self.rain.push([
                x,
                c.floor - between(&mut self.rng, 0., 1.) * c.height,
                between(&mut self.rng, 215., 345.),
            ]);
        }
    }
    fn advance(&mut self, dt: f32, publish: bool) {
        self.iteration += 1;
        self.flash_time += f64::from(dt);
        self.scheduled(self.tick);
        self.clock += dt;
        self.hunt_cooldown -= dt;
        if self.params.nightCycle {
            self.night_age += dt;
            if self.night_age >= self.night_length + 30. {
                self.night += 1;
                self.night_age = 0.;
                self.new_night();
                // A new population has new identities. Never connect strokes across that boundary.
                self.history = Arc::new(vec![Arc::from([]); self.trail_count.max(1)]);
            }
        }
        self.weather_step(dt);
        self.update_wind(dt);
        self.tree_oscillator(dt);
        self.move_and_flash(dt);
        self.step_distant(dt);
        if self.weather.kind == events::SHOWER {
            for r in &mut self.rain {
                let c = self.meadow.column(r[0]);
                r[1] += dt * r[2];
                if r[1] > c.floor {
                    r[1] -= c.height;
                }
            }
        }
        if publish {
            self.export(self.iteration.is_multiple_of(3));
        }
    }
    fn rate_ns(&self, t: u64) -> i64 {
        self.rate().tick_ns(t)
    }
}
impl LivingWorld for World {
    fn rate(&self) -> Rate {
        Rate::new(30, 1).unwrap()
    }
    fn step(&mut self, tick: u64) {
        self.tick = tick;
        if !self.still {
            self.advance(1. / 30., true);
        }
    }
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            tick: self.tick,
            time_ns: self.rate_ns(self.tick),
            generations: Generations {
                layout: self.layout_generation,
                topology: self.night,
                data: self.night + 1,
                sample: self.tick,
            },
            typed: Some(self.frame.as_ref().unwrap().clone()),
            emission_estimate: self.emission_estimate(),
            ..Default::default()
        }
    }
    fn emission_estimate(&self) -> f64 {
        self.emission as f64
    }
    fn inject(&mut self, event: Event) -> Result<(), &'static str> {
        if !(1..=5).contains(&event.id)
            || event.at_ns < 0
            || event
                .anchor
                .iter()
                .any(|v| !v.is_finite() || !(*v as f32).is_finite())
            || !event.strength.is_finite()
        {
            return Err("invalid meadow event");
        }
        self.script.push(event);
        self.script.sort_by_key(|e| e.at_ns);
        Ok(())
    }
    fn state_hash(&self) -> u64 {
        let mut h = hash64(self.seed ^ self.night);
        let f = self.frame.as_ref().unwrap();
        for r in f
            .heads
            .iter()
            .chain(f.grass.iter())
            .chain(f.history.iter().flat_map(|r| r.iter()))
        {
            for v in r {
                h = hash64(h ^ u64::from(v.to_bits()));
            }
        }
        h
    }
}
