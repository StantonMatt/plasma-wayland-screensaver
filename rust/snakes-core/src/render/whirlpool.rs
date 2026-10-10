// SPDX-License-Identifier: GPL-3.0-or-later
//! One viewport-culled quad, seam copies only when visible. No retained heap.
use super::*;
use super::shader::SpriteSink;
impl Renderer {
    pub(super) fn shader_vortex(&self,v:&ItemRecord,info:&FrameInfo,p:&Params,palette:&[Color],sink:&mut SpriteSink<'_>) {
        if v.kind!=8 || !items::valid_item(v) {return;}
        let scale=(p.scale_x*p.scale_y).sqrt();let pos=P::new(v.x as f64,v.y as f64);let r=v.radius as f64*scale;
        let (xs,ys)=copies(pos,pos,P::new(r/p.scale_x,r/p.scale_y),P::new(info.world_width,info.world_height),p.deadly_walls!=0);
        let extra=if self.reduced_motion {0.0} else {((p.presentation_time-info.simulation_time)/crate::STEP_SECONDS).max(0.0)};
        let charge=((v.charge_ticks as f64+extra)*255.0/150.0).clamp(0.0,255.0) as u8;
        // Core radius follows sqrt(V); V=25 (the 46-shard cap) fills the byte.
        let value=((v.captured_value.max(0.0) as f64).sqrt()*255.0/5.0).clamp(0.0,255.0) as u8;
        for x in xs.first..=xs.last {for y in ys.first..=ys.last {
            let q=pos+P::new(x as f64*xs.extent,y as f64*ys.extent);
            sink.sprite(P::new(q.x*p.scale_x+p.offset_x,q.y*p.scale_y+p.offset_y),r,items::accent(7,palette),[19,0,value,charge]);
        }}
    }
    pub(super) fn classic_vortex(&self,v:&ItemRecord,info:&FrameInfo,p:&Params,palette:&[Color],sink:&mut Sink<'_>) {
        if v.kind!=8 || !items::valid_item(v) {return;}
        let scale=(p.scale_x*p.scale_y).sqrt();let pos=P::new(v.x as f64,v.y as f64);let r=v.radius as f64*scale;
        let width=(r*0.003).max(0.5);let extent=r+width;
        let (xs,ys)=copies(pos,pos,P::new(extent/p.scale_x,extent/p.scale_y),P::new(info.world_width,info.world_height),p.deadly_walls!=0);
        let c=items::accent(7,palette).fade(0.32);let t=if self.reduced_motion {0.0} else {-p.presentation_time*0.9};
        for x in xs.first..=xs.last {for y in ys.first..=ys.last {
            let q=pos+P::new(x as f64*xs.extent,y as f64*ys.extent);let q=P::new(q.x*p.scale_x+p.offset_x,q.y*p.scale_y+p.offset_y);
            if !visible(q.x-extent,q.y-extent,2.0*extent,2.0*extent,sink.view) {continue;}
            for arm in 0..3 {for k in 0..24 {
                let point=|k:usize| {let radius=(0.05+0.95*k as f64/24.0)*r;let a=t+arm as f64*std::f64::consts::TAU/3.0-2.0*(radius/r).ln();q+P::new(a.cos(),a.sin())*radius};
                sink.segment(point(k),point(k+1),width,c);
            }}
        }}
    }
}
