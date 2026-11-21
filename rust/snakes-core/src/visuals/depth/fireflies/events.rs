use super::super::world::Event;
use super::{
    World,
    fly::{NOBODY, Species, State},
    terrain::{between, smooth},
};

pub const GUST: u32 = 1;
pub const SHOWER: u32 = 2;
pub const TREE: u32 = 3;
pub const HUNT: u32 = 4;
pub const COURTSHIP: u32 = 5;
pub const COURT_HEIGHT: f32 = 172.;
pub const COURT_SECONDS: f32 = 2.5;

#[derive(Clone, Copy, Default)]
pub struct Weather {
    pub kind: u32,
    pub age: f32,
    pub duration: f32,
    pub x: f32,
}
impl Weather {
    pub fn rain(self) -> f32 {
        if self.kind != SHOWER {
            return 0.;
        }
        smooth(self.age / 3.) * smooth((self.duration - self.age) / 2.)
    }
    pub fn gust(self, x: f32) -> f32 {
        if self.kind != GUST {
            return 0.;
        }
        smooth(1. - (x - (self.x + self.age * 450.)).abs() / 450.)
    }
}

impl World {
    pub(super) fn begin_event(&mut self, kind: u32, anchor: [f32; 2]) {
        if kind == COURTSHIP {
            if let Some(female) = self
                .flies
                .iter()
                .position(|f| f.species == Species::Dipper && f.female)
                && let Some(male) = self
                    .flies
                    .iter()
                    .position(|f| f.species == Species::Dipper && !f.female)
            {
                let c = self.meadow.column(anchor[0]);
                let size = c.height / 1440.;
                let a = &mut self.flies[female];
                a.x = anchor[0];
                a.y = anchor[1];
                a.vx = 0.;
                a.vy = 0.;
                a.altitude = (c.flight_floor - a.y) / c.height;
                a.roam = a.x - self.glades[a.home].x;
                a.state = State::Perched;
                a.target = male;
                let b = &mut self.flies[male];
                b.x = anchor[0] + 40. * size;
                b.y = anchor[1] - COURT_HEIGHT * size;
                b.vx = 0.;
                b.vy = 0.;
                b.heading = 0.;
                b.state = State::Court;
                b.target = female;
                b.timer = COURT_SECONDS;
                b.exchanges = 0;
                self.relocated.extend([female, male]);
            }
            return;
        }
        if kind == HUNT {
            if self.params.hunters {
                self.force_hunt(anchor)
            }
            return;
        }
        if !self.params.nightEvents {
            return;
        }
        self.release_tree();
        let duration = match kind {
            GUST => (self.meadow.bounds.width as f32 + 900.) / 450.,
            SHOWER => between(&mut self.rng, 32., 40.),
            TREE => between(&mut self.rng, 62., 78.),
            _ => return,
        };
        self.weather = Weather {
            kind,
            age: 0.,
            duration,
            x: self.meadow.bounds.x as f32 - 450.,
        };
        if kind == TREE {
            self.gathered = true;
            let r = *self
                .meadow
                .outputs
                .iter()
                .min_by(|a, b| {
                    let distance = |r: &&super::super::common::layout::Rect| {
                        (anchor[0] as f64 - anchor[0].clamp(r.x as f32, r.right() as f32) as f64)
                            .abs()
                    };
                    distance(a).total_cmp(&distance(b))
                })
                .unwrap();
            let scale = (r.height as f32 / 1440.).min(r.width as f32 / 1250.);
            let cx = anchor[0].clamp(r.x as f32 + 550. * scale, r.right() as f32 - 550. * scale);
            self.weather.x = cx;
            let perches = crown(cx, r.bottom() as f32, scale, self.art.scale, &mut self.rng);
            // Gather nearby syncers and distant lanterns first, then other local
            // fliers until the crown has a full, evenly separated constellation.
            let mut candidates = Vec::new();
            for (i, f) in self.flies.iter().enumerate() {
                if (f.x - cx).abs() <= 1200. * scale
                    && !matches!(f.state, State::Lost | State::Fading)
                {
                    candidates.push((
                        if f.species == Species::Syncer { 0 } else { 2 },
                        (f.x - cx).abs(),
                        i,
                    ));
                }
            }
            for (i, f) in self.distant.iter().enumerate() {
                if !f.near && (f.x - cx).abs() <= 1200. * scale {
                    candidates.push((1, (f.x - cx).abs(), self.flies.len() + i));
                }
            }
            candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.total_cmp(&b.1)));
            let occupied = &perches[..perches.len().min(candidates.len())];
            let sigma = 6.5 * self.art.scale * 1.25 * if self.art.palette == 5 { 1.25 } else { 1. };
            for (i, ((_, _, id), &perch)) in candidates.into_iter().zip(occupied).enumerate() {
                let gain = perch_flash_gain(perch, occupied, sigma, self.art.scale);
                // Each limb joins the shared pulse at a slightly different time.
                let delay = 0.47 * (i as f32 * 0.618_034).fract();
                if id < self.flies.len() {
                    let fly = &mut self.flies[id];
                    fly.tree = Some(perch);
                    fly.tree_delay = delay;
                    fly.tree_gain = gain;
                    fly.state = State::Patrol;
                    fly.target = NOBODY;
                } else {
                    let fly = &mut self.distant[id - self.flies.len()];
                    fly.tree = Some(perch);
                    fly.tree_delay = delay;
                    fly.tree_gain = gain;
                }
            }
        }
        if kind == SHOWER {
            self.last_shower = self.clock;
        }
    }
    fn release_tree(&mut self) {
        if self.weather.kind != TREE {
            return;
        }
        for f in &mut self.flies {
            if f.tree.take().is_some() && f.female {
                f.state = State::Perched;
            }
            f.tree_pulse = false;
        }
        for f in &mut self.distant {
            f.tree = None;
        }
    }
    fn force_hunt(&mut self, anchor: [f32; 2]) {
        let nearest = |species, female: bool| {
            self.flies
                .iter()
                .enumerate()
                .filter(|(_, f)| f.species == species && f.female == female)
                .min_by(|(_, a), (_, b)| {
                    let d =
                        |f: &super::fly::Fly| (f.x - anchor[0]).powi(2) + (f.y - anchor[1]).powi(2);
                    d(a).total_cmp(&d(b))
                })
                .map(|(i, _)| i)
        };
        if let (Some(h), Some(p)) = (
            nearest(Species::Hunter, true),
            nearest(Species::Dipper, false),
        ) {
            self.relocated.extend([h, p]);
            let c = self.meadow.column(anchor[0]);
            let floor = c.floor - c.height * 0.10;
            self.flies[h].x = anchor[0];
            self.flies[h].y = floor;
            self.flies[h].vx = 0.;
            self.flies[h].vy = 0.;
            self.flies[h].altitude = (c.flight_floor - floor) / c.height;
            self.flies[h].roam = anchor[0] - self.glades[self.flies[h].home].x;
            self.flies[h].perch_age = 60.;
            self.flies[h].fire(self.flash_time, 0.65);
            self.flies[h].state = State::Perched;
            self.flies[h].z = self.flies[p].z;
            self.flies[p].x = anchor[0] - 42.;
            self.flies[p].y = floor - 77.;
            self.flies[p].fire(self.flash_time, 1.);
            self.flies[h].target = p;
            self.flies[p].target = h;
            self.flies[p].state = State::Lured;
            self.flies[p].timer = 2.6;
        }
    }
    pub(super) fn weather_step(&mut self, dt: f32) {
        self.weather.age += dt;
        if self.weather.kind != 0 && self.weather.age >= self.weather.duration {
            if self.weather.kind == SHOWER {
                // Release wet perches over two seconds through each ordinary oscillator.
                for f in &mut self.flies {
                    if !f.female {
                        f.phase = 1. - between(&mut self.rng, 0.08, 1.95) / f.period;
                        f.remaining = 0;
                    }
                }
                self.after_rain = 6.;
            }
            self.release_tree();
            self.weather = Weather::default();
        }
        if self.weather.kind == TREE && self.weather.age > self.weather.duration - 15. {
            self.release_tree();
        }
        self.after_rain = (self.after_rain - dt).max(0.);
        self.next_event -= dt;
        if self.params.nightEvents && self.weather.kind == 0 && self.next_event <= 0. {
            let kind = if self.tree_night && !self.gathered && self.rng.unit() < 0.45 {
                TREE
            } else if self.clock - self.last_shower > between(&mut self.rng, 900., 1450.) {
                SHOWER
            } else {
                GUST
            };
            let home = self.glades[self.rng.next_u32() as usize % self.glades.len()];
            self.begin_event(kind, [home.x, 0.]);
            let area = (self.meadow.bounds.width as f32 / 2560.)
                .sqrt()
                .clamp(1., 2.);
            self.next_event = between(&mut self.rng, 300., 600.) / area;
        }
    }
    pub(super) fn scheduled(&mut self, tick: u64) {
        let now = self.rate_ns(tick);
        while self.script.first().is_some_and(|e| e.at_ns <= now) {
            let Event { id, anchor, .. } = self.script.remove(0);
            self.begin_event(id, anchor.map(|v| v as f32));
        }
    }
}

