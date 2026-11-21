use super::{
    super::common::rng::Pcg32,
    terrain::{Meadow, between, smooth},
};
use std::f32::consts::TAU;

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Species {
    Dipper,
    Syncer,
    Ghost,
    Hunter,
}
use Species::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Patrol,
    Perched,
    Approach,
    Court,
    Paired,
    Rest,
    Lured,
    Fading,
    Lost,
}

/// One flash follows a straight dip, a quarter-second rounded turn, and an
/// inclined climb. Distances are in pixels at the 1440 px reference height.
#[derive(Clone, Copy)]
pub struct Swoop {
    dip: f32,
    climb: f32,
    pub across: f32,
    turn_speed: f32,
    climb_speed: f32,
}

impl Swoop {
    pub fn new(key: u64) -> Self {
        let bits = super::hash64(key);
        let unit = |shift: u32| ((bits >> shift) & 0xffff) as f32 / 65535.;
        let dip = 8. + 8. * unit(0);
        let climb = 38. + 22. * unit(16);
        let turn_speed = dip / 0.2;
        // At the bottom smoothstep's slope is 1.5. With a 0.25 s turn,
        // |acceleration| = 12 * turn_speed and radius = across² / |acceleration|.
        let radius = 8.3 + 5.4 * unit(32);
        let across = (12. * turn_speed * radius).sqrt();
        let lean = (37.5 + (24. * unit(48) - 12.)).clamp(30., 45.);
        Self {
            dip,
            climb,
            across,
            turn_speed,
            climb_speed: across / lean.to_radians().tan(),
        }
    }
    pub const BOTTOM: f32 = 0.246875;
    const TURN_START: f32 = Self::BOTTOM - 0.125;
    const TURN_END: f32 = Self::BOTTOM + 0.125;

    pub fn end(self) -> f32 {
        Self::TURN_END
            + 0.1
            + (self.climb
                - self.turn_speed * 0.078125
                - 0.05 * (self.turn_speed + self.climb_speed))
                / self.climb_speed
    }
    pub fn offset(self, t: f32) -> f32 {
        let t = t.clamp(0., self.end());
        if t < Self::TURN_START {
            return self.turn_speed * t;
        }
        if t < Self::TURN_END {
            let u = (t - Self::TURN_START) / 0.25;
            // Integral of v * (1 - 2*smoothstep(u)); velocity is zero at u=0.5.
            return self.turn_speed * (Self::TURN_START + 0.25 * (u - u * u * u * (2. - u)));
        }
        let ramp = (t - Self::TURN_END).min(0.1);
        let u = ramp / 0.1;
        self.dip
            - 0.078125 * self.turn_speed
            - self.turn_speed * ramp
            - (self.climb_speed - self.turn_speed) * 0.1 * u * u * u * (1. - 0.5 * u)
            - self.climb_speed * (t - Self::TURN_END - ramp)
    }
}

#[derive(Clone, Copy)]
pub struct Fly {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub z: f32,
    pub species: Species,
    pub female: bool,
    pub state: State,
    pub home: usize,
    pub phase: f32,
    pub period: f32,
    pub burst: u8,
    pub remaining: u8,
    pub since: f32,
    // Safety uses absolute time; the envelope age can advance after a cross-fly onset.
    pub last_flash: f64,
    stroke_key: u64,
    pub swoop: Swoop,
    pub pulse: f32,
    pub light: f32,
    pub activity: f32,
    pub answer: f32,
    pub target: usize,
    pub exchanges: u8,
    pub timer: f32,
    pub altitude: f32,
    pub heading: f32,
    pub turn: f32,
    pub roam: f32,
    pub perch_age: f32,
    pub hover: f32,
    pub meal: f32,
    pub tree: Option<[f32; 2]>,
    pub tree_pulse: bool,
    pub tree_delay: f32,
    pub tree_gain: f32,
    pub jitter: f32,
    pub heard_phase: f32,
    pub heard_age: f32,
    pub heard_period: f32,
    pub gust_speed: f32,
    pub separating: [f32; 2],
}
pub const NOBODY: usize = usize::MAX;

#[derive(Clone, Copy)]
pub struct Glade {
    pub x: f32,
    pub basin: f32,
    pub drift: f32,
    pub width: f32,
}

