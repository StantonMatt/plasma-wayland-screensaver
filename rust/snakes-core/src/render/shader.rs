// SPDX-License-Identifier: GPL-3.0-or-later
//! Single-pass sprites/ribbon. No allocation after Renderer::new, including growth.
//! Bytes: kind, tier (low 2 bits)/active effect (bits 2..4), flags, wave/look.
//! Body-only flag bits 1, 4, 5 encode the originating wave (0 none, 1..5 item,
//! 6 white, 7 crown). The vertex shader decodes before interpolation.
//! Kind 8 is prism fruit, 10 is continuous contrail; 11..31 reserve future sprites.
use super::*;
pub(super) mod bounds;
use bounds::*;
use crate::shape::{short_tapers,SHORT_TAPER_MAX};

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
    snake_id: u32, generation: u32, seed: u8, pub(super) duration_ticks:u16,
}
#[derive(Clone, Copy, Default, PartialEq)]
pub(super) struct Wave { pub(super) time: f64, pub(super) active: bool, pub(super) kind: u8, pub(super) duration_ticks: u16 }

// One shared rainbow scratch buffer; retain only the last completed colour
// sample. A redraw at the same event age does not repeat exponentials/rounds.
#[derive(Clone,Copy,PartialEq)]
pub(super) struct RainbowSample {id:u32,n:usize,time:f64,calm:bool,base:Color,palette:Color,frozen:bool,waves:[Wave;2]}

#[inline(always)]
fn wave_flags(origin:u8)->u8 { ((origin&1)<<1)|((origin&6)<<3) }

