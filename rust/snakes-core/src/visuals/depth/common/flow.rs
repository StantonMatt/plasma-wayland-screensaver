//! Currents in logical pixels/second. Advance once per simulation substep;
//! sampling is read-only and never consumes randomness. Eddies may also be
//! sampled individually (e.g. the temporary vortex during a Ribbons event).
use super::{
    layout::{Layout, Vec2},
    rng::Pcg32,
};
use std::{
    f64::consts::TAU,
    ops::{Add, AddAssign, Div, Mul, Neg, Sub},
};

impl Add for Vec2 {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        p(self.x + b.x, self.y + b.y)
    }
}
impl AddAssign for Vec2 {
    fn add_assign(&mut self, b: Self) {
        self.x += b.x;
        self.y += b.y
    }
}
impl Sub for Vec2 {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        p(self.x - b.x, self.y - b.y)
    }
}
impl Mul<f64> for Vec2 {
    type Output = Self;
    fn mul(self, k: f64) -> Self {
        p(self.x * k, self.y * k)
    }
}
impl Div<f64> for Vec2 {
    type Output = Self;
    fn div(self, k: f64) -> Self {
        p(self.x / k, self.y / k)
    }
}
impl Neg for Vec2 {
    type Output = Self;
    fn neg(self) -> Self {
        p(-self.x, -self.y)
    }
}
pub fn p(x: f64, y: f64) -> Vec2 {
    Vec2 { x, y }
}
impl Vec2 {
    pub fn dot(self, b: Self) -> f64 {
        self.x * b.x + self.y * b.y
    }
    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }
    pub fn unit(self) -> Self {
        self / self.length().max(1e-9)
    }
    pub fn limited(self, max: f64) -> Self {
        let s = self.dot(self);
        if s > max * max {
            self * (max / s.sqrt())
        } else {
            self
        }
    }
    pub fn perpendicular(self) -> Self {
        p(-self.y, self.x)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Current {
    pub velocity: Vec2,
    /// Signed curl in inverse seconds (positive is clockwise in screen space).
    pub vorticity: f64,
}
impl Current {
    fn mix(self, other: Self, t: f64) -> Self {
        Self {
            velocity: self.velocity + (other.velocity - self.velocity) * t,
            vorticity: self.vorticity + (other.vorticity - self.vorticity) * t,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Eddy {
    pub centre: Vec2,
    pub core_radius: f64,
    /// Circulation divided by 2π, in square logical pixels/second.
    pub circulation: f64,
}
impl Eddy {
    pub fn sample(self, point: Vec2) -> Current {
        self.evaluate(
            point,
            1. / self.core_radius.max(1e-3).powi(2),
            self.circulation,
        )
    }
    fn evaluate(self, point: Vec2, inv_core: f64, strength: f64) -> Current {
        let delta = point - self.centre;
        let r2 = delta.dot(delta);
        let q = r2 * inv_core;
        // Relative error outside five core radii is below 1.4e-11.
        let gaussian = if q > 25. { 0. } else { (-q).exp() };
        let omega = if q < 1e-6 {
            strength * inv_core * (1. - 0.5 * q)
        } else {
            strength * (1. - gaussian) / r2
        };
        Current {
            velocity: delta.perpendicular() * omega,
            vorticity: 2. * strength * inv_core * gaussian,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct FlowSettings {
    pub drift_speed: f64,
    pub eddy_speed: f64,
    pub core_range: [f64; 2],
    pub life_range: [f64; 2],
    pub target_eddies: usize,
    pub limit: usize,
    pub heading_seconds: f64,
    pub noise_speed: f64,
    pub tides: bool,
    /// Optional angular limit around the horizontal tide axis.
    pub tide_heading_limit: Option<f64>,
}
impl FlowSettings {
    pub fn orbs(swirl: bool, area: f64, scale: f64) -> Self {
        Self {
            drift_speed: if swirl { 10. } else { 22. } * scale,
            eddy_speed: if swirl { 49. } else { 43. } * scale,
            core_range: if swirl {
                [340. * scale, 610. * scale]
            } else {
                [120. * scale, 340. * scale]
            },
            life_range: [40., 180.],
            target_eddies: ((if swirl { 2.5 } else { 7. }) * area)
                .round()
                .clamp(2., 12.) as usize,
            limit: 12,
            heading_seconds: 900.,
            noise_speed: 13. * scale,
            tides: false,
            tide_heading_limit: Some(25f64.to_radians()),
        }
    }
    pub fn ribbons(turbulence: f64, area: f64) -> Self {
        let t = turbulence.clamp(0., 1.);
        Self {
            drift_speed: 70.,
            eddy_speed: 20. + 130. * t,
            core_range: [120., 450.],
            life_range: [30., 120.],
            target_eddies: ((3. + 9. * t) * area).round().clamp(1., 16.) as usize,
            limit: 16,
            heading_seconds: 260.,
            noise_speed: 8. * t,
            tides: true,
            tide_heading_limit: None,
        }
    }
}
#[derive(Clone, Debug)]
struct LiveEddy {
    eddy: Eddy,
    age: f64,
    life: f64,
    centre_bounds: Option<super::layout::Rect>,
    inv_core: f64,
    strength: f64,
}
// Two octaves of a seeded, evolving stream function. Tabulate its analytic
// curl and Laplacian at grid nodes, then interpolate in space and time.
struct CurlGrid {
    origin: Vec2,
    spacing: Vec2,
    nx: usize,
    ny: usize,
    phase: [f64; 4],
    k: [Vec2; 4],
    a: Vec<Current>,
    b: Vec<Current>,
    time: f64,
    blend: f64,
    amplitude: f64,
}
impl CurlGrid {
    fn new(layout: &Layout, rng: &mut Pcg32, amplitude: f64) -> Self {
        let b = layout.bounds;
        let nx = (b.width / 164.).ceil().clamp(2., 48.) as usize + 1;
        let ny = (b.height / 96.).ceil().clamp(2., 16.) as usize + 1;
        let k = std::array::from_fn(|i| {
            let t = rng.unit() * TAU;
            p(t.cos(), t.sin()) * (if i < 2 { 0.0021 } else { 0.0042 })
        });
        let mut g = Self {
            origin: p(b.x, b.y),
            spacing: p((nx - 1) as f64 / b.width, (ny - 1) as f64 / b.height),
            nx,
            ny,
            phase: std::array::from_fn(|_| rng.unit() * TAU),
            k,
            a: vec![Current::default(); nx * ny],
            b: vec![Current::default(); nx * ny],
            time: 0.,
            blend: 0.,
            amplitude,
        };
        g.bake();
        std::mem::swap(&mut g.a, &mut g.b);
        g.time = 1.;
        g.bake();
        g
    }
    fn bake(&mut self) {
        for j in 0..self.ny {
            for i in 0..self.nx {
                let v = p(i as f64 / self.spacing.x, j as f64 / self.spacing.y);
                let mut c = Current::default();
                for (m, k) in self.k.iter().copied().enumerate() {
                    let freq = if m < 2 { 0.0021 } else { 0.0042 };
                    let amp = self.amplitude * (if m < 2 { 0.52 } else { 0.23 }) / freq;
                    let (s, co) =
                        (v.dot(k) + self.phase[m] + self.time * (0.027 + m as f64 * 0.013))
                            .sin_cos();
                    c.velocity += k.perpendicular() * (amp * co);
                    c.vorticity -= amp * freq * freq * s;
                }
                self.b[j * self.nx + i] = c;
            }
        }
    }
    fn advance(&mut self, dt: f64) {
        self.blend += dt;
        while self.blend >= 1. {
            self.blend -= 1.;
            self.time += 1.;
            std::mem::swap(&mut self.a, &mut self.b);
            self.bake();
        }
    }
    fn sample(&self, v: Vec2) -> Current {
        let v = v - self.origin;
        let x = (v.x * self.spacing.x).clamp(0., (self.nx - 1) as f64);
        let y = (v.y * self.spacing.y).clamp(0., (self.ny - 1) as f64);
        let i = (x as usize).min(self.nx - 2);
        let j = (y as usize).min(self.ny - 2);
        let idx = j * self.nx + i;
        let f = |grid: &[Current]| {
            grid[idx].mix(grid[idx + 1], x - i as f64).mix(
                grid[idx + self.nx].mix(grid[idx + self.nx + 1], x - i as f64),
                y - j as f64,
            )
        };
        f(&self.a).mix(f(&self.b), self.blend)
    }
}
pub struct FlowField {
    eddy_wall_clearance: f64,
    settings: FlowSettings,
    rng: Pcg32,
    eddies: Vec<LiveEddy>,
    grid: CurlGrid,
    pub drift: Vec2,
    heading: f64,
    turn: f64,
    next_turn: f64,
    tide: f64,
    weather: f64,
    weather_target: f64,
    next_weather: f64,
    next_birth: f64,
}
impl FlowField {
    pub fn new(seed: u64, layout: &Layout, settings: FlowSettings) -> Self {
        Self::with_wall_clearance(seed, layout, settings, 0.)
    }
    /// Keep eddy centres inside their source screen by this many core radii.
    pub fn with_wall_clearance(
        seed: u64,
        layout: &Layout,
        settings: FlowSettings,
        core_radii: f64,
    ) -> Self {
        let mut rng = Pcg32::new(seed, 0xf10);
        let grid = CurlGrid::new(layout, &mut rng, settings.noise_speed);
        let heading = if settings.tides {
            (rng.unit() - 0.5) * 0.85
        } else {
            rng.unit() * TAU
        };
        let mut f = Self {
            eddy_wall_clearance: core_radii,
            settings,
            rng,
            eddies: Vec::with_capacity(settings.limit),
            grid,
            drift: p(heading.cos(), heading.sin()) * settings.drift_speed,
            heading,
            turn: 0.,
            next_turn: 0.,
            tide: 650.,
            weather: 1.,
            weather_target: 1.,
            next_weather: 0.,
            next_birth: 0.,
        };
        for _ in 0..settings.target_eddies.min(settings.limit) {
            f.birth(layout, true)
        }
        f
    }
    fn birth(&mut self, layout: &Layout, mature: bool) {
        let s = self.settings;
        let centre = layout
            .choose_visible_anchor(&mut self.rng, 0.)
            .unwrap_or(layout.bounds.centre());
        let core = s.core_range[0] + (s.core_range[1] - s.core_range[0]) * self.rng.unit();
        let centre_bounds = (self.eddy_wall_clearance > 0.).then(|| {
            let rect = layout
                .outputs
                .iter()
                .find(|o| o.rect.contains(centre))
                .unwrap_or(&layout.outputs[0])
                .rect;
            let margin = (core * self.eddy_wall_clearance).min(0.49 * rect.width.min(rect.height));
            super::layout::Rect {
                x: rect.x + margin,
                y: rect.y + margin,
                width: rect.width - 2. * margin,
                height: rect.height - 2. * margin,
            }
        });
        let centre = centre_bounds.map_or(centre, |r| {
            p(
                centre.x.clamp(r.x, r.right()),
                centre.y.clamp(r.y, r.bottom()),
            )
        });
        let circulation = core
            * s.eddy_speed
            * (0.8 + self.rng.unit())
            * if self.rng.unit() < 0.5 { -1. } else { 1. };
        let life = s.life_range[0] + (s.life_range[1] - s.life_range[0]) * self.rng.unit();
        self.eddies.push(LiveEddy {
            eddy: Eddy {
                centre,
                core_radius: core,
                circulation,
            },
            age: if mature { life * self.rng.unit() } else { 0. },
            life,
            centre_bounds,
            inv_core: 1. / (core * core),
            strength: if mature { circulation } else { 0. },
        });
    }
    pub fn advance(&mut self, layout: &Layout, dt: f64) {
        let s = self.settings;
        self.next_turn -= dt;
        if self.next_turn <= 0. {
            self.next_turn = 12. + self.rng.unit() * 23.;
            self.turn = (self.rng.unit() - 0.5) * 12. / s.heading_seconds;
        }
        self.heading += self.turn * dt;
        let mut speed = s.drift_speed;
        if s.tides {
            // Keep the lanes predominantly horizontal on either side of a
            // tide turn. Reflect the slow random walk at the two angle limits.
            if let Some(limit) = s.tide_heading_limit {
                let axis = (self.heading / std::f64::consts::PI).round() * std::f64::consts::PI;
                let spread = self.heading - axis;
                if spread.abs() > limit {
                    self.heading = axis + spread.clamp(-limit, limit);
                    self.turn = -spread.signum() * self.turn.abs();
                }
            }
            self.next_weather -= dt;
            if self.next_weather <= 0. {
                self.next_weather = 100. + 160. * self.rng.unit();
                self.weather_target = 0.55 + 0.75 * self.rng.unit();
            }
            self.weather += (self.weather_target - self.weather) * (dt / 95.).min(1.);
            self.tide -= dt;
            if self.tide <= -20. {
                self.tide = 480. + 420. * self.rng.unit();
            } else if self.tide <= 0. && self.tide + dt > 0. {
                self.heading += std::f64::consts::PI + (self.rng.unit() - 0.5) * 0.7;
            }
            speed *= (self.tide.abs() / 20.).min(1.);
        }
        self.drift = p(self.heading.cos(), self.heading.sin()) * speed;
        self.grid.advance(dt);
        self.eddies.retain_mut(|e| {
            e.age += dt;
            e.eddy.centre += self.drift * (0.6 * dt);
            if let Some(b) = e.centre_bounds {
                e.eddy.centre.x = e.eddy.centre.x.clamp(b.x, b.right());
                e.eddy.centre.y = e.eddy.centre.y.clamp(b.y, b.bottom());
            }
            e.strength = e.eddy.circulation
                * self.weather
                * (e.age / 10.).min(1.)
                * ((e.life - e.age) / 10.).clamp(0., 1.);
            e.age < e.life
        });
        self.next_birth -= dt;
        if self.next_birth <= 0. {
            let target = (s.target_eddies as f64 * self.weather).max(1.);
            let rate = 2. * target / (s.life_range[0] + s.life_range[1]);
            self.next_birth = -(1. - self.rng.unit()).ln() / rate;
            if self.eddies.len() < s.limit {
                self.birth(layout, false)
            }
        }
    }
    /// Stream function and its curl for the small suspended particles in a
    /// closed basin. The long return current and compact eddy cores keep the
    /// potential finite even on a very wide screen union. A caller can taper
    /// the scalar at a wall before taking its curl, preserving area.
    pub fn tracer_stream(&self, point: Vec2, origin: Vec2, height: f64) -> (f64, Vec2) {
        let k = TAU / height.max(1.);
        let (sx, cx) = ((point.x - origin.x) * k * 0.65 + self.heading).sin_cos();
        let (sy, cy) = ((point.y - origin.y) * k + self.heading).sin_cos();
        let drift = self.drift;
        let mut stream = drift.x * sy / k - drift.y * sx / (0.65 * k);
        let mut velocity = p(drift.x * cy, drift.y * cx);
        for e in &self.eddies {
            let d = point - e.eddy.centre;
            let inverse = 1. / (1. + d.dot(d) * e.inv_core);
            stream += 0.5 * e.strength * inverse;
            velocity += d.perpendicular() * (e.strength * e.inv_core * inverse * inverse);
        }
        (stream, velocity)
    }
    pub fn sample(&self, point: Vec2) -> Current {
        let mut c = self.grid.sample(point);
        c.velocity += self.drift;
        for e in &self.eddies {
            let v = e.eddy.evaluate(point, e.inv_core, e.strength);
            c.velocity += v.velocity;
            c.vorticity += v.vorticity;
        }
        c
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::visuals::depth::common::layout::{OutputRect, Rect};
    #[test]
    fn core_is_finite_and_curl_is_the_spatial_derivative() {
        let e = Eddy {
            centre: p(2., 5.),
            core_radius: 160.,
            circulation: -3100.,
        };
        let c = e.sample(e.centre);
        assert_eq!(c.velocity, p(0., 0.));
        assert!((c.vorticity - 2. * e.circulation / 160f64.powi(2)).abs() < 1e-10);
        let a = p(83., 71.);
        let h = 0.001;
        let dx = (e.sample(a + p(h, 0.)).velocity - e.sample(a - p(h, 0.)).velocity) / (2. * h);
        let dy = (e.sample(a + p(0., h)).velocity - e.sample(a - p(0., h)).velocity) / (2. * h);
        assert!((dx.x + dy.y).abs() < 1e-8);
        assert!((e.sample(a).vorticity - dx.y + dy.x).abs() < 1e-7);
    }
    #[test]
    fn read_order_cannot_advance_weather_or_change_a_tide() {
        let l = Layout::new(
            vec![OutputRect {
                id: 1,
                primary: true,
                rect: Rect {
                    x: -200.,
                    y: 0.,
                    width: 1200.,
                    height: 800.,
                },
            }],
            1,
        )
        .unwrap();
        let s = FlowSettings::ribbons(0.8, 1.);
        let mut a = FlowField::new(80, &l, s);
        let mut b = FlowField::new(80, &l, s);
        let point = p(-180., 122.);
        let initial = a.sample(point);
        let origin = l.bounds.centre();
        let h = 0.01;
        let psi = |p| a.tracer_stream(p, origin, l.bounds.height).0;
        let curl = p(
            (psi(point + p(0., h)) - psi(point - p(0., h))) / (2. * h),
            (psi(point - p(h, 0.)) - psi(point + p(h, 0.))) / (2. * h),
        );
        assert!((curl - a.tracer_stream(point, origin, l.bounds.height).1).length() < 1e-5);
        for _ in 0..2400 {
            a.sample(p(30., 40.));
            a.advance(&l, 0.5);
            b.advance(&l, 0.5);
            assert_eq!(a.sample(point), b.sample(point));
            assert!(a.eddies.len() <= 16);
        }
        assert_ne!(initial, a.sample(point));
    }
}
