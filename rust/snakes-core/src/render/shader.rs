// SPDX-License-Identifier: GPL-3.0-or-later
//! Single-pass sprites/ribbon. No allocation after Renderer::new, including growth.
//! Bytes: kind, tier (low 2 bits)/active effect (bits 2..4), flags, wave/look.
//! Body-only flag bits 1, 4, 5 encode the originating wave (0 none, 1..5 item,
//! 6 white, 7 crown). The vertex shader decodes before interpolation.
//! Kind 8 is prism fruit, 10 is continuous contrail; 11..31 reserve future sprites.
use super::*;
pub(super) mod bounds;
use bounds::*;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShaderVertex {
    pub x: f32,
    pub y: f32,
    pub across: f32,
    pub along: f32,
    pub color: Color,
    pub params: [u8; 4],
}
#[derive(Clone, Copy, Default)]
pub(super) struct Effect {
    pub(super) p: P, pub(super) time: f64, pub(super) radius: f64, pub(super) color: u32, pub(super) kind: u8, pub(super) active: bool,
    snake_id: u32, generation: u32, seed: u8,
}
#[derive(Clone, Copy, Default)]
pub(super) struct Wave { pub(super) time: f64, pub(super) active: bool, pub(super) kind: u8 }

#[inline(always)]
fn wave_flags(origin:u8)->u8 { ((origin&1)<<1)|((origin&6)<<3) }

