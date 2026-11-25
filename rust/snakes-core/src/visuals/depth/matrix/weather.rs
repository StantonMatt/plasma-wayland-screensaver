//! Weather is sampled in desk coordinates. Counter-based rolls keep the same
//! column and weather clock on mirrored outputs of different sizes.
use super::{Params, hash64, unit};
use crate::visuals::depth::world::Event;

pub const DOWNPOUR: usize = 0;
pub const GUST: usize = 1;
pub const LIGHTNING: usize = 2;
pub const LULL: usize = 3;
pub const SWEEP: usize = 4;
pub const LANTERN: usize = 5;

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}

fn noise(seed: u64, x: f32) -> f32 {
    let i = x.floor() as i64;
    let a = unit(hash64(seed ^ i as u64));
    let b = unit(hash64(seed ^ (i + 1) as u64));
    a + (b - a) * smooth(x - i as f32)
}

#[derive(Clone, Copy, Debug)]
pub struct Moment {
    pub start: f32,
    pub end: f32,
    pub x: f32,
    pub key: u64,
}
impl Moment {
    pub fn age(self, t: f32) -> f32 {
        t - self.start
    }
    pub fn envelope(self, t: f32, edge: f32) -> f32 {
        smooth(self.age(t) / edge) * smooth((self.end - t) / edge)
    }
}
impl Default for Moment {
    fn default() -> Self {
        Self {
            start: -10000.,
            end: -9999.,
            x: 0.,
            key: 0,
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct Front {
    pub dialect: u8,
    pub tint: u8,
}
impl Front {
    fn from_key(key: u64) -> Self {
        Self {
            dialect: (key % 7) as u8,
            tint: (key.rotate_left(18) % 6) as u8,
        }
    }
}

pub struct Weather {
    seed: u64,
    width: f32,
    speed: f32,
    clock: f32,
    era_end: f32,
    era: u32,
    serial: u64,
    next: [f32; 6],
    pub moments: [Moment; 6],
    pub rhythm: super::lightning::Rhythm,
    pub counts: [u32; 6],
    pub wind: f32,
    phase: f32,
    pub steady: bool,
    strength: f32,
    scale: f32,
    pub last_lull_drop: bool,
    scripts: Vec<Event>,
}
impl Weather {
    pub fn new(seed: u64, width: f32, speed: f32, p: &Params) -> Self {
        let mut w = Self {
            seed,
            width,
            speed,
            clock: 0.,
            era_end: 0.,
            era: 1,
            serial: 0,
            next: [0.; 6],
            moments: [Moment::default(); 6],
            rhythm: Default::default(),
            counts: [0; 6],
            wind: 0.,
            phase: 0.,
            steady: p.weather == 0,
            strength: 1.,
            scale: 1.,
            last_lull_drop: true,
            scripts: Vec::new(),
        };
        for i in 0..6 {
            w.schedule(i, 0., p);
        }
        w
    }
    fn schedule(&mut self, i: usize, t: f32, p: &Params) {
        let (gap, mean) = [
            (420., 240.),
            (180., 160.),
            (90., 100.),
            (510., 340.),
            (150., 110.),
            (90., 105.),
        ][i];
        let u = unit(hash64(self.seed ^ (self.serial << 8) ^ i as u64 ^ 0xd993)).max(0.001);
        self.serial += 1;
        let storm = if p.weather == 2 && i == DOWNPOUR {
            3.
        } else {
            1.
        };
        self.next[i] = t + (gap - mean * u.ln()) / (storm * self.speed.clamp(1., 1.5));
    }
    pub fn inject(&mut self, e: Event) -> Result<(), &'static str> {
        if !(1..=6).contains(&e.id)
            || e.at_ns < 0
            || !e.anchor.iter().all(|n| n.is_finite())
            || !e.strength.is_finite()
        {
            return Err("invalid rain event");
        }
        self.scripts.push(e);
        self.scripts.sort_by(|a, b| b.at_ns.cmp(&a.at_ns));
        Ok(())
    }
    fn begin(&mut self, i: usize, t: f32, x: f32, p: &Params, forced: bool) {
        if (p.weather == 0 && matches!(i, DOWNPOUR | LIGHTNING | LULL))
            || (i == GUST && p.wind == 0.)
            || (i == LIGHTNING && (!p.lightning || (!forced && self.era != 3)))
        {
            return;
        }
        let key = hash64(self.seed ^ ((self.counts[i] as u64) << 8) ^ i as u64);
        let u = unit(key);
        let duration = [
            40. + 50. * u,
            20. + 40. * u,
            3.,
            if forced { 30. } else { 30. + 30. * u },
            4. + 2. * u,
            14.,
        ][i] * if i == DOWNPOUR && p.weather == 2 {
            1.3
        } else {
            1.
        };
        self.moments[i] = Moment {
            start: t,
            end: t + duration,
            x,
            key,
        };
        if i == LIGHTNING {
            self.rhythm = super::lightning::Rhythm::for_desk(key, x, self.width);
        }
        self.counts[i] += 1;
        if i == LULL {
            self.last_lull_drop = false;
        }
    }
    pub fn step(&mut self, t: f32, p: &Params) {
        let dt = t - self.clock;
        self.clock = t;
        if t >= self.era_end {
            let h = hash64(self.seed ^ t.to_bits() as u64 ^ 0xbe77);
            self.era = if p.weather == 0 {
                1
            } else if p.weather == 2 && (h & 3) < 2 {
                3
            } else {
                (h % 4) as u32
            };
            self.era_end = t + 480. + 720. * unit(hash64(h));
        }
        let era = if p.weather == 0 {
            1.
        } else {
            [0.58, 1., 1.26, 1.43][self.era as usize]
        };
        let climate = noise(self.seed ^ 41, t / 3239.);
        self.strength = era * (0.75 + 0.45 * climate);
        self.scale = 0.76 + climate * 0.56;
        self.wind = p.wind * (2. * noise(self.seed ^ 47, t / 390.) - 1.);
        self.phase += dt * self.speed.min(1.5) * (18. + 16. * self.wind);
        for i in 0..6 {
            if t >= self.next[i] {
                let x = self.width * unit(hash64(self.seed ^ self.next[i].to_bits() as u64));
                self.begin(i, t, x, p, false);
                self.schedule(i, t, p);
            }
        }
        while self
            .scripts
            .last()
            .is_some_and(|e| e.at_ns as f64 * 1e-9 <= t as f64 + 1e-5)
        {
            let e = self.scripts.pop().unwrap();
            self.begin(
                e.id as usize - 1,
                e.at_ns as f32 * 1e-9,
                e.anchor[0] as f32,
                p,
                true,
            );
        }
    }
    pub fn local(&self, x: f32, pitch: f32) -> (f32, [Front; 2], f32, f32) {
        let pos = x - self.phase;
        let large = noise(self.seed ^ 4, pos / (1350. * self.scale));
        let middle = noise(self.seed ^ 5, (pos + self.clock * 2.7) / 420.);
        let fine = noise(self.seed ^ 6, (pos - self.clock * 1.3) / 125.);
        let front = (large * 0.55 + middle * 0.32 + fine * 0.13).clamp(0., 1.);
        let jitter = 6. * (unit(hash64(self.seed ^ (x / pitch).floor().to_bits() as u64)) - 0.5);
        let region = (pos + pitch * jitter) / 1250. + 0.7 * middle;
        let boundary = region.round();
        let first = boundary as i64 - 1;
        let fronts = [first, first + 1].map(|r| Front::from_key(hash64(self.seed ^ r as u64)));
        let blend = smooth(0.5 + (region - boundary) * 1250. / (10. * pitch));
        if self.steady {
            return ((0.44 + 0.96 * front).clamp(0.6, 1.4), fronts, blend, 0.);
        }
        let rain = self.moments[DOWNPOUR];
        let hush = self.moments[LULL];
        let wall = (1. - ((x - rain.x - rain.age(self.clock) * 38.).abs() / 2500.)).clamp(0., 1.);
        let feather = (((x - hush.x).abs() - 800.) / 500.).clamp(0., 1.);
        let quiet = 0.5 + 0.5 * (std::f32::consts::PI * feather).cos();
        let hush_factor = 1. - 0.998 * quiet * hush.envelope(self.clock, 3.);
        let surge = wall * rain.envelope(self.clock, 7.) * hush_factor;
        // A storm carries moisture into even the driest era.
        let intensity =
            ((0.15 + 2.4 * front.powf(2.4)) * self.strength).clamp(0.15, 2.2) * hush_factor;
        (intensity + 4.4 * surge, fronts, blend, surge)
    }
    pub fn time(&self) -> f32 {
        self.clock
    }
    pub fn drift(&self, x: f32, p: &Params) -> f32 {
        let gust = self.moments[GUST];
        let arrival = (x - gust.x).abs() / 770.;
        let g = gust.envelope(self.clock - arrival, 5.);
        let sign = if gust.key & 1 == 0 { 1. } else { -1. };
        self.wind * (1. - g) + sign * p.wind * g * 4.2
    }
}
