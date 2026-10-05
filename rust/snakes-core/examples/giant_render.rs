// SPDX-License-Identifier: GPL-3.0-or-later
//! F2 geometry shared with simulation fixtures; Stage E can extend this bench.
use snakes_core::{*,ffi::*};
use std::time::Instant;
#[path="support/long_fixtures.rs"] mod long_fixtures;
fn main() {
    let n:usize=std::env::args().nth(1).map_or(6000,|s|s.parse().unwrap());
    let mut w=World::new(Config {width:7920.0,height:1440.0,scale:200.0,rules:RuleSet::V2,..Config::default()}).unwrap();
    let (points,angle)=long_fixtures::spiral(&w,0,n);w.diagnostic_body(0,&points,angle).unwrap();
    let body:Vec<_>=points.iter().map(|p|SegmentRecord{x:p.x as f32,y:p.y as f32,previous_x:p.x as f32,previous_y:p.y as f32}).collect();
    let s=SnakeRecord{id:0,generation:1,alive:1,radius:w.snake(0).unwrap().radius,angle,desired_angle:angle,segment_count:n as u32,..Default::default()};
    let info=FrameInfo{tick:600,simulation_time:20.0,world_width:7920.0,world_height:1440.0,..Default::default()};
    let params=RenderParams {viewport_width:3440.0,viewport_height:1440.0,offset_x:-2240.0,scale_x:1.0,scale_y:1.0,interpolation:1.0,presentation_time:20.0,deadly_walls:1,..Default::default()};
    let palette=[RenderColor{red:77,green:230,blue:255,alpha:255}];
    let mut renderer=RenderHandle::new();let mut output=vec![ShaderRenderVertex::default();n*18+2000];
    for _ in 0..100 {let result=renderer.build_shader(&info,&[s],&body,&[],&[],&palette,&params,&mut output);assert!(result.vertex_count<=output.len());}
    let mut samples=Vec::with_capacity(1000);let mut vertices=0;
    for _ in 0..1000 {let start=Instant::now();let result=renderer.build_shader(&info,&[s],&body,&[],&[],&palette,&params,&mut output);samples.push(start.elapsed().as_secs_f64()*1e6);assert!(result.vertex_count<=output.len());vertices=result.vertex_count;}
    let mean=samples.iter().sum::<f64>()/samples.len() as f64;samples.sort_unstable_by(f64::total_cmp);
    println!("length={n} viewport=3440x1440 mean_us={mean:.3} p99_us={:.3} vertices={vertices}",samples[990]);
}
