// SPDX-License-Identifier: GPL-3.0-or-later
//! Deterministic compact rounded-rectangle spiral, head first. Equal arc
//! spacing and >=4.5r corner radius; this sets geometry, never AI policy.
use snakes_core::{Point,World};
pub fn spiral(w:&World,id:usize,n:usize)->(Vec<Point>,f64) {
    let c=w.config();let r=w.snake(id).unwrap().radius;let spacing=r*1.18;
    let pitch=r*2.6;let corner=r*4.5;
    // Wider windings are necessary for the largest seeded radii. Keep the
    // seed-1 geometry unchanged; scale area with radius squared for others.
    let width=(c.width-8.0*r).min(5600.0*(r/17.0).max(1.0).powi(2));let height=c.height-8.0*r;
    let mut left=(c.width-width)*0.5;let mut right=left+width;
    let mut top=(c.height-height)*0.5;let mut bottom=top+height;
    let mut dense=Vec::new();
    while right-left>corner*2.0+pitch && bottom-top>corner*2.0+pitch {
        // The top-edge start's reverse tangent points left, away from the body.
        dense.push(Point{x:right-corner-pitch,y:top});
        for (x,y,a0) in [(right-corner,top+corner,-std::f64::consts::FRAC_PI_2),
                          (right-corner,bottom-corner,0.0),
                          (left+corner,bottom-corner,std::f64::consts::FRAC_PI_2),
                          (left+corner,top+corner,std::f64::consts::PI)] {
            for j in 0..=64 {let a=a0+j as f64/64.0*std::f64::consts::FRAC_PI_2;
                dense.push(Point{x:x+corner*a.cos(),y:y+corner*a.sin()});}
        }
        // Join to the next winding along its top edge. No sharp reversal.
        left+=pitch;right-=pitch;top+=pitch;bottom-=pitch;
    }
    let mut points=Vec::with_capacity(n);let mut carry=0.0;
    if let Some(&first)=dense.first() {points.push(first);}
    for edge in dense.windows(2) {
        let mut a=edge[0];let b=edge[1];let mut distance=((a.x-b.x).powi(2)+(a.y-b.y).powi(2)).sqrt();
        while carry+distance>=spacing && points.len()<n {
            let step=spacing-carry;let t=step/distance;
            a=Point{x:a.x+(b.x-a.x)*t,y:a.y+(b.y-a.y)*t};points.push(a);
            distance-=step;carry=0.0;
        }
        if points.len()==n {break;}carry+=distance;
    }
    assert_eq!(points.len(),n,"spiral does not fit arena/radius");
    (points,std::f64::consts::PI)
}