pub(super) struct SpriteSink<'a> { pub(super) out: &'a mut [ShaderVertex], pub(super) count: usize, pub(super) view: P }
impl SpriteSink<'_> {
    #[inline]
    fn make_vertex(p:P, u:f64, v:f64, color:Color, params:[u8;4])->ShaderVertex {
        ShaderVertex { x:p.x as f32,y:p.y as f32,across:u as f32,along:v as f32,color,params }
    }
    #[inline(always)]
    fn push_quad(&mut self, points:[ShaderVertex;4]) {
        // Check retained capacity once per primitive, and convert each of the
        // four corners once. Triangle-list duplicates become straight copies.
        if let Some(out)=self.out.get_mut(self.count..self.count+6) {
            out[0]=points[0];out[1]=points[1];out[2]=points[2];
            out[3]=points[2];out[4]=points[1];out[5]=points[3];
        } else if self.count<self.out.len() {
            let vertices=[points[0],points[1],points[2],points[2],points[1],points[3]];
            let available=(self.out.len()-self.count).min(6);
            self.out[self.count..self.count+available].copy_from_slice(&vertices[..available]);
        }
        self.count+=6;
    }
    fn quad(&mut self, pts: [P;4], uv: [[f64;2];4], c: Color, params: [u8;4]) {
        if !pts.iter().all(|v|v.finite()) { return; }
        let mut min=pts[0]; let mut max=min;
        for p in pts { min.x=min.x.min(p.x); min.y=min.y.min(p.y); max.x=max.x.max(p.x); max.y=max.y.max(p.y); }
        if !visible(min.x,min.y,max.x-min.x,max.y-min.y,self.view) { return; }
        self.quad_unchecked(pts,uv,c,params);
    }
    #[inline]
    fn quad_unchecked(&mut self, pts:[P;4], uv:[[f64;2];4], c:Color, params:[u8;4]) {
        self.push_quad(std::array::from_fn(|i|Self::make_vertex(pts[i],uv[i][0],uv[i][1],c,params)));
    }
    #[inline(always)]
    fn ribbon_edge(&mut self, pts:[P;4], across:[f64;2], tapers:[u8;2], along:[f64;2], c:Color, params:[u8;4], end_wave:u8,end_origin:Option<u8>) {
        let mut end=params;end[3]=end_wave;
        if let Some(origin)=end_origin {end[2]=(end[2]&!50)|origin;}
        let c0=c.alpha(tapers[0]);
        let c1=c.alpha(tapers[1]);
        self.push_quad([
            Self::make_vertex(pts[0],across[0],along[0],c0,params),
            Self::make_vertex(pts[1],-across[0],along[0],c0,params),
            Self::make_vertex(pts[2],across[1],along[1],c1,end),
            Self::make_vertex(pts[3],-across[1],along[1],c1,end),
        ]);
    }
    #[inline]
    pub(super) fn sprite(&mut self, center:P, radius:f64, c:Color, params:[u8;4]) {
        if !center.finite() || radius<=0.0 || !visible(center.x-radius,center.y-radius,radius*2.0,radius*2.0,self.view) { return; }
        self.quad_unchecked([center-P::new(radius,radius),center+P::new(radius,-radius),center+P::new(-radius,radius),center+P::new(radius,radius)],
            [[-1.0,-1.0],[1.0,-1.0],[-1.0,1.0],[1.0,1.0]],c,params);
    }
    pub(super) fn effect_sprite(&mut self, center:P, radius:f64, extent:f64, c:Color, params:[u8;4]) {
        if !center.finite() || radius<=0.0 || !visible(center.x-radius,center.y-radius,radius*2.0,radius*2.0,self.view) { return; }
        self.quad_unchecked([center-P::new(radius,radius),center+P::new(radius,-radius),center+P::new(-radius,radius),center+P::new(radius,radius)],
            [[-extent,-extent],[extent,-extent],[-extent,extent],[extent,extent]],c,params);
    }
    #[inline(always)]
    fn body_edge(&mut self,a:P,b:P,an:P,bn:P,width:f64,across:[f64;2],tapers:[u8;2],along:[f64;2],c:Color,params:[u8;4],end_wave:u8,end_origin:Option<u8>) {
        if !visible(a.x.min(b.x)-width,a.y.min(b.y)-width,(b.x-a.x).abs()+width*2.0,(b.y-a.y).abs()+width*2.0,self.view) { return; }
        self.ribbon_edge([a+an,a-an,b+bn,b-bn],across,tapers,along,c,params,end_wave,end_origin);
    }
    // Ordinary wall-bounded live bodies share every corner with the next
    // edge. Convert each point's pair once; retain the other walk for drifting
    // corpse pieces and exact f64 translated wrap copies.
    fn live_ribbon<const MIXED:bool>(&mut self, points:&[P], normals:&[P], valid:&[bool], widths:&[f64],
                   taper_bytes:&[u8], brightness:&[u8], origins:&[u8], r:f64, c:Color, params:[u8;4]) {
        let mut previous=[ShaderVertex::default();2];
        let mut previous_valid=false;let mut previous_cached=false;
        let mut previous_width=0.0_f64;let mut a=P::default();
        if self.view.x<=0.0 || self.view.y<=0.0 { return; }
        let envelope=r*BODY;
        for i in 0..points.len() {
            if !valid[i] { previous_valid=false;previous_cached=false;continue; }
            let b=points[i];let w=envelope*widths[i];
            let width=previous_width.max(w);
            // If the edge starts inside the viewport its bounds necessarily
            // intersect it. Only boundary/offscreen edges need the full AABB.
            let inside=a.x>0.0 && a.x<self.view.x && a.y>0.0 && a.y<self.view.y;
            if previous_valid && (inside || visible(a.x.min(b.x)-width,a.y.min(b.y)-width,
                (b.x-a.x).abs()+2.0*width,(b.y-a.y).abs()+2.0*width,self.view)) {
                let mut packed=params;packed[1]|=if i<=3 { 32 } else { 0 };
                let along=(points.len()-1-i) as f64;
                if !previous_cached {
                    let normal=normals[i-1]*previous_width;
                    let cc=c.alpha(taper_bytes[i-1]);
                    packed[3]=brightness[i-1];
                    if MIXED {packed[2]=(packed[2]&!50)|origins[i-1];}
                    previous=[Self::make_vertex(a+normal,widths[i-1],along+1.0,cc,packed),
                              Self::make_vertex(a-normal,-widths[i-1],along+1.0,cc,packed)];
                } else {
                    previous[0].params[1]=packed[1];previous[1].params[1]=packed[1];
                }
                let normal=normals[i]*w;let cc=c.alpha(taper_bytes[i]);
                packed[3]=brightness[i];
                if MIXED {packed[2]=(packed[2]&!50)|origins[i];}
                let current=[Self::make_vertex(b+normal,widths[i],along,cc,packed),
                             Self::make_vertex(b-normal,-widths[i],along,cc,packed)];
                self.push_quad([previous[0],previous[1],current[0],current[1]]);
                previous=current;previous_cached=true;
            } else { previous_cached=false; }
            previous_valid=true;previous_width=w;a=b;
        }
    }

    fn contrail(&mut self, a:P, b:P, r:f64, u:[f64;2], c:Color) {
        let d=b-a;let len=d.length();if len<0.001 { return; }
        let normal=P::new(-d.y,d.x)*(CONTRAIL*r/len);
        let pts=[a+normal*u[0],a-normal*u[0],b+normal*u[1],b-normal*u[1]];
        let width=CONTRAIL*r*u[0].max(u[1]);
        if !visible(a.x.min(b.x)-width,a.y.min(b.y)-width,(b.x-a.x).abs()+2.0*width,(b.y-a.y).abs()+2.0*width,self.view) { return; }
        let colors=[c.fade(0.35*u[0]*u[0]),c.fade(0.35*u[1]*u[1])];
        self.push_quad([
            Self::make_vertex(pts[0],1.0,0.0,colors[0],[10,0,0,0]),
            Self::make_vertex(pts[1],-1.0,0.0,colors[0],[10,0,0,0]),
            Self::make_vertex(pts[2],1.0,0.0,colors[1],[10,0,0,0]),
            Self::make_vertex(pts[3],-1.0,0.0,colors[1],[10,0,0,0]),
        ]);
    }
    fn streak(&mut self, a:P, b:P, width:f64, c:Color, params:[u8;4]) {
        let d=b-a;let len=d.length();if len<0.001 { return; }
        let normal=P::new(-d.y,d.x)*(width/len);
        self.quad([a+normal,a-normal,b+normal,b-normal],
            [[1.0,0.0],[-1.0,0.0],[1.0,1.0],[-1.0,1.0]],c,params);
    }
}
// Frame/record guards bound projected components far below 1e150, even after
// the bounded unwrap walk. Squaring is safe in f64; hypot's rescaling is unnecessary
// here. Classic keeps its original arithmetic and fingerprints.
fn prepare_shader(points:&[P],normals:&mut [P],valid:&mut [bool],limits:&mut [f64],widths:&[f64],envelope:f64) {
    limits.copy_from_slice(widths);
    // Retain raw point validity across the three-point window; a degenerate
    // normal must not invalidate a neighbour's otherwise usable tangent.
    let mut previous_finite=false;
    let mut current_finite=points.first().is_some_and(|p|p.finite());
    for i in 0..points.len() {
        let next=i+1<points.len() && points[i+1].finite();
        let prev=previous_finite;
        let finite=current_finite;
        previous_finite=current_finite;current_finite=next;
        valid[i]=false;
        if !finite { continue; }
        let d=match (prev,next) {
            (true,true)=>points[i+1]-points[i-1],
            (false,true)=>points[i+1]-points[i],
            (true,false)=>points[i]-points[i-1],
            _=>continue,
        };
        let length=(d.x*d.x+d.y*d.y).sqrt();
        // Finite projected coordinates are bounded by the frame/record guards;
        // their squared differences cannot overflow, so only degeneracy remains.
        if length<0.001 { continue; }
        let inverse=1.0/length;
        normals[i]=P::new(-d.y*inverse,d.x*inverse);valid[i]=true;
        if prev && next {
            let a=points[i]-points[i-1];let b=points[i+1]-points[i];
            let cross=a.x*b.y-a.y*b.x;
            let dot=a.x*b.x+a.y*b.y;
            let half=envelope*widths[i];
            // R >= |dot(a,b)|*|a+b|/(2*|cross(a,b)|). This conservative
            // lower bound rejects ordinary bends with just multiply/adds,
            // reusing the normal's length. Exact circumradius is only needed
            // when the bound admits a potentially folding glow envelope.
            if 2.0*half*cross.abs()>dot.abs()*length*0.95 {
                let numerator=(a.x*a.x+a.y*a.y)*(b.x*b.x+b.y*b.y)*(d.x*d.x+d.y*d.y)*0.95*0.95;
                let denominator=4.0*cross*cross;
                if numerator<half*half*denominator && denominator>1.0e-12 {
                    limits[i]=(numerator/denominator).sqrt()/envelope;
                }
            }
        }
    }
}
// Match entire shipped palettes once per build: spectrum oranges and pastel's
// shared peach must not make a crown white. No extra ABI field is necessary.
fn white_crown_palette(palette:&[Color])->bool {
    const EMBER:[[u8;3];6]=[[255,241,168],[255,200,87],[255,123,66],[239,62,54],[156,28,40],[255,214,165]];
    const MONO:[[u8;3];6]=[[255,255,255],[217,225,232],[174,184,194],[127,139,150],[237,242,244],[186,196,206]];
    palette.len()==6 && [EMBER,MONO].iter().any(|entries|palette.iter().zip(entries).all(|(c,rgb)|[c.red,c.green,c.blue]==*rgb))
}
#[inline]
fn head_angle_delta(angle:f64)->f64 {
    // Tick-to-tick turns and desired-heading offsets normally already lie on
    // the shortest arc. Avoid a libm remainder on every presentation, while
    // retaining the bounded normalization for arbitrary borrowed snapshots.
    if (-std::f64::consts::PI..=std::f64::consts::PI).contains(&angle) { angle }
    else { crate::normalize_angle(angle) }
}
impl Renderer {
    fn effect(&mut self, effect:Effect) {
        self.effects[self.effect_head]=effect;
        self.effect_head=(self.effect_head+1)%self.effects.len();
    }
    pub(super) fn shader_history(&mut self, info:&FrameInfo, snakes:&[SnakeRecord], segments:&[SegmentRecord], events:&[EventRecord]) {
        if self.last_frame.is_some_and(|(tick,g)|info.tick<tick || g!=info.geometry_generation) { self.reset(); }
        if self.last_frame==Some((info.tick,info.geometry_generation)) { return; }
        for s in snakes {
            let id=s.id as usize;
            if !snake_valid(s) { continue; }
            if self.last_frame.is_none() || self.shader_generations[id]!=s.generation {
                self.waves[id]=[Wave::default();2]; self.shader_flags[id]=0;
                self.shader_generations[id]=s.generation;
                self.shader_angles[id]=s.angle;
                self.shader_previous_angles[id]=s.angle;
            } else {
                self.shader_previous_angles[id]=self.shader_angles[id];
                self.shader_angles[id]=s.angle;
            }
        }
        for e in events {
            if e.tick>info.tick || self.event_tick.is_some_and(|t|e.tick<=t)
                || !coordinate32(e.x) || !coordinate32(e.y) { continue; }
            let time=info.simulation_time-(info.tick-e.tick) as f64*crate::STEP_SECONDS;
            let radius=snakes.iter().find(|s|s.id==e.snake_id).map_or(8.0,|s|s.radius);
            if e.kind==0 || e.kind==4 {
                self.effect(Effect { p:P::new(e.x as f64,e.y as f64),time,radius,
                    color:e.color_index,kind:if e.kind==0 { 6 } else { 7 },active:true,
                    snake_id:e.snake_id,generation:0,seed:(e.tick as u8).wrapping_add((e.snake_id as u8).wrapping_mul(13)) });
            }
            if matches!(e.kind,2|6|7) {
                self.effect(Effect {p:P::new(e.x as f64,e.y as f64),time,radius,
                    color:e.other_snake_id,kind:if e.kind==2 {12} else {13},active:true,
                    snake_id:e.snake_id,generation:0,seed:e.tick as u8});
                if e.kind==2 && (e.snake_id as usize)<MAX_SNAKES {
                    let waves=&mut self.waves[e.snake_id as usize];
                    waves[1]=waves[0];waves[0]=Wave {time,active:true,kind:e.other_snake_id as u8};
                }
            }
            if e.kind==0 && (e.other_snake_id as usize)<MAX_SNAKES {
                let waves=&mut self.waves[e.other_snake_id as usize];
                waves[1]=waves[0];waves[0]=Wave { time,active:true,kind:0 };
            }
        }
        for s in snakes {
            let id=s.id as usize;
            if !snake_valid(s) || s.segment_count==0 { continue; }
            if s.flags&flags::BOOSTING!=0 && self.shader_flags[id]&flags::BOOSTING==0 {
                let head=segments[s.segment_offset as usize];
                if segment_valid(&head) {
                    self.effect(Effect { p:if s.segment_count>1 { P::new(head.x as f64,head.y as f64) } else { P::new(f64::NAN,f64::NAN) },time:info.simulation_time,
                        radius:s.radius,color:s.color_index,kind:9,active:true,snake_id:s.id,generation:s.generation,seed:info.tick as u8 });
                }
            }
            self.shader_flags[id]=s.flags;
        }
        // Compact history has only tail samples. Locate a queued boost ring
        // from the first full head sample instead of drawing it at the tail.
        for e in &mut self.effects {
            if e.active && e.kind==9 && !e.p.finite() {
                if let Some(s)=snakes.iter().find(|s|s.id==e.snake_id && s.generation==e.generation && s.segment_count>1) {
                    let head=segments[s.segment_offset as usize];
                    if segment_valid(&head) { e.p=P::new(head.x as f64,head.y as f64); }
                }
            }
        }
        self.history(info,snakes,segments,events);
    }
    pub fn build_shader(&mut self, info:&FrameInfo, snakes:&[SnakeRecord], segments:&[SegmentRecord], food:&[FoodRecord], events:&[EventRecord], palette:&[Color], p:&Params, output:&mut [ShaderVertex]) -> Output {
        if !frame_valid(info,p) { return Output::default(); }
        self.shader_history(info,snakes,segments,events);
        if p.scale_x==0.0 || p.scale_y==0.0 { return Output::default(); }
        let motion_scale=if self.reduced_motion { 0.6 } else { 1.0 };
        let view=P::new(p.viewport_width,p.viewport_height);let walls=p.deadly_walls!=0;
        let sx=p.scale_x;let sy=p.scale_y;let scale=(sx*sy).sqrt();
        let arena=P::new(info.world_width,info.world_height);
        let map=|v:P|P::new(v.x*sx+p.offset_x,v.y*sy+p.offset_y);
        let color=|i:u32|palette.get(if palette.is_empty() { 0 } else { i as usize%palette.len() }).copied().unwrap_or(Color::new(0,255,255,255));
        let white_crown=white_crown_palette(palette);
        let mut sink=SpriteSink { out:output,count:0,view };
        for f in food {
            if !food_valid(f) { continue; }
            let size=f.size as f64*scale;
            let extent=size*FOOD;let pos=P::new(f.x as f64,f.y as f64);
            let mut min=pos;let mut max=pos;
            let mut streak=P::default();let sw=(size*VACUUM).max(0.5);
            if f.attraction>0.0 {
                let pull=P::new(delta(pos.x,f.attraction_x as f64,arena.x,walls)*sx,delta(pos.y,f.attraction_y as f64,arena.y,walls)*sy);
                let len=pull.length();if len>0.001 {
                    streak=pull/len*(size*(3.0+f.attraction as f64*8.0));
                    let end=pos-P::new(streak.x/sx,streak.y/sy);
                    min=P::new(min.x.min(end.x),min.y.min(end.y));max=P::new(max.x.max(end.x),max.y.max(end.y));
                }
            }
            let phase=f.phase as f64/std::f64::consts::TAU;
            let phase=if (0.0..1.0).contains(&phase) { phase } else { phase.rem_euclid(1.0) };
            let phase=(phase*255.0).round() as u8;
            let c=color(f.color_index);let params=[if f.kind==3 { 8 } else { 2+f.kind.min(2) },phase,f.life_fraction,0];
            if walls {
                let center=map(pos);
                if streak.x!=0.0 || streak.y!=0.0 { sink.streak(center,center-streak,sw,c.alpha(191),[5,0,0,0]); }
                sink.sprite(center,extent,c,params);
            } else {
                let margin=extent.max(sw);let (xs,ys)=copies(min,max,P::new(margin/sx,margin/sy),arena,false);
                for x in xs.first..=xs.last { for y in ys.first..=ys.last {
                    let center=map(pos+P::new(x as f64*xs.extent,y as f64*ys.extent));
                    if streak.x!=0.0 || streak.y!=0.0 { sink.streak(center,center-streak,sw,c.alpha(191),[5,0,0,0]); }
                    sink.sprite(center,extent,c,params);
                }}
            }
        }
        for s in snakes {
            let n=s.segment_count as usize;let id=s.id as usize;let corpse=s.flags&flags::CORPSE!=0;
            if !snake_valid(s) || (s.alive==0&&!corpse) || n<2 { continue; }
            let body=&segments[s.segment_offset as usize..s.segment_offset as usize+n];
            let points=&mut self.points[..n];
            if moving(s) {
                for (point,seg) in points.iter_mut().zip(body) { *point=position(seg,true,info,p); }
            } else {
                for (point,seg) in points.iter_mut().zip(body) { *point=position(seg,false,info,p); }
            }
            if !walls { for i in 0..n {
                let mut v=P::new(wrap(points[i].x,arena.x),wrap(points[i].y,arena.y));
                if i>0 && points[i-1].finite() { let prev=points[i-1];v=P::new(prev.x+delta(wrap(prev.x,arena.x),v.x,arena.x,false),prev.y+delta(wrap(prev.y,arena.y),v.y,arena.y,false)); }
                points[i]=v;
            }}
            let mapped=&mut self.mapped[..n];for (to,point) in mapped.iter_mut().zip(points.iter()) { *to=map(*point); }
            let r=s.radius*scale;
            let widths=&mut self.tapers[id*MAX_SEGMENTS..id*MAX_SEGMENTS+n];
            let taper_bytes=&mut self.shader_taper_bytes[id*MAX_SEGMENTS..id*MAX_SEGMENTS+n];
            if self.taper_lengths[id]!=n || self.shader_taper_lengths[id]!=n {
                for (i,w) in widths.iter_mut().enumerate() {
                    *w=crate::shape::taper(i as f64/(n-1) as f64);
                    taper_bytes[i]=(*w*255.0).round() as u8;
                }
                self.taper_lengths[id]=n;self.shader_taper_lengths[id]=n;
            }
            prepare_shader(mapped,&mut self.normals[..n],&mut self.valid[..n],&mut self.shader_limits[..n],widths,r*BODY);
            let tier=if n<24 { 0 } else if n<100 { 1 } else if n<250 { 2 } else { 3 };
            let active_kind=if s.alive!=0 && !corpse && s.effect_ticks>0 {s.effect_kind&7} else {0};
            let flags=s.flags as u8;let c=color(s.color_index);
            if moving(s) && s.flags&flags::BOOSTING!=0 {
                let trail=&self.trails[id];let mut prev:Option<(P,f64)>=None;
                for i in 0..trail.len { let index=(trail.head+15-trail.len+i)%15;
                    let age=p.presentation_time-info.simulation_time+info.tick.saturating_sub(trail.ticks[index]) as f64*crate::STEP_SECONDS;
                    if age>=0.5 { continue; }let pos=trail.points[index];
                    let u=1.0-age.max(0.0)/0.5;
                    if let Some((a,ua))=prev { let b=a+P::new(delta(a.x,pos.x,arena.x,walls),delta(a.y,pos.y,arena.y,walls));let w=CONTRAIL*r*ua.max(u);
                        let (xs,ys)=copies(P::new(a.x.min(b.x),a.y.min(b.y)),P::new(a.x.max(b.x),a.y.max(b.y)),P::new(w/sx,w/sy),arena,walls);
                        for x in xs.first..=xs.last { for y in ys.first..=ys.last { let shift=P::new(x as f64*xs.extent*sx,y as f64*ys.extent*sy);sink.contrail(map(a)+shift,map(b)+shift,r,[ua,u],c.boost()); }}
                    }prev=Some((pos,u));
                }
            }
            let age=if corpse { ((p.presentation_time-self.corpses[id].time)/(0.55*motion_scale)).clamp(0.0,1.0) } else { 0.0 };
            // Select at most two waves once per snake, not once per vertex.
            let mut waves=[(0.0_f64,0.0_f64,0_u8);2];let mut wave_count=0;
            for w in self.waves[id] {
                let center=(p.presentation_time-w.time)/motion_scale*if w.kind==0 {1.1} else {1.3}*(n-1) as f64;
                if w.active && center>=-3.5 && center<(n as f64+3.5) {
                    waves[wave_count]=(center,if w.kind==0 {1.0} else {0.95},if w.kind==0 {6} else {w.kind});wave_count+=1;
                }
            }
            if s.flags&flags::BOOSTING!=0 {
                let phase=(p.presentation_time/0.11).fract();
                for j in 0..2-wave_count { waves[wave_count+j]=((phase+j as f64)*0.11*2.4*(n-1) as f64/motion_scale,0.55,6); }
                wave_count=2;
            } else if s.flags&flags::LEADER!=0 && wave_count<2 {
                let center=(p.presentation_time%4.0)/motion_scale*0.75*(n-1) as f64;
                if center<(n as f64+3.5) { waves[wave_count]=(center,0.5,7);wave_count+=1; }
            }
            let common_origin=waves[0].2;
            let mixed=wave_count==2 && waves[1].2!=common_origin;
            let body_flags=(flags & !50)|if mixed {0} else {wave_flags(common_origin)};
            self.brightness[..n].fill(0);
            if mixed {self.wave_origins[..n].fill(0);}
            for &(center,strength,kind) in &waves[..wave_count] {
                let origin=wave_flags(kind);
                let first=(center-3.5).max(0.0).ceil() as usize;
                let last=(center+3.5).max(0.0).floor() as usize;
                for j in first..=last.min(n-1) {
                    let light=((1.0-(j as f64-center).abs()/3.5).max(0.0)*strength*255.0).round() as u8;
                    if mixed {
                        // Newest wins ties, independently of the active effect.
                        if light>self.brightness[j] {self.brightness[j]=light;self.wave_origins[j]=origin;}
                    } else {self.brightness[j]=self.brightness[j].max(light);}
                }
            }
            // Body envelope includes waves, breathing and taper quantization. Per-edge seam selection
            // bounds even multi-lap bodies, and normals are computed only once.
            if walls && !corpse {
                let params=[0,tier|(active_kind<<2)|if white_crown { 64 } else { 0 },body_flags,0];
                if mixed {
                    sink.live_ribbon::<true>(mapped,&self.normals[..n],&self.valid[..n],&self.shader_limits[..n],
                        taper_bytes,&self.brightness[..n],&self.wave_origins[..n],r,c,params);
                } else {
                    sink.live_ribbon::<false>(mapped,&self.normals[..n],&self.valid[..n],&self.shader_limits[..n],
                        taper_bytes,&self.brightness[..n],&[],r,c,params);
                }
            } else { for i in 1..n {
                if !self.valid[i-1] || !self.valid[i] { continue; }
                let a=points[i-1];let b=points[i];
                let u0=self.shader_limits[i-1];let u1=self.shader_limits[i];
                let w0=r*u0*BODY;let w1=r*u1*BODY;
                let mut an=self.normals[i-1]*w0;let mut bn=self.normals[i]*w1;
                // Keep the exact shared tail envelope. The fragment shader
                // points the final edge without collapsing its across UVs.
                let mut ma=mapped[i-1];let mut mb=mapped[i];let mut alpha=1.0;
                if corpse {
                    let piece=(i-1)/3;let local=(age*1.5-(piece*3) as f64/n as f64*0.5).clamp(0.0,1.0);
                    alpha=1.0-local;
                    if alpha<=0.0 { continue; }
                    let normal=if self.valid[piece*3] { self.normals[piece*3] } else { self.normals[i-1] };
                    let drift=normal*(local*r*CORPSE_DRIFT*if piece%2==0 { 1.0 } else { -1.0 });
                    ma=ma+drift;mb=mb+drift;an=an*(1.0-local*0.5);bn=bn*(1.0-local*0.5);
                }
                let mut params=[0,tier|(active_kind<<2)|if i<=3 { 32 } else { 0 }|if white_crown { 64 } else { 0 },body_flags,
                    if corpse { (alpha*255.0).round() as u8 } else {self.brightness[i-1]}];
                if !corpse && mixed {params[2]|=self.wave_origins[i-1];}
                let cc=if corpse && age<0.08/0.55 { Color::new(255,255,255,c.alpha) } else if corpse { c.boost() } else { c };
                let shrink=if corpse { 1.0-(1.0-alpha)*0.5 } else { 1.0 };
                // Cached bytes remove round() from every ordinary body edge.
                let tapers=if corpse {
                    [(widths[i-1]*shrink*255.0).round() as u8,(widths[i]*shrink*255.0).round() as u8]
                } else { [taper_bytes[i-1],taper_bytes[i]] };
                // Physical UV remains correct at the shader's compact glow edge.
                let across=[u0*shrink,u1*shrink];
                let end_wave=if corpse { params[3] } else { self.brightness[i] };
                let end_origin=if !corpse && mixed {Some(self.wave_origins[i])} else {None};
                let width=w0.max(w1);let along=[(n-i) as f64,(n-1-i) as f64];
                if walls {
                    sink.body_edge(ma,mb,an,bn,width,across,tapers,along,cc,params,end_wave,end_origin);
                } else {
                    let margin=P::new((width+r*CORPSE_DRIFT)/sx,(width+r*CORPSE_DRIFT)/sy);
                    let (xs,ys)=copies(P::new(a.x.min(b.x),a.y.min(b.y)),P::new(a.x.max(b.x),a.y.max(b.y)),margin,arena,false);
                    for x in xs.first..=xs.last { for y in ys.first..=ys.last {
                        let shift=P::new(x as f64*xs.extent*sx,y as f64*ys.extent*sy);
                        sink.body_edge(ma+shift,mb+shift,an,bn,width,across,tapers,along,cc,params,end_wave,end_origin);
                    }}
                }

            }
            }
            if corpse { continue; }
            let angle=if moving(s) {
                self.shader_previous_angles[id]+head_angle_delta(s.angle-self.shader_previous_angles[id])*p.interpolation
            } else { s.angle };
            let head=points[0];let forward=P::new(angle.cos()*sx,angle.sin()*sy);
            // Scale is bounded to 1e6; unlike general hypot this normalization
            // needs no overflow/underflow rescaling on each head presentation.
            let len=(forward.x*forward.x+forward.y*forward.y).sqrt();
            if len<0.001 { continue; }let forward=forward/len;let side=P::new(-forward.y,forward.x);
            let hr=r*if tier==0 { 1.24 } else { 1.14 };
            let boosting=s.flags&flags::BOOSTING!=0;
            let back=if boosting { HEAD_BOOST_BACK } else { HEAD_BACK };
            let half_width=if boosting { HEAD_BOOST_SIDE } else { HEAD_SIDE };
            let half_length=(HEAD_FRONT+back)*0.5;let offset=(HEAD_FRONT-back)*0.5;
            // UV stores head units: x forward, y across; tongue included in this quad.
            let a=forward*(hr*half_length);let b=side*(hr*half_width);let center=map(head)+forward*(hr*offset);
            let look=(head_angle_delta(s.desired_angle-angle).clamp(-0.9,0.9)/0.9*127.0+128.0).round() as u8;
            let hash=((s.id.wrapping_mul(73)^s.generation.wrapping_mul(151))%16) as u8;
            let flare=self.waves[id].iter().any(|w|w.active && (0.0..0.5*motion_scale).contains(&(p.presentation_time-w.time)));
            let head_flags=(flags & !(flags::PHASED as u8)) | if active_kind==3 {flags::PHASED as u8} else {0};
            let params=[1,tier|(hash<<2)|if white_crown { 64 } else { 0 }|if flare { 128 } else { 0 },head_flags,look];
            let margin=P::new(hr*(HEAD_FRONT*HEAD_FRONT+half_width*half_width).sqrt()/sx,hr*(HEAD_FRONT*HEAD_FRONT+half_width*half_width).sqrt()/sy);let (xs,ys)=copies(head,head,margin,arena,walls);
            for x in xs.first..=xs.last { for y in ys.first..=ys.last {
                let shift=P::new(x as f64*xs.extent*sx,y as f64*ys.extent*sy);
                sink.quad([center-a-b+shift,center+a-b+shift,center-a+b+shift,center+a+b+shift],
                    [[-back,-half_width],[HEAD_FRONT,-half_width],[-back,half_width],[HEAD_FRONT,half_width]],c.alpha(taper_bytes[1]),params);
            }}
            if p.developer_mode!=0 {
                // Steering can reach beyond the head envelope (including the
                // minimum-length arrow in tiny arenas). Select its own copies
                // from both endpoints plus the full screen-space half-width.
                let direction=P::new(s.desired_angle.cos(),s.desired_angle.sin());
                let length=(s.radius*7.0).max(52.0);let end=head+direction*length;
                let (xs,ys)=copies(P::new(head.x.min(end.x),head.y.min(end.y)),
                    P::new(head.x.max(end.x),head.y.max(end.y)),P::new(DEVELOPER/sx,DEVELOPER/sy),arena,walls);
                let start=map(head);
                let end=start+P::new(direction.x*sx,direction.y*sy)*length;
                for x in xs.first..=xs.last { for y in ys.first..=ys.last {
                    let shift=P::new(x as f64*xs.extent*sx,y as f64*ys.extent*sy);
                    sink.streak(start+shift,end+shift,DEVELOPER,Color::new(255,255,255,235),[5,0,0,0]);
                }}
            }

        }
        self.shader_items(info,p,palette,&mut sink);
        let mut effect_budget=EffectBudget::default();
        for age_index in 0..self.effects.len() {
            let e=self.effects[(self.effect_head+self.effects.len()-1-age_index)%self.effects.len()];
            let duration=(if e.kind==6 { 0.5 } else if e.kind==9 { 0.28 } else if e.kind>=12 {0.5} else { 0.6 })*motion_scale;let age=p.presentation_time-e.time;
            if !e.active || !(0.0..duration).contains(&age) { continue; }
            let extent=if e.kind==6 { IMPACT } else { RING };
            let r=e.radius*scale*extent;let (xs,ys)=copies(e.p,e.p,P::new(r/sx,r/sy),arena,walls);
            for x in xs.first..=xs.last { for y in ys.first..=ys.last {
                if effect_budget.full() {continue;}
                let before=sink.count;
                sink.effect_sprite(map(e.p+P::new(x as f64*xs.extent,y as f64*ys.extent)),r,extent,if e.kind>=12 {items::accent(e.color as u8,palette)} else {color(e.color)},[e.kind,if e.kind==7 && white_crown { e.seed|64 } else if e.kind==7 { e.seed&!64 } else { e.seed },0,(age/duration*255.0).round() as u8]);
                effect_budget.emitted(before,sink.count);
            }}
        }
        // Persistent expiry warning shares the eight-quad effect budget.
        for s in snakes {
            if effect_budget.full() || !snake_valid(s) || s.alive==0 || s.segment_count==0 || s.effect_kind==0
                || s.effect_ticks==0 || s.effect_ticks>crate::effects::WARNING_TICKS {continue;}
            let head=position(&segments[s.segment_offset as usize],moving(s),info,p);
            let r=s.radius*scale*RING;
            let (xs,ys)=copies(head,head,P::new(r/sx,r/sy),arena,walls);
            for x in xs.first..=xs.last {for y in ys.first..=ys.last {
                if effect_budget.full() {continue;}
                let before=sink.count;
                sink.effect_sprite(map(head+P::new(x as f64*xs.extent,y as f64*ys.extent)),r,RING,
                    items::accent(s.effect_kind,palette),[14,s.effect_kind,0,
                    ((1.0-s.effect_ticks as f64/36.0)*255.0).round() as u8]);
                effect_budget.emitted(before,sink.count);
            }}
        }
        // Magnet shares the transient/warning cap; warnings have priority.
        for s in snakes {
            if effect_budget.full() || !snake_valid(s) || s.alive==0 || s.segment_count==0
                || s.effect_kind!=2 || s.effect_ticks==0 {continue;}
            let head=position(&segments[s.segment_offset as usize],moving(s),info,p);
            let r=s.radius*scale*MAGNET;
            let (xs,ys)=copies(head,head,P::new(r/sx,r/sy),arena,walls);
            for x in xs.first..=xs.last {for y in ys.first..=ys.last {
                if effect_budget.full() {continue;}
                let before=sink.count;
                sink.effect_sprite(map(head+P::new(x as f64*xs.extent,y as f64*ys.extent)),r,MAGNET,
                    items::accent(2,palette),[15,2,0,0]);
                effect_budget.emitted(before,sink.count);
            }}
        }
        Output { vertex_count:sink.count,..Output::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wave_origins_round_trip_without_changing_body_flags_or_light() {
        for origin in 0..8 {
            for light in 0..=255 {
                let base=[0,3|12|64,1|4|8|64|128,0];
                let packed=[base[0],base[1],base[2]|wave_flags(origin),light as u8];
                let decoded=((packed[2]>>1)&1)|((packed[2]>>3)&6);
                assert_eq!(decoded,origin);
                assert_eq!(packed[2]&!50,base[2]);
                assert_eq!(packed[3],light as u8);
                assert_eq!(packed[1],base[1]);
            }
        }
    }
}
