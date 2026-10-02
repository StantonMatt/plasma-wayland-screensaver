// SPDX-License-Identifier: GPL-3.0-or-later
use std::f64::consts::{ PI, TAU };
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64
}
impl Point {
    pub(crate) fn distance2(self, other: Self) -> f64 {
        (self.x-other.x).powi(2)+(self.y-other.y).powi(2)
    }
}
pub fn normalize_angle(mut angle: f64) -> f64 {
    // Preserve the JS sign at exactly +/- PI, but bound work for external inputs.
    angle %= TAU;
    if angle > PI {
        angle -= TAU;
    }
    if angle < -PI {
        angle += TAU;
    }
    angle
}
pub fn wrap_coordinate(value: f64, extent: f64) -> f64 {
    if extent <= 0.0 {
        0.0
    } else {
        ((value % extent)+extent)%extent
    }
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Geometry {
    pub width: f64,
    pub height: f64,
    pub deadly: bool
}
impl Geometry {
    pub fn axis_delta(self, from: f64, to: f64, extent: f64) -> f64 {
        let mut d = to-from;
        if !self.deadly && extent>0.0 {
            if d>extent/2.0 {
                d-=extent;
            } else if d< -extent/2.0 {
                d+=extent;
            }
        }
        d
    }
    pub fn delta(self, from: Point, to: Point) -> Point {
        Point {
            x: self.axis_delta(from.x, to.x, self.width),
            y: self.axis_delta(from.y, to.y, self.height)
        }
    }
    pub fn distance2(self, a: Point, b: Point) -> f64 {
        let d = self.delta(a, b);
        d.x*d.x+d.y*d.y
    }
    pub fn wrap(self, p: Point) -> Point {
        if self.deadly {
            p
        } else {
            Point {
                x: wrap_coordinate(p.x, self.width),
                y: wrap_coordinate(p.y, self.height)
            }
        }
    }
    pub fn segment_distance2(self, p: Point, a: Point, b: Point) -> f64 {
        let segment = self.delta(a, b);
        let d = self.delta(p, a);
        let a = Point {
            x: p.x+d.x,
            y: p.y+d.y
        };
        let d = segment;
        planar_point_segment(p, a, Point {
            x: a.x+d.x,
            y: a.y+d.y
        })
    }
    pub fn segments_distance2(self, a: Point, b: Point, c: Point, d: Point) -> f64 {
        let ab = self.delta(a, b);
        let ac = self.delta(a, c);
        let cd = self.delta(c, d);
        let b = Point {
            x: a.x+ab.x,
            y: a.y+ab.y
        };
        let c = Point {
            x: a.x+ac.x,
            y: a.y+ac.y
        };
        let d = Point {
            x: c.x+cd.x,
            y: c.y+cd.y
        };
        let crosses = [cross(a, b, c), cross(a, b, d), cross(c, d, a), cross(c, d, b)];
        if (crosses[0]*crosses[1]<0.0 && crosses[2]*crosses[3]<0.0)
        || (crosses[0].abs()<0.0001 && on_segment(c, a, b))
        || (crosses[1].abs()<0.0001 && on_segment(d, a, b))
        || (crosses[2].abs()<0.0001 && on_segment(a, c, d))
        || (crosses[3].abs()<0.0001 && on_segment(b, c, d)) {
            return 0.0;
        }
        planar_point_segment(a, c, d).min(planar_point_segment(b, c, d)).min(planar_point_segment(c, a, b)).min(planar_point_segment(d, a, b))
    }
}
fn cross(a: Point, b: Point, c: Point) -> f64 {
    (b.x-a.x)*(c.y-a.y)-(b.y-a.y)*(c.x-a.x)
}
fn on_segment(p: Point, a: Point, b: Point) -> bool {
    p.x>=a.x.min(b.x)-0.0001 && p.x<=a.x.max(b.x)+0.0001 && p.y>=a.y.min(b.y)-0.0001 && p.y<=a.y.max(b.y)+0.0001
}
fn planar_point_segment(p: Point, a: Point, b: Point) -> f64 {
    let x = b.x-a.x;
    let y = b.y-a.y;
    let l = x*x+y*y;
    if l<0.0001 {
        return p.distance2(a);
    }
    let t = (((p.x-a.x)*x+(p.y-a.y)*y)/l).clamp(0.0, 1.0);
    p.distance2(Point {
        x: a.x+x*t,
        y: a.y+y*t
    })
}
