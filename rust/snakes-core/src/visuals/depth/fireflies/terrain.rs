use super::super::common::{
    layout::{Layout, Rect},
    rng::Pcg32,
};

pub fn between(rng: &mut Pcg32, a: f32, b: f32) -> f32 {
    a + (b - a) * rng.unit() as f32
}
pub fn smooth(x: f32) -> f32 {
    let t = x.clamp(0., 1.);
    t * t * (3. - 2. * t)
}

#[derive(Clone, Copy, Default)]
pub struct Column {
    pub floor: f32,
    pub height: f32,
    pub flight_floor: f32,
    pub escape: f32,
    refuge_x: f32,
}

pub struct Meadow {
    pub bounds: Rect,
    pub outputs: Vec<Rect>,
    pub columns: Vec<Column>,
    pub pitch: f32,
    vertical_holes: Vec<Vec<[f32; 2]>>,
}

impl Meadow {
    pub fn new(layout: &Layout) -> Self {
        let b = layout.bounds;
        let count = (b.width / 32.).ceil().clamp(1., 2048.) as usize;
        let pitch = b.width as f32 / count as f32;
        let mut columns = Vec::with_capacity(count);
        let mut vertical_holes = Vec::with_capacity(count);
        for i in 0..count {
            let x = b.x + (i as f64 + 0.5) * pitch as f64;
            let intervals = layout.vertical_intervals(x);
            vertical_holes.push(
                intervals
                    .windows(2)
                    .map(|w| [w[0].end as f32, w[1].start as f32])
                    .collect(),
            );
            let (top, bottom, escape, refuge_x) =
                if let (Some(top), Some(bottom)) = (intervals.first(), intervals.last()) {
                    (top.start, bottom.end, 0., x as f32)
                } else {
                    let nearest = layout
                        .outputs
                        .iter()
                        .min_by(|a, b| {
                            let distance = |r: Rect| (x - x.clamp(r.x, r.right())).abs();
                            distance(a.rect).total_cmp(&distance(b.rect))
                        })
                        .unwrap()
                        .rect;
                    (
                        nearest.y,
                        nearest.bottom(),
                        (nearest.centre().x - x).signum() as f32,
                        x.clamp(
                            nearest.x + nearest.width.min(32.) * 0.5,
                            nearest.right() - nearest.width.min(32.) * 0.5,
                        ) as f32,
                    )
                };
            columns.push(Column {
                floor: bottom as f32,
                height: (bottom - top) as f32,
                flight_floor: bottom as f32,
                escape,
                refuge_x,
            });
        }
        // Start lifting before a raised bank, while blade roots stay at the actual bottom.
        for i in 0..count {
            for j in i.saturating_sub(4)..(i + 5).min(count) {
                let weight = smooth(1. - (i.abs_diff(j) as f32 * pitch / 128.));
                let candidate = columns[i].floor + (columns[j].floor - columns[i].floor) * weight;
                columns[i].flight_floor = columns[i].flight_floor.min(candidate);
            }
        }
        Self {
            bounds: b,
            outputs: layout.outputs.iter().map(|o| o.rect).collect(),
            columns,
            vertical_holes,
            pitch,
        }
    }
    pub fn index(&self, x: f32) -> usize {
        (((x - self.bounds.x as f32) / self.pitch) as usize).min(self.columns.len() - 1)
    }
    // A home may drift across an unoccupied region. Steer toward a visible
    // resting place, while currents and courtship can still carry a flier across it.
    pub fn safe_target(&self, x: f32, mut y: f32) -> (f32, f32) {
        // A glade can extend beyond the outermost bank. Fold its preferred
        // resting places back into the meadow to avoid attracting flies to a wall.
        let width = self.bounds.width as f32;
        let x = self.bounds.x as f32 + width
            - ((x - self.bounds.x as f32).rem_euclid(2. * width) - width).abs();
        let index = self.index(x);
        let escape = self.columns[index].escape;
        if escape != 0. {
            return (self.columns[index].refuge_x, y);
        }
        for &[a, b] in &self.vertical_holes[index] {
            if y > a && y < b {
                y = if y - a < b - y { a - 8. } else { b + 8. };
                break;
            }
        }
        (x, y)
    }
    pub fn column(&self, x: f32) -> Column {
        let u = ((x - self.bounds.x as f32) / self.pitch - 0.5)
            .clamp(0., (self.columns.len() - 1) as f32);
        let a = self.columns[u as usize];
        let b = self.columns[(u as usize + 1).min(self.columns.len() - 1)];
        let t = smooth(u.fract());
        Column {
            floor: a.floor + t * (b.floor - a.floor),
            height: a.height + t * (b.height - a.height),
            flight_floor: a.flight_floor + t * (b.flight_floor - a.flight_floor),
            ..self.columns[self.index(x)]
        }
    }
}

// Linked bins have fixed storage; clearing a tick never allocates.
pub struct Neighbours {
    pitch: f32,
    origin: [f32; 2],
    width: usize,
    height: usize,
    head: Vec<usize>,
    next: Vec<usize>,
}
impl Neighbours {
    pub fn new(bounds: Rect, n: usize) -> Self {
        let pitch = 128_f32
            .max(bounds.width as f32 / 384.)
            .max(bounds.height as f32 / 128.);
        let width = (bounds.width as f32 / pitch).ceil() as usize;
        let height = (bounds.height as f32 / pitch).ceil() as usize;
        Self {
            pitch,
            origin: [bounds.x as f32, bounds.y as f32],
            width,
            height,
            head: vec![usize::MAX; width * height],
            next: vec![usize::MAX; n],
        }
    }
    fn cell(&self, x: f32, y: f32) -> (usize, usize) {
        (
            (((x - self.origin[0]) / self.pitch) as usize).min(self.width - 1),
            (((y - self.origin[1]) / self.pitch) as usize).min(self.height - 1),
        )
    }
    pub fn fill(&mut self, flies: &[super::fly::Fly]) {
        self.head.fill(usize::MAX);
        for (i, f) in flies.iter().enumerate() {
            let (x, y) = self.cell(f.x, f.y);
            let h = &mut self.head[y * self.width + x];
            self.next[i] = *h;
            *h = i;
        }
    }
    pub fn visit(&self, x: f32, y: f32, r: f32, mut visit: impl FnMut(usize)) {
        let (x0, y0) = self.cell(x - r, y - r);
        let (x1, y1) = self.cell(x + r, y + r);
        for cy in y0..=y1 {
            for cx in x0..=x1 {
                let mut i = self.head[cy * self.width + cx];
                while i != usize::MAX {
                    visit(i);
                    i = self.next[i];
                }
            }
        }
    }
}
