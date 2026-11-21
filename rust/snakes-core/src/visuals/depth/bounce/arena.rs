//! Solid walls are the exposed spans of one connected component. Sweep the face
//! and its two end caps so the lip of a monitor step is a round circle contact.
use super::common::layout::{Layout, Rect, Vec2};
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub(super) struct V {
    pub x: f64,
    pub y: f64,
}
pub(super) const fn v(x: f64, y: f64) -> V {
    V { x, y }
}
impl From<Vec2> for V {
    fn from(p: Vec2) -> Self {
        v(p.x, p.y)
    }
}
impl From<V> for Vec2 {
    fn from(p: V) -> Self {
        Self { x: p.x, y: p.y }
    }
}
impl Add for V {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        v(self.x + b.x, self.y + b.y)
    }
}
impl Sub for V {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        v(self.x - b.x, self.y - b.y)
    }
}
impl AddAssign for V {
    fn add_assign(&mut self, b: Self) {
        *self = *self + b;
    }
}
impl SubAssign for V {
    fn sub_assign(&mut self, b: Self) {
        *self = *self - b;
    }
}
impl Mul<f64> for V {
    type Output = Self;
    fn mul(self, s: f64) -> Self {
        v(self.x * s, self.y * s)
    }
}
impl Div<f64> for V {
    type Output = Self;
    fn div(self, s: f64) -> Self {
        self * (1. / s)
    }
}
impl Neg for V {
    type Output = Self;
    fn neg(self) -> Self {
        v(-self.x, -self.y)
    }
}
impl V {
    pub fn dot(self, b: Self) -> f64 {
        self.x * b.x + self.y * b.y
    }
    pub fn len2(self) -> f64 {
        self.dot(self)
    }
    pub fn len(self) -> f64 {
        self.len2().sqrt()
    }
    pub fn unit(self) -> Self {
        self / self.len().max(1e-12)
    }
    pub fn perp(self) -> Self {
        v(-self.y, self.x)
    }
    pub fn limit(self, max: f64) -> Self {
        if self.len2() > max * max {
            self.unit() * max
        } else {
            self
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Wall {
    pub a: V,
    pub b: V,
    pub inward: V,
}
impl Wall {
    pub fn nearest(self, p: V) -> V {
        v(p.x.clamp(self.a.x, self.b.x), p.y.clamp(self.a.y, self.b.y))
    }
    // A circle initially on the interior side can only enter the capsule
    // through its front face or a tip. Faces moving away cannot be hit.
    fn sweep(self, p: V, travel: V, r: f64, limit: &mut f64, normal: &mut V) {
        let reach = r + travel.x.abs().max(travel.y.abs());
        if p.x < self.a.x - reach
            || p.x > self.b.x + reach
            || p.y < self.a.y - reach
            || p.y > self.b.y + reach
        {
            return;
        }
        let closing = -travel.dot(self.inward);
        if closing > 1e-10 {
            let t = ((p - self.a).dot(self.inward) - r) / closing;
            if t >= -1e-8 && t < *limit {
                let q = p + travel * t;
                if (self.inward.x == 0. && q.x >= self.a.x && q.x <= self.b.x)
                    || (self.inward.y == 0. && q.y >= self.a.y && q.y <= self.b.y)
                {
                    *limit = t.max(0.);
                    *normal = self.inward;
                    return;
                }
            }
        }
        let aa = travel.len2();
        for tip in [self.a, self.b] {
            let rel = p - tip;
            let half = rel.dot(travel);
            if half >= 0. {
                continue;
            }
            let c = rel.len2() - r * r;
            let discr = half * half - aa * c;
            if discr <= 0. {
                continue;
            }
            let t = c / (-half + discr.sqrt());
            if t >= -1e-8 && t < *limit {
                *limit = t.max(0.);
                *normal = (rel + travel * t).unit();
            }
        }
    }
}
#[derive(Clone)]
pub(super) struct Room {
    pub walls: Vec<Wall>,
    pub outputs: Vec<Rect>,
    pub area: f64,
}
impl Room {
    pub fn from_layout(layout: &Layout) -> Vec<Self> {
        layout
            .islands
            .iter()
            .map(|island| {
                let contains = |p: V| island.cells.iter().any(|c| c.contains(p.into()));
                let outputs = layout
                    .outputs
                    .iter()
                    .filter(|o| contains(o.rect.centre().into()))
                    .map(|o| o.rect)
                    .collect();
                let walls = layout
                    .outline
                    .iter()
                    .filter_map(|s| {
                        let wall = Wall {
                            a: s.a.into(),
                            b: s.b.into(),
                            inward: -V::from(s.normal),
                        };
                        // Collinear spans may meet at a diagonally touching pair of
                        // components, but their outward normals then differ.
                        let cell = island.cells.iter().any(|c| {
                            let q = wall.nearest(c.centre().into());
                            c.contains((q + wall.inward * (1e-7 * c.width.min(c.height))).into())
                        });
                        cell.then_some(wall)
                    })
                    .collect();
                Self {
                    walls,
                    outputs,
                    area: island.area,
                }
            })
            .collect()
    }
    /// Independent and canonical rooms have four walls. Most airborne
    /// circles are far enough inside all four to skip the wall loop.
    #[inline]
    pub fn clear_of_walls(&self, p: V, margin: f64) -> bool {
        let [r] = self.outputs.as_slice() else {
            return false;
        };
        p.x - margin > r.x
            && p.x + margin < r.right()
            && p.y - margin > r.y
            && p.y + margin < r.bottom()
    }
    pub fn sweep(&self, p: V, travel: V, r: f64) -> Option<(f64, V, Wall)> {
        if self.clear_of_walls(p, r + travel.x.abs().max(travel.y.abs()) + 1e-7) {
            return None;
        }
        let mut t = 1. + 1e-9;
        let mut normal = V::default();
        let mut hit = None;
        for &w in &self.walls {
            let old = t;
            w.sweep(p, travel, r, &mut t, &mut normal);
            if t < old {
                hit = Some(w);
            }
        }
        hit.map(|w| (t.min(1.), normal, w))
    }
    /// Correct round-off and bounded solver displacements, including simultaneous corner contacts.
    pub fn project(&self, p: &mut V, r: f64) {
        if self.clear_of_walls(*p, r) {
            return;
        }
        for w in &self.walls {
            let q = w.nearest(*p);
            let d = *p - q;
            let d2 = d.len2();
            if d2 >= r * r {
                continue;
            }
            let length = d2.sqrt();
            let n = if length > 1e-9 && d.dot(w.inward) >= 0. {
                d / length
            } else {
                w.inward
            };
            *p += n * (r - d.dot(n));
        }
    }
    pub fn nearest(&self, p: V) -> (Wall, V, f64) {
        let mut w = self.walls[0];
        let mut q = w.nearest(p);
        let mut d = (p - q).len2();
        for &other in &self.walls[1..] {
            let at = other.nearest(p);
            let d2 = (p - at).len2();
            if d2 < d {
                w = other;
                q = at;
                d = d2;
            }
        }
        (w, q, d.sqrt())
    }
}