/// A bent limb, in reference-height pixels; positive y points into the canopy.
#[derive(Clone, Copy)]
struct Limb {
    start: [f32; 2],
    delta: [f32; 2],
    bow: f32,
}
impl Limb {
    fn grow(start: [f32; 2], angle: f32, length: f32, rng: &mut super::Pcg32) -> Self {
        let bend = between(rng, 0.10, 0.18) * if rng.unit() < 0.5 { -1. } else { 1. };
        let (sin, cos) = angle.sin_cos();
        Self {
            start,
            delta: [length * sin, length * cos],
            bow: bend,
        }
    }
    fn point(self, t: f32) -> [f32; 2] {
        let offset = 4. * self.bow * t * (1. - t);
        [
            self.start[0] + self.delta[0] * t - self.delta[1] * offset,
            self.start[1] + self.delta[1] * t + self.delta[0] * offset,
        ]
    }
}

// Evaluate the same broad halo and tail as the shader once, at recruitment.
// In a close group, reserve room for neighbours at the top of the shared pulse.
fn perch_flash_gain(perch: [f32; 2], neighbours: &[[f32; 2]], sigma: f32, size: f32) -> f32 {
    if size <= 1. {
        return 1.;
    }
    let mut spill = 0.;
    for &q in neighbours {
        let d = (q[0] - perch[0]).hypot(q[1] - perch[1]) / sigma;
        if d > 0. && d < 8. {
            spill += 0.75 * (-0.5 * d * d).exp() + 0.30 * (-0.48 * d).exp();
        }
    }
    1. / (1. + 3.5 * smooth(size - 1.) * spill)
}

