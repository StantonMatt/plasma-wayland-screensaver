// SPDX-License-Identifier: GPL-3.0-or-later
//! Item sprites and classic fallback. Shader capsules use exactly one quad.
use super::*;
use super::shader::SpriteSink;
pub(crate) fn valid_item(item: &ItemRecord) -> bool {
    coordinate32(item.x) && coordinate32(item.y)
        && (1..=5).contains(&item.kind) && item.life_ticks<=780 && item.reserved==0
        && item.reserved_byte==0
}
pub(super) fn accent(kind:u8,palette:&[Color])->Color {
    let c=match kind {1=>Color::new(255,225,77,255),2=>Color::new(255,95,210,255),
        3=>Color::new(169,139,255,255),4=>Color::new(157,255,58,255),_=>Color::new(189,243,255,255)};
    tint(c,palette)
}
pub(super) fn tint(mut c:Color,palette:&[Color])->Color {
    let mono=palette.first().is_some_and(|c|c.red==255 && c.green==255 && c.blue==255);
    let pastel=palette.first().is_some_and(|c|c.red==255 && c.green==200 && c.blue==221);
    let grey=c.red as f64*0.2126+c.green as f64*0.7152+c.blue as f64*0.0722;
    for v in [&mut c.red,&mut c.green,&mut c.blue] {
        if mono {*v=(*v as f64*0.12+grey*0.88).round() as u8;}
        else if pastel {*v=(*v as f64*0.7+255.0*0.3).round() as u8;}
    }
    c
}

