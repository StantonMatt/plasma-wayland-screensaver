use super::{
    World,
    events::{COURT_HEIGHT, COURT_SECONDS, TREE},
    fly::{Fly, NOBODY, Species::*, State::*},
    terrain::{between, smooth},
};
use std::f32::consts::TAU;

fn crown_pulse(age: f32) -> f32 {
    smooth(age / 0.22) * (1. - smooth((age - 0.26) / 0.31))
}

impl World {
    pub(super) fn move_and_flash(&mut self, dt: f32) {
        self.grid.fill(&self.flies);
        self.signals.clear();
        #[cfg(test)]
        self.coupling_edges.clear();
        let activity = self.night_activity();
        // Give Ocean a longer afterglow as the meadow reaches peak night.
        // Burst onsets and the short dark gaps between Syncer flashes keep
        // their timing. Rain and the gathering have their own envelopes.
        let afterglow = if self.art.palette == 0 && self.weather.kind == 0 && self.after_rain <= 0.
        {
            if self.params.nightCycle {
                1.55 * smooth((self.night_age / self.night_length - 0.04) / 0.07) * activity
            } else {
                1.55
            }
        } else {
            0.
        };
        // Syncers are abundant at peak. Dusk belongs to Ghosts and early Dippers.
        let dusk = if self.params.nightCycle {
            1. - smooth(self.night_age / (0.125 * self.night_length))
        } else {
            0.
        };
        let rain = self.weather.rain();
        let wet_attack = 0.14 * smooth((self.after_rain - 2.) / 2.);
        let tree_ready = self.tree_ready;
        let mut landed = 0;
        let mut invited = 0;
        let left = self.meadow.bounds.x as f32;
        let right = self.meadow.bounds.right() as f32;
        for i in 0..self.flies.len() {
            let mut f = self.flies[i];
            let c = self.meadow.column(f.x);
            let size = (c.height / 1440.).clamp(0.12, 3.);
            let home = self.glades[f.home];
            let mut want_x = home.x + f.roam;
            let mut want_y = c.flight_floor - c.height * f.altitude;
            if f.species == Hunter
                && f.female
                && self
                    .flies
                    .get(f.target)
                    .is_some_and(|p| p.state == Lured && p.target == i)
            {
                want_y -= 38. * size;
            }
            let active = f.activity
                <= activity
                    + match f.species {
                        Ghost => 0.18 + 0.22 * dusk,
                        Hunter => 0.12 - 0.26 * dusk,
                        Dipper => -0.04 * dusk,
                        Syncer => -0.28 * dusk,
                    };
            let shelter = rain * smooth((self.weather.age - f.activity * 2.) / 2.);
            if !active {
                want_y = c.floor - c.height * (0.02 + f.activity * 0.055);
            }
            let speed = match f.species {
                Dipper => 45.,
                Syncer => 30.,
                Ghost => 15.,
                Hunter => 55.,
            } * self.pace
                * size;
            f.since += dt;
            f.timer -= dt;
            f.perch_age -= dt;
            f.hover = (f.hover - dt).max(0.);
            f.meal = (f.meal - dt).max(0.);
            let mut fired = false;
            let mut may_cycle = true;
            let mut phase_speed = 1. / f.period;
            f.heard_age += dt;
            if f.heard_age < 14. && f.tree.is_none() && f.species == Syncer {
                // Continue the beat estimated from the flashes this individual
                // actually saw. Outside sight of its neighbours it free-runs.
                f.heard_phase = (f.heard_phase + dt / f.heard_period).fract();
                let error = (f.heard_phase - f.sync_offset / f.heard_period - f.phase + 0.5)
                    .rem_euclid(1.)
                    - 0.5;
                phase_speed = (phase_speed + self.phase_coupling * error).max(0.007);
            }
            f.phase += dt * phase_speed;
            if f.female && f.tree.is_none() {
                f.answer -= dt;
                if f.answer <= 0. && f.answer > -dt && f.state == Perched && rain < 0.5 {
                    fired = f.fire(self.flash_time, 0.55);
                }
                if f.perch_age <= 0. && f.state == Perched {
                    f.roam = between(&mut self.rng, -home.width, home.width);
                    f.perch_age = between(&mut self.rng, 60., 180.);
                }
                may_cycle = false;
            }
            if let Some(perch) = f.tree {
                if self.weather.kind == TREE {
                    invited += 1;
                    let dx = perch[0] - f.x;
                    let dy = perch[1] - f.y;
                    if dx * dx + dy * dy < 400. {
                        landed += 1;
                    }
                    f.vx += (dx * 0.34 - f.vx) * (dt * 3.).min(1.);
                    f.vy += (dy * 0.34 - f.vy) * (dt * 3.).min(1.);
                    if tree_ready {
                        may_cycle = false;
                        f.remaining = 0;
                        if self.tree_since >= f.tree_delay && self.tree_since < f.tree_delay + dt {
                            fired = f.fire(self.flash_time, 1.);
                            f.tree_pulse = fired;
                        }
                    }
                }
            } else {
                let mut steering = 0.30;
                let mut wander = 1.;
                if f.female {
                    steering = 0.55;
                    wander = 0.025;
                }
                match f.state {
                    Lured | Approach | Court => {
                        if let Some(other) = self.flies.get(f.target) {
                            if matches!(other.state, Paired | Rest | Lost) {
                                f.state = Patrol;
                                f.target = NOBODY;
                            } else {
                                want_x = other.x;
                                want_y = other.y;
                                if f.state == Approach {
                                    // Enter the helix at its high, wide first turn.
                                    want_x += 40. * size;
                                    want_y -= COURT_HEIGHT * size;
                                }
                                steering = 2.4;
                                wander = 0.;
                                if f.state == Lured
                                    && (f.x - other.x).hypot(f.y - other.y) <= 12. * size
                                    && f.since >= 0.5
                                    && other.since >= 0.5
                                {
                                    f.state = Fading;
                                    f.timer = 1.7;
                                    f.exchanges = 0;
                                    fired = f.fire(self.flash_time, 1.);
                                    self.flies[f.target].meal = 10.;
                                    self.flies[f.target].fire(self.flash_time, 1.);
                                } else if f.state == Approach
                                    && (f.x - want_x).hypot(f.y - want_y) < 8. * size
                                {
                                    f.state = Court;
                                    f.timer = COURT_SECONDS;

                                    f.exchanges = 0;
                                } else if f.state == Court {
                                    if f.since >= 0.5 && f.exchanges < 5 {
                                        fired = f.fire(self.flash_time, 0.64);
                                        f.exchanges += u8::from(fired);
                                        if self.flies[f.target].answer < 0. {
                                            self.flies[f.target].answer = 0.37;
                                        }
                                    }
                                    if f.timer <= 0. {
                                        let duration = between(&mut self.rng, 15., 25.);
                                        f.state = Paired;
                                        f.timer = duration;
                                        self.flies[f.target].state = Paired;
                                        self.flies[f.target].timer = duration;
                                    }
                                }
                            }
                        }
                    }
                    Paired => {
                        wander = 0.;
                        steering = 0.;
                        may_cycle = false;
                        if f.timer <= 0. {
                            f.state = Rest;
                            f.timer = between(&mut self.rng, 30., 90.);
                        }
                    }
                    Rest => {
                        wander = 0.;
                        steering = 0.;
                        may_cycle = false;
                        if f.timer <= 0. {
                            f.state = if f.female { Perched } else { Patrol };
                            f.target = NOBODY;
                            f.vy = -12. * size;
                        }
                    }
                    Fading => {
                        may_cycle = false;
                        steering = 0.;
                        wander = 0.;
                        if f.timer <= 0. {
                            f.state = Lost;
                            f.timer = 20.;
                        } else if f.since >= 0.5
                            && let Some(&pulse) = [0.35, 0.22, 0.12].get(f.exchanges as usize)
                        {
                            fired = f.fire(self.flash_time, pulse);
                            f.exchanges += u8::from(fired);
                        }
                    }
                    Lost => {
                        if f.timer <= 0. {
                            f = Fly::spawn(
                                i,
                                Dipper,
                                &self.glades,
                                &self.meadow,
                                &mut self.rng,
                                self.cadence,
                            );
                            f.last_flash = self.flash_time - f64::from(f.since);
                            f.y = self.meadow.column(f.x).floor - 3.;
                            self.relocated.push(i);
                            f.light = 0.;
                        }
                        may_cycle = false;
                    }
                    _ => {}
                }
                if f.state == Patrol && !f.female && self.rng.unit() < dt as f64 / 600. {
                    f.hover = between(&mut self.rng, 1., 3.);
                }
                if f.hover > 0. {
                    wander *= 0.03;
                    steering *= 0.03;
                }
                if i % 12 == self.iteration as usize % 12 {
                    // Stagger the small personal-space query; hold its force between samples.
                    f.separating = [0.; 2];
                    self.grid.visit(f.x, f.y, 28. * size, |j| {
                        let q = self.flies[j];
                        let dx = f.x - q.x;
                        let dy = f.y - q.y;
                        let d2 = dx * dx + dy * dy;
                        if i != j
                            && d2 > 0.05
                            && d2 < (28. * size).powi(2)
                            && (f.z - q.z).abs() < 0.18
                        {
                            let k = 18. * size / (1. + d2.sqrt());
                            f.separating[0] += dx * k;
                            f.separating[1] += dy * k;
                        }
                    });
                }
                let wind = self.wind[self.meadow.index(f.x) / 4];
                f.turn += (-0.9 * f.turn) * dt + between(&mut self.rng, -1., 1.) * self.noise_step;
                f.heading += f.turn * dt;
                let (sin, cos) = f.heading.sin_cos();
                let noise_x = speed * (cos + 0.38 * wind[0]) * wander;
                let noise_y = speed * (0.6 * sin + 0.25 * wind[1]) * wander;
                let gust = self.weather.gust(f.x);
                want_y += (c.floor - c.height * (0.047 + 0.036 * f.activity) - want_y) * shelter;
                let (want_x, want_y) = self.meadow.safe_target(want_x, want_y);
                let mut dx = (want_x - f.x) * steering
                    + noise_x
                    + f.separating[0] * wander
                    + c.escape * 105. * size;
                let mut dy = (want_y - f.y) * steering + noise_y + f.separating[1] * wander;
                if f.hover > 0. {
                    dy += 4. * (self.clock * 2. + f.jitter).sin();
                }
                if matches!(f.state, Paired | Rest | Fading | Lost) {
                    dx = 0.;
                    dy = 0.;
                }
                dx = dx.clamp(-speed * 2. - 140. * gust, speed * 2. + 140. * gust);
                dy = dy.clamp(-speed * 2. - 25., speed * 2. + 25.);
                let inertia = (dt / (0.6 + dt)).min(1.);
                f.vx += (dx - f.vx) * inertia;
                f.vy += (dy - f.vy) * inertia;
                // Integrate the two legs directly. The ordinary steering filter
                // would average away the reversal at the bottom of the hook.
                if f.species == Dipper
                    && !f.female
                    && f.state == Patrol
                    && active
                    && f.since <= f.swoop.end() + dt
                    && shelter < 0.6
                {
                    f.vy = (f.swoop.offset(f.since) - f.swoop.offset(f.since - dt)) * size / dt;
                    let sign = if f.vx.abs() > 0.5 {
                        f.vx.signum()
                    } else {
                        cos.signum()
                    };
                    f.vx = sign * (f.swoop.across * size).max(0.9 * speed);
                }
                let gust_target = if matches!(f.state, Patrol | Lured | Approach) {
                    145. * gust * size
                } else {
                    0.
                };
                f.gust_speed += (gust_target - f.gust_speed) * (dt / 0.25).min(1.);
            }
            f.x = (f.x + (f.vx + f.gust_speed) * dt).clamp(left, right);
            if f.state == Court && f.target < self.flies.len() {
                let partner = self.flies[f.target];
                let t = (1. - f.timer / COURT_SECONDS).clamp(0., 1.);
                let radius = (40. - 34. * smooth(t)) * size;
                let angle = TAU * 2. * t;
                let x = partner.x + radius * angle.cos() + 2. * size * smooth(t);
                let y = partner.y + 0.45 * radius * angle.sin() - COURT_HEIGHT * size * (1. - t);
                // Ease onto the spiral, then follow its tightening arc exactly.
                let catch = smooth(t / 0.12);
                f.x += (x - f.x) * catch;
                f.y += (y - f.y) * catch;
                f.vx = 0.;
                f.vy = 0.;
            }
            let band = self.meadow.column(f.x);
            let clearance = if f.species == Ghost { 0.03 } else { 0.002 };
            let floor = band.flight_floor - clearance * band.height;
            if f.vy > 0. {
                f.vy *= smooth((floor - f.y) / (24. * size));
            }
            f.y = (f.y + f.vy * dt).clamp(
                band.floor - band.height,
                floor.max(band.floor - band.height),
            );
            may_cycle &= !matches!(f.state, Court | Fading | Lost | Paired | Rest | Lured);
            if f.phase >= 1. {
                f.phase = f.phase.fract();
                if may_cycle && active && shelter < 0.6 && f.fire(self.flash_time, 1.) {
                    fired = true;
                    if f.species == Syncer {
                        f.remaining = f.burst - 1;
                    }
                }
            }
            if f.remaining > 0
                && f.since + 1e-5 >= self.flash_interval
                && may_cycle
                && active
                && shelter < 0.6
            {
                fired |= f.fire(self.flash_time, 0.93);
                if fired {
                    f.remaining -= 1;
                }
            }
            let mut light = match f.state {
                Lost | Rest => 0.,
                Paired => 0.23,
                Court => {
                    0.03 + 0.94 * smooth(f.since / 0.055) * (1. - smooth((f.since - 0.1) / 0.09))
                }
                Fading => {
                    f.pulse * smooth(f.since / 0.033) * (1. - smooth((f.since - 0.076) / 0.04))
                }
                _ => {
                    f.envelope(wet_attack, afterglow, self.cadence)
                        * (if active { 1. } else { 0.08 })
                }
            };
            if f.tree.is_some() && tree_ready {
                light = 0.19 + 0.81 * f.tree_gain * f32::from(f.tree_pulse) * crown_pulse(f.since);
            }
            if f.meal > 0. {
                light += 0.15 * smooth(f.meal / 1.);
            }
            light += (0.08 - light) * shelter;
            // A short continuous chemical response also eases changes in activity or state.
            f.light += (light - f.light)
                * (dt
                    * if matches!(f.state, Court | Fading) {
                        30.
                    } else if f.species == Syncer && f.tree.is_none() {
                        22. * self.cadence
                    } else {
                        22.
                    })
                .min(1.);
            if fired {
                self.signals.push(i);
            }
            self.flies[i] = f;
        }
        self.tree_ready = self.weather.kind == TREE
            && ((invited > 0 && landed * 5 >= invited * 3)
                || (invited == 0 && self.weather.age > 19.));
        self.communicate();
    }
    fn communicate(&mut self) {
        for &i in &self.signals {
            let source = self.flies[i];
            if matches!(source.state, Lost | Fading | Paired | Rest) || source.tree.is_some() {
                continue;
            }
            let size = (self.meadow.column(source.x).height / 1440.).clamp(0.15, 3.);
            let radius = 420. * size;
            self.grid.visit(source.x, source.y, radius, |j| {
                if i == j {
                    return;
                }
                let f = &mut self.flies[j];
                let dx = source.x - f.x;
                let dy = source.y - f.y;
                let d2 = dx * dx + dy * dy;
                if d2 > radius * radius || (source.z - f.z).abs() > 0.35 {
                    return;
                }
                if source.species == Syncer
                    && !source.female
                    && f.species == Syncer
                    && !f.female
                    && f.state == Patrol
                    && source.state == Patrol
                    && f.tree.is_none()
                    && self.sync_gain > 0.
                    && d2 < (260. * size).powi(2)
                    && f.since >= 0.5
                {
                    // Estimate phase from this burst's observed flash number. The
                    // eastward transit delay gives a local front a finite speed.
                    let beat = (source.burst.saturating_sub(1 + source.remaining)) as f32;
                    let observed =
                        (beat * self.flash_interval + dx / (740. * size) + source.sync_offset)
                            / source.period;
                    let correction = (observed - f.heard_phase + 0.5).rem_euclid(1.) - 0.5;
                    let learn = if f.heard_age > 12. { 1. } else { 0.4 };
                    f.heard_phase = (f.heard_phase + learn * correction).rem_euclid(1.);
                    f.heard_period += (source.period - f.heard_period) * learn;
                    f.heard_age = 0.;
                    #[cfg(test)]
                    self.coupling_edges.push((i, j));
                }
                if !source.female
                    && !f.female
                    && d2 < (260. * size).powi(2)
                    && f.since > self.flash_interval
                    && f.state == Patrol
                    && f.remaining == 0
                {
                    let weight = match (source.species, f.species) {
                        (Syncer, Syncer) => 0.,
                        (Dipper, Dipper) => 0.35,
                        _ => 0.,
                    };
                    #[cfg(test)]
                    if weight > 0. && self.sync_gain > 0. {
                        self.coupling_edges.push((i, j));
                    }
                    f.phase += weight * self.sync_gain * (0.18 + 0.82 * f.phase) * 0.21;
                }
                if f.state == Perched
                    && !source.female
                    && source.state == Patrol
                    && f.answer < 0.
                    && f.since > 1.5
                {
                    let mimic = self.params.hunters
                        && f.species == Hunter
                        && source.species == Dipper
                        && self.hunt_cooldown <= 0.;
                    if (source.species == f.species && source.species != Ghost) || mimic {
                        f.answer = (if mimic {
                            1.65
                        } else if f.species == Syncer {
                            0.76
                        } else {
                            2.
                        }) + between(&mut self.rng, -0.14, 0.14);
                        f.target = i;
                    }
                } else if source.female
                    && !f.female
                    && f.state == Patrol
                    && (f.vx * dx + f.vy * dy >= 0. || source.target == j)
                {
                    let interest = if source.target == j { 0.028 } else { 0.002 };
                    if source.species == f.species && self.rng.unit() < interest {
                        f.target = i;
                        f.state = Approach;
                    } else if source.species == Hunter
                        && f.species == Dipper
                        && source.target == j
                        && self.params.hunters
                        && self.hunt_cooldown <= 0.
                    {
                        f.target = i;
                        f.state = Lured;
                        f.timer = (d2.sqrt() / (70. * size)).clamp(2.5, 8.);
                        self.hunt_cooldown = between(&mut self.rng, 180., 360.);
                    }
                }
            });
        }
    }
    pub(super) fn update_wind(&mut self, dt: f32) {
        self.wind_phase += dt * 0.077;
        for (i, wind) in self.wind.iter_mut().enumerate() {
            let u = i as f32 * 0.28;
            // Curl of a sum of gently advected stream functions.
            *wind = [
                (u + self.wind_phase).cos() + 0.45 * (1.71 * u - self.wind_phase * 0.7).cos(),
                -(0.6 * u - self.wind_phase * 0.65).sin()
                    + 0.32 * (2.3 * u + self.wind_phase).cos(),
            ];
        }
        for g in &mut self.glades {
            g.x += g.drift * dt;
            let margin = (g.width * 0.6).min(self.meadow.bounds.width as f32 * 0.2);
            if g.x < (self.meadow.bounds.x as f32 + margin).max(g.basin - 0.70 * g.width) {
                g.drift = g.drift.abs();
            }
            if g.x > (self.meadow.bounds.right() as f32 - margin).min(g.basin + 0.70 * g.width) {
                g.drift = -g.drift.abs();
            }
        }
    }
    pub(super) fn tree_oscillator(&mut self, dt: f32) {
        self.tree_phase += dt;
        if self.tree_phase >= 4.5 {
            self.tree_phase -= 4.5;
            self.tree_pulses = 4;
        }
        self.tree_since += dt;
        if self.tree_pulses > 0 && self.tree_since >= 0.66 {
            self.tree_pulses -= 1;
            self.tree_since = 0.;
        }
    }
    pub(super) fn night_activity(&self) -> f32 {
        if !self.params.nightCycle {
            return 1.;
        }
        let t = self.night_age / self.night_length;
        if t < 0.12 {
            0.5 + 0.5 * smooth(t / 0.12)
        } else if t < 0.55 {
            1.
        } else if t < 0.85 {
            1. - 0.55 * smooth((t - 0.55) / 0.30)
        } else {
            0.45 - 0.15 * smooth((t - 0.85) / 0.15)
        }
    }
    pub(super) fn step_distant(&mut self, dt: f32) {
        for f in &mut self.distant {
            if f.near {
                let mut area = self.meadow.outputs[f.home % self.meadow.outputs.len()];
                f.age += dt;
                if f.age >= f.period {
                    if self.meadow.outputs.len() > self.near_count {
                        f.home = self.rng.next_u32() as usize % self.meadow.outputs.len();
                        area = self.meadow.outputs[f.home];
                    }
                    f.rebirth(area, &mut self.rng);
                    f.age = -between(&mut self.rng, 8., 25.);
                }
                if f.age >= 0. {
                    let size = area.height as f32 / 1440.;
                    f.vy += (3. * (0.14 * f.age + f.phase).sin() * size - f.vy) * dt;
                    f.x += (f.vx + 65. * size * self.weather.gust(f.x)) * dt;
                    f.y += f.vy * dt;
                    if self.iteration.is_multiple_of(9)
                        && let Some(lamp) =
                            self.flies.iter().filter(|q| q.light > 0.18).min_by(|a, b| {
                                let d = |q: &Fly| (q.x - f.x).powi(2) + (q.y - f.y).powi(2);
                                d(a).total_cmp(&d(b))
                            })
                    {
                        f.tint = lamp.species as usize;
                    }
                }
                f.light = smooth(f.age / 2.5)
                    * smooth((f.period - f.age) / 2.5)
                    * (1. - 0.75 * self.weather.rain());
                continue;
            }
            let c = self.meadow.column(f.x);
            let home = self.glades[f.home];
            let mut tx = home.x + f.roam;
            let mut ty = c.floor - c.height * f.altitude;
            let mut steer = 0.07;
            if let Some(p) = f.tree {
                tx = p[0];
                ty = p[1];
                steer = 0.34;
            }
            let (tx, ty) = self.meadow.safe_target(tx, ty);
            let wind = self.wind[self.meadow.index(f.x) / 4];
            let freedom = if f.tree.is_some() { 0. } else { 1. };
            f.vx += ((tx - f.x) * steer + freedom * (wind[0] * 6. + c.escape * 30.) - f.vx)
                * dt.min(1.);
            f.vy += ((ty - f.y) * steer + freedom * wind[1] * 5. - f.vy) * dt.min(1.);
            f.x = (f.x + (f.vx + freedom * 130. * self.weather.gust(f.x) * c.height / 1440.) * dt)
                .clamp(
                    self.meadow.bounds.x as f32,
                    self.meadow.bounds.right() as f32,
                );
            f.y += f.vy * dt;
            f.since += dt;
            f.phase += dt / f.period;
            let gathered = f.tree.is_some() && self.tree_ready;
            let onset = if gathered {
                self.tree_since >= f.tree_delay && self.tree_since < f.tree_delay + dt
            } else {
                f.phase >= 1.
            };
            f.phase = f.phase.fract();
            if onset {
                super::fly::commit_flash(self.flash_time, &mut f.last_flash, &mut f.since);
            }
            f.light = if gathered {
                0.19 + 0.81 * f.tree_gain * crown_pulse(f.since)
            } else {
                0.35 * smooth(f.since / 0.10) * (1. - smooth((f.since - 0.16) / 0.24))
            };
            f.light *= 1. - 0.75 * self.weather.rain();
        }
    }
}

impl super::Distant {
    pub(super) fn rebirth(
        &mut self,
        area: super::super::common::layout::Rect,
        rng: &mut super::Pcg32,
    ) {
        let margin = 200_f32.min(area.width as f32 * 0.45);
        self.x = between(rng, area.x as f32 + margin, area.right() as f32 - margin);
        self.y = area.bottom() as f32 - between(rng, 0.10, 0.46) * area.height as f32;
        let sign = if rng.unit() < 0.5 { -1. } else { 1. };
        self.vx = sign * between(rng, 6., 12.) * area.height as f32 / 1440.;
        self.vy = 0.;
        self.phase = between(rng, 0., TAU);
        self.period = between(rng, 20., 60.);
        self.light = 0.;
    }
}
