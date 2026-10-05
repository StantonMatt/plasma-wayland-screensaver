// SPDX-License-Identifier: GPL-3.0-or-later
//! Stage E C-ABI benchmark. Run pinned through heavy; `reverse` alternates order.
use snakes_core::ffi::*;
use std::{mem::size_of, time::Instant};
#[path="../tests/support/giant_render_fixture.rs"] mod giant_render_fixture;
fn main() {
    let reverse=std::env::args().any(|a|a=="reverse");
    if std::env::args().any(|a|a=="orphan") {orphan(reverse);return;}
    let mut cases=vec![(false,"single",0.0,0.0,3440.0,1440.0),
        (true,"left",0.0,0.0,2560.0,1440.0),(true,"middle",2560.0,0.0,3440.0,1440.0),
        (true,"right",6000.0,195.0,1920.0,1080.0)];
    if reverse {cases.reverse();}
    for (shared,name,x,y,width,height) in cases {
        let (mut info,snakes,body)=giant_render_fixture::fixture(shared);
        let visible=body[..6000].iter().filter(|b|b.x as f64>=x && (b.x as f64)<x+width && b.y as f64>=y && (b.y as f64)<y+height).count();
        let mut p=RenderParams {viewport_width:width,viewport_height:height,scale_x:1.0,scale_y:1.0,
            offset_x:-x,offset_y:-y,interpolation:1.0,presentation_time:20.0,deadly_walls:1,..Default::default()};
        let pal=giant_render_fixture::palette();
        let mut paths=[true,false];if reverse {paths.reverse();}
        for shader in paths {
            let mut r=RenderHandle::new();
            let mut classic=vec![RenderVertex::default();500000];
            let mut gpu=vec![ShaderRenderVertex::default();150000];
            let mut out=RenderOutput::default();
            let mut samples=Vec::with_capacity(2000);
            for iteration in 0..2100 {
                info.tick+=1;info.simulation_time+=1.0/30.0;p.presentation_time=info.simulation_time;
                let start=Instant::now();
                let status=unsafe {if shader {
                    snakes_core_render_build_shader(&mut r,&info,snakes.as_ptr(),snakes.len(),body.as_ptr(),body.len(),
                        std::ptr::null(),0,std::ptr::null(),0,pal.as_ptr(),pal.len(),&p,gpu.as_mut_ptr(),gpu.len(),&mut out)
                } else {
                    snakes_core_render_build(&mut r,&info,snakes.as_ptr(),snakes.len(),body.as_ptr(),body.len(),
                        std::ptr::null(),0,std::ptr::null(),0,pal.as_ptr(),pal.len(),&p,classic.as_mut_ptr(),classic.len(),&mut out)
                }};
                let elapsed=start.elapsed().as_secs_f64()*1000.0;
                assert_eq!(status,OK);if iteration>=100 {samples.push(elapsed);}
            }
            let mean=samples.iter().sum::<f64>()/samples.len() as f64;samples.sort_unstable_by(f64::total_cmp);
            let stride=if shader {size_of::<ShaderRenderVertex>()} else {size_of::<RenderVertex>()};
            println!("case={name} path={} visible_giant={visible} total_segments={} mean_ms={mean:.6} p99_ms={:.6} vertices={} bytes={} capacity_bytes={}",
                if shader {"shader"} else {"classic"},body.len(),samples[1980],out.vertex_count,out.vertex_count*stride,
                if shader {gpu.len()*stride} else {classic.len()*stride});
        }
    }
}

fn orphan(reverse:bool) {
    let (base,mut snakes,body)=giant_render_fixture::fixture(false);snakes.truncate(1);
    let pal=giant_render_fixture::palette();
    let p=RenderParams{viewport_width:3440.0,viewport_height:1440.0,scale_x:1.0,scale_y:1.0,
        interpolation:1.0,presentation_time:20.0,deadly_walls:1,..Default::default()};
    let mut paths=[true,false];if reverse {paths.reverse();}
    for shader in paths {
        let mut r=RenderHandle::new();r.reduced_motion=true;
        let mut gpu=vec![ShaderRenderVertex::default();150000];let mut classic=vec![RenderVertex::default();500000];
        let mut fresh=Vec::with_capacity(1000);let mut cached=Vec::with_capacity(1000);
        let mut count=0;
        for iteration in 0..1000 {
            r.reset();let mut info=base;snakes[0].segment_count=6000;snakes[0].alive=1;
            if shader {r.build_shader(&info,&snakes,&body,&[],&[],&pal,&p,&mut gpu);}
            else {r.build(&info,&snakes,&body,&[],&[],&pal,&p,&mut classic);}
            info.tick+=1;info.simulation_time+=1.0/30.0;snakes[0].segment_count=3001;snakes[0].alive=0;
            let event=EventRecord{tick:info.tick,kind:1,snake_id:0,generation:1,cut_index:3001,duration_ticks:33,
                release_tick:info.tick+33,..Default::default()};
            for cache in [false,true] {
                let start=Instant::now();
                count=if shader {r.build_shader(&info,&snakes,&body,&[],&[event],&pal,&p,&mut gpu).vertex_count}
                    else {r.build(&info,&snakes,&body,&[],&[event],&pal,&p,&mut classic).vertex_count};
                let elapsed=start.elapsed().as_secs_f64()*1000.0;
                if iteration>=100 {if cache {cached.push(elapsed);} else {fresh.push(elapsed);}}
            }
        }
        for (name,mut samples) in [("fresh",fresh),("cached",cached)] {
            let mean=samples.iter().sum::<f64>()/samples.len() as f64;samples.sort_unstable_by(f64::total_cmp);
            println!("case=orphan-{name} path={} mean_ms={mean:.6} p99_ms={:.6} vertices={count}",
                if shader {"shader"} else {"classic"},samples[samples.len()*99/100]);
        }
    }
}
