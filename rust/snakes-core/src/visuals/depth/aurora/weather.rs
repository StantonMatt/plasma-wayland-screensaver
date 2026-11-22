// SPDX-License-Identifier: GPL-3.0-or-later
use super::super::world::Event;
use super::{Pcg32, range, smooth};

pub(super) const SUBSTORM: u32 = 1;
pub(super) const CORONA: u32 = 2;
pub(super) const METEOR: u32 = 3;
pub(super) const FIREBALL: u32 = 4;

#[derive(Clone, Copy, Default)]
pub(super) struct Storm {
    pub arc: u32,
    pub origin: f32,
    pub start: f64,
    pub expansion: f32,
    pub recovery: f32,
    pub speed: f32,
    pub power: f32,
}
#[derive(Clone, Copy)]
pub(super) struct Meteor {
    pub x: f32,
    pub y: f32,
    pub born: f64,
    pub duration: f32,
    pub dx: f32,
    pub dy: f32,
    pub fire: bool,
    pub power: f32,
}
#[derive(Clone, Copy)]
pub(super) struct Crown {
    pub arc: u32,
    pub x: f32,
    pub born: f64,
    pub life: f32,
    pub power: f32,
}
#[derive(Clone, Copy)]
pub(super) struct Patch {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub period: f32,
    pub phase: f32,
    pub duty: f32,
}

pub(super) struct Weather {
    pub time: f64,
    pub storm: Option<Storm>,
    pub meteor: Option<Meteor>,
    pub crown: Option<Crown>,
    pub patches: Vec<Patch>,
    pub growth: f32,
    pub expansion: f32,
    pub recovery: f32,
    pub activity: f32,
    pub night: f32,
    energy: f32,
    threshold: f32,
    wind: f32,
    clock: f64,
    next_meteor: f64,
    rare_after: f64,
    pace: f32,
}
fn gaussian(r: &mut Pcg32) -> f32 {
    // Six uniforms provide unit variance after scaling, no unbounded tails.
    ((0..6).map(|_| r.unit() as f32).sum::<f32>() - 3.) * std::f32::consts::SQRT_2
}
impl Weather {
    pub fn meteor_due(&self) -> bool {
        self.time >= self.next_meteor
    }
    pub fn new(r: &mut Pcg32, activity: f32, pace: f32) -> Self {
        Self {
            time: 0.,
            storm: None,
            meteor: None,
            crown: None,
            patches: Vec::with_capacity(12),
            growth: 0.,
            expansion: 0.,
            recovery: 0.,
            activity,
            night: 0.5,
            energy: range(r, 0., 0.45),
            threshold: (gaussian(r) * 0.10).exp(),
            wind: range(r, -0.3, 0.4),
            clock: r.unit() * 5760.,
            next_meteor: range(r, 105., 299.) as f64,
            rare_after: 0.,
            pace,
        }
    }
    pub fn advance(&mut self, dt: f32, forecast: f32, r: &mut Pcg32) {
        self.time += f64::from(dt);
        self.clock += f64::from(dt * (1. + 0.04 * self.wind));
        self.night = 0.5 + 0.5 * (self.clock * std::f64::consts::TAU / 5760.).cos() as f32;
        let k = dt / 1500.;
        self.wind += -k * self.wind + 0.42 * (2. * k).sqrt() * gaussian(r);
        let loading = 0.00025 + 0.00062 * self.activity + 0.0017 * self.activity.powi(3);
        if self.activity > 0. {
            self.energy += dt
                * loading
                * (1. + self.wind.clamp(-0.6, 0.9) * 0.42)
                * (0.77 + 0.44 * self.night);
        }
        let loading = smooth((self.energy / self.threshold - 0.65) / 0.35).max(forecast);
        self.growth += (loading - self.growth) * (1. - (-dt / 14.).exp());
        self.expansion = 0.;
        self.recovery = 0.;
        if let Some(s) = self.storm {
            let age = (self.time - s.start) as f32;
            self.expansion = s.power * smooth(age / 6.) * smooth((s.expansion + 24. - age) / 36.);
            self.recovery = s.power
                * smooth((age - s.expansion + 12.) / 36.)
                * smooth((s.expansion + s.recovery - age) / 55.);
            if age > s.expansion + s.recovery {
                self.storm = None;
                self.patches.clear();
            }
        }
        if self
            .crown
            .is_some_and(|c| self.time - c.born > f64::from(c.life))
        {
            self.crown = None;
        }
        if self
            .meteor
            .is_some_and(|m| self.time - m.born > f64::from(m.duration) + 22.)
        {
            self.meteor = None;
        }
    }
    pub fn natural_events(
        &mut self,
        stars: bool,
        rare: bool,
        x: f32,
        y: f32,
        r: &mut Pcg32,
    ) -> [u32; 2] {
        let mut events = [0, 0];
        if self.energy >= self.threshold && self.activity > 0. {
            events[0] = SUBSTORM;
            if rare && self.time >= self.rare_after && r.unit() < 0.30 {
                events[1] = CORONA;
            }
        }
        // Toggling the visible star field must not reseed the curtain story.
        // Schedule the same meteor and consume its draws even when hidden.
        if self.time >= self.next_meteor {
            self.next_meteor = self.time + f64::from(range(r, 110., 303.));
            let fire =
                rare && self.time >= self.rare_after && events[1] == 0 && r.unit() < 1. / 15.;
            self.launch_meteor(x, y, fire, 1., r);
            if !stars {
                self.meteor = None;
            }
        }
        events
    }
    pub fn onset(
        &mut self,
        arc: u32,
        x: f32,
        bounds: [f32; 2],
        power: f32,
        density: f32,
        r: &mut Pcg32,
    ) {
        let [width, height] = bounds;
        self.storm = Some(Storm {
            arc,
            origin: x,
            start: self.time,
            expansion: range(r, 64., 116.),
            recovery: range(r, 190., 349.),
            speed: range(r, 380., 450.),
            power,
        });
        self.energy = 0.;
        self.threshold = (0.14 * gaussian(r)).exp();
        self.patches.clear();
        for _ in 0..(3. + density * 0.085) as usize {
            let width_fraction = range(r, 0.10, 0.28);
            let aspect = range(r, 1.2, 3.0);
            self.patches.push(Patch {
                x: range(r, -0.06, 0.96) * width,
                y: range(r, 0.31, 0.73) * height,
                width: width_fraction * height,
                height: width_fraction * height / aspect,
                period: range(r, 5., 15.),
                phase: r.unit() as f32,
                duty: range(r, 0.30, 0.50),
            });
        }
    }
    pub fn corona(&mut self, arc: u32, x: f32, power: f32, r: &mut Pcg32) {
        self.crown = Some(Crown {
            arc,
            x,
            born: self.time,
            life: range(r, 43., 68.),
            power,
        });
        self.rare_after = self.time + 600.;
    }
    pub fn launch_meteor(&mut self, x: f32, y: f32, fire: bool, power: f32, r: &mut Pcg32) {
        self.meteor = Some(Meteor {
            x,
            y,
            born: self.time,
            duration: if fire {
                2.1f32.max(1.5 * self.pace)
            } else {
                range(r, 0.70, 1.16).max(self.pace)
            },
            dx: range(r, 350., 640.),
            dy: range(r, 95., 220.),
            fire,
            power,
        });
        if fire {
            self.rare_after = self.time + 600.;
        }
    }
    pub fn validate(e: &Event) -> bool {
        (1..=4).contains(&e.id)
            && e.at_ns >= 0
            && e.anchor.iter().all(|v| v.is_finite())
            && e.strength.is_finite()
    }
}