pub fn glades(meadow: &Meadow, rng: &mut Pcg32) -> Vec<Glade> {
    let mut glades = Vec::new();
    for o in &meadow.outputs {
        let width = 0.18 * o.width as f32;
        for i in 0..4 {
            let x = o.x as f32 + (i as f32 + between(rng, 0.44, 0.56)) * o.width as f32 / 4.;
            glades.push(Glade {
                x,
                basin: x,
                drift: between(rng, -3., 3.),
                width,
            });
        }
    }

    glades
}
impl Fly {
    pub fn spawn(
        id: usize,
        species: Species,
        glades: &[Glade],
        meadow: &Meadow,
        rng: &mut Pcg32,
        cadence: f32,
    ) -> Self {
        let home = rng.next_u32() as usize % glades.len();
        let g = glades[home];
        let x = (g.x + (between(rng, -1., 1.) + between(rng, -1., 1.)) * g.width)
            .clamp(meadow.bounds.x as f32, meadow.bounds.right() as f32);
        let c = meadow.column(x);
        let female = rng.unit() < 0.21;
        let altitude = if female {
            between(rng, 0.04, 0.115)
        } else {
            match species {
                Dipper => between(rng, 0.065, 0.42),
                Syncer => between(rng, 0.16, 0.61),
                Ghost => between(rng, 0.04, 0.25),
                Hunter => between(rng, 0.12, 0.5),
            }
        };
        let burst = 5 + (rng.next_u32() % 3) as u8;
        let period = match species {
            Dipper => between(rng, 4.7, 6.3) / cadence,
            Syncer => {
                between(rng, 6.4, 7.9) / cadence + (burst - 1) as f32 * (0.5 / cadence).max(0.5)
            }
            Ghost => between(rng, 8., 12.),
            Hunter => between(rng, 4., 7.) / cadence,
        };
        let stroke_key = super::hash64(u64::from(x.to_bits()) ^ id as u64);
        Self {
            stroke_key,
            swoop: Swoop::new(stroke_key),
            x,
            y: c.flight_floor - altitude * c.height,
            vx: between(rng, -9., 9.),
            vy: 0.,
            z: between(rng, 0.3, 0.84),
            species,
            female,
            state: if female {
                State::Perched
            } else {
                State::Patrol
            },
            home,
            phase: between(rng, 0., 1.),
            period,
            burst,
            remaining: 0,
            since: between(rng, 0., period),
            last_flash: f64::NEG_INFINITY,
            pulse: 1.,
            light: 0.,
            activity: between(rng, 0., 1.),
            answer: -1.,
            target: NOBODY,
            exchanges: 0,
            timer: 0.,
            altitude,
            heading: between(rng, 0., TAU),
            turn: 0.,
            roam: (between(rng, -1., 1.) + between(rng, -1., 1.) + between(rng, -1., 1.)) * g.width,
            perch_age: between(rng, 10., 180.),
            hover: 0.,
            meal: 0.,
            tree: None,
            tree_pulse: false,
            tree_delay: 0.,
            tree_gain: 1.,
            jitter: between(rng, 0., TAU) + id as f32 * 0.01,
            heard_phase: 0.,
            heard_age: f32::INFINITY,
            heard_period: period,
            gust_speed: 0.,
            separating: [0.; 2],
        }
    }
    pub fn fire(&mut self, now: f64, strength: f32) -> bool {
        // Every source, including scripted answers, post-rain and hunters, shares this refractory.
        if (self.species == Ghost && self.tree.is_none())
            || !commit_flash(now, &mut self.last_flash, &mut self.since)
        {
            return false;
        }
        if self.species == Dipper && !self.female && self.state == State::Patrol {
            self.stroke_key = super::hash64(self.stroke_key);
            self.swoop = Swoop::new(self.stroke_key);
        }
        self.pulse = strength;
        true
    }
    pub fn envelope(&self, wet_attack: f32, afterglow: f32) -> f32 {
        if self.species == Ghost {
            return 0.40 + 0.06 * (TAU * self.phase).sin();
        }
        if self.species == Dipper && !self.female && self.state == State::Patrol {
            let t = self.since;
            // The chemical response in the integrator contributes about one tick.
            let peak = Swoop::BOTTOM + 0.10 - 1. / 30. + wet_attack;
            let rise = 0.55 * smooth(t / (0.055 + wet_attack)) + 0.45 * smooth(t / peak);
            let climb = 1. - 0.23 * smooth((t - peak) / (self.swoop.end() - peak).max(0.1));
            let fade = 1. - smooth((t - self.swoop.end()) / (0.20 + afterglow));
            return 0.045 + 0.955 * self.pulse * rise * climb * fade;
        }
        let (attack, hold, decay) = match self.species {
            Dipper => (0.065, 0.72, 0.20 + afterglow),
            Syncer => (0.085, 0.09, 0.265),
            Hunter => (0.14, 0.35, 0.62 + afterglow),
            Ghost => unreachable!(),
        };
        let attack = attack + wet_attack;
        let hold = hold + wet_attack;
        let t = self.since;
        let chem = if t < attack {
            smooth(t / attack)
        } else if t < hold {
            1.
        } else {
            let tail = (t - hold) / decay;
            (1. - smooth(tail)).powi(2)
        };
        let ember = if self.species == Syncer { 0. } else { 0.045 };
        ember + (1. - ember) * chem * self.pulse
    }
}

/// Commit all flashing lights against the same simulation time, never a loop-local age.
pub(super) fn commit_flash(now: f64, last: &mut f64, since: &mut f32) -> bool {
    if now - *last < 0.5 {
        return false;
    }
    *last = now;
    *since = 0.;
    true
}