impl Renderer {
    pub fn set_items(&mut self,items:&[ItemRecord],radius:f64) {
        self.item_count=items.len().min(crate::MAX_ITEMS);
        self.item_radius=radius;
        self.items[..self.item_count].copy_from_slice(&items[..self.item_count]);
    }
    pub(super) fn shader_items(&self,info:&FrameInfo,p:&Params,palette:&[Color],sink:&mut SpriteSink<'_>) {
        let scale=(p.scale_x*p.scale_y).sqrt();let arena=P::new(info.world_width,info.world_height);
        let map=|v:P|P::new(v.x*p.scale_x+p.offset_x,v.y*p.scale_y+p.offset_y);
        for item in &self.items[..self.item_count] {
            if !valid_item(item) {continue;}
            let pos=P::new(item.x as f64,item.y as f64);let extent=self.item_radius*super::shader::bounds::CAPSULE;
            let (xs,ys)=copies(pos,pos,P::new(extent*scale/p.scale_x,extent*scale/p.scale_y),arena,p.deadly_walls!=0);
            let extra=if self.reduced_motion {0.0} else {((p.presentation_time-info.simulation_time)/crate::STEP_SECONDS).max(0.0)};
            // High birth-byte range is reserved for the 30-tick incoming state.
            let age=if item.landing_ticks>0 {128+((1.0-(item.landing_ticks as f64-extra).clamp(0.0,30.0)/30.0)*127.0).round() as u8}
                else {((item.age_ticks as f64+extra).clamp(0.0,15.0)/15.0*127.0).round() as u8};
            let life=(item.life_ticks as f64/750.0*255.0).round() as u8;
            for x in xs.first..=xs.last {for y in ys.first..=ys.last {
                sink.sprite(map(pos+P::new(x as f64*xs.extent,y as f64*ys.extent)),extent*scale,
                    accent(item.kind,palette),[11,item.kind,age,life]);
            }}
        }
    }
    pub(super) fn classic_items(&self,info:&FrameInfo,p:&Params,palette:&[Color],sink:&mut Sink<'_>) {
        let scale=(p.scale_x*p.scale_y).sqrt();let arena=P::new(info.world_width,info.world_height);
        let map=|v:P|P::new(v.x*p.scale_x+p.offset_x,v.y*p.scale_y+p.offset_y);
        let time=if self.reduced_motion {0.0} else {p.presentation_time};
        for item in &self.items[..self.item_count] {
            if !valid_item(item) {continue;}
            let pos=P::new(item.x as f64,item.y as f64);let r=self.item_radius*scale;
            let fade=if item.life_ticks<90 {(0.65+0.35*(time*(18.0+24.0*(1.0-item.life_ticks as f64/90.0))).sin()).max(0.1)} else {1.0};
            let c=accent(item.kind,palette).fade(fade);
            // Enclose all emitted geometry, including pixel-minimum hex strokes.
            let extent=(r*3.4).max(r+(r*0.055).max(0.5))
                .max(r*0.78+(r*0.025).max(0.4));
            let (xs,ys)=copies(pos,pos,P::new(extent/p.scale_x,extent/p.scale_y),arena,p.deadly_walls!=0);
            for x in xs.first..=xs.last {for y in ys.first..=ys.last {
                let center=map(pos+P::new(x as f64*xs.extent,y as f64*ys.extent));
                if item.landing_ticks>0 {
                    let progress=1.0-item.landing_ticks.min(30) as f64/30.0;
                    self.classic_ring(center,r*(3.0-2.0*progress),r*0.05,c.fade(0.55),sink);
                    for k in [0,2,4] {
                        let a=k as f64*std::f64::consts::TAU/6.0;
                        let b=a+std::f64::consts::TAU/6.0;
                        sink.segment(center+P::new(a.cos(),a.sin())*r,center+P::new(b.cos(),b.sin())*r,(r*0.055).max(0.5),c.fade(0.35));
                    }
                    self.classic_icon(center,r*0.52,item.kind,c.fade(0.22),sink);
                    continue;
                }
                sink.disc(center,r*3.0,c.alpha((20.0*fade) as u8),12);
                sink.disc(center,r,Color::new(8,11,22,(245.0*fade) as u8),6);
                let rotation=time*0.18;
                for k in 0..6 {
                    let a=rotation+k as f64*std::f64::consts::TAU/6.0;
                    let b=a+std::f64::consts::TAU/6.0;
                    let v=P::new(a.cos(),a.sin());let w=P::new(b.cos(),b.sin());
                    sink.segment(center+v*r,center+w*r,(r*0.055).max(0.5),c);
                    sink.segment(center+v*(r*0.78),center+w*(r*0.78),(r*0.025).max(0.4),c.fade(0.35));
                }
                let spark=time*1.8;
                sink.disc(center+P::new(spark.cos(),spark.sin())*(r*1.25),r*0.12,c,6);
                self.classic_icon(center,r*0.52,item.kind,c,sink);
                if item.age_ticks<15 {self.classic_ring(center,r*(3.0-2.0*item.age_ticks as f64/15.0),r*0.05,c.fade(1.0-item.age_ticks as f64/15.0),sink);}
            }}
        }
    }
    pub(super) fn classic_effects(&self,info:&FrameInfo,p:&Params,palette:&[Color],sink:&mut Sink<'_>,budget:&mut EffectBudget) {
        let scale=(p.scale_x*p.scale_y).sqrt();let arena=P::new(info.world_width,info.world_height);
        let map=|v:P|P::new(v.x*p.scale_x+p.offset_x,v.y*p.scale_y+p.offset_y);
        // Walk the retained ring newest first, sharing a visible-copy budget
        // with warnings and Magnet. Offscreen events consume no capacity.
        for age_index in 0..self.effects.len() {
            let e=self.effects[(self.effect_head+self.effects.len()-1-age_index)%self.effects.len()];
            if !e.active || !matches!(e.kind,6|12|13) || budget.full() {continue;}
            let age=(p.presentation_time-e.time)/0.5;
            if !(0.0..1.0).contains(&age) {continue;}
            if e.kind==6 {
                // Preserve the classic death flash geometry, but share history
                // order and the visible-copy cap with the other transients.
                let radius=(8.0+age*24.0)*scale;
                let c=palette.get(if palette.is_empty() {0} else {e.color as usize%palette.len()})
                    .copied().unwrap_or(Color::new(0,255,255,255));
                let (xs,ys)=copies(e.p,e.p,P::new(radius/p.scale_x,radius/p.scale_y),arena,p.deadly_walls!=0);
                for x in xs.first..=xs.last {for y in ys.first..=ys.last {
                    if budget.full() {continue;}
                    let center=map(e.p+P::new(x as f64*xs.extent,y as f64*ys.extent));
                    let before=sink.count;
                    sink.disc(center,radius,c.alpha((140.0*(1.0-age)).round() as u8),12);
                    sink.disc(center,radius*0.45,Color::new(255,255,255,(230.0*(1.0-age)).round() as u8),12);
                    budget.emitted(before,sink.count);
                }}
                continue;
            }
            let c=accent(e.color as u8,palette).fade((1.0-age)*0.8);
            let radius=e.radius*scale*(if e.kind==12 {1.0+5.0*age} else {2.5*(1.0-age)});
            let width=(e.radius*scale*0.10).max(0.5);
            let extent=radius+width;
            let (xs,ys)=copies(e.p,e.p,P::new(extent/p.scale_x,extent/p.scale_y),arena,p.deadly_walls!=0);
            for x in xs.first..=xs.last {for y in ys.first..=ys.last {
                if budget.full() {continue;}
                let center=map(e.p+P::new(x as f64*xs.extent,y as f64*ys.extent));
                let before=sink.count;
                self.classic_ring(center,radius,width,c,sink);
                if e.kind==12 {self.classic_ring(center,radius*0.65,(e.radius*scale*0.06).max(0.4),Color::new(255,255,255,255).fade((1.0-age)*0.85),sink);}
                budget.emitted(before,sink.count);
            }}
        }
    }
    pub(super) fn classic_warnings(&self,info:&FrameInfo,p:&Params,palette:&[Color],snakes:&[SnakeRecord],segments:&[SegmentRecord],sink:&mut Sink<'_>,budget:&mut EffectBudget) {
        let scale=(p.scale_x*p.scale_y).sqrt();let arena=P::new(info.world_width,info.world_height);
        for s in snakes {
            if budget.full() || !snake_valid(s) || s.alive==0 || s.segment_count==0 || s.effect_kind==0
                || s.effect_ticks==0 || s.effect_ticks>crate::effects::WARNING_TICKS {continue;}
            let head=position(&segments[s.segment_offset as usize],moving(s),info,p);let radius=s.radius*2.0*scale;
            let width=(s.radius*scale*0.1).max(0.5);let extent=radius+width;
            let (xs,ys)=copies(head,head,P::new(extent/p.scale_x,extent/p.scale_y),arena,p.deadly_walls!=0);
            let fade=if self.reduced_motion {0.5} else {0.5+0.5*(p.presentation_time*22.0).sin()};
            for x in xs.first..=xs.last {for y in ys.first..=ys.last {
                if budget.full() {continue;}
                let pos=head+P::new(x as f64*xs.extent,y as f64*ys.extent);
                let center=P::new(pos.x*p.scale_x+p.offset_x,pos.y*p.scale_y+p.offset_y);
                if !visible(center.x-extent,center.y-extent,2.0*extent,2.0*extent,sink.view) {continue;}
                let before=sink.count;
                self.classic_ring(center,radius,width,accent(s.effect_kind,palette).fade(fade*0.8),sink);
                budget.emitted(before,sink.count);
            }}
        }
        let time=if self.reduced_motion {0.0} else {p.presentation_time};
        for s in snakes {
            if budget.full() || !snake_valid(s) || s.alive==0 || s.segment_count==0 || s.effect_kind!=2 || s.effect_ticks==0 {continue;}
            let head=position(&segments[s.segment_offset as usize],moving(s),info,p);
            let r=s.radius*scale;let width=(r*0.08).max(0.5);
            let extent=(r*10.1).max(r*9.0+width);
            let (xs,ys)=copies(head,head,P::new(extent/p.scale_x,extent/p.scale_y),arena,p.deadly_walls!=0);
            let c=accent(2,palette);
            for x in xs.first..=xs.last {for y in ys.first..=ys.last {
                if budget.full() {continue;}
                let v=head+P::new(x as f64*xs.extent,y as f64*ys.extent);
                let center=P::new(v.x*p.scale_x+p.offset_x,v.y*p.scale_y+p.offset_y);
                if !visible(center.x-extent,center.y-extent,2.0*extent,2.0*extent,sink.view) {continue;}
                let before=sink.count;
                for k in 0..18 {
                    let a=time*1.2+k as f64*std::f64::consts::TAU/18.0;
                    let b=a+std::f64::consts::TAU/72.0;
                    sink.segment(center+P::new(a.cos(),a.sin())*(r*9.0),center+P::new(b.cos(),b.sin())*(r*9.0),width,c.fade(0.4));
                }
                for k in 0..3 {
                    let a=time*1.2+k as f64*std::f64::consts::TAU/3.0;
                    let spark=center+P::new(a.cos(),a.sin())*(r*9.0);
                    sink.disc(spark,r*1.1,c.alpha(45),8);
                    sink.disc(spark,r*0.2,c,6);
                }
                budget.emitted(before,sink.count);
            }}
        }
    }
    fn classic_ring(&self,center:P,r:f64,width:f64,c:Color,sink:&mut Sink<'_>) {
        for k in 0..12 {sink.segment(center+self.circles[12][k]*r,center+self.circles[12][k+1]*r,width,c);}
    }
    fn classic_icon(&self,center:P,r:f64,kind:u8,c:Color,sink:&mut Sink<'_>) {
        let line=|sink:&mut Sink<'_>,a:P,b:P|sink.segment(center+a*r,center+b*r,r*0.14,c);
        match kind {
            1=>{let points=[P::new(0.3,-1.0),P::new(-0.7,0.1),P::new(0.15,0.1),P::new(-0.3,1.0)];for v in points.windows(2) {line(sink,v[0],v[1]);}},
            2=>{for k in 0..6 {let a=k as f64*std::f64::consts::PI/6.0;let b=(k+1) as f64*std::f64::consts::PI/6.0;line(sink,P::new(a.cos()*0.7,a.sin()*0.8),P::new(b.cos()*0.7,b.sin()*0.8));}line(sink,P::new(-0.7,0.0),P::new(-0.7,-0.75));line(sink,P::new(0.7,0.0),P::new(0.7,-0.75));},
            3=>{for x in [-0.35,0.35] {let v=[P::new(x,-0.9),P::new(x+0.35,0.0),P::new(x,0.9),P::new(x-0.35,0.0)];for k in 0..4 {line(sink,v[k],v[(k+1)%4]);}}},
            4=>{for x in [-0.5,0.5] {line(sink,P::new(x,-0.8),P::new(x*0.4,0.9));}},
            _=>{for k in 0..6 {let a=k as f64*std::f64::consts::TAU/6.0;line(sink,P::default(),P::new(a.cos(),a.sin()));}},
        }
    }
}
