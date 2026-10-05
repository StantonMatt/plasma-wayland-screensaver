// SPDX-License-Identifier: GPL-3.0-or-later
//! Render-only fixture: real r18 / 21.24px spacing, all 6000 source points on screen.
use snakes_core::ffi::*;
pub fn fixture(shared: bool) -> (FrameInfo, Vec<SnakeRecord>, Vec<SegmentRecord>) {
    let origin = if shared { 2560.0 } else { 0.0 };
    let mut path: Vec<(f64, f64)> = Vec::new();
    // Inset rounded rectangles joined in the top-left corner form a compact
    // spiral. Dense construction samples are resampled by physical arc length.
    for lap in 0..20 {
        let inset = 25.0 + lap as f64 * 34.0;
        let (left, top, right, bottom) = (inset, inset, 3440.0-inset, 1440.0-inset);
        let radius = 48.0;
        for (cx,cy,start) in [(right-radius,top+radius,-std::f64::consts::FRAC_PI_2),
            (right-radius,bottom-radius,0.0),(left+radius,bottom-radius,std::f64::consts::FRAC_PI_2),
            (left+radius,top+radius,std::f64::consts::PI)] {
            for step in 0..=32 {
                let a=start+step as f64*std::f64::consts::FRAC_PI_2/32.0;
                path.push((cx+radius*a.cos(),cy+radius*a.sin()));
            }
        }
    }
    let mut body = Vec::with_capacity(6880);
    let mut remaining=0.0;
    for pair in path.windows(2) {
        let (mut x,mut y)=pair[0];
        let (end_x,end_y)=pair[1];
        let mut distance=(end_x-x).hypot(end_y-y);
        while remaining<=distance && body.len()<6000 {
            let t=remaining/distance;
            x+=(end_x-x)*t;y+=(end_y-y)*t;
            body.push(SegmentRecord {x:(origin+x) as f32,y:y as f32,previous_x:(origin+x) as f32,previous_y:y as f32});
            distance-=remaining;remaining=21.24;
        }
        remaining-=distance;
        if body.len()==6000 {break;}
    }
    assert_eq!(body.len(),6000);
    let angle=((body[0].y-body[1].y) as f64).atan2((body[0].x-body[1].x) as f64);
    let mut snakes=vec![SnakeRecord {id:0,generation:1,alive:1,radius:18.0,angle,desired_angle:angle,
        segment_count:6000,color_index:1,grudge_snake_id:u32::MAX,..Default::default()}];
    for id in 1..12 {
        let offset=body.len();
        let x=if shared { 850.0+((id-1)%4) as f64*1950.0 } else {850.0+((id-1)%4) as f64*780.0};
        let y=200.0+((id-1)/4) as f64*430.0;
        for j in 0..80 {
            let px=(x-j as f64*9.44) as f32;
            let py=(y+(j as f64*0.08).sin()*18.0) as f32;
            body.push(SegmentRecord{x:px,y:py,previous_x:px,previous_y:py});
        }
        snakes.push(SnakeRecord{id,generation:1,alive:1,radius:8.0,segment_offset:offset as u32,
            segment_count:80,color_index:id%6,grudge_snake_id:u32::MAX,..Default::default()});
    }
    (FrameInfo {tick:600,simulation_time:20.0,world_width:if shared {7920.0} else {3440.0},world_height:1440.0,
        geometry_generation:1,..Default::default()},snakes,body)
}
pub fn palette() -> [RenderColor;6] {
    [(217,251,255),(61,214,232),(58,134,255),(115,88,214),(42,168,137),(155,246,255)]
        .map(|(red,green,blue)|RenderColor{red,green,blue,alpha:255})
}
