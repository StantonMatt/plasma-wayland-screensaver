// SPDX-License-Identifier: GPL-3.0-or-later
use super::super::common::rng::Pcg32;
use super::space::*;
use std::sync::Arc;

pub const SHELL: [f64; 2] = [1., 92.];
pub const ROWS: usize = 250;
pub const WIDTH: usize = 1024;
pub const GROUP: usize = 128;
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Star {
    pub body: [f32; 4],
    pub light: [f32; 4],
}
#[derive(Debug)]
pub struct Row {
    pub cell: Cell,
    pub stamp: u64,
    pub stars: Vec<Star>,
}
pub type Atlas = Vec<Arc<Row>>;
fn star(p: V, b: f64, temp: f64, kind: f32, r: &mut Pcg32) -> Star {
    let pulse = if b > 0.60 && r.unit() < 0.02 {
        std::f64::consts::TAU / (4. + 36. * r.unit())
    } else {
        0.
    };
    Star {
        body: p.four(b),
        light: [
            temp as f32,
            kind,
            pulse as f32,
            (r.unit() * std::f64::consts::TAU) as f32,
        ],
    }
}
fn field_population(layer: usize, density: f64, field: f64) -> usize {
    let stars = if layer == 0 {
        7. + density * 0.86
    } else {
        17. + density * 5.58
    };
    (stars * (0.78 + 0.39 * field)).round() as usize
}

