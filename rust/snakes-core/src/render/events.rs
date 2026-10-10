// SPDX-License-Identifier: GPL-3.0-or-later
//! Event sprites use the shared material, wrap/cull rules and fixed race budget.
use super::*;
use super::shader::SpriteSink;
pub(super) fn dim(c:Color,ambient:f32)->Color {
    if ambient>=1.0 || ambient<=0.0 || !ambient.is_finite() {return c;}
    let a=ambient.clamp(0.28,1.0) as f64;
    Color::new((c.red as f64*a).round() as u8,(c.green as f64*a).round() as u8,(c.blue as f64*a).round() as u8,c.alpha)
}
pub(super) fn gold(palette:&[Color])->Color {items::tint(Color::new(255,222,86,255),palette)}
fn extra(info:&FrameInfo,p:&Params)->f64 {(p.presentation_time-info.simulation_time).max(0.0)*30.0}
fn meteor(f:&FoodRecord,info:&FrameInfo,p:&Params,calm:bool)->(P,P,f64) {
    let landing=P::new(f.attraction_x as f64,f.attraction_y as f64);
    let origin=P::new(f.motion_origin_x as f64,f.motion_origin_y as f64);
    // The origin is deliberately unwrapped relative to this exact landing.
    // Preserve the full diagonal even if it exceeds half a tiny wrapped world.
    let d=landing-origin;
    let q=((14-f.motion_ticks.min(14)) as f64+extra(info,p)).min(14.0)/14.0;
    (if calm {landing} else {origin+d*q},d,if calm {(q/0.6).min(1.0)} else {1.0})
}
impl Renderer {
    pub(super) fn shader_season_bats(&self,info:&FrameInfo,p:&Params,sink:&mut SpriteSink<'_>) {
        let night=info.world_event.night as f64;
        if self.season!=1 || self.reduced_motion || !night.is_finite() || night<=0.05 {return;}
        let u=((night-0.4)/0.5).clamp(0.0,1.0);
        let night_fade=u*u*(3.0-2.0*u);
        if night_fade<=0.0 {return;}
        let interval=(900.0/(info.world_width*info.world_height/1.0e6)).round().clamp(60.0,300.0) as u64;
        let slot=info.tick/interval;
        let radius=if self.item_radius>0.0 {self.item_radius/2.1} else {18.0};
        let scale=(p.scale_x*p.scale_y).sqrt();
        let arena=P::new(info.world_width,info.world_height);
        for previous in 0..3 {
            let Some(k)=slot.checked_sub(previous) else {continue;};
            let ticks=info.tick-k*interval;
            if ticks>=180 {continue;}
            let age=ticks as f64*crate::STEP_SECONDS;
            let fade=(age/0.5).min(1.0)*((6.0-age)/0.5).min(1.0)*night_fade;
            if fade<=0.0 {continue;}
            // Stateless SplitMix64; never consume simulation random state.
            let mut state=k;
            let h:[f64;7]=std::array::from_fn(|_| {
                state=state.wrapping_add(0x9e3779b97f4a7c15);
                let mut z=state;
                z=(z^(z>>30)).wrapping_mul(0xbf58476d1ce4e5b9);
                z=(z^(z>>27)).wrapping_mul(0x94d049bb133111eb);
                ((z^(z>>31))>>11) as f64*(1.0/9007199254740992.0)
            });
            let angle=(h[3]-0.5)*0.5;
            let direction=P::new(angle.cos()*if h[2]<0.5 {-1.0} else {1.0},angle.sin());
            let side=P::new(-direction.y,direction.x);
            let phase=std::f64::consts::TAU*h[5];
            let pos=P::new(h[0]*arena.x,(0.12+0.76*h[1])*arena.y)
                +direction*((6.0+3.0*h[4])*radius*(age-3.0))
                +side*(1.6*radius*(1.9*age+phase).sin());
            let extent=(1.8+0.4*h[6])*radius*scale;
            let (xs,ys)=copies(pos,pos,P::new(extent/p.scale_x,extent/p.scale_y),arena,p.deadly_walls!=0);
            for x in xs.first..=xs.last {for y in ys.first..=ys.last {
                let v=pos+P::new(x as f64*xs.extent,y as f64*ys.extent);
                let center=P::new(v.x*p.scale_x+p.offset_x,v.y*p.scale_y+p.offset_y);
                let [cx,cy,cw,ch]=self.clock_rect;
                if cw>0.0 && ch>0.0 && center.x+extent>=cx && center.x-extent<=cx+cw
                    && center.y+extent>=cy && center.y-extent<=cy+ch {continue;}
                sink.sprite(center,extent,Color::new(255,255,255,255).fade(fade),[25,1,(h[5]*255.0) as u8,0]);
            }}
        }
    }
    pub(super) fn shader_world_event(&self,info:&FrameInfo,p:&Params,palette:&[Color],sink:&mut SpriteSink<'_>) {
        let e=info.world_event;
        if e.kind!=1 || e.phase==0 || !coordinate32(e.x) || !coordinate32(e.y) || !(0.0..=info.world_width.hypot(info.world_height)).contains(&(e.radius as f64)) || e.radius==0.0 {return;}
        let pos=P::new(e.x as f64,e.y as f64);let radius=e.radius as f64;
        let scale=(p.scale_x*p.scale_y).sqrt();let extent=radius*1.2;
        let arena=P::new(info.world_width,info.world_height);
        let age=info.tick.saturating_sub(e.start_tick) as f64+extra(info,p);
        let fade=if e.phase==1 {1.0} else {(1.0-(age-45.0).max(0.0)/74.0).max(0.0)};
        let (xs,ys)=copies(pos,pos,P::new(extent*scale/p.scale_x,extent*scale/p.scale_y),arena,p.deadly_walls!=0);
        for x in xs.first..=xs.last {for y in ys.first..=ys.last {
            let v=pos+P::new(x as f64*xs.extent,y as f64*ys.extent);
            sink.effect_sprite(P::new(v.x*p.scale_x+p.offset_x,v.y*p.scale_y+p.offset_y),extent*scale,1.2,gold(palette).fade(fade),[20,0,0,0]);
        }}
    }
    #[inline(always)]
    pub(super) fn shader_event_food(&self,f:&FoodRecord,info:&FrameInfo,p:&Params,scale:f64,c:Color,sink:&mut SpriteSink<'_>)->bool {
        if !matches!(f.kind,5|6) {return false;}
        if f.kind==5 && (!coordinate32(f.motion_origin_x) || !coordinate32(f.motion_origin_y)) {return true;}
        let size=f.size as f64*scale;
        let walls=p.deadly_walls!=0;let arena=P::new(info.world_width,info.world_height);
        let map=|v:P|P::new(v.x*p.scale_x+p.offset_x,v.y*p.scale_y+p.offset_y);
        let (pos,d,fade)=if f.kind==5 {meteor(f,info,p,self.reduced_motion)} else {(P::new(f.x as f64,f.y as f64),P::default(),1.0)};
        // Validated f32 coordinates and bounded projection make squared
        // lengths safe. Normalize once and share it with the streak quad.
        let screen_d=P::new(d.x*p.scale_x,d.y*p.scale_y);
        let length_squared=screen_d.x*screen_d.x+screen_d.y*screen_d.y;
        let flying=!self.reduced_motion && f.kind==5 && length_squared>1.0e-6;
        let direction=if flying {screen_d*(1.0/length_squared.sqrt())} else {P::default()};
        let streak=direction*(size*12.0);
        let extent=size*4.6;
        let birth=if f.kind==6 && f.ripe_tick!=0 {((info.tick.saturating_sub(f.ripe_tick) as f64+extra(info,p))*255.0/if self.reduced_motion {18.0} else {30.0}).min(255.0) as u8} else {255};
        let phase=if f.kind==6 {
            let phase=f.phase as f64/std::f64::consts::TAU;
            (if (0.0..1.0).contains(&phase) {phase} else {phase.rem_euclid(1.0)}*255.0) as u8
        } else {1};
        let head_color=if fade==1.0 {c} else {c.fade(fade)};
        let emit=|center:P,sink:&mut SpriteSink<'_>| {
            if flying {sink.event_streak(center,streak,direction,size,c);}
            sink.sprite(center,extent,head_color,[if f.kind==5 {22} else {21},phase,f.life_fraction,birth]);
        };
        if walls {emit(map(pos),sink);return true;}
        let end=pos-P::new(streak.x/p.scale_x,streak.y/p.scale_y);
        let (xs,ys)=copies(P::new(pos.x.min(end.x),pos.y.min(end.y)),P::new(pos.x.max(end.x),pos.y.max(end.y)),P::new(extent/p.scale_x,extent/p.scale_y),arena,false);
        for x in xs.first..=xs.last {for y in ys.first..=ys.last {
            let center=map(pos+P::new(x as f64*xs.extent,y as f64*ys.extent));
            emit(center,sink);
        }}true
    }
    pub(super) fn shader_event_races(&self,info:&FrameInfo,p:&Params,palette:&[Color],snakes:&[SnakeRecord],segments:&[SegmentRecord],sink:&mut SpriteSink<'_>) {
        let e=info.world_event;if e.kind!=1 || e.phase==0 || e.radius<=0.0 {return;}
        let mut item=ItemRecord {x:e.x,y:e.y,kind:1,life_ticks:750,leader_snake_id:u32::MAX,
            contender_ids:[u32::MAX;2],contender_etas:[f32::INFINITY;2],..Default::default()};
        for s in snakes.iter().filter(|s|s.face_flags&8!=0 && s.alive!=0 && snake_valid(s)) {
            if s.face_flags&64!=0 {item.leader_snake_id=s.id;}
            if s.face_flags&48==0 {continue;}
            let slot=usize::from(s.face_flags&32!=0);item.contender_ids[slot]=s.id;
            // Synthetic ETAs encode only the authoritative contested bit.
            // Leader membership is independent of the nearest pair.
            item.contender_etas[slot]=if s.face_flags&128!=0 {1.0} else {1.0+slot as f32};
        }
        item.contender_count=item.contender_ids.iter().filter(|&&id|id!=u32::MAX).count() as u8;
        self.shader_race_items(info,p,palette,snakes,segments,sink,&[item],e.radius as f64/1.62,2);
    }
    pub(super) fn classic_world_event(&self,info:&FrameInfo,p:&Params,palette:&[Color],sink:&mut Sink<'_>) {
        let e=info.world_event;if e.kind!=1 || e.phase==0 || !(0.0..=info.world_width.hypot(info.world_height)).contains(&(e.radius as f64)) || e.radius==0.0 {return;}
        let pos=P::new(e.x as f64,e.y as f64);let r=e.radius as f64*(p.scale_x*p.scale_y).sqrt();
        let extent=(r*1.2).max(r+(r*0.012).max(0.5));
        let (xs,ys)=copies(pos,pos,P::new(extent/p.scale_x,extent/p.scale_y),P::new(info.world_width,info.world_height),p.deadly_walls!=0);
        let age=info.tick.saturating_sub(e.start_tick) as f64+extra(info,p);
        let fade=if e.phase==1 {1.0} else {(1.0-(age-45.0).max(0.0)/74.0).max(0.0)};
        let t=if self.reduced_motion {0.0} else {p.presentation_time*0.12};
        for x in xs.first..=xs.last {for y in ys.first..=ys.last {
            let v=pos+P::new(x as f64*xs.extent,y as f64*ys.extent);
            let center=P::new(v.x*p.scale_x+p.offset_x,v.y*p.scale_y+p.offset_y);
            for k in 0..24 {let a=t+k as f64*std::f64::consts::TAU/24.0;let b=a+0.15;
                sink.segment(center+P::new(a.cos(),a.sin())*r,center+P::new(b.cos(),b.sin())*r,(r*0.012).max(0.5),gold(palette).fade(0.65*fade));}
        }}
    }
    pub(super) fn classic_event_food(&self,f:&FoodRecord,info:&FrameInfo,p:&Params,palette:&[Color],sink:&mut Sink<'_>)->bool {
        if !matches!(f.kind,5|6) {return false;}
        let (pos,d,fade)=if f.kind==5 {meteor(f,info,p,self.reduced_motion)} else {(P::new(f.x as f64,f.y as f64),P::default(),1.0)};
        let size=f.size as f64*(p.scale_x*p.scale_y).sqrt();let extent=size*4.6;
        let projected=P::new(d.x*p.scale_x,d.y*p.scale_y);
        let tail=if !self.reduced_motion && projected.length()>0.001 {projected*(size*12.0/projected.length())} else {P::default()};
        let end=pos-P::new(tail.x/p.scale_x,tail.y/p.scale_y);let (xs,ys)=copies(P::new(pos.x.min(end.x),pos.y.min(end.y)),P::new(pos.x.max(end.x),pos.y.max(end.y)),P::new(extent/p.scale_x,extent/p.scale_y),P::new(info.world_width,info.world_height),p.deadly_walls!=0);
        for x in xs.first..=xs.last {for y in ys.first..=ys.last {
            let v=pos+P::new(x as f64*xs.extent,y as f64*ys.extent);let c=P::new(v.x*p.scale_x+p.offset_x,v.y*p.scale_y+p.offset_y);
            if tail.length()>0.0 {sink.segment(c,c-tail,size*0.45,gold(palette).fade(0.5));}
            sink.disc(c,size*3.5,gold(palette).fade(0.16*fade*(1.0+0.6*info.world_event.night as f64)),8);
            sink.segment(c-P::new(size,0.0),c+P::new(size,0.0),size*0.2,gold(palette).fade(fade));
            sink.segment(c-P::new(0.0,size),c+P::new(0.0,size),size*0.2,gold(palette).fade(fade));
        }}true
    }
}
