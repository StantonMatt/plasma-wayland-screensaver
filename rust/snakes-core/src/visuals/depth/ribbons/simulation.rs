use super::{
    Params,
    agent::*,
    common::{
        flow::{FlowField, FlowSettings, p},
        layout::{Rect, Vec2},
        rng::Pcg32,
    },
    water::{Kind, Lanes, Neighbours, Weather},
    world::{MonitorMode, WorldInput},
};

#[derive(Clone, Copy)]
pub(super) struct Mote {
    pub pos: Vec2,
    vel: Vec2,
    scatter: Vec2,
    pub age: f64,
    pub life: f64,
    pub radius: f64,
    pub hue: usize,
    pub id: u32,
}
impl Mote {
    pub fn twinkle(&self) -> f64 {
        let hash = super::common::rng::hash64(self.id as u64 ^ 0x6d6f_7465);
        let at = 0.6 + (self.life - 1.2) * (hash >> 11) as f64 / (1_u64 << 53) as f64;
        let u = ((self.age - at).abs() / 0.15).clamp(0., 1.);
        1. + 0.30 * (1. - smooth(u))
    }
}
#[derive(Clone, Copy)]
struct Bond {
    a: usize,
    b: usize,
    age: f64,
    life: f64,
    radius: f64,
    frequency: f64,
    axis: Vec2,
}
pub(super) struct Sea {
    pub agents: Vec<Ribbon>,
    pub left: f64,
    pub shared: bool,
    target: usize,
    pub motes: Vec<Mote>,
    pub weather: Weather,
    pub flow: FlowField,
    neighbours: Neighbours,
    lanes: Lanes,
    bonds: Vec<Bond>,
    directions: [Vec2; 40],
    partners: [Option<(Vec2, f64)>; 40],
    gust_carry: [f64; 40],
    gust_course: [f64; 40],
    rng: Pcg32,
    spawn_after: f64,
    knot_wait: Vec<f64>,
    next_id: u32,
    step: u64,
    tide_bias: Vec2,
    still_relaxation: f64,
    pub births: u64,
    pub frays: u64,
    pub dances: u64,
}
impl Sea {
    pub fn new(input: &WorldInput, params: &Params, seed: u64) -> Self {
        let area = input.layout.visible_area / (2560. * 1440.);
        let count = ((2. + 0.12 * input.density) * area).round().clamp(3., 40.) as usize;
        let mut rng = Pcg32::new(seed, 0x51_1c);
        let flow = FlowField::new(
            seed,
            &input.layout,
            FlowSettings::ribbons(params.turbulence, area),
        );
        let weather = Weather::new(&mut rng);
        let mut sea = Self {
            left: input.layout.bounds.x,
            shared: input.layout.outputs.len() > 1,
            agents: Vec::with_capacity((count * 2).min(40)),
            target: count,
            motes: Vec::with_capacity(512),
            weather,
            flow,
            neighbours: Neighbours::new(),
            lanes: Lanes::new(input.layout.bounds),
            bonds: Vec::with_capacity(20),
            directions: [Vec2::default(); 40],
            partners: [None; 40],
            gust_carry: [1.; 40],
            gust_course: [0.; 40],
            rng,
            spawn_after: 8.,
            knot_wait: vec![0.; input.layout.outputs.len()],
            next_id: 0,
            step: 0,
            tide_bias: Vec2::default(),
            still_relaxation: 0.,
            births: 0,
            frays: 0,
            dances: 0,
        };
        let far = (count as f64 * 0.35).round() as usize;
        let near = (count as f64 * 0.15).round() as usize;
        for n in 0..count {
            let layer = if n < far {
                0
            } else if n >= count - near {
                2
            } else {
                1
            };
            let mut r = sea.spawn(input, params, layer, count == 3 || n != far);
            // A staggered first generation includes both new growth and a mature
            // ribbon approaching retirement, even in a short viewing session.
            if n == count - 1 {
                r.age = r.life - range(&mut sea.rng, 18., 40.);
            }
            sea.agents.push(r);
        }
        sea
    }
    fn spawn(&mut self, input: &WorldInput, params: &Params, layer: usize, mature: bool) -> Ribbon {
        let seed = input
            .layout
            .choose_visible_anchor(&mut self.rng, 0.)
            .unwrap_or(input.layout.bounds.centre());
        let home = input
            .layout
            .outputs
            .iter()
            .find(|o| o.rect.contains(seed))
            .map_or(input.layout.bounds, |o| o.rect);
        let drift = self.flow.drift.x.signum()
            * if params.turbulence <= 0.15 || self.next_id.is_multiple_of(2) {
                1.
            } else {
                -1.
            };
        // Seed into the emptiest of several broad water lanes. Count visible
        // silk (including mature tails), so a bright near scarf doesn't crowd
        // the next arrivals out of the other two thirds of the screen.
        let mut load = [0.0; 5];
        for r in self.agents.iter().filter(|r| r.alive) {
            let end = (r.extent() * (r.count - 1) as f64) as usize;
            for n in r.nodes[..=end]
                .iter()
                .step_by(8)
                .filter(|n| home.contains(n.pos))
            {
                let u = (n.pos.y - home.y) / home.height;
                for (j, value) in load.iter_mut().enumerate() {
                    let d = (u - (0.12 + 0.19 * j as f64)).abs();
                    *value += (1. - d / 0.31).max(0.) * r.width.sqrt();
                }
            }
        }
        for v in &mut load {
            *v += self.rng.unit() * 1.4;
        }
        let lane = load
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.total_cmp(b))
            .unwrap()
            .0;
        let x = if self.step == 0 && self.target == 3 {
            // A sparse, slow composition shows three established swimmers at
            // activation. Put enough of each short strand inside its viewport.
            home.x + home.width * 0.28 + 0.44 * (seed.x - home.x)
        } else if self.step == 0 {
            seed.x
        } else {
            // Young growth emerges on the upstream side of its swimming lane.
            let inlet = range(&mut self.rng, -0.025, 0.17);
            home.x + home.width * if drift > 0. { inlet } else { 1. - inlet }
        };
        let head = p(
            x,
            home.y
                + home.height * (0.12 + 0.19 * lane as f64 + range(&mut self.rng, -0.045, 0.045)),
        );
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        Ribbon::new(
            &mut self.rng,
            Spawn {
                id,
                layer,
                home,
                head,
                mature,
                direction: drift,
                population: self.target,
            },
            params.length,
            input.scale,
            params.twist,
        )
    }
    fn goal(rng: &mut Pcg32, home: Rect, pos: Vec2, lane: f64, drift: f64) -> Vec2 {
        p(
            pos.x + drift * range(rng, 490., 980.),
            (lane + range(rng, -0.19, 0.19) * home.height)
                .clamp(home.y + 0.10 * home.height, home.y + 0.90 * home.height),
        )
    }
    pub fn balance(&self, layout: &super::common::layout::Layout) -> f64 {
        let mut worst = 0f64;
        for output in &layout.outputs {
            let rect = output.rect;
            let mut bands = [0.; 6];
            for r in self.agents.iter().filter(|r| r.alive) {
                let end = (r.extent() * (r.count - 1) as f64) as usize;
                for n in r.nodes[..=end]
                    .iter()
                    .step_by(3)
                    .filter(|n| rect.contains(n.pos))
                {
                    let x = ((n.pos.x - rect.x) * 3. / rect.width) as usize;
                    let y = ((n.pos.y - rect.y) * 3. / rect.height) as usize;
                    let weight = r.width * r.spacing * (0.12 + 0.88 * n.phase.cos().abs());
                    bands[x.min(2)] += weight;
                    bands[3 + y.min(2)] += weight;
                }
            }
            let total = bands[..3].iter().sum::<f64>().max(0.001);
            for band in bands {
                worst = worst.max((band / total - 1. / 3.).abs());
            }
        }
        worst
    }
    fn school(&mut self, dt: f64, enabled: bool) {
        self.partners.fill(None);
        self.directions.fill(Vec2::default());
        if !enabled {
            return;
        }
        let mut bonded = [false; 40];
        self.bonds.retain_mut(|bond| {
            let (a, b) = (&self.agents[bond.a], &self.agents[bond.b]);
            bond.age += dt;
            if !a.alive
                || a.leaving
                || b.leaving
                || !b.alive
                || a.life - a.age < 8.
                || b.life - b.age < 8.
                || bond.age > bond.life
                || self.flow.sample(a.head()).vorticity.abs() > 2.6
            {
                self.directions[bond.a] += bond.axis.perpendicular() * 50.;
                self.directions[bond.b] += bond.axis.perpendicular() * -50.;
                return false;
            }
            let mid = (a.head() + b.head()) * 0.5;
            let heading = (a.heading + b.heading).unit();
            bond.axis = (bond.axis + heading * dt).unit();
            let phase = bond.age * bond.frequency;
            let side = bond.axis.perpendicular();
            let rise = smooth(bond.age / 3.) * smooth((bond.life - bond.age) / 2.);
            for (i, sign) in [(bond.a, 1.), (bond.b, -1.)] {
                let rest = mid + side * (sign * bond.radius * phase.sin());
                self.partners[i] = Some((
                    (bond.axis * 95.
                        + (rest - self.agents[i].head()) * 1.8
                        + side * (sign * bond.radius * bond.frequency * phase.cos()))
                        * rise,
                    phase
                        + if sign < 0. {
                            std::f64::consts::FRAC_PI_2
                        } else {
                            0.
                        },
                ));
                bonded[i] = true;
            }
            true
        });
        for i in 0..self.agents.len() {
            for j in i + 1..self.agents.len() {
                let (a, b) = (&self.agents[i], &self.agents[j]);
                if !a.alive || !b.alive || a.leaving || b.leaving {
                    continue;
                }
                let d = a.head() - b.head();
                if d.dot(d) > 250. * 250. || a.heading.dot(b.heading) < 0.766 {
                    continue;
                }
                self.directions[i] += b.heading * 20.;
                self.directions[j] += a.heading * 20.;
                if d.dot(d) < 180. * 180.
                    && a.heading.dot(b.heading) > 0.819
                    && a.age > 30.
                    && b.age > 30.
                    && !bonded[i]
                    && !bonded[j]
                    && a.life - a.age > 20.
                    && b.life - b.age > 20.
                    && self.rng.unit() < dt * 0.65
                {
                    self.bonds.push(Bond {
                        a: i,
                        b: j,
                        age: 0.,
                        life: range(&mut self.rng, 10., 30.),
                        radius: (a.width + b.width) * range(&mut self.rng, 0.80, 1.25),
                        frequency: range(&mut self.rng, 1.65, 2.),
                        axis: (a.heading + b.heading).unit(),
                    });
                    self.agents[i].pulse();
                    self.agents[j].pulse();
                    bonded[i] = true;
                    bonded[j] = true;
                    self.dances += 1;
                }
            }
        }
    }
    #[cfg(test)]
    pub fn partnered(&self, index: usize) -> bool {
        self.partners[index].is_some()
    }

    #[cfg(test)]
    pub fn gust_banking(&self, index: usize) -> bool {
        self.gust_carry[index] < 1.
    }

    #[cfg(test)]
    pub fn gust_saved_course(&self, index: usize) -> Option<f64> {
        self.gust_banking(index).then_some(self.gust_course[index])
    }

    #[cfg(test)]
    pub fn braid_midpoints(&self) -> impl Iterator<Item = (Vec2, Vec2)> + '_ {
        self.bonds
            .iter()
            .filter(|b| (b.age - b.life * 0.5).abs() < 1. / 60.)
            .map(|b| (self.agents[b.a].head(), self.agents[b.b].head()))
    }
    pub fn advance(
        &mut self,
        input: &WorldInput,
        params: &Params,
        dt: f64,
        pulse_dt: f64,
        ns: i64,
    ) {
        self.step += 1;
        let layout = &input.layout;
        let independent = input.mode == MonitorMode::Independent;
        self.flow.advance(layout, dt);
        let start_event = self.weather.active.is_none()
            || self.weather.script.first().is_some_and(|e| e.at_ns <= ns);
        // Ambient events have their own clock. At Speed 300 the current still
        // advances threefold, while event pacing tops out at one and a half.
        let event_pace = input.speed.min(1.5);
        let event_dt = dt * event_pace / input.speed.max(0.01);
        let ended = self.weather.step(
            layout,
            &mut self.rng,
            event_dt,
            event_pace,
            ns,
            params.world_events,
        );
        if ended.is_some_and(|m| m.kind == Kind::Still) {
            self.tide_bias = polar(range(&mut self.rng, 0., std::f64::consts::TAU)) * 75.;
        }
        if self.weather.active.is_some_and(|m| m.kind == Kind::Still) {
            self.still_relaxation = 16.;
        } else {
            self.still_relaxation = (self.still_relaxation - event_dt).max(0.);
        }
        if start_event
            && let Some(event) = self.weather.active.filter(|m| m.kind == Kind::Maelstrom)
        {
            let panel = layout
                .outputs
                .iter()
                .find(|o| o.rect.contains(event.centre))
                .map_or(layout.bounds, |o| o.rect);
            let mut candidates = [(f64::INFINITY, 0usize); 40];
            let mut n = 0;
            for (i, r) in self.agents.iter_mut().enumerate() {
                r.caught = false;
                if r.alive && r.release_age == 0. && panel.contains(r.head()) {
                    candidates[n] = ((r.head() - event.centre).length(), i);
                    n += 1;
                }
            }
            for (i, r) in self.agents.iter().enumerate() {
                if r.alive && r.release_age == 0. && !panel.contains(r.head()) {
                    candidates[n] = ((r.head() - event.centre).length(), i);
                    n += 1;
                }
            }
            let visible_count = candidates[..n]
                .iter()
                .filter(|(_, i)| panel.contains(self.agents[*i].head()))
                .count();
            let mut free = visible_count;
            let target = (n * 3 / 5).clamp(3, 6);
            let mut caught = 0;
            // Reserve a representative of each depth first. Favor a nearby
            // off-panel head when that leaves room for a visible missing layer.
            for layer in [Some(0), Some(1), Some(2), None] {
                while caught < target {
                    let selected = candidates[..n]
                        .iter()
                        .filter(|(_, i)| {
                            let r = &self.agents[*i];
                            !r.caught
                                && layer.is_none_or(|l| l == r.layer)
                                && (free > 2 || !panel.contains(r.head()))
                        })
                        .min_by(|(ad, ai), (bd, bi)| {
                            let cost = |d: f64, i: usize| {
                                let r = &self.agents[i];
                                d + if panel.contains(r.head()) && free <= 4 {
                                    1600.
                                } else {
                                    0.
                                } + (35. - (r.life - r.age)).max(0.) * 100.
                            };
                            cost(*ad, *ai).total_cmp(&cost(*bd, *bi))
                        });
                    let Some(&(_, i)) = selected else { break };
                    let r = &mut self.agents[i];
                    free -= usize::from(panel.contains(r.head()));
                    r.caught = true;
                    caught += 1;
                    if layer.is_some() {
                        break;
                    }
                }
            }
        }
        if let Some(m) = self.weather.active.filter(|m| m.collapsing()) {
            for r in self.agents.iter_mut().filter(|r| r.caught) {
                r.release_orbit(m.centre);
                // Keep the release visible even when all three pulse slots are occupied.
                let slot = r.pulses.iter().position(|d| *d < 0.).unwrap_or_else(|| {
                    r.pulses
                        .iter()
                        .enumerate()
                        .max_by(|a, b| a.1.total_cmp(b.1))
                        .unwrap()
                        .0
                });
                r.pulses[slot] = 0.;
            }
        }
        self.tide_bias = self.tide_bias * (1. - dt / 65.);
        if self.step % 3 == 1 {
            self.neighbours.rebuild(&self.agents);
        }
        if self.step % 8 == 1 {
            self.lanes.rebuild(&self.agents, layout);
        }
        self.school(dt, params.dance);
        self.spawn_after -= event_dt;
        let wall_dt = dt / input.speed.max(0.01);
        for wait in &mut self.knot_wait {
            *wait -= wall_dt;
        }
        let gust_event = self.weather.active.filter(|m| m.kind == Kind::Gust);
        if start_event && gust_event.is_some() {
            for (i, r) in self.agents.iter_mut().enumerate() {
                if self.gust_carry[i] < 1. {
                    r.course_sign = self.gust_course[i];
                    if independent {
                        r.leaving = false;
                    }
                }
            }
            self.gust_carry.fill(1.);
        }
        // Smoothly shoulder the force at 450 world pixels per wall second.
        // Leave the separate 500 motor allowance and geometric turn bounds intact.
        let gust_force = if gust_event.is_some() {
            let wind = 395. * input.speed;
            450. * (wind / 450.).tanh() / wind.max(0.01)
        } else {
            1.
        };
        let mut jumped = [false; 40];
        for (i, did_jump) in jumped.iter_mut().enumerate().take(self.agents.len()) {
            if !self.agents[i].alive {
                continue;
            }
            let r = &mut self.agents[i];
            let pos = r.head();
            if gust_event.is_some() {
                let right = layout.bounds.x + layout.bounds.width;
                if self.gust_carry[i] < 1. || pos.x > right - 1200. {
                    if self.gust_carry[i] == 1. {
                        self.gust_course[i] = r.course_sign;
                    }
                    // Bank before the shore. Stay sheltered for this event so
                    // the returning swimmer is not blown straight back out.
                    self.gust_carry[i] = (self.gust_carry[i] - wall_dt / 0.8).max(0.);
                    r.course_sign = -1.;
                    let goal_x = if independent {
                        // Leave turning room at both shores of a small output.
                        let margin = (layout.bounds.width * 0.5).min(600.);
                        (right - 2400.).clamp(layout.bounds.x + margin, right - margin)
                    } else {
                        right - 2400.
                    };
                    r.goal = p(goal_x, r.lane);
                    r.course_for = 4.;
                }
            } else {
                if self.gust_carry[i] < 1. {
                    // Banking is temporary. Calm flow resumes its tide-facing
                    // heading, so restore the matching departure direction too;
                    // otherwise offscreen heads keep occupying population slots.
                    r.course_sign = self.gust_course[i];
                    if independent {
                        r.leaving = false;
                    }
                    r.goal = p(pos.x + 800. * r.course_sign, r.lane);
                    r.course_for = 4.;
                }
                self.gust_carry[i] = 1.;
            }
            let gust_drive = gust_event.map(|front| GustDrive {
                front,
                left: layout.bounds.x,
                right: layout.bounds.x + layout.bounds.width,
                carry: smooth(self.gust_carry[i]),
                force: gust_force,
                world_rate: input.speed,
            });
            let water = self.flow.sample(pos);
            r.knot_for = (r.knot_for - wall_dt).max(0.);
            if r.knot_for > 0. && !layout.outputs[r.knot_panel].rect.contains(pos) {
                r.knot_for = 0.;
            }
            if (self.step + i as u64).is_multiple_of(4) {
                r.avoid = Vec2::default();
                if r.knot_for <= 0.
                    && !r.caught
                    && let Some(escape) = r.crossing_ahead()
                {
                    let panel = layout.outputs.iter().position(|o| o.rect.contains(pos));
                    if let Some(j) =
                        panel.filter(|&j| self.knot_wait[j] <= 0. && params.turbulence > 0.15)
                    {
                        self.knot_wait[j] = range(&mut self.rng, 120., 240.);
                        r.knot_panel = j;
                        r.knot_for = 3.;
                        r.pulse();
                    } else {
                        r.avoid = escape * (1.25 * r.speed);
                    }
                }
            }
            let boundary = layout.signed_distance_and_normal(pos);
            let margin = 200f64.min(r.home.height * 0.23);
            // A narrow bezel is passable water. Vertical voids above a short
            // panel, and the outer edges, still exert an inward steering force.
            let outer = pos.x < layout.bounds.x + margin
                || pos.x > layout.bounds.x + layout.bounds.width - margin;
            let passable = boundary.normal.x.abs() > 0.7
                && (outer
                    || layout.outputs.len() > 1
                    || layout.contains(pos + boundary.normal * (200. + boundary.distance.abs())));
            let wall = if passable {
                Vec2::default()
            } else {
                -boundary.normal * (smooth(1. + boundary.distance / margin) * 150.)
            };
            r.course_for -= dt;
            if !(independent && self.gust_carry[i] < 1.)
                && (r.course_for <= 0. || (r.goal - pos).length() < 70.)
            {
                r.goal = Self::goal(&mut self.rng, r.home, pos, r.lane, r.course_sign);
                r.course_for = range(&mut self.rng, 4., 10.);
            }
            let (spacing, crossing) = self.neighbours.steer(i, pos);
            let forward = (r.goal - pos).unit();
            let drift = self.flow.drift;
            let local = (water.velocity - drift).limited(170.);
            r.eddy_age = if water.vorticity.abs() > 0.45 {
                r.eddy_age + dt
            } else {
                (r.eddy_age - dt * 2.).max(0.)
            };
            let escape = smooth((r.eddy_age - 18.) / 4.);
            let lane_steer = ((r.lane - pos.y) * 0.22).clamp(-90., 90.);
            let mut desired = drift * 0.38
                + local * (0.26 + params.turbulence * 0.38) * (1. - 0.8 * escape)
                + self.tide_bias
                + forward * (86. + 30. * r.temperament)
                + r.heading * 12.
                + p(0., lane_steer)
                + self.lanes.at(pos)
                + wall
                + spacing
                + r.avoid
                + self.directions[i].limited(28.);
            let mut curl = water.vorticity;
            let mut gust = 0.;
            let mut launch = if r.release_age > 0. {
                smooth(r.release_age / 6.)
            } else {
                0.
            };
            let mut still = 0.;
            let braid = self.partners[i];
            if let Some((drive, _)) = braid {
                desired = desired * 0.32 + drive + wall * 0.5;
            }
            if let Some(m) = self.weather.active {
                match m.kind {
                    Kind::Gust => {
                        gust = gust_drive.unwrap().weight(pos);
                        // A gust carries both up- and down-current swimmers. Its
                        // translation is applied to the whole scarf in swim().
                        desired = desired * (1. - gust) + p(r.course_sign * 90., 0.) * gust;
                        // Turn the existing swimming drive downwind without
                        // spending any extra translation or changing its length.
                        let angle = turn(desired, p(1., 0.)) * m.gust_steering() * gust;
                        let (sn, cs) = angle.sin_cos();
                        desired = desired * cs + desired.perpendicular() * sn;
                    }
                    Kind::Maelstrom if r.caught => {
                        let (flow, spin) = m.swirl(pos, r.layer);
                        desired = desired * (1. - m.weight()) + flow;
                        curl += spin;
                        launch = m.release_weight();
                        if launch > 0. {
                            r.prepare_release(m.centre);
                        }
                    }
                    Kind::Maelstrom => {}
                    Kind::Still => {
                        still = m.weight();
                        let resting = r.heading * 85. + drift * 0.08;
                        desired = desired * (1. - still) + resting * still;
                    }
                }
            }
            if gust_event.is_some() && self.gust_carry[i] < 1. {
                // The bank must also overcome schooling/crossing avoidance;
                // otherwise a crowded group can keep steering out to sea even
                // after the wind has faded. Use the existing turn/radius budget.
                let bank = 1. - smooth(self.gust_carry[i]);
                // A sheltered Independent swimmer turns back at its interior
                // goal instead of continuing west through the opposite shore.
                let across = if independent {
                    (r.goal.x - pos.x).clamp(-180., 180.)
                } else {
                    -180.
                };
                desired = desired * (1. - bank) + p(across, lane_steer + wall.y) * bank;
            }
            let orbital_phase = self
                .weather
                .active
                .filter(|m| m.kind == Kind::Maelstrom && r.caught)
                .map(|m| {
                    let d = pos - m.centre;
                    d.y.atan2(d.x) + r.layer as f64 * std::f64::consts::TAU / 3.
                });
            if launch > 0. {
                desired = desired * (1. - 0.75 * launch);
            }
            let steering = Steering {
                desired,
                curl,
                gust,
                launch,
                gust_front: gust_drive,
                turbulence: params.turbulence,
                still,
                smooth_twist: self.still_relaxation > 0.,
                spanning: layout.outputs.len() > 1,
                braid_phase: orbital_phase.or(braid.map(|(_, phase)| phase)),
                orbit: self
                    .weather
                    .active
                    .filter(|m| m.kind == Kind::Maelstrom && r.caught),
            };
            r.swim(
                steering,
                &self.flow,
                &self.lanes,
                params.twist,
                dt,
                (self.step + i as u64).is_multiple_of(6) || dt > 0.2,
            );
            r.release_age = (r.release_age - event_dt).max(0.);
            // A captured scarf finishes its release before starting a fray.
            // Pause the lifecycle clock if it was already fraying at capture.
            if (!r.caught && r.release_age == 0.) || r.life - r.age > 32. {
                r.age += event_dt;
            }
            r.pulse_wait -= dt;
            r.crossing_wait -= dt;
            if r.pulse_wait <= 0. || crossing && r.crossing_wait <= 0. {
                r.pulse();
                r.pulse_wait = range(&mut self.rng, 20., 40.);
                r.crossing_wait = 6.;
                *did_jump = true;
            }
            for pulse in &mut r.pulses {
                if *pulse >= 0. {
                    *pulse += 400. * pulse_dt;
                    if *pulse > r.spacing * r.count as f64 + 90. {
                        *pulse = -1000.;
                    }
                }
            }
            let boundary_x = if r.course_sign > 0. {
                layout.bounds.x + layout.bounds.width
            } else {
                layout.bounds.x
            };
            if (pos.x - boundary_x) * r.course_sign > 35. {
                r.leaving = true;
            }
            let mid = r.nodes[(r.count - 1) / 2].pos.x;
            if (r.course_sign > 0. && mid > layout.bounds.x + layout.bounds.width
                || r.course_sign < 0. && mid < layout.bounds.x)
                && r.age > 15.
                && !r.caught
                && r.release_age <= 0.
            {
                r.retire_downstream();
            }
            let fray = smooth((r.age - r.life + 9.) / 9.);
            if fray > 0. && r.shed == 0 {
                self.frays += 1;
            }
            while fray > r.shed_at && r.shed < r.shed_limit && self.motes.len() < 472 {
                let knot = (r.count - 1) as f64 * r.extent();
                let index = (knot as usize).min(r.count - 2);
                let tangent = (r.nodes[index + 1].pos - r.nodes[index].pos).unit();
                let side = tangent.perpendicular();
                let sign = if r.shed.is_multiple_of(2) { -1. } else { 1. };
                // Reuse the original two random draws: scattering sparks must
                // not perturb the seeded births, schools or weather schedule.
                let angle = range(&mut self.rng, 0., std::f64::consts::TAU);
                let kick = range(&mut self.rng, 20., 40.);
                let pos = r.nodes[index].pos
                    + (r.nodes[index + 1].pos - r.nodes[index].pos) * (knot - index as f64)
                    + side * (sign * r.width * (0.75 + 0.25 * angle.sin()));
                let scatter = (side * sign + tangent * (0.45 * angle.cos())).unit() * kick;
                let vel = self.flow.sample(pos).velocity * 0.33;
                self.motes.push(Mote {
                    pos,
                    vel,
                    scatter,
                    age: 0.,
                    life: range(&mut self.rng, 4., 6.),
                    radius: range(&mut self.rng, 2.1, 3.2),
                    hue: r.hue,
                    id: r.id.wrapping_mul(64).wrapping_add(r.shed as u32),
                });
                r.shed += 1;
                r.shed_at += range(&mut self.rng, 0.4, 1.) / (0.7 * r.shed_limit as f64);
            }
            let downstream = r.course_sign;
            let exit = if downstream > 0. {
                layout.bounds.x + layout.bounds.width + 90.
            } else {
                layout.bounds.x - 90.
            };
            let gone = (pos.x - exit) * downstream > 0.
                && r.nodes[..r.count]
                    .iter()
                    .step_by(4)
                    .all(|n| (n.pos.x - exit) * downstream > 0.);
            if r.age >= r.life || gone && !r.caught && r.release_age == 0. {
                r.alive = false;
            }
        }
        for b in &self.bonds {
            if jumped[b.a] {
                self.agents[b.b].pulse();
            }
            if jumped[b.b] {
                self.agents[b.a].pulse();
            }
        }
        // The head can leave downstream while its tail is still on screen.
        // An established swimmer enters upstream as that head departs. Their
        // lives are independent and their total storage is bounded at forty.
        let incoming = self.agents.iter().filter(|r| r.alive && !r.leaving).count();
        if incoming < self.target {
            let vacancy = self.agents.iter().position(|r| !r.alive);
            if vacancy.is_some() || self.agents.len() < self.agents.capacity() {
                let far = self
                    .agents
                    .iter()
                    .filter(|r| r.alive && !r.leaving && r.layer == 0)
                    .count();
                let near = self
                    .agents
                    .iter()
                    .filter(|r| r.alive && !r.leaving && r.layer == 2)
                    .count();
                let layer = if far < (self.target as f64 * 0.35).round() as usize {
                    0
                } else if near < (self.target as f64 * 0.15).round() as usize {
                    2
                } else {
                    1
                };
                let newborn = vacancy.is_some() && self.spawn_after <= 0.;
                let mut incoming = self.spawn(input, params, layer, !newborn);
                if !newborn {
                    // Its whole body is outside the viewport at entry.
                    let x = if incoming.course_sign > 0. {
                        layout.bounds.x - 8.
                    } else {
                        layout.bounds.x + layout.bounds.width + 8.
                    };
                    let dx = x - incoming.head().x;
                    for n in &mut incoming.nodes[..incoming.count] {
                        n.pos.x += dx;
                    }
                } else {
                    self.spawn_after = range(&mut self.rng, 6., 15.);
                    self.births += 1;
                }
                if let Some(i) = vacancy {
                    self.bonds.retain(|b| b.a != i && b.b != i);
                    self.agents[i] = incoming;
                    self.gust_carry[i] = 1.;
                } else {
                    self.gust_carry[self.agents.len()] = 1.;
                    self.agents.push(incoming);
                }
            }
        }
        self.motes.retain_mut(|m| {
            m.age += dt;
            m.pos += (m.vel + m.scatter * (-m.age / 1.5).exp()) * dt;
            if self.step.is_multiple_of(8) {
                m.vel += (self.flow.sample(m.pos).velocity * 0.62 - m.vel) * (dt * 1.4).min(1.);
            }
            m.age < m.life
        });
    }
}
