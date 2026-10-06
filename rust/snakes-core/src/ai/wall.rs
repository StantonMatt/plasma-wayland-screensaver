// SPDX-License-Identifier: GPL-3.0-or-later
//! Constant-cost wall reachability, independent of candidate intent/utility.
use super::*;

/// A complete left/right circle inside the inset arena is a constructive
/// indefinite wall exit. If neither fits, accept only a straight ray that
/// reaches the interior where BOTH circles fit, without crossing a wall.
/// Circles use exact rotate-then-move centres/radii; the arc cold path keeps
/// a one-tick travel bound for its continuous approximation. Phase never disables deadly walls.
#[inline]
pub(super) fn reachable(w:&World,p:Point,d:Point,radius:f64,travel:f64,body:f64)->bool {
    let cfg=w.config();
    if !cfg.deadly_walls {return true;}
    let physical_band=body*0.5+2.0;
    let band=physical_band+travel;
    let wall=p.x.min(cfg.width-p.x).min(p.y).min(cfg.height-p.y);
    if wall<physical_band {return false;}
    if wall>=2.0*radius+band {return true;}
    // Exact rotate-then-move circles. A continuous tangent circle's centre
    // drifts by fractions of a pixel per movement, so a padded tangent-circle
    // predicate is not invariant even while its real discrete turn stays safe.
    let half_step=travel/(2.0*radius.max(0.01));
    if half_step>=std::f64::consts::FRAC_PI_2 {return false;}
    let (sin,cos)=half_step.sin_cos();
    let circle=if sin>1e-12 {travel/(2.0*sin)} else {radius};
    let normal=circle*cos;let shift=travel*0.5;
    let reserve=circle+physical_band;
    for side in [-1.0,1.0] {
        let center=Point {x:p.x-shift*d.x-side*d.y*normal,y:p.y-shift*d.y+side*d.x*normal};
        if center.x>=reserve && center.x<=cfg.width-reserve
            && center.y>=reserve && center.y<=cfg.height-reserve {return true;}
    }
    if straight_exit(cfg.width,cfg.height,p,d,radius,band) {return true;}
    // A full circle is sufficient, but unnecessary for an inward arc at a
    // corner. Check each directed arc to the inward diagonal, including every
    // cardinal extremum, then the same straight exit to the interior. This
    // cold path has a fixed bound (two arcs/four extrema), never a search.
    let inward=Point {x:if p.x<cfg.width*0.5 {std::f64::consts::FRAC_1_SQRT_2} else {-std::f64::consts::FRAC_1_SQRT_2},
        y:if p.y<cfg.height*0.5 {std::f64::consts::FRAC_1_SQRT_2} else {-std::f64::consts::FRAC_1_SQRT_2}};
    let angle=d.y.atan2(d.x);let target=inward.y.atan2(inward.x);
    for side in [-1.0,1.0] {
        let sweep=(side*(target-angle)).rem_euclid(std::f64::consts::TAU);
        let center=Point {x:p.x-side*d.y*radius,y:p.y+side*d.x*radius};
        let end=Point {x:center.x+side*inward.y*radius,y:center.y-side*inward.x*radius};
        let inside=|q:Point|q.x>=band && q.x<=cfg.width-band && q.y>=band && q.y<=cfg.height-band;
        if !inside(end) {continue;}
        let mut clear=true;
        for (a,v) in [(0.0,Point{x:0.0,y:-1.0}),
            (std::f64::consts::FRAC_PI_2,Point{x:1.0,y:0.0}),
            (std::f64::consts::PI,Point{x:0.0,y:1.0}),
            (-std::f64::consts::FRAC_PI_2,Point{x:-1.0,y:0.0})] {
            if (side*(a-angle)).rem_euclid(std::f64::consts::TAU)<=sweep
                && !inside(Point{x:center.x+side*v.x*radius,y:center.y+side*v.y*radius}) {clear=false;break;}
        }
        if clear && straight_exit(cfg.width,cfg.height,end,inward,radius,band) {return true;}
    }
    false
}

#[inline]
fn straight_exit(width:f64,height:f64,p:Point,d:Point,radius:f64,band:f64)->bool {
    // Slab intersection with the convex interior rectangle proves the entire
    // straight exit, rather than accepting an inward-looking single step.
    let inset=2.0*radius+band;
    if width<2.0*inset || height<2.0*inset {return false;}
    let mut enter=0.0_f64;let mut exit=f64::INFINITY;
    for (position,direction,extent) in [(p.x,d.x,width),(p.y,d.y,height)] {
        if direction.abs()<1e-12 {
            if position<inset || position>extent-inset {return false;}
        } else {
            let a=(inset-position)/direction;let b=(extent-inset-position)/direction;
            enter=enter.max(a.min(b));exit=exit.min(a.max(b));
        }
    }
    enter<=exit
}

/// Recovery ordering when an observed pose has no admitted sampled exit.
/// Preserve the greatest coupled wall margin, rather than reversing an
/// inward turn to buy a longer body-safe prefix outside the reachable set.
#[inline]
pub(super) fn margin(w:&World,p:Point,d:Point,radius:f64,travel:f64,body:f64)->f64 {
    let half_step=travel/(2.0*radius.max(0.01));
    let (sin,cos)=half_step.sin_cos();
    let circle=if sin>1e-12 {travel/(2.0*sin)} else {radius};
    let normal=circle*cos;let shift=travel*0.5;
    let reserve=circle+body*0.5+2.0;let cfg=w.config();
    [-1.0,1.0].into_iter().map(|side| {
        let center=Point{x:p.x-shift*d.x-side*d.y*normal,y:p.y-shift*d.y+side*d.x*normal};
        center.x.min(cfg.width-center.x).min(center.y).min(cfg.height-center.y)-reserve
    }).fold(f64::NEG_INFINITY,f64::max)
}
