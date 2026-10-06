// SPDX-License-Identifier: GPL-3.0-or-later
use super::{Color, Vertex};
use std::ops::{Add, Sub, Mul, Div};
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct P {
    pub x: f64,
    pub y: f64
}
impl P {
    pub fn new(x: f64, y: f64) -> Self {
        Self {
            x,
            y
        }
    }
    pub fn finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }
}
impl Add for P {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        Self::new(self.x+b.x, self.y+b.y)
    }
}
impl Sub for P {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        Self::new(self.x-b.x, self.y-b.y)
    }
}
impl Mul<f64> for P {
    type Output = Self;
    fn mul(self, b: f64) -> Self {
        Self::new(self.x*b, self.y*b)
    }
}
impl Div<f64> for P {
    type Output = Self;
    fn div(self, b: f64) -> Self {
        Self::new(self.x/b, self.y/b)
    }
}
#[derive(Clone, Copy, Default)]
pub(super) struct Offsets {
    pub first: i32,
    pub last: i32,
    pub extent: f64
}
// A primitive either gets its complete finite tiling or is rejected. This
// budget bounds malformed finite geometry as well as nonfinite inputs, without
// silently clipping the range to translations near the head/original arena.
// Long bodies are split into edges, so their budget is linear in body length.
pub(super) const MAX_PRIMITIVE_COPIES: i64 = 4096;
impl Offsets {
    fn empty() -> Self { Self { first: 1, last: 0, extent: 0.0 } }
    fn count(self) -> i64 { (self.last as i64-self.first as i64+1).max(0) }
}
pub(super) fn offsets(min: f64, max: f64, extent: f64, margin: f64) -> Offsets {
    if !(min.is_finite() & max.is_finite() & extent.is_finite() & margin.is_finite())
        || extent<=0.0 || margin<0.0 || min>max {
        return Offsets::empty();
    }
    let low = min-margin;
    let high = max+margin;
    if low>0.0 && high<extent {
        return Offsets { extent, ..Offsets::default() };
    }
    if low > -extent && high < extent*2.0 && low <= extent && high >= 0.0 {
        return Offsets { first: if high>=extent { -1 } else { 0 },
            last: if low<=0.0 { 1 } else { 0 }, extent };
    }
    let first = (-high/extent).ceil();
    let last = ((extent-low)/extent).floor();
    // Check derived bounds before casting or creating any loop. In particular,
    // saturated float-to-int casts must never turn huge finite input into a
    // multi-billion-iteration range.
    if !first.is_finite() || !last.is_finite()
        || first<i32::MIN as f64 || last>i32::MAX as f64
        || first>last || last-first+1.0>MAX_PRIMITIVE_COPIES as f64 {
        return Offsets::empty();
    }
    Offsets { first: first as i32, last: last as i32, extent }
}
// All primitives select arena copies from their full world-space bounds.
#[inline]
pub(super) fn copies(min: P, max: P, margin: P, arena: P, walls: bool) -> (Offsets, Offsets) {
    if walls {
        (Offsets::default(), Offsets::default())
    } else {
        let x=offsets(min.x, max.x, arena.x, margin.x);
        let y=offsets(min.y, max.y, arena.y, margin.y);
        if x.count()*y.count()>MAX_PRIMITIVE_COPIES {
            (Offsets::empty(), Offsets::empty())
        } else { (x,y) }
    }
}
pub(super) fn delta(from: f64, to: f64, extent: f64, walls: bool) -> f64 {
    let mut d = to-from;
    if !walls && extent>0.0 {
        if d>extent/2.0 {
            d -= extent;
        } else if d < -extent/2.0 {
            d += extent;
        }
    }
    d
}
pub(super) fn wrap(x: f64, extent: f64) -> f64 {
    if extent<=0.0 {
        return 0.0;
    }
    // Most interpolated and preceding body points are already canonical.
    // f64 remainder returns x in this range, so avoid its libc call entirely.
    if x>=0.0 && x<extent {
        return x;
    }
    let x = x%extent;
    if x<0.0 {
        x+extent
    } else {
        x
    }
}
// Match QRectF::intersects: strict overlap, normalized negative extents, NaNs rejected.
pub(super) fn visible(x: f64, y: f64, w: f64, h: f64, view: P) -> bool {
    let (l, r) = if w<0.0 {
        (x+w, x)
    } else {
        (x, x+w)
    };
    let (t, b) = if h<0.0 {
        (y+h, y)
    } else {
        (y, y+h)
    };
    view.x>0.0 && view.y>0.0 && l<view.x && r>0.0 && t<view.y && b>0.0
}
pub(super) struct Sink<'a> {
    pub output: &'a mut [Vertex],
    pub count: usize,
    pub view: P,
    pub circles: &'a [[P;13];13]
}
impl Sink<'_> {
    #[inline]
    pub fn vertex(&mut self, p: P, c: Color) {
        if let Some(v) = self.output.get_mut(self.count) {
            *v = Vertex {
                x: p.x as f32,
                y: p.y as f32,
                color: c
            };
        }
        self.count += 1;
    }
    pub fn triangle(&mut self, a: P, b: P, c: P, color: Color) {
        if a.finite() && b.finite() && c.finite() {
            self.vertex(a, color);
            self.vertex(b, color);
            self.vertex(c, color);
        }
    }
    pub fn disc(&mut self, p: P, r: f64, c: Color, sides: usize) {
        if !p.finite() || !r.is_finite() || r<=0.0 || !visible(p.x-r, p.y-r, r*2.0, r*2.0, self.view) {
            return;
        }
        for i in 0..sides {
            self.vertex(p, c);
            self.vertex(p+self.circles[sides][i]*r, c);
            self.vertex(p+self.circles[sides][i+1]*r, c);
        }
    }
    pub fn segment(&mut self, a: P, b: P, w: f64, c: Color) {
        if !a.finite() || !b.finite() || !w.is_finite() {
            return;
        }
        let d = b-a;
        let len = d.length();
        if len<0.001 || w<=0.0 {
            return;
        }
        if !visible(a.x.min(b.x)-w, a.y.min(b.y)-w, (b.x-a.x).abs()+w*2.0, (b.y-a.y).abs()+w*2.0, self.view) {
            return;
        }
        let n = P::new(-d.y/len*w, d.x/len*w);
        self.vertex(a+n, c);
        self.vertex(a-n, c);
        self.vertex(b+n, c);
        self.vertex(b+n, c);
        self.vertex(a-n, c);
        self.vertex(b-n, c);
    }
    pub fn ribbon(&mut self, points: &[P], normals: &[P], valid: &[bool], w: f64, c: Color) {
        if points.len()<2 || !w.is_finite() || w<=0.0 {
            return;
        }
        for i in 1..points.len() {
            if !valid[i-1] || !valid[i] {
                continue;
            }
            let a = points[i-1];
            let b = points[i];
            if !visible(a.x.min(b.x)-w, a.y.min(b.y)-w, (b.x-a.x).abs()+w*2.0, (b.y-a.y).abs()+w*2.0, self.view) {
                continue;
            }
            let an = normals[i-1]*w;
            let bn = normals[i]*w;
            self.vertex(a+an, c);
            self.vertex(a-an, c);
            self.vertex(b+bn, c);
            self.vertex(b+bn, c);
            self.vertex(a-an, c);
            self.vertex(b-bn, c);
        }
    }
}
pub(super) fn prepare(points: &[P], normals: &mut [P], valid: &mut [bool]) {
    valid.fill(false);
    for i in 0..points.len() {
        if !points[i].finite() {
            continue;
        }
        let prev = i>0 && points[i-1].finite();
        let next = i+1<points.len() && points[i+1].finite();
        let d = match(prev, next) {
            (true, true)=>points[i+1]-points[i-1], (false, true)=>points[i+1]-points[i], (true, false)=>points[i]-points[i-1], _=>continue
        };
        let len = d.length();
        if !len.is_finite() || len<0.001 {
            continue;
        }
        normals[i] = P::new(-d.y/len, d.x/len);
        valid[i] = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wrap_copy_ranges_reject_nonfinite_and_excessive_work_before_iteration() {
        for (min,max,margin) in [(0.0,10.0,f64::INFINITY),
            (-f64::MAX,f64::MAX,0.0),(-1e9,1e9,1e9),(0.0,10.0,f64::NAN)] {
            let range=offsets(min,max,10.0,margin);
            assert_eq!(range.count(),0);
        }
        let range=offsets(50.0,323.0,100.0,18.0);
        assert_eq!((range.first,range.last),(-3,0));
        let (x,y)=copies(P::new(-1000.0,-1000.0),P::new(1000.0,1000.0),
            P::default(),P::new(10.0,10.0),false);
        assert_eq!(x.count()*y.count(),0,"bound the Cartesian product too");
    }
}