fn crown(
    cx: f32,
    ground: f32,
    scale: f32,
    halo_size: f32,
    rng: &mut super::Pcg32,
) -> Vec<[f32; 2]> {
    let trunk = Limb::grow([0., 64.], between(rng, -0.12, 0.12), 255., rng);
    let mut limbs = vec![trunk];
    let mut growing = vec![];
    let mut clusters = vec![];
    let main_count = 3 + rng.next_u32() as usize % 3;
    for i in 0..main_count {
        let across = 2. * i as f32 / (main_count - 1) as f32 - 1.;
        let root = trunk.point(between(rng, 0.76, 1.));
        let angle = across * 1.0 + between(rng, -0.26, 0.26);
        growing.push((root, angle, between(rng, 175., 252.)));
    }
    for depth in 0..4 {
        let mut next = vec![];
        for (root, angle, length) in growing {
            let limb = Limb::grow(root, angle, length, rng);
            limbs.push(limb);
            if depth >= 1 {
                clusters.push(limb.point(between(rng, 0.68, 1.)));
            }
            if depth < 3 {
                for side in [-1., 1.] {
                    let fork = limb.point(between(rng, 0.67, 1.));
                    let direction =
                        (0.75 * angle + side * between(rng, 0.30, 0.85)).clamp(-1.55, 1.55);
                    next.push((fork, direction, length * between(rng, 0.57, 0.83)));
                }
            }
        }
        growing = next;
    }
    let top = clusters.iter().map(|p| p[1]).fold(0_f32, f32::max);
    let fit = |p: [f32; 2]| {
        [
            p[0] * 1.20,
            if p[1] > 305. {
                305. + (p[1] - 305.) * 530. / (top - 305.)
            } else {
                p[1]
            },
        ]
    };
    let spacing = halo_size.max(1.).powf(0.6);
    let foliage_sigma = 22. * halo_size.max(1.).powf(0.3);
    let minimum2 = (14.2 * spacing / scale.clamp(0.68, 1.)).powi(2);
    let mut points = Vec::with_capacity(112);
    let insert = |points: &mut Vec<[f32; 2]>, p: [f32; 2]| {
        if points
            .iter()
            .all(|q| (q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2) >= minimum2)
        {
            points.push(p);
        }
    };
    // About a third of the lights hint at a trunk and curved lower branches.
    // Start independent runs along each limb so their perches never mirror.
    let mut samples = Vec::new();
    for limb in &limbs {
        let length = limb.delta[0].hypot(limb.delta[1]);
        let mut distance = between(rng, 2., 18.);
        let mut run = vec![];
        while distance < length {
            let mut p = fit(limb.point(distance / length));
            let jitter = between(rng, -6., 6.) / length;
            p[0] -= limb.delta[1] * jitter;
            p[1] += limb.delta[0] * jitter;
            run.push(p);
            distance += between(rng, 14., 34.);
        }
        samples.push(run);
    }
    for &p in samples[0].iter() {
        insert(&mut points, p);
    }
    // Breadth first across the main limbs, then their less prominent forks.
    for pass in 0..12 {
        for (i, run) in samples.iter().enumerate().skip(1) {
            let lower = i <= main_count * 3;
            if (lower || pass >= 2)
                && let Some(&p) = run.get(pass)
            {
                insert(&mut points, p);
                if points.len() == 39 {
                    break;
                }
            }
        }
        if points.len() == 39 {
            break;
        }
    }
    let branch_count = points.len();
    // Grow loose groups around real twig endpoints. Box-Muller supplies a
    // circular 22 px Gaussian in rendered space, after fitting the crown.
    let apex = *clusters
        .iter()
        .max_by(|a, b| a[1].total_cmp(&b[1]))
        .unwrap();
    let west = *clusters
        .iter()
        .min_by(|a, b| a[0].total_cmp(&b[0]))
        .unwrap();
    let east = *clusters
        .iter()
        .max_by(|a, b| a[0].total_cmp(&b[0]))
        .unwrap();
    for group_index in 0..256 {
        // Include the three outermost twig groups in every crown so a small
        // population still describes the full height and width of the dome.
        let centre = fit(match group_index {
            0 => apex,
            1 => west,
            2 => east,
            _ => clusters[rng.next_u32() as usize % clusters.len()],
        });
        let group = 3 + rng.next_u32() % 5;
        let mut planted = 0;
        for _ in 0..20 {
            let radius = foliage_sigma * (-2. * (rng.unit() as f32).max(1e-7).ln()).sqrt();
            let (sin, cos) = between(rng, 0., std::f32::consts::TAU).sin_cos();
            let p = [centre[0] + radius * cos, centre[1] + radius * sin];
            if p[1] > 857. || p[0].abs() > 525. {
                continue;
            }
            let before = points.len();
            insert(&mut points, p);
            planted += (points.len() - before) as u32;
            if planted == group || points.len() == 112 {
                break;
            }
        }
        if points.len() == 112 {
            break;
        }
    }
    // With a sparse population, recruit the same proportion of foliage lights
    // as at full density. Keep the crown filled throughout the first arrivals.
    let count = points.len();
    for i in (branch_count + 1..count).rev() {
        points.swap(
            i,
            branch_count + rng.next_u32() as usize % (i + 1 - branch_count),
        );
    }
    let mut branch = 0;
    let mut leaf = branch_count;
    (0..count)
        .map(|i| {
            let index = if (i + 1) * branch_count / count > branch {
                branch += 1;
                branch - 1
            } else {
                leaf += 1;
                leaf - 1
            };
            let p = points[index];
            [cx + scale * p[0], ground - scale * p[1]]
        })
        .collect()
}
