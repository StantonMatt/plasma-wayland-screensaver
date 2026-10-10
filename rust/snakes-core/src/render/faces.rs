// SPDX-License-Identifier: GPL-3.0-or-later
//! ABI-v3 presentation only: fixed bubble budget, screen-space clock exclusion,
//! and two exported contenders per capsule. No retained per-frame heap state.
use super::*;
use super::shader::SpriteSink;

/// Available radius excludes the pupil radius, keeping its entire disc inside.
pub(super) fn pupil_offset(offset:P,available:f64)->P {
    let length2=offset.x*offset.x+offset.y*offset.y;
    if length2>available*available {offset*(available/length2.sqrt())} else {offset}
}
pub(super) fn has_mood(s:&SnakeRecord)->bool {
    s.mood<=8 && (s.face_flags & crate::ffi::FACE_OBSERVED!=0 || s.mood>0)
}
pub(super) fn head_intensity(s:&SnakeRecord)->u8 {
    if !has_mood(s) && head_mood(s)!=0 {255} else {s.mood_intensity}
}
pub(super) fn head_mood(s:&SnakeRecord)->u8 {
    // Calm is a valid observation; only absent legacy faces derive from flags.
    if has_mood(s) {s.mood}
    else if s.flags&flags::FROZEN!=0 {8}
    else if s.flags&flags::TRAPPED!=0 {6}
    else if s.flags&(flags::HUNTING|flags::BOOSTING)!=0 {2} else {0}
}
pub(super) fn palette_accent(mut c:Color,palette:&[Color])->Color {
    let mono=palette.first().is_some_and(|p|[p.red,p.green,p.blue]==[255,255,255]);
    let pastel=palette.first().is_some_and(|p|[p.red,p.green,p.blue]==[255,200,221]);
    let grey=c.red as f64*0.2126+c.green as f64*0.7152+c.blue as f64*0.0722;
    for v in [&mut c.red,&mut c.green,&mut c.blue] {
        if mono {*v=grey.round() as u8;} else if pastel {*v=(*v as f64*0.7+76.5).round() as u8;}
    }
    c
}
pub(super) fn eye_color(mood:u8,palette:&[Color])->Color {
    palette_accent(match mood {2=>Color::new(255,190,80,255),4=>Color::new(255,105,55,255),
        1=>Color::new(170,205,240,255),5=>Color::new(255,224,150,255),
        6=>Color::new(210,225,240,255),7=>Color::new(205,170,255,255),8=>Color::new(200,238,255,255),
        _=>Color::new(255,255,255,255)},palette)
}
impl Renderer {
    /// Local screen-space rectangle; an empty rectangle disables exclusion.
    pub fn set_clock_rect(&mut self,rect:[f64;4]) {self.clock_rect=rect;}
    pub(super) fn shader_bubbles(&self,info:&FrameInfo,p:&Params,palette:&[Color],snakes:&[SnakeRecord],segments:&[SegmentRecord],sink:&mut SpriteSink<'_>) {
        let scale=(p.scale_x*p.scale_y).sqrt();let arena=P::new(info.world_width,info.world_height);
        let extra=if self.reduced_motion {0.0} else {((p.presentation_time-info.simulation_time)/crate::STEP_SECONDS).max(0.0)};
        let lifetime=if self.reduced_motion {27.0} else {45.0};
        let mut owners=0u32;
        for bubble in &info.bubbles[..(info.bubble_count as usize).min(3)] {
            if bubble.glyph>11 || bubble.snake_id as usize>=MAX_SNAKES || owners&(1<<bubble.snake_id)!=0 {continue;}
            let Some(s)=snakes.iter().find(|s|s.id==bubble.snake_id && s.generation==bubble.generation && s.alive!=0 && s.flags&flags::CORPSE==0 && s.segment_count>0 && snake_valid(s)) else {continue;};
            let age=(bubble.age_ticks as f64+extra).max(0.0);
            if age>=lifetime {continue;}
            owners|=1<<s.id;
            let head=position(&segments[s.segment_offset as usize],moving(s),info,p);
            if !head.finite() {continue;}
            let r=s.radius*scale;
            // 0.18s ease-out back pop, maximum 10% overshoot. Calm fades only.
            let pop=if self.reduced_motion {1.0} else {
                let u=(age/5.4).clamp(0.0,1.0)-1.0;1.0+2.0*u*u*u+u*u
            };
            let extent=(1.3*r).max(12.5)*pop;
            if extent<=0.0 || extent*2.0>sink.view.x.min(sink.view.y) {continue;}
            let fade=(age/if self.reduced_motion {3.24} else {5.4}).min(1.0)
                *((lifetime-age)/if self.reduced_motion {5.4} else {9.0}).min(1.0);
            // One bubble per owner, even at wrap seams. Choose a visible head
            // copy, then clamp its full quad (including AA) to this monitor.
            let (xs,ys)=copies(head,head,P::new(r/p.scale_x,r/p.scale_y),arena,p.deadly_walls!=0);
            let mut best=None;let mut best_distance=f64::INFINITY;
            for x in xs.first..=xs.last {for y in ys.first..=ys.last {
                let h=P::new((head.x+x as f64*xs.extent)*p.scale_x+p.offset_x,(head.y+y as f64*ys.extent)*p.scale_y+p.offset_y);
                if !visible(h.x-r,h.y-r,2.0*r,2.0*r,sink.view) {continue;}
                let distance=(h-sink.view*0.5).length();
                if distance<best_distance {best=Some(h+P::new(1.9*r,-2.6*r));best_distance=distance;}
            }}
            let Some(center)=best.and_then(|c|bubble_position(c,extent,sink.view,self.clock_rect)) else {continue;};
            let c=match bubble.glyph {
                0=>self.items.get(s.target_item as usize).filter(|_|(s.target_item as usize)<self.item_count)
                    .map_or_else(||items::tint(Color::new(255,245,224,255),palette),|item|items::accent(item.kind,palette)),
                1=>Color::new(183,205,222,255),2=>Color::new(255,105,55,255),
                3=>Color::new(189,222,255,255),5..=11=>items::accent(bubble.glyph-4,palette),_=>Color::new(255,150,200,255),
            };
            // Mood colours use greys on Mono; emote accents retain 12% chroma.
            let c=if bubble.glyph==0 || bubble.glyph>=5 {c} else {items::tint(c,palette)};
            sink.sprite(center,extent,c.fade(fade),[16,bubble.glyph,0,0]);
        }
    }
    pub(super) fn shader_races(&self,info:&FrameInfo,p:&Params,palette:&[Color],snakes:&[SnakeRecord],segments:&[SegmentRecord],sink:&mut SpriteSink<'_>) {
        self.shader_race_items(info,p,palette,snakes,segments,sink,&self.items[..self.item_count],self.item_radius,6);
    }
    pub(super) fn shader_prism_races(&self,info:&FrameInfo,p:&Params,palette:&[Color],snakes:&[SnakeRecord],segments:&[SegmentRecord],food:&[FoodRecord],sink:&mut SpriteSink<'_>) {
        let Some(f)=food.iter().find(|f|matches!(f.kind,3|4) && food_valid(f)) else {return;};
        let pos=P::new(f.x as f64,f.y as f64);let arena=P::new(info.world_width,info.world_height);
        let mut item=ItemRecord {x:f.x,y:f.y,kind:1,life_ticks:750,leader_snake_id:u32::MAX,
            contender_ids:[u32::MAX;2],contender_etas:[f32::INFINITY;2],..ItemRecord::default()};
        let mut distances=[f64::INFINITY;2];
        for s in snakes.iter().filter(|s|s.face_flags&4!=0 && s.alive!=0 && snake_valid(s) && s.segment_count>0) {
            let head=position(&segments[s.segment_offset as usize],moving(s),info,p);
            let d=P::new(delta(head.x,pos.x,arena.x,p.deadly_walls!=0),delta(head.y,pos.y,arena.y,p.deadly_walls!=0));
            let distance=d.length();let eta=(distance/s.radius.max(1.0)+crate::normalize_angle(d.y.atan2(d.x)-s.angle).abs()) as f32;
            if let Some(slot)=(0..2).find(|&i|distance<distances[i]) {
                if slot==0 {distances[1]=distances[0];item.contender_ids[1]=item.contender_ids[0];item.contender_etas[1]=item.contender_etas[0];}
                distances[slot]=distance;item.contender_ids[slot]=s.id;item.contender_etas[slot]=eta;
            }
        }
        if f.food_flags!=0 {
            item.contender_ids=[(f.food_flags&15).checked_sub(1).unwrap_or(u32::MAX),((f.food_flags>>4)&15).checked_sub(1).unwrap_or(u32::MAX)];
            item.contender_etas=if f.food_flags&512!=0 {[1.0,1.0]} else if f.food_flags&256!=0 {[2.0,1.0]} else {[1.0,2.0]};
        }
        item.contender_count=item.contender_ids.iter().filter(|&&id|id!=u32::MAX).count() as u8;
        item.leader_snake_id=item.contender_ids[if f.food_flags!=0 {usize::from(f.food_flags&256!=0)} else {usize::from(item.contender_etas[1]<item.contender_etas[0])}];
        // Six arcs total, including the two prism racers. Capsules keep their
        // established ordering when no seed/fruit is present.
        self.shader_race_items(info,p,palette,snakes,segments,sink,&[item],f.size as f64*2.04*super::shader::PRISM_VISUAL,2);
    }
    pub(super) fn shader_race_items(&self,info:&FrameInfo,p:&Params,palette:&[Color],snakes:&[SnakeRecord],segments:&[SegmentRecord],sink:&mut SpriteSink<'_>,items:&[ItemRecord],item_radius:f64,budget:usize) {
        let sx=p.scale_x;let sy=p.scale_y;let scale=(sx*sy).sqrt();
        let arena=P::new(info.world_width,info.world_height);let walls=p.deadly_walls!=0;
        let mut drawn=0;
        for item in items {
            if !(1..=7).contains(&item.kind) || !items::valid_item(item) || item_radius<=0.0 {continue;}
            let pos=P::new(item.x as f64,item.y as f64);
            let etas=item.contender_etas;
            let contested=etas.iter().all(|v|v.is_finite() && *v>=0.0) && etas[0].max(etas[1])<=etas[0].min(etas[1])*1.18;
            let contenders=&item.contender_ids[..(item.contender_count as usize).min(2)];
            let track_owner=if contenders.contains(&item.leader_snake_id) {item.leader_snake_id}
                else {contenders.first().copied().unwrap_or(u32::MAX)};
            for &id in contenders {
                if drawn==budget {return;}
                let Some(s)=snakes.iter().find(|s|s.id==id && s.alive!=0 && s.flags&flags::CORPSE==0 && s.segment_count>0 && s.radius>0.0 && snake_valid(s)) else {continue;};
                let head=position(&segments[s.segment_offset as usize],moving(s),info,p);
                if !head.finite() {continue;}
                drawn+=1;
                let d=P::new(delta(pos.x,head.x,arena.x,walls),delta(pos.y,head.y,arena.y,walls));
                let closeness=(1.0-(d.length()-3.0*s.radius)/(34.0*s.radius)).clamp(0.0,1.0);
                let angle=(d.y*sy).atan2(d.x*sx).rem_euclid(std::f64::consts::TAU);
                let radius=item_radius*scale;
                // Stroke is in pixels, so minimum-size projections need a
                // larger quad; UV remains in capsule radii for the 1.62 ring.
                let extent=(radius*3.4).max(radius*1.62+12.0);
                let (xs,ys)=copies(pos,pos,P::new(extent/sx,extent/sy),arena,walls);
                let c=palette.get(if palette.is_empty() {0} else {s.color_index as usize%palette.len()}).copied().unwrap_or(Color::new(0,255,255,255));
                let params=[17,(closeness*255.0).round() as u8,(angle/std::f64::consts::TAU*255.0).round() as u8,
                    u8::from(id==item.leader_snake_id)|if contested {2} else {0}|if id==track_owner {4} else {0}];
                for x in xs.first..=xs.last {for y in ys.first..=ys.last {
                    let center=P::new((pos.x+x as f64*xs.extent)*sx+p.offset_x,(pos.y+y as f64*ys.extent)*sy+p.offset_y);
                    sink.effect_sprite(center,extent,extent/radius,c,params);
                }}
            }
        }
    }
}
fn bubble_position(center:P,r:f64,view:P,clock:[f64;4])->Option<P> {
    let clamp=|c:P|P::new(c.x.clamp(r,view.x-r),c.y.clamp(r,view.y-r));
    let c=clamp(center);
    let [cx,cy,cw,ch]=clock;
    let (x,y,w,h)=if cw>0.0 && ch>0.0 {(cx-1.0,cy-1.0,cw+2.0,ch+2.0)} else {(cx,cy,cw,ch)};
    let overlaps=|p:P|w>0.0 && h>0.0 && p.x+r>x && p.x-r<x+w && p.y+r>y && p.y-r<y+h;
    if !overlaps(c) {return Some(c);}
    let mut best=None;let mut distance=f64::INFINITY;
    for candidate in [P::new(x-r,c.y),P::new(x+w+r,c.y),P::new(c.x,y-r),P::new(c.x,y+h+r)] {
        let candidate=clamp(candidate);
        if overlaps(candidate) {continue;}
        let d=(candidate-c).length();if d>2.0*r {continue;}
        if d<distance {best=Some(candidate);distance=d;}
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clock_relocation_is_local_or_bubble_is_dropped() {
        let c=P::new(100.0,100.0);let view=P::new(400.0,300.0);
        // Inflated clock edge is exactly two half-extents from the centre.
        assert_eq!((bubble_position(c,12.5,view,[113.5,75.0,30.0,50.0]).unwrap()-c).length(),0.0);
        let local=bubble_position(c,12.5,view,[88.5,75.0,24.0,50.0]).unwrap();
        assert_eq!((local-c).length(),25.0);
        assert!(bubble_position(c,12.5,view,[88.0,75.0,24.0,50.0]).is_none());
        assert!(bubble_position(c,12.5,view,[20.0,20.0,200.0,200.0]).is_none());
        assert_eq!((bubble_position(P::new(0.0,0.0),12.5,view,[0.0;4]).unwrap()-P::new(12.5,12.5)).length(),0.0);
    }
    #[test]
    fn accents_and_mood_colours_obey_mono_and_pastel_rules() {
        let mono=[Color::new(255,255,255,255)];
        let pastel=[Color::new(255,200,221,255)];
        for mood in 0..9 {
            let c=eye_color(mood,&mono);assert_eq!(c.red,c.green);assert_eq!(c.green,c.blue);
        }
        let orange=Color::new(255,105,55,255);
        assert_eq!(items::tint(orange,&mono),Color::new(148,130,124,255));
        assert_eq!(items::tint(orange,&pastel),Color::new(255,150,115,255));
    }
}
