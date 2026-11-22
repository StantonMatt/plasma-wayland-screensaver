//! Luminous silk carried by the shared world-space current.
mod agent;
mod material;
mod render;
pub use render::{
    BodyKnot, CapPair, Draw, MotionPair, Vertex, VertexPair, visuals_ribbons_write,
    visuals_ribbons_write_packed,
};
mod simulation;
#[cfg(test)]
mod tests;
mod water;

use super::{Descriptor, MirroredPolicy, common, ffi, schema::Parameter, world};
use common::{clock::Rate, rng::hash64};
use material::Art;
use render::Frame;
use simulation::Sea;
use std::{cell::RefCell, sync::Arc};
use world::{Event, Generations, LivingWorld, Snapshot, WorldInput};

#[derive(Clone, Debug)]
pub struct Params {
    turbulence: f64,
    twist: f64,
    length: f64,
    dance: bool,
    world_events: bool,
}
impl Default for Params {
    fn default() -> Self {
        Self {
            turbulence: 0.40,
            twist: 0.55,
            length: 0.55,
            dance: true,
            world_events: true,
        }
    }
}
impl Params {
    fn read(input: &WorldInput) -> Self {
        let mut p = Self::default();
        for v in &input.parameters {
            match v.id {
                1 => p.turbulence = v.payload as f64 / 100.,
                2 => p.twist = v.payload as f64 / 100.,
                3 => p.length = v.payload as f64 / 100.,
                4 => p.dance = v.payload != 0,
                5 => p.world_events = v.payload != 0,
                _ => {}
            }
        }
        if input.reduced_motion {
            p.turbulence = p.turbulence.min(0.40);
            p.world_events = false;
        }
        p
    }
}
pub const PARAMETERS: &[Parameter] = &[
    Parameter::integer(1, c"turbulence", 0., 100., 40.),
    Parameter::integer(2, c"twist", 0., 100., 55.),
    Parameter::integer(3, c"length", 0., 100., 55.),
    Parameter::boolean(4, c"dance", true),
    Parameter::boolean(5, c"worldEvents", true),
];
pub const DESCRIPTOR: Descriptor = Descriptor {
    id: 4,
    key: "ribbons",
    available: true,
    rate: 30,
    mirrored: MirroredPolicy::SharedCanonical,
    fit: super::MirroredFit::Cover,
    parameters: PARAMETERS,
    create: |input| Some(Box::new(Ribbons::new(input))),
};
pub struct Ribbons {
    input: WorldInput,
    params: Params,
    sea: Sea,
    art: Arc<Art>,
    tick: u64,
    frames: RefCell<Vec<Arc<Frame>>>,
    empty: Snapshot,
}
impl Ribbons {
    pub fn new(input: WorldInput) -> Self {
        let params = Params::read(&input);
        let art = Arc::new(Art::new(&input));
        let sea = Self::composition(&input, &params, input.seed);
        Self {
            input,
            params,
            sea,
            art,
            tick: 0,
            frames: RefCell::new(vec![]),
            empty: Snapshot::default(),
        }
    }
    fn composition(input: &WorldInput, params: &Params, seed: u64) -> Sea {
        let mut best = Sea::new(input, params, seed);
        if !input.reduced_motion {
            return best;
        }
        // The still occupies each screen for ten minutes. Prefer the first
        // well-balanced deterministic composition, examining at most eight.
        let preroll = |sea: &mut Sea| {
            for k in 1..=180 {
                sea.advance(input, params, 0.25, 0., k * 250_000_000);
            }
        };
        preroll(&mut best);
        let mut score = best.balance(&input.layout);
        for i in 1..8 {
            if score < 0.13 {
                break;
            }
            let mut next = Sea::new(input, params, seed.wrapping_add(i * 0x1_0000_0001));
            preroll(&mut next);
            let candidate = next.balance(&input.layout);
            if candidate < score {
                best = next;
                score = candidate;
            }
        }
        best
    }
}
impl LivingWorld for Ribbons {
    fn rate(&self) -> Rate {
        if self.input.reduced_motion {
            Rate::new(1, 600).unwrap()
        } else {
            Rate::new(30, 1).unwrap()
        }
    }
    fn step(&mut self, tick: u64) {
        self.tick = tick;
        if self.input.reduced_motion {
            if tick > 1 {
                self.sea = Self::composition(
                    &self.input,
                    &self.params,
                    self.input.seed.wrapping_add(tick - 1),
                );
            }
        } else {
            self.sea.advance(
                &self.input,
                &self.params,
                self.input.speed / 30.,
                (0.7 + 0.3 * self.input.speed) / 30.,
                self.rate().tick_ns(tick),
            );
        }
    }
    fn next_wake_ns(&self, ns: i64) -> Option<i64> {
        if !self.input.reduced_motion {
            return None;
        }
        let period = 600_000_000_000;
        let within = ns % period;
        Some(if ns >= period && within < 8_000_000_000 {
            ns.saturating_add((8_000_000_000 - within).min(83_333_333))
        } else {
            ns.saturating_add(period - within)
        })
    }
    fn snapshot(&self) -> Snapshot {
        let mut pool = self.frames.borrow_mut();
        let idx = pool
            .iter()
            .position(|f| Arc::strong_count(f) == 1)
            .unwrap_or_else(|| {
                pool.push(Arc::new(Frame::new(
                    self.art.clone(),
                    self.sea.agents.capacity(),
                    self.input.reduced_motion,
                )));
                pool.len() - 1
            });
        Arc::get_mut(&mut pool[idx])
            .unwrap()
            .fill(&self.sea, self.params.twist);
        Snapshot {
            tick: self.tick,
            time_ns: self.rate().tick_ns(self.tick),
            generations: Generations {
                layout: self.input.layout.generation,
                topology: u64::from(self.sea.agents.len() as u32),
                data: 1,
                sample: self.tick,
            },
            typed: Some(pool[idx].clone()),
            emission_estimate: self.emission_estimate(),
            ..self.empty.clone()
        }
    }
    fn emission_estimate(&self) -> f64 {
        self.sea
            .agents
            .iter()
            .filter(|r| r.alive)
            .map(|r| r.width * r.spacing * r.count as f64 * r.extent() * 0.17)
            .sum::<f64>()
            / self.input.layout.visible_area
    }
    fn inject(&mut self, e: Event) -> Result<(), &'static str> {
        self.sea.weather.inject(e)
    }
    fn state_hash(&self) -> u64 {
        let mut h = 0;
        for r in &self.sea.agents {
            for n in &r.nodes[..r.count] {
                for v in [n.pos.x, n.pos.y, n.phase] {
                    h = hash64(h ^ v.to_bits());
                }
            }
        }
        h
    }
}
