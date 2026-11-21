//! Continuous half-open rectangle union in logical coordinates. Cell decomposition
//! makes overlapping/cloned outputs count once and keeps disconnected columns apart.
use super::rng::Pcg32;
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl Rect {
    pub fn right(self) -> f64 {
        self.x + self.width
    }
    pub fn bottom(self) -> f64 {
        self.y + self.height
    }
    pub fn area(self) -> f64 {
        self.width * self.height
    }
    pub fn centre(self) -> Vec2 {
        Vec2 {
            x: self.x + self.width / 2.,
            y: self.y + self.height / 2.,
        }
    }
    pub fn valid(self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|v| v.is_finite())
            && self.width > 0.
            && self.height > 0.
            && self.right() > self.x
            && self.bottom() > self.y
            && self.area() > 0.
            && self.x.abs() + self.width <= 1e7
            && self.y.abs() + self.height <= 1e7
    }
    pub fn contains(self, p: Vec2) -> bool {
        p.x >= self.x && p.x < self.right() && p.y >= self.y && p.y < self.bottom()
    }
}
#[derive(Clone, Debug)]
pub struct OutputRect {
    pub id: u64,
    pub rect: Rect,
    pub primary: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Interval {
    pub start: f64,
    pub end: f64,
}
#[derive(Clone, Copy, Debug)]
pub struct BoundarySegment {
    pub a: Vec2,
    pub b: Vec2,
    pub normal: Vec2,
}
#[derive(Clone, Copy, Debug)]
pub struct Distance {
    pub distance: f64,
    pub normal: Vec2,
}
#[derive(Clone, Debug)]
pub struct Island {
    pub cells: Vec<Rect>,
    pub area: f64,
}
#[derive(Clone, Debug)]
pub struct Layout {
    pub generation: u64,
    pub outputs: Vec<OutputRect>,
    pub bounds: Rect,
    pub visible_area: f64,
    pub islands: Vec<Island>,
    pub outline: Vec<BoundarySegment>,
    cells: Vec<Rect>,
    columns: Vec<(Interval, Vec<Interval>)>,
}
impl Layout {
    pub fn new(mut outputs: Vec<OutputRect>, generation: u64) -> Result<Self, &'static str> {
        if outputs.is_empty() || outputs.len() > 64 || outputs.iter().any(|o| !o.rect.valid()) {
            return Err("invalid layout");
        }
        outputs.sort_by_key(|o| o.id);
        if outputs.windows(2).any(|w| w[0].id == w[1].id) {
            return Err("duplicate output identity");
        }
        let mut xs: Vec<_> = outputs
            .iter()
            .flat_map(|o| [o.rect.x, o.rect.right()])
            .collect();
        let mut ys: Vec<_> = outputs
            .iter()
            .flat_map(|o| [o.rect.y, o.rect.bottom()])
            .collect();
        xs.sort_by(f64::total_cmp);
        xs.dedup();
        ys.sort_by(f64::total_cmp);
        ys.dedup();
        let bounds = Rect {
            x: xs[0],
            y: ys[0],
            width: xs[xs.len() - 1] - xs[0],
            height: ys[ys.len() - 1] - ys[0],
        };
        let mut cells = Vec::new();
        let mut columns = Vec::new();
        let mut outline = Vec::new();
        let nx = xs.len() - 1;
        let ny = ys.len() - 1;
        let mut occupied = vec![false; nx * ny];
        for x in 0..nx {
            for y in 0..ny {
                let rect = Rect {
                    x: xs[x],
                    y: ys[y],
                    width: xs[x + 1] - xs[x],
                    height: ys[y + 1] - ys[y],
                };
                if outputs.iter().any(|o| o.rect.contains(rect.centre())) {
                    occupied[x * ny + y] = true;
                    cells.push(rect);
                }
            }
        }
        for x in 0..nx {
            let mut intervals: Vec<Interval> = Vec::new();
            for y in 0..ny {
                if !occupied[x * ny + y] {
                    continue;
                }
                if let Some(last) = intervals.last_mut().filter(|i| i.end == ys[y]) {
                    last.end = ys[y + 1];
                } else {
                    intervals.push(Interval {
                        start: ys[y],
                        end: ys[y + 1],
                    });
                }
                let r = Rect {
                    x: xs[x],
                    y: ys[y],
                    width: xs[x + 1] - xs[x],
                    height: ys[y + 1] - ys[y],
                };
                let mut edge = |exposed: bool, a: Vec2, b: Vec2, normal: Vec2| {
                    if exposed {
                        outline.push(BoundarySegment { a, b, normal });
                    }
                };
                edge(
                    x == 0 || !occupied[(x - 1) * ny + y],
                    Vec2 { x: r.x, y: r.y },
                    Vec2 {
                        x: r.x,
                        y: r.bottom(),
                    },
                    Vec2 { x: -1., y: 0. },
                );
                edge(
                    x + 1 == nx || !occupied[(x + 1) * ny + y],
                    Vec2 {
                        x: r.right(),
                        y: r.y,
                    },
                    Vec2 {
                        x: r.right(),
                        y: r.bottom(),
                    },
                    Vec2 { x: 1., y: 0. },
                );
                edge(
                    y == 0 || !occupied[x * ny + y - 1],
                    Vec2 { x: r.x, y: r.y },
                    Vec2 {
                        x: r.right(),
                        y: r.y,
                    },
                    Vec2 { x: 0., y: -1. },
                );
                edge(
                    y + 1 == ny || !occupied[x * ny + y + 1],
                    Vec2 {
                        x: r.x,
                        y: r.bottom(),
                    },
                    Vec2 {
                        x: r.right(),
                        y: r.bottom(),
                    },
                    Vec2 { x: 0., y: 1. },
                );
            }
            columns.push((
                Interval {
                    start: xs[x],
                    end: xs[x + 1],
                },
                intervals,
            ));
        }
        // Merge adjacent collinear boundary spans, retaining outward normals.
        outline.sort_by(|a, b| {
            a.normal
                .x
                .total_cmp(&b.normal.x)
                .then(a.normal.y.total_cmp(&b.normal.y))
                .then(if a.normal.x != 0. {
                    a.a.x.total_cmp(&b.a.x).then(a.a.y.total_cmp(&b.a.y))
                } else {
                    a.a.y.total_cmp(&b.a.y).then(a.a.x.total_cmp(&b.a.x))
                })
        });
        let mut merged: Vec<BoundarySegment> = Vec::new();
        for e in outline {
            if let Some(last) = merged
                .last_mut()
                .filter(|s| s.normal == e.normal && s.b == e.a)
            {
                last.b = e.b;
            } else {
                merged.push(e);
            }
        }
        let mut islands = Vec::new();
        let mut visited = vec![false; occupied.len()];
        for i in 0..occupied.len() {
            if !occupied[i] || visited[i] {
                continue;
            }
            let mut stack = vec![i];
            visited[i] = true;
            let mut part = Vec::new();
            while let Some(j) = stack.pop() {
                let x = j / ny;
                let y = j % ny;
                part.push(Rect {
                    x: xs[x],
                    y: ys[y],
                    width: xs[x + 1] - xs[x],
                    height: ys[y + 1] - ys[y],
                });
                for next in [
                    x.checked_sub(1).map(|a| a * ny + y),
                    (x + 1 < nx).then_some((x + 1) * ny + y),
                    y.checked_sub(1).map(|a| x * ny + a),
                    (y + 1 < ny).then_some(j + 1),
                ]
                .into_iter()
                .flatten()
                {
                    if occupied[next] && !visited[next] {
                        visited[next] = true;
                        stack.push(next);
                    }
                }
            }
            islands.push(Island {
                area: part.iter().map(|r| r.area()).sum(),
                cells: part,
            });
        }
        Ok(Self {
            generation,
            outputs,
            bounds,
            visible_area: cells.iter().map(|r| r.area()).sum(),
            islands,
            outline: merged,
            cells,
            columns,
        })
    }
    pub fn contains(&self, p: Vec2) -> bool {
        self.vertical_intervals(p.x)
            .iter()
            .any(|i| p.y >= i.start && p.y < i.end)
    }
    pub fn vertical_intervals(&self, x: f64) -> &[Interval] {
        self.columns
            .iter()
            .find(|(i, _)| x >= i.start && x < i.end)
            .map_or(&[], |(_, v)| v.as_slice())
    }
    pub fn signed_distance_and_normal(&self, p: Vec2) -> Distance {
        let mut best = f64::INFINITY;
        let mut normal = Vec2::default();
        for e in &self.outline {
            let dx = e.b.x - e.a.x;
            let dy = e.b.y - e.a.y;
            let t = (((p.x - e.a.x) * dx + (p.y - e.a.y) * dy) / (dx * dx + dy * dy)).clamp(0., 1.);
            let delta = Vec2 {
                x: p.x - (e.a.x + dx * t),
                y: p.y - (e.a.y + dy * t),
            };
            let d = delta.x.hypot(delta.y);
            if d < best {
                best = d;
                normal = if d > 0. {
                    let sign = if self.contains(p) { -1. } else { 1. };
                    Vec2 {
                        x: sign * delta.x / d,
                        y: sign * delta.y / d,
                    }
                } else {
                    e.normal
                };
            }
        }
        Distance {
            distance: if self.contains(p) { -best } else { best },
            normal,
        }
    }
    pub fn circle_fits(&self, p: Vec2, radius: f64) -> bool {
        radius.is_finite()
            && radius >= 0.
            && self.contains(p)
            && -self.signed_distance_and_normal(p).distance >= radius
    }
    pub fn nearest_visible(&self, p: Vec2) -> Vec2 {
        if self.contains(p) {
            return p;
        }
        self.cells
            .iter()
            .map(|r| Vec2 {
                x: p.x.clamp(r.x, r.right() - r.width * 1e-12),
                y: p.y.clamp(r.y, r.bottom() - r.height * 1e-12),
            })
            .min_by(|a, b| {
                (a.x - p.x)
                    .hypot(a.y - p.y)
                    .total_cmp(&(b.x - p.x).hypot(b.y - p.y))
            })
            .unwrap()
    }
    pub fn choose_visible_anchor(&self, rng: &mut Pcg32, margin: f64) -> Option<Vec2> {
        if !margin.is_finite() || margin < 0. {
            return None;
        }
        // Bounded rejection from disjoint union cells (unbiased in visible area).
        for _ in 0..256 {
            let mut pick = rng.unit() * self.visible_area;
            let r = self
                .cells
                .iter()
                .find(|r| {
                    pick -= r.area();
                    pick < 0.
                })
                .unwrap_or(self.cells.last().unwrap());
            let p = Vec2 {
                x: r.x + rng.unit() * r.width,
                y: r.y + rng.unit() * r.height,
            };
            if self.circle_fits(p, margin) {
                return Some(p);
            }
        }
        self.cells
            .iter()
            .map(|r| r.centre())
            .find(|p| self.circle_fits(*p, margin))
    }
    /// Use the centre of the output containing the panorama centre. Otherwise
    /// nearest centre, then primary, then area and connector hash (stable order).
    pub fn eye_anchor(&self) -> Vec2 {
        let p = self.bounds.centre();
        self.outputs
            .iter()
            .min_by(|a, b| {
                let rank = |o: &OutputRect| {
                    if o.rect.contains(p) {
                        0.
                    } else {
                        (o.rect.centre().x - p.x).hypot(o.rect.centre().y - p.y)
                    }
                };
                rank(a)
                    .total_cmp(&rank(b))
                    .then(b.primary.cmp(&a.primary))
                    .then(b.rect.area().total_cmp(&a.rect.area()))
                    .then(a.id.cmp(&b.id))
            })
            .unwrap()
            .rect
            .centre()
    }
    pub fn open_seams(&self) -> Vec<BoundarySegment> {
        let mut seams = Vec::new();
        for (i, a) in self.outputs.iter().enumerate() {
            for b in &self.outputs[i + 1..] {
                let (a, b, vertical) = if a.rect.right() == b.rect.x {
                    (a, b, true)
                } else if b.rect.right() == a.rect.x {
                    (b, a, true)
                } else if a.rect.bottom() == b.rect.y {
                    (a, b, false)
                } else if b.rect.bottom() == a.rect.y {
                    (b, a, false)
                } else {
                    continue;
                };
                let (lo, hi) = if vertical {
                    (a.rect.y.max(b.rect.y), a.rect.bottom().min(b.rect.bottom()))
                } else {
                    (a.rect.x.max(b.rect.x), a.rect.right().min(b.rect.right()))
                };
                if hi > lo {
                    seams.push(BoundarySegment {
                        a: if vertical {
                            Vec2 { x: b.rect.x, y: lo }
                        } else {
                            Vec2 { x: lo, y: b.rect.y }
                        },
                        b: if vertical {
                            Vec2 { x: b.rect.x, y: hi }
                        } else {
                            Vec2 { x: hi, y: b.rect.y }
                        },
                        normal: Vec2::default(),
                    });
                }
            }
        }
        seams
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn output(id: u64, x: f64, y: f64, w: f64, h: f64) -> OutputRect {
        OutputRect {
            id,
            rect: Rect {
                x,
                y,
                width: w,
                height: h,
            },
            primary: id == 2,
        }
    }
    #[test]
    fn matthew_l3() {
        let l = Layout::new(
            vec![
                output(1, 0., 0., 2560., 1440.),
                output(2, 2560., 0., 3440., 1440.),
                output(3, 6000., 195., 1920., 1080.),
            ],
            1,
        )
        .unwrap();
        assert_eq!(l.visible_area, 10_713_600.);
        assert_eq!(l.outline.len(), 8);
        assert_eq!(l.islands.len(), 1);
        assert_eq!(l.eye_anchor(), Vec2 { x: 4280., y: 720. });
        let s = l.open_seams().into_iter().find(|s| s.a.x == 6000.).unwrap();
        assert_eq!((s.a.y, s.b.y), (195., 1275.));
        assert!(!l.contains(Vec2 { x: 6000., y: 194. }));
        assert!(l.contains(Vec2 { x: 6000., y: 195. }));
        assert!(!l.contains(Vec2 { x: 7920., y: 195. }));
        assert!(l.circle_fits(Vec2 { x: 6000., y: 700. }, 100.));
        assert!(!l.circle_fits(Vec2 { x: 6000., y: 200. }, 10.));
        assert_eq!(
            l.vertical_intervals(7000.),
            [Interval {
                start: 195.,
                end: 1275.
            }]
        );
        assert_eq!(
            l.signed_distance_and_normal(Vec2 { x: 6500., y: 100. })
                .normal,
            Vec2 { x: 0., y: -1. }
        );
        let mut rng = Pcg32::new(7, 1);
        assert!(l.choose_visible_anchor(&mut rng, 100.).is_some());
        assert!(l.choose_visible_anchor(&mut rng, 2000.).is_none());
    }
    #[test]
    fn overlaps_islands_negative_and_vertical_holes() {
        let l = Layout::new(
            vec![
                output(1, -100., -100., 100., 100.),
                output(2, -100., 100., 100., 100.),
                output(3, -100., 100., 100., 100.),
            ],
            9,
        )
        .unwrap();
        assert_eq!(l.visible_area, 20_000.);
        assert_eq!(l.islands.len(), 2);
        assert_eq!(l.vertical_intervals(-50.).len(), 2);
        assert!(!l.contains(Vec2 { x: -50., y: 50. }));
        assert!(!l.circle_fits(Vec2 { x: -50., y: 100. }, 1.));
        assert!(l.contains(l.nearest_visible(Vec2 { x: 0., y: 0. })));
        let touching = Layout::new(
            vec![
                output(1, -100., -100., 100., 100.),
                output(2, -100., 0., 100., 100.),
            ],
            10,
        )
        .unwrap();
        assert_eq!(touching.islands.len(), 1);
        assert_eq!(touching.outline.len(), 4);
        let seams = touching.open_seams();
        assert_eq!(seams.len(), 1);
        assert_eq!(seams[0].a, Vec2 { x: -100., y: 0. });
        assert_eq!(seams[0].b, Vec2 { x: 0., y: 0. });
        assert!(touching.circle_fits(Vec2 { x: -50., y: 0. }, 40.));
    }
}