// One near cell may contain the entire largest globular knot and its landmark.
// Far rows contain only field stars. Four-star packing pads the last group.
pub fn row_capacity(density: f64) -> usize {
    let near = field_population(0, density, 1.) + (150. + 1.5 * density) as usize + 1;
    near.max(field_population(1, density, 1.))
        .next_multiple_of(4)
}
fn in_cell(p: V) -> bool {
    [p.0, p.1, p.2].into_iter().all(|v| (0. ..1.).contains(&v))
}
fn add_knot(stars: &mut Vec<Star>, centre: V, mark: Mark, knot: u64, density: f64) {
    if [centre.0, centre.1, centre.2]
        .into_iter()
        .any(|v| !(-0.40..1.40).contains(&v))
    {
        return;
    }
    let mut r = Pcg32::new(mark.id, 17 + 2 * knot);
    let n = match mark.kind {
        Kind::Globular => (150. + 1.5 * density) as usize,
        Kind::Binary => 1,
        _ => (44. + 0.70 * density) as usize,
    };
    for i in 0..n {
        let dir = V(r.unit() - 0.5, r.unit() - 0.5, r.unit() - 0.5).unit();
        let radial = r.unit();
        let radius = match mark.kind {
            Kind::Globular => 0.22 * radial.powf(1.8),
            Kind::Binary => 0.023,
            _ => 0.33 * radial.sqrt(),
        };
        let p = centre + dir * radius;
        let brightness = match mark.kind {
            Kind::Binary => 0.97,
            Kind::Open if i % 6 == 0 => 0.89 + 0.20 * r.unit(),
            Kind::Open => 0.16 + 0.52 * r.unit(),
            _ => 0.05 + 0.74 * r.unit().powi(3),
        };
        let temp = if mark.kind == Kind::Globular {
            0.12 + 0.28 * r.unit()
        } else {
            (mark.temp + 0.36 * (r.unit() - 0.5)).clamp(0., 1.)
        };
        let record = star(p, brightness, temp, 0., &mut r);
        if in_cell(p) {
            stars.push(record);
        }
    }
}
/// Record positions stay cell-local for their entire GPU lifetime.
fn populate(seed: u64, c: Cell, layer: usize, density: f64) -> Row {
    let key = c.hash(seed ^ [0x1161, 0x2455][layer]) | 1;
    // A recreated world can retain the same window/texture. Density changes
    // the row contents; the procedural positions of surviving stars stay fixed.
    let stamp = super::super::common::rng::hash64(key ^ density.to_bits()) | 1;
    let mut r = Pcg32::new(key, 71);
    let base = c.base();
    let field = region((base + V(0.5, 0.5, 0.5)) * SHELL[layer], seed);
    let count = field_population(layer, density, field);
    let mut stars = Vec::with_capacity(count + 12);
    for _ in 0..count {
        let p = V(r.unit(), r.unit(), r.unit());
        let u = r.unit();
        let b = 0.025 + 0.975 * u.powi(7);
        let temp = (0.13 + 0.29 * field + 0.58 * r.unit()).clamp(0., 1.);
        stars.push(star(p, b, temp, 0., &mut r));
    }
    if layer == 0 {
        // Include neighbouring birth districts whose gas complex reaches this
        // cell. Membership, temperatures and positions depend only on the seed.
        let lo = Cell::at(base - V(0.85, 0.65, 3.27), DISTRICT);
        let hi = Cell::at(base + V(2.08, 1.65, 1.50), DISTRICT);
        for z in lo.2..=hi.2 {
            for y in lo.1..=hi.1 {
                for x in lo.0..=hi.0 {
                    let mark = landmark(seed, Cell(x, y, z));
                    if in_cell(mark.p - base) {
                        stars.push(star(
                            mark.p - base,
                            1.2 + 0.5 * mark.flux,
                            mark.temp,
                            1.,
                            &mut Pcg32::new(mark.id, 17),
                        ));
                    }
                    match mark.kind {
                        Kind::Open => {
                            // The bright central association belongs to the
                            // landmark itself. Its angular size grows on approach.
                            let mut rng = Pcg32::new(mark.id, 83);
                            for _ in 0..12 {
                                let offset =
                                    V(rng.unit() - 0.5, rng.unit() - 0.5, rng.unit() - 0.5);
                                let p = mark.p - base + offset * 0.037;
                                let member =
                                    star(p, 0.73 + 0.23 * rng.unit(), mark.temp, 2., &mut rng);
                                if in_cell(p) {
                                    stars.push(member);
                                }
                            }
                            for (i, knot) in nursery(mark).into_iter().enumerate() {
                                add_knot(&mut stars, knot - base, mark, i as u64, density);
                            }
                        }
                        Kind::Globular | Kind::Binary => {
                            add_knot(&mut stars, mark.p - base, mark, 0, density)
                        }
                        _ => {}
                    }
                }
            }
        }
        stars.truncate(row_capacity(density));
    }
    debug_assert!(stars.len() <= row_capacity(density) && row_capacity(density) <= WIDTH);
    Row {
        cell: c,
        stamp,
        stars,
    }
}
pub struct Stream {
    pub atlas: Arc<Atlas>,
    pub centres: [Cell; 2],
    pub revision: u64,
    seed: u64,
    density: f64,
}
impl Stream {
    pub fn new(seed: u64, p: V, density: f64) -> Self {
        let centres = SHELL.map(|s| Cell::at(p, s));
        let mut rows = Vec::with_capacity(ROWS);
        for (layer, &centre) in centres.iter().enumerate() {
            for i in 0..125 {
                let shift = |v: i64, slot: i64| (slot - v + 2).rem_euclid(5) - 2;
                let c = centre.offset(
                    shift(centre.0, i % 5),
                    shift(centre.1, i / 5 % 5),
                    shift(centre.2, i / 25),
                );
                rows.push(Arc::new(populate(seed, c, layer, density)));
            }
        }
        Self {
            atlas: Arc::new(rows),
            centres,
            revision: 1,
            seed,
            density,
        }
    }
    pub fn follow(&mut self, p: V) {
        for (layer, &s) in SHELL.iter().enumerate() {
            let c = Cell::at(p, s);
            if self.centres[layer] == c {
                continue;
            }
            let rows = Arc::make_mut(&mut self.atlas);
            for z in -2..=2 {
                for y in -2..=2 {
                    for x in -2..=2 {
                        let key = c.offset(x, y, z);
                        let slot = layer * 125 + key.slot();
                        if rows[slot].cell != key {
                            rows[slot] = Arc::new(populate(self.seed, key, layer, self.density))
                        }
                    }
                }
            }
            self.centres[layer] = c;
            self.revision += 1;
        }
    }
}