pub(super) const PRISM_VISUAL:f64=1.6;
/// Soft additive Venom-accent glow (stump and orphan cut end); uv in +-1.
pub(super) const ACID_GLOW:u8=25;
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
    pub(super) fn quad(&mut self, pts: [P;4], uv: [[f64;2];4], c: Color, params: [u8;4]) {
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
    pub(super) fn body_edge(&mut self,a:P,b:P,an:P,bn:P,width:f64,across:[f64;2],tapers:[u8;2],along:[f64;2],c:Color,params:[u8;4],end_wave:u8,end_origin:Option<u8>) {
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

    // Separate feature path keeps the ordinary ribbon ABI and hot loop intact.
    fn live_prism_ribbon<const MIXED:bool>(&mut self, points:&[P], normals:&[P], valid:&[bool], widths:&[f64],
                   taper_bytes:&[u8], brightness:&[u8], origins:&[u8], r:f64, c:Color, params:[u8;4], colors:&[Color], rainbow:bool, gulp:bool) {
        let mut previous=[ShaderVertex::default();2];
        let mut previous_valid=false;let mut previous_cached=false;
        let mut previous_width=0.0_f64;let mut a=P::default();
        if self.view.x<=0.0 || self.view.y<=0.0 { return; }
        let envelope=r*BODY;
        for i in 0..points.len() {
            if !valid[i] { previous_valid=false;previous_cached=false;continue; }
            let b=points[i];let w=envelope*widths[i];
            let width=previous_width.max(w)*if gulp {1.35} else {1.0};
            // If the edge starts inside the viewport its bounds necessarily
            // intersect it. Only boundary/offscreen edges need the full AABB.
            let inside=a.x>0.0 && a.x<self.view.x && a.y>0.0 && a.y<self.view.y;
            if previous_valid && (inside || visible(a.x.min(b.x)-width,a.y.min(b.y)-width,
                (b.x-a.x).abs()+2.0*width,(b.y-a.y).abs()+2.0*width,self.view)) {
                let mut packed=params;packed[1]|=if i<=3 { 32 } else { 0 };
                let along=(points.len()-1-i) as f64;
                if !previous_cached {
                    let normal=normals[i-1]*previous_width;
                    let cc=if rainbow {colors[i-1]} else {c}.alpha(taper_bytes[i-1]);
                    packed[3]=brightness[i-1];
                    if MIXED {packed[2]=(packed[2]&!50)|origins[i-1];}
                    previous=[Self::make_vertex(a+normal,widths[i-1],along+1.0,cc,packed),
                              Self::make_vertex(a-normal,-widths[i-1],along+1.0,cc,packed)];
                } else {
                    previous[0].params[1]=packed[1];previous[1].params[1]=packed[1];
                }
                let normal=normals[i]*w;let cc=if rainbow {colors[i]} else {c}.alpha(taper_bytes[i]);
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

    fn contrail(&mut self, a:P, b:P, r:f64, u:[f64;2], c:Color,strength:f64) {
        let d=b-a;let len=d.length();if len<0.001 { return; }
        let normal=P::new(-d.y,d.x)*(CONTRAIL*r/len);
        let pts=[a+normal*u[0],a-normal*u[0],b+normal*u[1],b-normal*u[1]];
        let width=CONTRAIL*r*u[0].max(u[1]);
        if !visible(a.x.min(b.x)-width,a.y.min(b.y)-width,(b.x-a.x).abs()+2.0*width,(b.y-a.y).abs()+2.0*width,self.view) { return; }
        let colors=[c.fade(strength*u[0]*u[0]),c.fade(strength*u[1]*u[1])];
        self.push_quad([
            Self::make_vertex(pts[0],1.0,0.0,colors[0],[10,0,0,0]),
            Self::make_vertex(pts[1],-1.0,0.0,colors[0],[10,0,0,0]),
            Self::make_vertex(pts[2],1.0,0.0,colors[1],[10,0,0,0]),
            Self::make_vertex(pts[3],-1.0,0.0,colors[1],[10,0,0,0]),
        ]);
    }
    pub(super) fn streak(&mut self, a:P, b:P, width:f64, c:Color, params:[u8;4]) {
        let d=b-a;let len=d.length();if len<0.001 { return; }
        let normal=P::new(-d.y,d.x)*(width/len);
        self.quad([a+normal,a-normal,b+normal,b-normal],
            [[1.0,0.0],[-1.0,0.0],[1.0,1.0],[-1.0,1.0]],c,params);
    }
    pub(super) fn event_streak(&mut self, head:P, streak:P, direction:P, width:f64, c:Color) {
        let tail=head-streak;
        let normal=P::new(direction.y,-direction.x)*width;
        let margin=P::new(normal.x.abs(),normal.y.abs());
        if !visible(head.x.min(tail.x)-margin.x,head.y.min(tail.y)-margin.y,
            streak.x.abs()+2.0*margin.x,streak.y.abs()+2.0*margin.y,self.view) {return;}
        self.quad_unchecked([head+normal,head-normal,tail+normal,tail-normal],
            [[1.0,0.0],[-1.0,0.0],[1.0,1.0],[-1.0,1.0]],c,[22,0,0,0]);
    }
}
// Frame/record guards bound projected components far below 1e150, even after
// the bounded unwrap walk. Squaring is safe in f64; hypot's rescaling is unnecessary
// here. Classic keeps its original arithmetic and fingerprints.
fn prepare_shader<const CULL:bool>(points:&[P],normals:&mut [P],valid:&mut [bool],limits:&mut [f64],widths:&[f64],envelope:f64,view:P) {
    limits.copy_from_slice(widths);
    // Retain raw point validity across the three-point window; a degenerate
    // normal must not invalidate a neighbour's otherwise usable tangent.
    let margin=envelope*1.35;
    let planes=|p:P|->u8 {u8::from(p.x < -margin) | (u8::from(p.x > view.x+margin)<<1)
        | (u8::from(p.y < -margin)<<2) | (u8::from(p.y > view.y+margin)<<3)};
    let mut previous_plane=0;
    let mut current_plane=if CULL {points.first().map_or(0,|p|planes(*p))} else {0};
    let mut previous_finite=false;
    let mut current_finite=points.first().is_some_and(|p|p.finite());
    for i in 0..points.len() {
        let next=i+1<points.len() && points[i+1].finite();
        let next_plane=if CULL && next {planes(points[i+1])} else {current_plane};
        let offscreen=CULL && current_plane!=0
            && current_plane & (if i==0 {current_plane} else {previous_plane}) & next_plane != 0;
        previous_plane=current_plane;current_plane=next_plane;
        let prev=previous_finite;
        let finite=current_finite;
        previous_finite=current_finite;current_finite=next;
        valid[i]=false;
        if !finite || offscreen { continue; }
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
// Reject a wall-bounded snake before normal/taper/ribbon preparation only
// when all endpoints lie beyond the same padded viewport plane. Previous and
// current endpoints bound every interpolation phase; contrail samples have
// their own contribution. History/effects/races still advance outside this loop.
// The broad envelope covers the head quad, gulp, corpse drift and stump glow.
fn snake_on_screen(s:&SnakeRecord,body:&[SegmentRecord],trail:&Trail,p:&Params,scale:f64)->bool {
    let radius=s.radius*scale;
    let margin=radius*(BODY*1.35+CORPSE_DRIFT).max((HEAD_FRONT+HEAD_BOOST_SIDE)*1.24);
    let left=-margin;let right=p.viewport_width+margin;
    let top=-margin;let bottom=p.viewport_height+margin;
    let planes=|x:f64,y:f64|->u8 {
        let x=x*p.scale_x+p.offset_x;let y=y*p.scale_y+p.offset_y;
        u8::from(x<left) | (u8::from(x>right)<<1) | (u8::from(y<top)<<2) | (u8::from(y>bottom)<<3)
    };
    let mut outside=15;
    for seg in body {
        if !segment_valid(seg) {continue;}
        outside &= planes(seg.x as f64,seg.y as f64);
        if moving(s) {outside &= planes(seg.previous_x as f64,seg.previous_y as f64);}
        if outside==0 {return true;}
    }
    if moving(s) && inventory::trailing(s) {
        for i in 0..trail.len {
            let point=trail.points[(trail.head+15-trail.len+i)%15];
            if point.finite() {outside &= planes(point.x,point.y);}
            if outside==0 {return true;}
        }
    }
    outside==0
}
impl Renderer {
    fn effect(&mut self, effect:Effect) {
        self.effects[self.effect_head]=effect;
        self.effect_head=(self.effect_head+1)%self.effects.len();
    }
    pub(super) fn shader_history(&mut self, info:&FrameInfo, snakes:&[SnakeRecord], segments:&[SegmentRecord], events:&[EventRecord]) {
        if self.last_frame.is_some_and(|(tick,g)|info.tick<tick || g!=info.geometry_generation) { self.reset(); }
        let same_frame=self.last_frame==Some((info.tick,info.geometry_generation));
        let needs_fizzles=self.inventory.needs_fizzles(events);
        if same_frame && !needs_fizzles {return;}
        if needs_fizzles {
            let fizzles=self.inventory.observe_fizzles(info,snakes,events,self.event_tick);
            for e in fizzles.into_iter().flatten() {
                let radius=snakes.iter().find(|s|s.id==e.snake_id).map_or(8.0,|s|s.radius);
                self.effect(Effect {p:P::new(e.x as f64,e.y as f64),time:info.simulation_time-(info.tick-e.tick) as f64*crate::STEP_SECONDS,radius,
                    color:e.other_snake_id,kind:13,active:true,snake_id:e.snake_id,generation:0,
                    duration_ticks:if self.reduced_motion {5} else {8},seed:e.tick as u8});
            }
        }
        if same_frame {return;}
        self.inventory.observe(info,snakes,segments,events,self.event_tick);
        self.venom.observe(info,snakes,segments,events,self.event_tick);
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
            let radius=snakes.iter().find(|s|s.id==if e.kind==1 {e.other_snake_id} else {e.snake_id}).map_or(8.0,|s|s.radius);
            if e.kind==crate::EventKind::Feast as u8 && (e.snake_id as usize)<MAX_SNAKES {
                if snakes.iter().any(|s|s.id==e.snake_id && s.generation==e.generation && s.alive!=0) {
                    let waves=&mut self.waves[e.snake_id as usize];waves[1]=waves[0];waves[0]=Wave {time,active:true,kind:8,duration_ticks:e.duration_ticks};
                }
            }
            if e.kind==0 || e.kind==1 || e.kind==4 {
                self.effect(Effect { p:P::new(e.x as f64,e.y as f64),time,radius,
                    color:if e.kind==1 {0x80000004} else {e.color_index},kind:if e.kind==0 || e.kind==1 { 6 } else { 7 },active:true,
                    snake_id:e.snake_id,generation:0,duration_ticks:0,seed:(e.tick as u8).wrapping_add((e.snake_id as u8).wrapping_mul(13)) });
            }
            if matches!(e.kind,2|6|7|14) {
                self.effect(Effect {p:P::new(e.x as f64,e.y as f64),time,radius,
                    color:if e.kind==7 && e.other_snake_id==5 {0x80000005} else {e.other_snake_id},kind:if e.kind==2 {12} else if e.kind==7 && e.other_snake_id==5 {6} else {13},active:true,
                    snake_id:e.snake_id,generation:0,duration_ticks:if e.kind==7 && e.other_snake_id==5 {e.duration_ticks} else if e.kind==14 {9} else if e.kind==15 {if self.reduced_motion {5} else {8}} else {0},seed:e.tick as u8});
                if e.kind==2 && (e.snake_id as usize)<MAX_SNAKES {
                    let waves=&mut self.waves[e.snake_id as usize];
                    waves[1]=waves[0];waves[0]=Wave {time,active:true,kind:e.other_snake_id as u8,..Wave::default()};
                }
            }
            if e.kind==crate::EventKind::Nova as u8 && e.value.is_finite() && e.value>0.0 {
                self.effect(Effect {p:P::new(e.x as f64,e.y as f64),time,radius:e.value as f64,
                    color:0x80000005,kind:18,active:true,snake_id:e.snake_id,generation:e.generation,
                    seed:e.tick as u8,duration_ticks:e.duration_ticks});
            }
            if e.kind==0 && (e.other_snake_id as usize)<MAX_SNAKES {
                let waves=&mut self.waves[e.other_snake_id as usize];
                waves[1]=waves[0];waves[0]=Wave { time,active:true,kind:0,..Wave::default() };
            }
            // Sever payoff (prototype): an acid wave runs down the biter, like a kill's white wave.
            if e.kind==1 && (e.other_snake_id as usize)<MAX_SNAKES {
                let waves=&mut self.waves[e.other_snake_id as usize];
                waves[1]=waves[0];waves[0]=Wave { time,active:true,kind:4,..Wave::default() };
            }
        }
        for s in snakes {
            let id=s.id as usize;
            if !snake_valid(s) || s.segment_count==0 { continue; }
            if s.flags&flags::BOOSTING!=0 && self.shader_flags[id]&flags::BOOSTING==0 {
                let head=segments[s.segment_offset as usize];
                if segment_valid(&head) {
                    self.effect(Effect { p:if s.segment_count>1 { P::new(head.x as f64,head.y as f64) } else { P::new(f64::NAN,f64::NAN) },time:info.simulation_time,
                        radius:s.radius,color:s.color_index,kind:9,active:true,snake_id:s.id,generation:s.generation,duration_ticks:0,seed:info.tick as u8 });
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
        self.inventory.resolve_drops(info,snakes,segments,&self.items[..self.item_count]);
        self.inventory.resolve_novas(info,snakes,segments,p.deadly_walls!=0);
        if p.scale_x==0.0 || p.scale_y==0.0 { return Output::default(); }
        #[cfg(test)]
        let cull=self.shader_culling;
        #[cfg(not(test))]
        let cull=true;
        let motion_scale=if self.reduced_motion { 0.6 } else { 1.0 };
        let view=P::new(p.viewport_width,p.viewport_height);let walls=p.deadly_walls!=0;
        let sx=p.scale_x;let sy=p.scale_y;let scale=(sx*sy).sqrt();
        let arena=P::new(info.world_width,info.world_height);
        let map=|v:P|P::new(v.x*sx+p.offset_x,v.y*sy+p.offset_y);
        let color=|i:u32|palette.get(if palette.is_empty() { 0 } else { i as usize%palette.len() }).copied().unwrap_or(Color::new(0,255,255,255));
        let white_crown=white_crown_palette(palette);
        let mut sink=SpriteSink { out:output,count:0,view };
        self.shader_world_event(info,p,palette,&mut sink);
        // Share event colour/scale across the shower; ordinary food never
        // enters the larger event sprite path.
        let mut event_gold=None;
        for f in food {
            if !food_valid(f) { continue; }
            if matches!(f.kind,5|6) {
                let accent=*event_gold.get_or_insert_with(||events::gold(palette));
                self.shader_event_food(f,info,p,scale,accent,&mut sink);continue;
            }
            let raw_size=f.size as f64*scale;
            let size=raw_size*if matches!(f.kind,3|4) {PRISM_VISUAL} else {1.0};
            let extent=size*FOOD;let pos=P::new(f.x as f64,f.y as f64);
            let mut min=pos;let mut max=pos;
            let mut streak=P::default();let sw=(raw_size*VACUUM).max(0.5);
            if f.attraction>0.0 {
                let pull=P::new(delta(pos.x,f.attraction_x as f64,arena.x,walls)*sx,delta(pos.y,f.attraction_y as f64,arena.y,walls)*sy);
                let len=pull.length();if len>0.001 {
                    streak=pull/len*(raw_size*(3.0+f.attraction as f64*8.0));
                    let end=pos-P::new(streak.x/sx,streak.y/sy);
                    min=P::new(min.x.min(end.x),min.y.min(end.y));max=P::new(max.x.max(end.x),max.y.max(end.y));
                }
            }
            if cull && walls {
                let margin=extent.max(sw);
                let min=map(min);let max=map(max);
                if !visible(min.x-margin,min.y-margin,max.x-min.x+2.0*margin,max.y-min.y+2.0*margin,view) {continue;}
            }
            let phase=f.phase as f64/std::f64::consts::TAU;
            let phase=if (0.0..1.0).contains(&phase) { phase } else { phase.rem_euclid(1.0) };
            let phase=(phase*255.0).round() as u8;
            let c=color(f.color_index);let ripe_age=if f.kind==3 && f.ripe_tick!=0 {
                ((info.tick.saturating_sub(f.ripe_tick) as f64+(p.presentation_time-info.simulation_time).max(0.0)*30.0)*254.0/if self.reduced_motion {18.0} else {30.0}).min(254.0) as u8
            } else {254};
            let params=[if f.kind==3 || f.kind==4 { 8 } else { 2+f.kind.min(2) },phase,f.life_fraction,if f.kind==4 {255} else if f.kind==3 {ripe_age} else {0}];
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
        let mut slipstreams=0;let mut pip_budget=0;
        for snapshot in snakes {
            let visual=self.inventory.visual_snake(snapshot,info,p,segments,self.reduced_motion);
            let s=&visual;
            let n=s.segment_count as usize;let id=s.id as usize;let corpse=s.flags&flags::CORPSE!=0;
            if !snake_valid(s) || (s.alive==0&&!corpse) || n<2 { continue; }
            let body=&segments[s.segment_offset as usize..s.segment_offset as usize+n];
            // Wrapped copies and developer arrows keep their exact old walk.
            if cull && walls && p.developer_mode==0 && !self.inventory.has_transients(s) && !snake_on_screen(s,body,&self.trails[id],p,scale) {continue;}
            let points=&mut self.points[..n];let mapped=&mut self.mapped[..n];
            if walls {
                if moving(s) {
                    for ((point,to),seg) in points.iter_mut().zip(mapped.iter_mut()).zip(body) {
                        *point=position(seg,true,info,p);*to=map(*point);
                    }
                } else {
                    for ((point,to),seg) in points.iter_mut().zip(mapped.iter_mut()).zip(body) {
                        *point=position(seg,false,info,p);*to=map(*point);
                    }
                }
            } else {
                if moving(s) {
                    for (point,seg) in points.iter_mut().zip(body) { *point=position(seg,true,info,p); }
                } else {
                    for (point,seg) in points.iter_mut().zip(body) { *point=position(seg,false,info,p); }
                }
                for i in 0..n {
                    let mut v=P::new(wrap(points[i].x,arena.x),wrap(points[i].y,arena.y));
                    if i>0 && points[i-1].finite() { let prev=points[i-1];v=P::new(prev.x+delta(wrap(prev.x,arena.x),v.x,arena.x,false),prev.y+delta(wrap(prev.y,arena.y),v.y,arena.y,false)); }
                    points[i]=v;
                }
                for (to,point) in mapped.iter_mut().zip(points.iter()) { *to=map(*point); }
            }
            let r=s.radius*scale;
            let (widths,taper_bytes)=if n<=SHORT_TAPER_MAX {short_tapers(n)} else {
                let cache_len=if n>1600 {n.div_ceil(64)*64} else {n};
                let cache_len=cache_len.min(MAX_SEGMENTS);
                if self.taper_lengths[id]!=cache_len || self.shader_taper_lengths[id]!=cache_len {
                    for i in 0..cache_len {
                        let width=crate::shape::taper(i as f64/(cache_len-1) as f64);
                        self.tapers[id*MAX_SEGMENTS+i]=width;
                        self.shader_taper_bytes[id*MAX_SEGMENTS+i]=(width*255.0).round() as u8;
                    }
                    self.taper_lengths[id]=cache_len;self.shader_taper_lengths[id]=cache_len;
                }
                (&self.tapers[id*MAX_SEGMENTS..id*MAX_SEGMENTS+n],
                 &self.shader_taper_bytes[id*MAX_SEGMENTS..id*MAX_SEGMENTS+n])
            };
            if cull && walls && !corpse {
                prepare_shader::<true>(mapped,&mut self.normals[..n],&mut self.valid[..n],&mut self.shader_limits[..n],widths,r*BODY,view);
            } else {
                prepare_shader::<false>(mapped,&mut self.normals[..n],&mut self.valid[..n],&mut self.shader_limits[..n],widths,r*BODY,view);
            }
            let gulp_centers=if s.bulges.iter().any(|b|b.duration_ticks!=0 && b.strength>0.0) {prism::centers(s,info,p,self.reduced_motion)} else {[(-100.0,0.0);2]};
            let gulp=gulp_centers.iter().any(|c|c.1>0.0);
            if gulp {prism::widen(&mut self.normals[..n],gulp_centers);}
            let tier=if n<24 { 0 } else if n<100 { 1 } else if n<250 { 2 } else { 3 };
            let active_kind=if s.alive!=0 && !corpse && s.effect_ticks>0 {s.effect_kind&7} else {0};
            let phase_progress=if active_kind==3 {self.inventory.phase_fade(s,info,p,self.reduced_motion)} else {255};
            let body_kind=if phase_progress<255 {6} else {active_kind};
            let phase_bits=wave_flags(((phase_progress as u16*7+127)/255) as u8);
            let flags=s.flags as u8;let c=frost::ice(color(s.color_index),s.flags,palette);
            let sample=RainbowSample {id:s.id,n,time:event_time(info,p,self.reduced_motion),calm:self.reduced_motion,
                base:color(s.color_index),palette:palette.first().copied().unwrap_or_default(),frozen:s.flags&flags::FROZEN!=0,waves:self.waves[id]};
            let rainbow=if s.alive==0 || corpse || !self.waves[id].iter().any(|w|w.active && w.kind==8) {false}
                else if self.rainbow_sample==Some(sample) {true}
                else {
                    let active=prism::rainbow(s,self.waves[id],info,p,self.reduced_motion,palette,color(s.color_index),&mut self.rainbow[..n]);
                    if active && sample.frozen {for c in &mut self.rainbow[..n] {*c=frost::ice(*c,s.flags,palette);}}
                    self.rainbow_sample=if active {Some(sample)} else {None};active
                };
            if moving(s) && inventory::trailing(s) {
                let trail=&self.trails[id];let mut prev:Option<(P,f64)>=None;
                for i in 0..trail.len { let index=(trail.head+15-trail.len+i)%15;
                    let age=event_time(info,p,self.reduced_motion)-info.simulation_time+info.tick.saturating_sub(trail.ticks[index]) as f64*crate::STEP_SECONDS;
                    if age>=0.5 { continue; }let pos=trail.points[index];
                    let u=1.0-age.max(0.0)/0.5;
                    if let Some((a,ua))=prev { let b=a+P::new(delta(a.x,pos.x,arena.x,walls),delta(a.y,pos.y,arena.y,walls));let w=CONTRAIL*r*ua.max(u);
                        let (xs,ys)=copies(P::new(a.x.min(b.x),a.y.min(b.y)),P::new(a.x.max(b.x),a.y.max(b.y)),P::new(w/sx,w/sy),arena,walls);
                        for x in xs.first..=xs.last { for y in ys.first..=ys.last { let shift=P::new(x as f64*xs.extent*sx,y as f64*ys.extent*sy);sink.contrail(map(a)+shift,map(b)+shift,r,[ua,u],if active_kind==1 {items::accent(1,palette)} else {c.boost()},if active_kind==1 && flags&1!=0 {0.55} else {0.35}); }}
                    }prev=Some((pos,u));
                }
            }
            let age=if corpse { ((event_time(info,p,self.reduced_motion)-self.corpses[id].time)/(0.55*motion_scale)).clamp(0.0,1.0) } else { 0.0 };
            // Select at most two waves once per snake, not once per vertex.
            let mut waves=[(0.0_f64,0.0_f64,0_u8);2];let mut wave_count=0;
            for w in self.waves[id] {
                if w.kind==8 {continue;}
                let center=(event_time(info,p,self.reduced_motion)-w.time)/motion_scale*if w.kind==0 {1.1} else {1.3}*(n-1) as f64;
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
            let mixed=phase_progress==255 && (s.stump_ticks>0 || (wave_count==2 && waves[1].2!=common_origin) || (rainbow && wave_count>0));
            let body_flags=(flags & !50)|if phase_progress<255 {phase_bits} else if mixed || rainbow {0} else {wave_flags(common_origin)};
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
            if s.stump_ticks>0 && !corpse {
                for j in n.saturating_sub(3)..n {
                    self.brightness[j]=((s.stump_ticks.min(48) as f64/48.0)*220.0) as u8;
                    self.wave_origins[j]=wave_flags(4);
                }
            }
            if rainbow {
                for j in 0..n {
                    let light=self.rainbow[j].alpha;
                    if light>self.brightness[j] {self.brightness[j]=light;if mixed {self.wave_origins[j]=0;}}
                }
            }
            // Body envelope includes waves, breathing and taper quantization. Per-edge seam selection
            // bounds even multi-lap bodies, and normals are computed only once.
            if walls && !corpse {
                let params=[0,tier|(body_kind<<2)|if s.flags&flags::FROZEN!=0 {128} else {0}|if white_crown { 64 } else { 0 },body_flags,0];
                if !rainbow && !gulp {
                    if mixed {
                        sink.live_ribbon::<true>(mapped,&self.normals[..n],&self.valid[..n],&self.shader_limits[..n],
                            taper_bytes,&self.brightness[..n],&self.wave_origins[..n],r,c,params);
                    } else {
                        sink.live_ribbon::<false>(mapped,&self.normals[..n],&self.valid[..n],&self.shader_limits[..n],
                            taper_bytes,&self.brightness[..n],&[],r,c,params);
                    }
                } else if mixed {
                    sink.live_prism_ribbon::<true>(mapped,&self.normals[..n],&self.valid[..n],&self.shader_limits[..n],
                        taper_bytes,&self.brightness[..n],&self.wave_origins[..n],r,c,params,&self.rainbow[..n],rainbow,gulp);
                } else {
                    sink.live_prism_ribbon::<false>(mapped,&self.normals[..n],&self.valid[..n],&self.shader_limits[..n],
                        taper_bytes,&self.brightness[..n],&[],r,c,params,&self.rainbow[..n],rainbow,gulp);
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
                let mut params=[0,tier|(body_kind<<2)|if s.flags&flags::FROZEN!=0 {128} else {0}|if i<=3 { 32 } else { 0 }|if white_crown { 64 } else { 0 },body_flags,
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
                let width=w0.max(w1)*if gulp {1.35} else {1.0};let along=[(n-i) as f64,(n-1-i) as f64];
                if walls {
                    let begin=sink.count;
                    sink.body_edge(ma,mb,an,bn,width,across,tapers,along,cc,params,end_wave,end_origin);
                    if rainbow {prism_edge_colors(&mut sink,begin,self.rainbow[i-1],self.rainbow[i]);}
                } else {
                    let margin=P::new((width+r*CORPSE_DRIFT)/sx,(width+r*CORPSE_DRIFT)/sy);
                    let (xs,ys)=copies(P::new(a.x.min(b.x),a.y.min(b.y)),P::new(a.x.max(b.x),a.y.max(b.y)),margin,arena,false);
                    for x in xs.first..=xs.last { for y in ys.first..=ys.last {
                        let shift=P::new(x as f64*xs.extent*sx,y as f64*ys.extent*sy);
                        let begin=sink.count;
                        sink.body_edge(ma+shift,mb+shift,an,bn,width,across,tapers,along,cc,params,end_wave,end_origin);
                        if rainbow {prism_edge_colors(&mut sink,begin,self.rainbow[i-1],self.rainbow[i]);}
                    }}
                }

            }
            }
            if s.stump_ticks>0 && !corpse && self.valid[n-1] {
                // Prototype stump: a 2.2r acid glow on the new tail tip, alpha 0.5 fading over 48 ticks.
                let a=(s.stump_ticks.min(48) as f64/48.0*0.5*255.0).round() as u8;
                let extent=r*2.2;let tip=points[n-1];
                let (xs,ys)=copies(tip,tip,P::new(extent/sx,extent/sy),arena,walls);
                for x in xs.first..=xs.last { for y in ys.first..=ys.last {
                    sink.sprite(mapped[n-1]+P::new(x as f64*xs.extent*sx,y as f64*ys.extent*sy),extent,Color::new(255,255,255,a),[ACID_GLOW,0,0,0]);
                }}
            }
            if corpse {
                inventory::shader_pips(&mut self.inventory,s,points,r,info,p,palette,self.reduced_motion,&mut pip_budget,&mut sink);
                continue;
            }
            if active_kind==1 && slipstreams<4 {
                let before=sink.count;
                inventory::slipstream(s,points,mapped,&self.normals[..n],&self.valid[..n],widths,r,gulp,info,p,palette,&mut sink);
                slipstreams+=usize::from(sink.count>before);
            }
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
            let back=if boosting || active_kind==1 { HEAD_BOOST_BACK } else { HEAD_BACK };
            let half_width=if boosting || active_kind==1 { HEAD_BOOST_SIDE } else { HEAD_SIDE };
            let half_length=(HEAD_FRONT+back)*0.5;let offset=(HEAD_FRONT-back)*0.5;
            // UV stores head units: x forward, y across; tongue included in this quad.
            let a=forward*(hr*half_length);let b=side*(hr*half_width);let center=map(head)+forward*(hr*offset);
            // Head bits 1..4 are the ABI-v3 mood; body glow flags are untouched.
            // Byte y keeps crown/flare and packs 4-bit intensity instead of blink hash.
            // Byte w: 3-bit pupils per axis + low 2 jaw bits; flags bit 7 is jaw bit 2.
            let mood=head_mood(s);
            let intensity=(faces::head_intensity(s) as u16*15/255) as u8;
            let quantize=|v:f32,limit:f32|(v.clamp(-limit,limit)/limit*3.0+3.0).round() as u8;
            let jaw=if mood==5 && ((45-s.happy_ticks.min(45)) as f64)<27.0*motion_scale {7} else if mood==1 && s.jaw_ticks>0 {
                ((std::f64::consts::PI*((39-s.jaw_ticks.min(39)) as f64/(39.0*motion_scale)).min(1.0)).sin()*7.0).round() as u8
            } else {0};
            let look=quantize(s.pupil_x,0.18)|(quantize(s.pupil_y,0.35)<<3)|((jaw&3)<<6);
            let flare=self.waves[id].iter().any(|w|w.active && (0.0..0.5*motion_scale).contains(&(event_time(info,p,self.reduced_motion)-w.time)));
            let head_flags=(flags & 65) | (mood<<1) | ((jaw&4)<<5) | if active_kind==3 {flags::PHASED as u8} else {0};
            let params=[if active_kind==4 {if s.flags&flags::STRIKE!=0 {24} else {23}} else if active_kind==1 {28} else if phase_progress<255 {29+((phase_progress as u16*226+127)/255) as u8} else {1},tier|(intensity<<2)|if white_crown {64} else {0}|if flare {128} else {0},head_flags,look];
            let margin=P::new(hr*(HEAD_FRONT*HEAD_FRONT+half_width*half_width).sqrt()/sx,hr*(HEAD_FRONT*HEAD_FRONT+half_width*half_width).sqrt()/sy);let (xs,ys)=copies(head,head,margin,arena,walls);
            for x in xs.first..=xs.last { for y in ys.first..=ys.last {
                let shift=P::new(x as f64*xs.extent*sx,y as f64*ys.extent*sy);
                sink.quad([center-a-b+shift,center+a-b+shift,center-a+b+shift,center+a+b+shift],
                    [[-back,-half_width],[HEAD_FRONT,-half_width],[-back,half_width],[HEAD_FRONT,half_width]],c.alpha(taper_bytes[1]),params);
            }}
            inventory::shader_pips(&mut self.inventory,s,points,r,info,p,palette,self.reduced_motion,&mut pip_budget,&mut sink);
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
        let event_race=info.world_event.kind==1 && info.world_event.phase!=0 && snakes.iter().any(|s|s.alive!=0 && s.face_flags&48!=0);
        if event_race {self.shader_event_races(info,p,palette,snakes,segments,&mut sink);}
        if snakes.iter().any(|s|s.alive!=0 && s.face_flags&4!=0) {
            self.shader_race_items(info,p,palette,snakes,segments,&mut sink,&self.items[..self.item_count.min(crate::MAX_CAPSULES)],self.item_radius,if event_race {2} else {4});
            self.shader_prism_races(info,p,palette,snakes,segments,food,&mut sink);
        } else if event_race {self.shader_race_items(info,p,palette,snakes,segments,&mut sink,&self.items[..self.item_count.min(crate::MAX_CAPSULES)],self.item_radius,4);}
        else {self.shader_races(info,p,palette,snakes,segments,&mut sink);}
        self.shader_bubbles(info,p,palette,snakes,segments,&mut sink);
        self.shader_orphans(info,p,palette,&mut sink);
        let mut effect_budget=EffectBudget::default();
        for age_index in 0..self.effects.len() {
            let e=self.effects[(self.effect_head+self.effects.len()-1-age_index)%self.effects.len()];
            let duration=if e.duration_ticks>0 {e.duration_ticks as f64*crate::STEP_SECONDS} else {(if e.kind==6 { 0.5 } else if e.kind==9 { 0.28 } else if e.kind>=12 {0.5} else { 0.6 })*motion_scale};let age=event_time(info,p,self.reduced_motion)-e.time;
            if !e.active || !(0.0..duration).contains(&age) || (e.duration_ticks>0 && age+1e-9>=duration) { continue; }
            let extent=if e.kind==18 {1.05} else if e.kind==6 && e.duration_ticks>0 {FROST_CRACK} else if e.kind==6 { IMPACT } else { RING };
            let r=e.radius*scale*extent;let (xs,ys)=copies(e.p,e.p,P::new(r/sx,r/sy),arena,walls);
            for x in xs.first..=xs.last { for y in ys.first..=ys.last {
                if effect_budget.full() {continue;}
                let before=sink.count;
                sink.effect_sprite(map(e.p+P::new(x as f64*xs.extent,y as f64*ys.extent)),r,extent,if e.kind>=12 || e.color&0x80000000!=0 {items::accent(e.color as u8,palette)} else {color(e.color)},[e.kind,if e.kind==7 && white_crown { e.seed|64 } else if e.kind==7 { e.seed&!64 } else { e.seed },if e.kind==6 && e.duration_ticks>0 {1} else {0},(age/duration*255.0).round() as u8]);
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
                    items::accent(2,palette),[15,2,0,self.inventory.magnet_opening(s,info,p,self.reduced_motion)]);
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
    fn viewport_broadphase_preserves_vertices_and_history() {
        let mut fast=Renderer::new();let mut reference=Renderer::new();reference.shader_culling=false;
        let mut output=vec![ShaderVertex::default();30_000];let mut expected=output.clone();
        let palette=[Color::new(30,200,255,255)];
        let food:Vec<_>=(0..600).map(|i|FoodRecord {id:i+1,x:(i%60) as f32*140.0-200.0,
            y:(i/60) as f32*160.0-40.0,size:4.0,phase:i as f32*0.1,
            attraction:if i%3==0 {0.8} else {0.0},attraction_x:3500.0,attraction_y:720.0,
            kind:(i%5) as u8,ripe_tick:1,..Default::default()}).collect();
        let mut body=vec![SegmentRecord::default();12*80];
        let mut snakes:Vec<_>=(0..12).map(|id|SnakeRecord {id,generation:1,alive:1,radius:18.0,
            segment_offset:id*80,segment_count:80,..Default::default()}).collect();
        for tick in 1..=90 {
            let info=FrameInfo {tick,simulation_time:tick as f64/30.0,world_width:7920.0,world_height:1440.0,
                geometry_generation:if tick<60 {0} else {1},..Default::default()};
            for (id,s) in snakes.iter_mut().enumerate() {
                s.flags=if id%3==0 {flags::BOOSTING} else if id%3==1 {flags::CORPSE} else {flags::FROZEN};
                s.alive=u32::from(s.flags&flags::CORPSE==0);s.angle=tick as f64*0.01;
                s.stump_ticks=if id%2==0 {48} else {0};
                s.bulges[0]=crate::world::Bulge {start_tick:tick.saturating_sub(4),duration_ticks:30,origin_segment:5,strength:0.8};
                for j in 0..80 {
                    let x=id as f32*660.0-j as f32*14.0+tick as f32*4.0;
                    let y=if id%4==0 {-100.0} else {100.0+id as f32*115.0};
                    body[id*80+j]=SegmentRecord {x,y,previous_x:x-100.0,previous_y:y-20.0};
                }
            }
            // Invalid neighbours, a long crossing edge, and a moving head that
            // is currently outside but whose previous sample is still visible.
            body[80+3].x=f32::NAN;body[80+12].previous_y=f32::INFINITY;
            body[240+40].x=7500.0;body[240+41].x=0.0;
            for (width,offset) in [(3440.0,0.0),(1920.0,-3440.0),(2560.0,-5360.0)] {
                for (sx,sy) in [(1.0,1.0),(0.25,2.0),(2.0,0.5)] {
                    for alpha in [0.0,0.5,1.0] {
                        let p=Params {viewport_width:width,viewport_height:1440.0,scale_x:sx,scale_y:sy,offset_x:offset,
                            interpolation:alpha,presentation_time:info.simulation_time+alpha/30.0,deadly_walls:1,..Default::default()};
                        let a=fast.build_shader(&info,&snakes,&body,&food,&[],&palette,&p,&mut output);
                        let b=reference.build_shader(&info,&snakes,&body,&food,&[],&palette,&p,&mut expected);
                        assert_eq!(a.vertex_count,b.vertex_count,"tick={tick} offset={offset} scale={sx}/{sy} alpha={alpha}");
                        assert_eq!(&output[..a.vertex_count],&expected[..b.vertex_count]);
                    }
                }
            }
        }
    }

    #[test]
    fn resting_inventory_rejects_offscreen_giant_before_geometry_preparation() {
        let mut r=Renderer::new();
        let s=SnakeRecord {id:0,generation:1,alive:1,radius:18.0,segment_count:6000,inv_count:3,inv_kind:[1,2,3],inv_life:[255;3],..Default::default()};
        let body=vec![SegmentRecord {x:5000.0,y:3000.0,previous_x:4990.0,previous_y:3000.0};6000];
        let info=FrameInfo {tick:600,simulation_time:20.0,world_width:8000.0,world_height:4000.0,..Default::default()};
        let p=Params {viewport_width:1000.0,viewport_height:1000.0,scale_x:1.0,scale_y:1.0,interpolation:1.0,presentation_time:20.0,deadly_walls:1,..Default::default()};
        let marker=P::new(-99.0,-98.0);r.points.fill(marker);r.mapped.fill(marker);r.normals.fill(marker);r.brightness.fill(77);
        assert_eq!(r.build_shader(&info,&[s],&body,&[],&[],&[],&p,&mut []).vertex_count,0);
        assert!(r.points.iter().all(|&v|v==marker));assert!(r.mapped.iter().all(|&v|v==marker));
        assert!(r.normals.iter().all(|&v|v==marker));assert!(r.brightness.iter().all(|&v|v==77));
        // Stash flight from a visible origin still admits the offscreen body.
        let event=EventRecord {tick:601,kind:14,snake_id:0,generation:1,other_snake_id:1,x:100.0,y:100.0,..Default::default()};
        let info=FrameInfo {tick:601,simulation_time:601.0/30.0,..info};let p=Params {presentation_time:info.simulation_time,..p};
        let mut output=[ShaderVertex::default();64];
        let n=r.build_shader(&info,&[s],&body,&[],&[event],&[],&p,&mut output).vertex_count;
        assert!(output[..n].iter().any(|v|v.params[0]==26));assert_ne!(r.points[0],marker);
    }

    #[test]
    fn viewport_broadphase_keeps_visible_trails_and_interpolation() {
        let mut s=SnakeRecord {alive:1,radius:8.0,flags:flags::BOOSTING,..Default::default()};
        let p=Params {viewport_width:100.0,viewport_height:100.0,scale_x:1.0,scale_y:1.0,..Default::default()};
        let mut body=[SegmentRecord {x:300.0,y:40.0,previous_x:290.0,previous_y:40.0};2];
        let mut trail=Trail::default();
        assert!(!snake_on_screen(&s,&body,&trail,&p,1.0));
        trail.points[0]=P::new(50.0,50.0);trail.head=1;trail.len=1;
        assert!(snake_on_screen(&s,&body,&trail,&p,1.0));
        trail.len=0;body[0].previous_x=50.0;
        assert!(snake_on_screen(&s,&body,&trail,&p,1.0));
        s.flags=flags::FROZEN;
        // Frost slows movement rather than stopping it. Its visible previous
        // sample still contributes at intermediate presentation phases.
        assert!(snake_on_screen(&s,&body,&trail,&p,1.0));
        body[0].previous_x=290.0;
        assert!(!snake_on_screen(&s,&body,&trail,&p,1.0));
        // An edge crossing the viewport has no common separating plane.
        body[0].x=-100.0;
        assert!(snake_on_screen(&s,&body,&trail,&p,1.0));
    }

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

fn prism_edge_colors(sink:&mut SpriteSink<'_>,begin:usize,a:Color,b:Color) {
    for (j,v) in sink.out.iter_mut().enumerate().take(sink.count).skip(begin) {
        let c=if [0,1,4].contains(&(j-begin)) {a} else {b};v.color=c.alpha(v.color.alpha);
    }
}
