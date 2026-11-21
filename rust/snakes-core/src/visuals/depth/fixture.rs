//! Test-only living world; not part of the user-facing visual catalog.
use super::{
    common::{clock::Rate, rng::hash64},
    world::*,
};
use std::sync::Arc;
pub struct Fixture {
    input: WorldInput,
    tick: u64,
    events: Vec<Event>,
    texture: Arc<DataTexture>,
}
impl Fixture {
    pub fn new(input: WorldInput) -> Self {
        let asset_colour = (super::common::rng::hash64(input.seed) % 200 + 40) as u8;
        Self {
            input,
            tick: 0,
            events: Vec::new(),
            texture: Arc::new(DataTexture {
                revision: 1,
                width: 2,
                height: 2,
                rgba: Arc::from([
                    asset_colour,
                    80,
                    120,
                    255,
                    80,
                    30,
                    60,
                    255,
                    30,
                    80,
                    40,
                    255,
                    90,
                    60,
                    20,
                    255,
                ]),
            }),
        }
    }
}
impl LivingWorld for Fixture {
    fn rate(&self) -> Rate {
        Rate::new(60, 1).unwrap()
    }
    fn step(&mut self, tick: u64) {
        self.tick = tick;
        let revision = tick / 120 + 1;
        if revision != self.texture.revision {
            let mut bytes = self.texture.rgba.to_vec();
            bytes[0] = (revision % 200 + 40) as u8;
            self.texture = Arc::new(DataTexture {
                revision,
                width: 2,
                height: 2,
                rgba: bytes.into(),
            });
        }
    }
    fn snapshot(&self) -> Snapshot {
        let time = self.rate().tick_ns(self.tick);
        let t = time as f64 * 1e-9;
        let r = self.input.layout.bounds;
        let event = self.events.iter().filter(|e| e.at_ns <= time).count() as f64;
        let phase = (hash64(self.input.seed) % 1000) as f64 * 0.001;
        let generation = self.tick / 180;
        let circles = (0..4)
            .filter(|id| *id != 3 || self.tick % 180 < 120)
            .map(|id| Circle {
                id,
                generation: if id == 3 { generation } else { 0 },
                x: r.x
                    + (r.width * ([0.12, 0.305, 0.56, 0.745][id as usize])
                        + t * 80.
                        + phase * 100.)
                        .rem_euclid(r.width),
                y: r.y + r.height * (0.35 + 0.1 * (t * 0.4 + id as f64).sin()),
                radius: 24. + id as f32 * 12. + event as f32 * 8.,
                rgba: [0.2 + id as f32 * 0.15, 0.6, 0.9, 0.85],
                flags: 0,
                death_ns: if id == 3 {
                    self.rate().tick_ns(generation * 180 + 120)
                } else {
                    i64::MAX
                },
                birth_ns: if id == 3 {
                    self.rate().tick_ns(generation * 180)
                } else {
                    -1_000_000_000
                },
            })
            .collect::<Vec<_>>();
        // Test-only environment echo: explicit mode, background and immutable
        // obstacle geometry let the Qt contract tests inspect the bridge.
        let mut payload = Vec::new();
        payload.extend_from_slice(&(self.input.mode as u32).to_le_bytes());
        payload.extend_from_slice(&self.input.environment.background.to_le_bytes());
        payload.extend_from_slice(&(self.input.environment.obstacles.len() as u64).to_le_bytes());
        for o in &self.input.environment.obstacles {
            for v in [o.rect.x, o.rect.y, o.rect.width, o.rect.height] {
                payload.extend_from_slice(&v.to_le_bytes());
            }
        }
        // Exercise texture removal and restoration as well as revision changes.
        let texture_present = !(120..180).contains(&self.tick);
        let mut snapshot = Snapshot {
            tick: self.tick,
            time_ns: time,
            generations: Generations {
                layout: self.input.layout.generation,
                topology: self.tick / 60,
                data: if texture_present {
                    self.texture.revision
                } else {
                    0
                },
                sample: self.tick,
            },
            circles: circles.into(),
            payload: payload.into(),
            typed: None,
            texture: texture_present.then(|| self.texture.clone()),
            emission_estimate: 0.01 + event * 0.001,
        };
        if self.input.reduced_motion {
            snapshot.time_ns = 0;
        }
        snapshot
    }
    fn inject(&mut self, event: Event) -> Result<(), &'static str> {
        if event.id != 1
            || event.at_ns < 0
            || !event.strength.is_finite()
            || !event.anchor.iter().all(|v| v.is_finite())
        {
            return Err("invalid fixture event");
        }
        self.events.push(event);
        self.events.sort_by_key(|e| e.at_ns);
        Ok(())
    }
    fn state_hash(&self) -> u64 {
        let snapshot = self.snapshot();
        let mut hash = hash64(self.input.seed ^ self.tick);
        let mut word = |value: u64| {
            hash = hash64(hash ^ value);
        };
        for c in snapshot.circles.iter() {
            for v in [
                c.id,
                c.generation,
                c.x.to_bits(),
                c.y.to_bits(),
                u64::from(c.radius.to_bits()),
                c.birth_ns as u64,
                c.death_ns as u64,
            ] {
                word(v);
            }
        }
        if let Some(texture) = &snapshot.texture {
            word(texture.revision);
            for &byte in texture.rgba.iter() {
                word(u64::from(byte));
            }
        }
        for &byte in snapshot.payload.iter() {
            word(u64::from(byte));
        }
        for e in &self.events {
            for v in [
                u64::from(e.id),
                e.at_ns as u64,
                e.anchor[0].to_bits(),
                e.anchor[1].to_bits(),
                e.strength.to_bits(),
            ] {
                word(v);
            }
        }
        hash
    }
}
