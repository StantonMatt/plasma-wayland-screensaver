// SPDX-License-Identifier: GPL-3.0-or-later
use super::super::common::rng::hash64;
use super::{
    Art, Frame,
    space::*,
    voyage::{Leg, Pose},
};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct Moment {
    pub id: u32,
    pub start: f64,
    pub duration: f64,
    pub retire: f64,
    pub birth: Pose,
    pub seed: f32,
    pub sun: Mark,
}
const GAPS: [(f64, f64); 4] = [(480., 900.), (1050., 1350.), (360., 720.), (3600., 5400.)];
pub struct Calendar {
    next: [f64; 4],
    occurrence: [u64; 4],
}
fn unit(seed: u64, id: usize, k: u64) -> f64 {
    (hash64(seed ^ hash64(k) ^ ((id as u64 + 1) * 137)) >> 11) as f64 / (1u64 << 53) as f64
}
impl Calendar {
    pub fn new(seed: u64) -> Self {
        Self {
            next: std::array::from_fn(|i| GAPS[i].0 + (GAPS[i].1 - GAPS[i].0) * unit(seed, i, 0)),
            occurrence: [0; 4],
        }
    }
    pub fn plan(&mut self, f: &mut Frame) {
        let a = &f.art;
        let leg = &f.leg;
        let moments = Arc::make_mut(&mut f.moments);
        moments.retain(|m| m.start + m.duration > leg.start - 0.15 && m.start < leg.start);
        if !a.params.events || a.input.reduced_motion {
            return;
        }
        for (i, &(short, long)) in GAPS.iter().enumerate() {
            let id = i as u32 + 4;
            let due = self.next[i].max(leg.start);
            let warp = leg.warp_start();
            let out = leg.arrival_start();
            let at = match (id, a.params.flight) {
                (6, "cruise") | (4, "hyperspace") => continue,
                (6, "voyage") => {
                    let t = due.max(warp + 0.1);
                    if t + 5.5 > out - 0.6 {
                        continue;
                    }
                    t
                }
                (4, "voyage") if due + 13. > warp - 3.25 => due.max(out),
                _ => due,
            };
            if at < leg.end {
                self.occurrence[i] += 1;
                moments.push(Moment::new(a.input.seed, leg, id, at));
                self.next[i] =
                    at + short + (long - short) * unit(a.input.seed, i, self.occurrence[i]);
            }
        }
        for e in f.script.iter().filter(|e| (4..=7).contains(&e.id)) {
            let t = e.at_ns as f64 * 1e-9;
            if (leg.start..leg.end).contains(&t) {
                moments.push(Moment::new(a.input.seed, leg, e.id, t))
            }
        }
        // The band, four crossfading volumes, nursery, destination and bloom
        // may use eight slots. Budget seven for events and keep one spare.
        moments.sort_by(|a, b| a.start.total_cmp(&b.start));
        for i in 0..moments.len() {
            let start = moments[i].start;
            if start < leg.start {
                continue;
            }
            let lens = leg.pose(start);
            let layers = |m: &Moment| if m.id == 5 { 1 } else { 2 };
            loop {
                let mut used = layers(&moments[i]);
                let mut faintest = i;
                let mut least = moments[i].contribution(lens, start + 1.5, a);
                for (k, m) in moments[..i]
                    .iter()
                    .enumerate()
                    .filter(|(_, m)| m.retire > start)
                {
                    used += layers(m);
                    // A retained event cannot be retroactively faded in a
                    // previous leg. Protect it if there is too little notice.
                    if m.start < leg.start && start < leg.start + 1.5 {
                        continue;
                    }
                    let light = m.contribution(lens, start, a);
                    if light < least {
                        faintest = k;
                        least = light;
                    }
                }
                if used <= 7 {
                    break;
                }
                moments[faintest].retire = start;
                if faintest == i {
                    break;
                }
            }
        }
    }
}
impl Moment {
    pub fn position(&self, age: f64) -> V {
        let local = match self.id {
            4 => V(-1.1 + 0.135 * age, 0.28 - 0.011 * age, 3.1),
            7 => V(0.7, 0.38, 4.) * 120.,
            _ => V(-0.08, 0.04, 1.),
        };
        self.birth.p + self.birth.attitude.inverse().apply(local)
    }
    fn contribution(&self, camera: Pose, t: f64, art: &Art) -> f64 {
        let age = (t - self.start).max(0.);
        let light = match self.id {
            4 => 0.85,
            5 => 0.65,
            6 => 0.60,
            _ => 0.15 + 0.85 * (-(age - 3.).max(0.) / 6.).exp(),
        };
        let visible = if self.id == 4 || self.id == 7 {
            // Score the actual emitting body on the common windshield. The
            // margin includes its halo or comet tail just beyond an edge.
            let ray = camera.attitude.apply(self.position(age) - camera.p);
            let rect = art.input.layout.bounds;
            let f = rect.height / (2. * 25f64.to_radians().tan());
            let (x, y) = if art.params.panoramic {
                let f = f.max(rect.width / std::f64::consts::PI);
                (
                    art.anchor[0] - rect.x + f * ray.0.atan2(ray.2),
                    art.anchor[1] - rect.y - f * ray.1 / ray.0.hypot(ray.2).max(1e-6),
                )
            } else if ray.2 > 0. {
                (
                    rect.width / 2. + f * ray.0 / ray.2,
                    rect.height / 2. - f * ray.1 / ray.2,
                )
            } else {
                return 0.;
            };
            let margin = 0.2 * rect.height;
            smooth(1. + x.min(rect.width - x) / margin)
                * smooth(1. + y.min(rect.height - y) / margin)
        } else {
            1.
        };
        light * visible * smooth(age / 1.5) * smooth((self.retire - t) / 1.5)
    }
    fn new(seed: u64, leg: &Leg, id: u32, start: f64) -> Self {
        let birth = leg.pose(start);
        let point = birth.p + birth.attitude.inverse().apply(V(-1.1, 0.28, 3.1));
        let cell = Cell::at(point, DISTRICT);
        let mut sun = leg.destination;
        if id == 4 {
            let mut dist = (sun.p - point).length();
            for z in -1..=1 {
                for y in -1..=1 {
                    for x in -1..=1 {
                        let m = landmark(seed, cell.offset(x, y, z));
                        let d = (m.p - point).length();
                        if d < dist {
                            sun = m;
                            dist = d
                        }
                    }
                }
            }
        }
        let duration = [13., 7., 5.5, 160.][id as usize - 4];
        Self {
            id,
            start,
            birth,
            sun,
            duration,
            retire: start + duration,
            seed: unit(seed, id as usize, start.to_bits()) as f32,
        }
    }
}
