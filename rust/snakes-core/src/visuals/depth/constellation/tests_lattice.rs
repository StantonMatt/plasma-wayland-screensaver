use super::*;
use crate::visuals::depth::common::{layout::Vec2, rng::output_seed};

#[derive(Default, Debug)]
struct Lattice {
    occupancy: usize,
    largest: usize,
    eligible: usize,
    low_cv: usize,
    min_cv: Option<f32>,
}

impl Lattice {
    // A close patch is a connected set of specks at most 28 px apart.
    // Also test local patches inside radius-60 disks so a long loose tail
    // cannot conceal a crystalline core in the overall component's CV.
    fn sample(&mut self, points: &[Point], ids: &[usize]) {
        self.largest = self.largest.max(ids.len());
        if ids.len() < 8 {
            return;
        }
        let nearest: Vec<_> = ids
            .iter()
            .map(|&i| {
                ids.iter()
                    .filter(|&&j| j != i)
                    .map(|&j| (points[i] - points[j]).len())
                    .fold(f32::INFINITY, f32::min)
            })
            .collect();
        let mean = nearest.iter().sum::<f32>() / ids.len() as f32;
        let variance = nearest.iter().map(|&d| (d - mean).powi(2)).sum::<f32>() / ids.len() as f32;
        let cv = variance.sqrt() / mean.max(f32::EPSILON);
        self.min_cv = Some(self.min_cv.map_or(cv, |prev| prev.min(cv)));
        self.eligible += 1;
        self.low_cv += usize::from(cv < 0.30);
    }
    fn measure(points: &[Point]) -> Self {
        use std::collections::BTreeSet;
        let mut result = Self::default();
        let mut patches = BTreeSet::new();
        let mut disk = |centre: Point| {
            let ids: Vec<_> = points
                .iter()
                .enumerate()
                .filter(|(_, p)| (**p - centre).len2() <= 3600.1)
                .map(|(i, _)| i)
                .collect();
            result.occupancy = result.occupancy.max(ids.len());
            if ids.len() >= 8 {
                patches.insert(ids);
            }
        };
        for (i, &a) in points.iter().enumerate() {
            disk(a);
            for &b in &points[..i] {
                let d = b - a;
                if d.len2() > 14400. || d.len2() == 0. {
                    continue;
                }
                // A maximal disk can be translated until two points lie on
                // its boundary. Both centres matter at screen edges and seams.
                let offset = pt(-d.y, d.x) * (3600. / d.len2() - 0.25).max(0.).sqrt();
                disk(a.mix(b, 0.5) + offset);
                disk(a.mix(b, 0.5) - offset);
            }
        }
        patches.insert((0..points.len()).collect());
        let mut sampled = BTreeSet::new();
        for patch in patches {
            let mut remaining = patch;
            while let Some(i) = remaining.pop() {
                let mut group = vec![i];
                let mut idx = 0;
                while idx < group.len() {
                    let p = points[group[idx]];
                    remaining.retain(|&j| {
                        if (p - points[j]).len2() <= 28_f32.powi(2) {
                            group.push(j);
                            false
                        } else {
                            true
                        }
                    });
                    idx += 1;
                }
                group.sort_unstable();
                if sampled.insert(group.clone()) {
                    result.sample(points, &group);
                }
            }
        }
        result
    }
}

#[test]
fn lattice_metric_detects_a_hex_patch_inside_an_offset_disk() {
    let mut stars = Vec::new();
    for j in 0..3 {
        for i in 0..4 {
            stars.push(pt(
                (i as f32 + 0.5 * (j % 2) as f32) * 13.,
                j as f32 * 13. * 0.75_f32.sqrt(),
            ));
        }
    }
    stars.push(pt(165., 0.));
    let m = Lattice::measure(&stars);
    assert_eq!((m.occupancy, m.largest), (12, 12));
    assert!(m.low_cv > 0 && m.min_cv.unwrap() < 0.001);
    let irregular = [0., 4., 28., 37., 59., 74., 97., 119.].map(|x| pt(x, 0.));
    let m = Lattice::measure(&irregular);
    assert!(m.eligible > 0 && m.low_cv == 0 && m.min_cv.unwrap() > 0.30);
    let rim: Vec<_> = (0..7)
        .map(|j| Point::polar(j as f32 * std::f32::consts::TAU / 7.) * 59.)
        .collect();
    let m = Lattice::measure(&rim);
    assert_eq!((m.occupancy, m.eligible, m.min_cv), (7, 0, None));
}

