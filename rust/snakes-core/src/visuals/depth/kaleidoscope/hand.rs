use super::{Pcg32, range};
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Mood {
    Drift,
    Hold,
    Twist,
    Rock,
    Reverse,
}
pub(super) fn smooth(u: f64) -> f64 {
    let t = u.clamp(0., 1.);
    t * t * t * (t * (t * 6. - 15.) + 10.)
}
pub(super) struct Hand {
    pub angle: f64,
    pub mood: Mood,
    origin: f64,
    age: f64,
    duration: f64,
    travel: f64,
    turning: f64,
    last_moving: Mood,
}
impl Hand {
    pub fn new(rng: &mut Pcg32, turning: f64) -> Self {
        let angle = range(rng, -0.35, 0.35);
        Self {
            angle,
            origin: angle,
            mood: Mood::Hold,
            age: 0.,
            duration: range(rng, 3., 8.),
            travel: 0.,
            turning,
            last_moving: Mood::Hold,
        }
    }
    pub fn begin(&mut self, mood: Mood, rng: &mut Pcg32) {
        self.mood = mood;
        self.origin = self.angle;
        self.age = 0.;
        if mood != Mood::Hold {
            self.last_moving = mood;
        }
        let calm = (0.4 - self.turning).max(0.) / 0.4;
        self.duration = match mood {
            Mood::Hold => range(rng, 9., 25.) * (1.65 - self.turning * 1.3 + 0.85 * calm),
            Mood::Twist => range(rng, 3., 6.),
            Mood::Rock => range(rng, 4., 8.),
            _ => range(rng, 15., 38.) * (0.75 + 0.5 * self.turning) * (1. - 0.28 * calm),
        };
        let sign = if self.angle.abs() > 0.72 {
            -self.angle.signum()
        } else if mood == Mood::Reverse {
            -1.
        } else {
            1.
        };
        self.travel = match mood {
            Mood::Twist => {
                // Four turns in five steer back over the heap. The fifth may
                // point into its air pocket until the next return turn.
                let breath = rng.unit() < 0.20 && self.angle.abs() < 1.45;
                let target = if breath {
                    range(rng, 1.55, 2.25) * sign
                } else {
                    let dir = if self.angle > 0. { -1. } else { 1. };
                    dir * range(rng, 0.45, 1.02)
                };
                (target - self.angle).clamp(-2.44, 2.44)
            }
            Mood::Hold => 0.,
            Mood::Rock => 8_f64.to_radians(),
            _ => sign * range(rng, 2., 4.).to_radians() * self.duration,
        };
    }
    pub fn step(&mut self, dt: f64, speed: f64, rng: &mut Pcg32) {
        let pace = match self.mood {
            // Extra speed above the physical cap only shortens pauses.
            Mood::Hold => speed.min(1.) + (speed - 1.).max(0.) * 0.25,
            Mood::Twist => speed.min(1.30),
            _ => speed.min(super::PHYSICAL_SPEED_LIMIT),
        };
        self.age += dt * pace;
        if self.age >= self.duration {
            let draw = rng.unit();
            let mood = if self.mood != Mood::Hold && draw < (0.98 - 0.35 * self.turning) {
                Mood::Hold
            } else if self.last_moving != Mood::Twist && draw < 0.53 {
                Mood::Twist
            } else if draw < 0.67 {
                Mood::Rock
            } else if draw < 0.84 {
                Mood::Drift
            } else {
                Mood::Reverse
            };
            self.begin(mood, rng);
        }
        let u = self.age / self.duration;
        let fraction = match self.mood {
            Mood::Hold => 0.,
            Mood::Twist => smooth(u),
            Mood::Rock => {
                (self.age * std::f64::consts::TAU * 0.4).sin()
                    * smooth(u * 5.)
                    * smooth((1. - u) * 5.)
            }
            _ => {
                // Integral of a trapezoidal velocity profile: eased at both ends.
                const R: f64 = 0.12;
                (if u < R {
                    u * u / (2. * R)
                } else if u > 1. - R {
                    1. - R - (1. - u).powi(2) / (2. * R)
                } else {
                    u - R / 2.
                }) / (1. - R)
            }
        };
        self.angle = self.origin + self.travel * fraction;
    }
}
