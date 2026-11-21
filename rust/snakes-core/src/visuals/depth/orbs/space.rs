use super::*;
/// Each body is stored once. Hash collisions only lengthen a bucket; the cell
/// check keeps them from duplicating a force or contact.
pub(super) struct Grid {
    heads: Vec<usize>,
    next: Vec<usize>,
    cells: Vec<(i32, i32)>,
    scale: f64,
}
impl Grid {
    pub fn new(n: usize, width: f64) -> Self {
        Self {
            heads: vec![usize::MAX; (n * 4).next_power_of_two()],
            next: vec![usize::MAX; n],
            cells: vec![(0, 0); n],
            scale: 1. / width.max(1.),
        }
    }
    fn cell(&self, p: Vec2) -> (i32, i32) {
        (
            (p.x * self.scale).floor() as i32,
            (p.y * self.scale).floor() as i32,
        )
    }
    fn bucket(&self, x: i32, y: i32) -> usize {
        ((x as u32).wrapping_mul(73856093) ^ (y as u32).wrapping_mul(19349663)) as usize
            & (self.heads.len() - 1)
    }
    pub fn clear(&mut self) {
        self.heads.fill(usize::MAX)
    }
    pub fn add(&mut self, i: usize, p: Vec2) {
        let (x, y) = self.cell(p);
        let b = self.bucket(x, y);
        self.cells[i] = (x, y);
        self.next[i] = self.heads[b];
        self.heads[b] = i;
    }
    pub fn visit(&self, p: Vec2, r: f64, mut f: impl FnMut(usize)) {
        let (x0, y0) = self.cell(p - super::p(r, r));
        let (x1, y1) = self.cell(p + super::p(r, r));
        for y in y0..=y1 {
            for x in x0..=x1 {
                let mut i = self.heads[self.bucket(x, y)];
                while i != usize::MAX {
                    if self.cells[i] == (x, y) {
                        f(i)
                    }
                    i = self.next[i];
                }
            }
        }
    }
}
/// Distance to exposed union edges. The interior of a touching seam has no wall.
pub fn wall(layout: &Layout, point: Vec2, margin: f64) -> Vec2 {
    let mut force = Vec2::default();
    for edge in &layout.outline {
        let q = p(
            point.x.clamp(edge.a.x, edge.b.x),
            point.y.clamp(edge.a.y, edge.b.y),
        );
        let d = point - q;
        let d2 = d.dot(d);
        if d2 < margin * margin {
            let n = if d2 > 1e-12 {
                d / d2.sqrt()
            } else {
                -edge.normal
            };
            force += n * (1. - d2.sqrt() / margin).powi(2);
        }
    }
    force
}
/// Strip the inward normal near exposed walls, preserving tangential drift at seams.
pub fn slip(layout: &Layout, point: Vec2, mut velocity: Vec2, band: f64) -> Vec2 {
    for edge in &layout.outline {
        let q = p(
            point.x.clamp(edge.a.x, edge.b.x),
            point.y.clamp(edge.a.y, edge.b.y),
        );
        let delta = point - q;
        let distance2 = delta.dot(delta);
        if distance2 < band * band {
            let n = if distance2 < 1e-9 {
                -edge.normal
            } else {
                delta / distance2.sqrt()
            };
            velocity += n * (-velocity.dot(n).min(0.) * ease(2. * (1. - distance2.sqrt() / band)));
        }
    }
    velocity
}
/// A closed basin turns water along its shore. Different sections of the
/// same shoreline shed water back into the basin, avoiding a stationary row.
pub fn shore_flow(layout: &Layout, point: Vec2, flow: Vec2, band: f64) -> Vec2 {
    let mut result = flow;
    for edge in &layout.outline {
        let q = p(
            point.x.clamp(edge.a.x, edge.b.x),
            point.y.clamp(edge.a.y, edge.b.y),
        );
        let delta = point - q;
        let d2 = delta.dot(delta);
        if d2 >= band * band {
            continue;
        }
        let inward = -edge.normal;
        let tangent = p(-inward.y, inward.x);
        let weight = 1. - ease(d2.sqrt() / band);
        let inflow = (-flow.dot(inward)).max(0.);
        let phase = point.dot(tangent) / (band * 1.8);
        let slip = 8. + 0.68 * inflow;
        result += (inward * (inflow + (4. + 0.40 * inflow) * (1. + phase.sin()))
            + tangent * slip * if flow.dot(tangent) < 0. { -1. } else { 1. })
            * weight;
    }
    result
}
/// v = curl(w ψ). Multiplying just the velocity by a wall mask would
/// compress a whole sheet of motes into a line at the edge of the mask.
pub struct TracerCurrent {
    pub velocity: Vec2,
    pub wall_weight: f64,
}
pub fn tracer_current(layout: &Layout, flow: &FlowField, pos: Vec2) -> TracerCurrent {
    let band = (layout.bounds.height * 0.21).clamp(90., 300.);
    let mut nearest = band * band;
    let mut normal = Vec2::default();
    for edge in &layout.outline {
        let q = p(
            pos.x.clamp(edge.a.x, edge.b.x),
            pos.y.clamp(edge.a.y, edge.b.y),
        );
        let d = pos - q;
        let d2 = d.dot(d);
        if d2 < nearest {
            nearest = d2;
            normal = if d2 > 1e-8 {
                d / d2.sqrt()
            } else {
                -edge.normal
            };
        }
    }
    let t = nearest.sqrt() / band;
    let (psi, velocity) = flow.tracer_stream(pos, layout.bounds.centre(), layout.bounds.height);
    let weight = ease(t);
    // A small shoreline circulation has zero normal flux and remains finite
    // at the wall where the Hermite taper's derivative is zero.
    let tangent = p(normal.y, -normal.x);
    let wall_weight = 1. - weight;
    TracerCurrent {
        velocity: velocity * weight + tangent * (psi * 6. * t * (1. - t) / band + 8. * wall_weight),
        wall_weight,
    }
}
pub fn repel(rect: Rect, point: Vec2, margin: f64) -> Vec2 {
    let centre = rect.centre();
    let d = point - centre;
    let q = p(
        d.x.abs() - rect.width * 0.55,
        d.y.abs() - rect.height * 0.55,
    );
    if q.x > margin || q.y > margin {
        return Vec2::default();
    }
    let n = if q.x.max(q.y) <= 0. {
        if q.x > q.y {
            p(if d.x < 0. { -1. } else { 1. }, 0.)
        } else {
            p(0., if d.y < 0. { -1. } else { 1. })
        }
    } else {
        p(q.x.max(0.) * d.x.signum(), q.y.max(0.) * d.y.signum()).unit()
    };
    let dist = p(q.x.max(0.), q.y.max(0.)).length() + q.x.max(q.y).min(0.);
    n * (1. - dist / margin).clamp(0., 3.)
}
#[derive(Clone, Copy)]
pub struct Clock {
    pub rect: Rect,
    pub v: Vec2,
    pub output: u64,
    bounds: Option<Rect>,
}
impl Clock {
    pub fn new(input: &WorldInput) -> Vec<Self> {
        let mut clocks = Vec::new();
        for o in input
            .environment
            .obstacles
            .iter()
            .filter(|o| o.enabled != 0 && o.kind == 1)
        {
            let mut r = o.rect;
            if input.mode == MonitorMode::Spanning && o.velocity != Vec2::default() {
                if !clocks.is_empty() {
                    continue;
                }
                let screen = input
                    .layout
                    .outputs
                    .iter()
                    .find(|x| x.primary)
                    .unwrap_or(&input.layout.outputs[0])
                    .rect;
                r.x = screen.x + ((screen.width - r.width) * 0.28).max(0.);
                r.y = screen.y + ((screen.height - r.height) * 0.22).max(0.);
            }
            clocks.push(Self {
                rect: r,
                v: o.velocity,
                output: o.output_id,
                bounds: input
                    .environment
                    .obstacles
                    .iter()
                    .find(|b| b.kind == 2 && b.enabled != 0 && b.output_id == o.output_id)
                    .map(|b| b.rect),
            });
        }
        clocks
    }
    pub fn advance(&mut self, l: &Layout, dt: f64) {
        if self.v == Vec2::default() {
            return;
        }
        let valid = |r: Rect| {
            // A rectangle whose interior intersects an exposed boundary cannot
            // fit inside the union, including an L-shaped concave corner.
            if let Some(b) = self.bounds {
                return r.x >= b.x
                    && r.right() <= b.right()
                    && r.y >= b.y
                    && r.bottom() <= b.bottom();
            }
            l.contains(r.centre())
                && !l.outline.iter().any(|e| {
                    e.a.x.min(e.b.x) < r.right()
                        && e.a.x.max(e.b.x) > r.x
                        && e.a.y.min(e.b.y) < r.bottom()
                        && e.a.y.max(e.b.y) > r.y
                })
        };
        let x = self.rect.x + self.v.x * dt;
        if valid(Rect { x, ..self.rect }) {
            self.rect.x = x
        } else {
            self.v.x = -self.v.x
        }
        let y = self.rect.y + self.v.y * dt;
        if valid(Rect { y, ..self.rect }) {
            self.rect.y = y
        } else {
            self.v.y = -self.v.y
        }
    }
}