#[test]
fn dim_field_lattice_acceptance_scenes() {
    let mut failed = Vec::new();
    for (name, seconds, seed) in [
        ("G", 600, 7),
        ("A", 1800, 1),
        ("gravity100", 600, 1),
        ("D", 240, 11),
        ("B", 240, 7),
        ("Pastel-Spanning", 240, 7),
        ("Gathering", 80, 1),
    ] {
        let wide = matches!(name, "B" | "Pastel-Spanning");
        let mut config = if wide {
            let mut i = tests::spanning();
            i.seed = seed;
            i
        } else {
            tests::input(output_seed(seed, 2, "Virtual-0"))
        };
        config.palette = 0;
        config.environment.background = 1;
        if wide || name == "G" {
            config.environment.background = 0;
            config.speed = 3.;
            config.scale = 2.;
            config.density = 80.;
            config.trails = 1.;
            config.palette = if name == "Pastel-Spanning" { 5 } else { 2 };
        } else if name == "D" {
            config.palette = 3;
            config.environment.background = 3;
        }
        if name == "gravity100" {
            config.parameters.push(super::super::schema::Value {
                id: 1,
                kind: 1,
                payload: 100,
            });
        }
        let mut sky = ConstellationWorld::new(config);
        if matches!(name, "D" | "Gathering") {
            let (id, sec, y) = if name == "D" {
                (4, 6, 720.)
            } else {
                (7, 10, 600.)
            };
            sky.inject(Event {
                id,
                at_ns: sec * 1_000_000_000,
                anchor: [1280., y],
                strength: 1.,
            })
            .unwrap();
        }
        for tick in 1..=seconds * 60 {
            sky.step(tick);
        }
        let points: Vec<_> = sky
            .stars
            .iter()
            .filter(|s| s.alive && s.mag >= 4.)
            .map(|s| s.p)
            .collect();
        let mut measure = |label: &str, field: &[Point]| {
            let m = Lattice::measure(field);
            eprintln!(
                "lattice {name}/{seconds} {label} dim={} max_disk={} largest={} eligible={} low_cv={} min_cv={:?}",
                field.len(),
                m.occupancy,
                m.largest,
                m.eligible,
                m.low_cv,
                m.min_cv
            );
            if m.low_cv > 0 || m.occupancy > 6 {
                failed.push(format!("{name}/{label}"));
            }
        };
        measure("world", &points);
        if name == "G" {
            for (label, xmin, ymin, xmax, ymax) in [
                ("upper-right-crop", 2330., 120., 2520., 320.),
                ("lower-left-crop", 440., 1020., 500., 1110.),
            ] {
                let inside: Vec<_> = points
                    .iter()
                    .copied()
                    .filter(|p| (xmin..=xmax).contains(&p.x) && (ymin..=ymax).contains(&p.y))
                    .collect();
                measure(label, &inside);
            }
        }
        if wide {
            for (output, id) in sky
                .input
                .layout
                .outputs
                .iter()
                .zip(["HDMI-A-5", "DP-5", "DP-4"])
            {
                let field: Vec<_> = points
                    .iter()
                    .copied()
                    .filter(|p| {
                        output.rect.contains(Vec2 {
                            x: p.x as f64,
                            y: p.y as f64,
                        })
                    })
                    .collect();
                measure(id, &field);
            }
        }
    }
    assert!(
        failed.is_empty(),
        "crystallised or crowded fields: {failed:?}"
    );
}
