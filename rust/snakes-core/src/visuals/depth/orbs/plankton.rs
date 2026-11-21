//! The far field follows the same water as the colony. These motes have
//! negligible mass, so they need neither the contact solver nor the mass grid.
use super::*;

pub(super) struct Speck {
    pub pos: Vec2,
    pub r: f64,
    pub hue: f64,
    pub light: f64,
    v: Vec2,
    age: f64,
    aging_rate: f64,
    life: f64,
}
pub(super) struct Plankton {
    pub field: Vec<Speck>,
    rng: Pcg32,
    phase: usize,
}
impl Plankton {
    pub fn new(input: &WorldInput, depth: i32, height: f64) -> Self {
        let mut rng = Pcg32::new(input.seed, 0xfade);
        let count =
            (input.layout.visible_area / (2560. * 1440.) * f64::from(depth) * 1.4).round() as usize;
        let field = (0..count.min(640))
            .map(|_| {
                let life = range(&mut rng, 48., 95.);
                Speck {
                    pos: input.layout.choose_visible_anchor(&mut rng, 0.).unwrap(),
                    r: range(&mut rng, 3., 10.) * height / 1440.,
                    hue: rng.unit(),
                    light: range(&mut rng, 0.16, 0.32),
                    v: Vec2::default(),
                    age: range(&mut rng, 0., life),
                    aging_rate: 1.,
                    life,
                }
            })
            .collect();
        Self {
            field,
            rng,
            phase: 0,
        }
    }
    pub fn advance(&mut self, layout: &Layout, flow: &FlowField, dt: f64) {
        for (i, s) in self.field.iter_mut().enumerate() {
            if i % 16 == self.phase {
                let current = space::tracer_current(layout, flow, s.pos);
                s.v = current.velocity * 0.52;
                // Suspended lights disperse faster in the shear along an
                // exposed shore. Their replacements enter from the whole basin.
                s.aging_rate = 1. + 3. * current.wall_weight.powi(2);
            }
            s.pos += s.v * dt;
            s.age += dt * s.aging_rate;
            if s.age >= s.life || !layout.contains(s.pos) {
                s.pos = layout.choose_visible_anchor(&mut self.rng, 0.).unwrap();
                s.age = 0.;
                s.aging_rate = 1.;
                s.v = Vec2::default();
            }
        }
        self.phase = (self.phase + 1) % 16;
    }
}
impl Speck {
    pub fn opacity(&self) -> f64 {
        ease(self.age / 3.) * ease((self.life - self.age) / 4.)
    }
}
