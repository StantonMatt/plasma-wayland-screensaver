// SPDX-License-Identifier: GPL-3.0-or-later
//! Inventory presentation history is bounded by the physical slots/capsule cap.
//! All ages come from simulation events, including Calm and frame retries.
use super::*;
use super::shader::SpriteSink;
const MAX_VISIBLE_PIPS:usize=3*MAX_SNAKES;
const OFFSETS:[f64;3]=[2.2,4.3,6.4];
#[derive(Clone,Copy,Default)]
struct Motion {time:f64,origin:P,from:f64,kind:u8,active:bool}
#[derive(Clone,Copy,Default)]
struct Held {
    generation:u32,animating:bool,stash:[Motion;3],slide:[Motion;3],fizzle:[Motion;3],
    use_motion:Motion,use_slot:usize,activation:Motion,nova:Motion,nova_radius:f64,nova_duration:f64,
}
#[derive(Clone,Copy,Default)]
pub(super) struct Drop {tick:u64,pub(super) time:f64,pub(super) origin:P,pub(super) target:P,pub(super) kind:u8,pub(super) active:bool,snake_id:u32,generation:u32,slot:usize,pending:bool}
impl Drop {
    fn matches(&self,item:&ItemRecord,info:&FrameInfo)->bool {
        item.state==1 && info.tick.checked_sub(item.age_ticks as u64)==Some(self.tick)
            && self.kind==item.kind && (self.target-P::new(item.x as f64,item.y as f64)).length()<0.01
    }
}
#[derive(Clone,Copy,Default)]
struct Nova {motion:Motion,radius:f64,duration:f64,tick:u64,owner:u32}
#[derive(Clone,Copy,Default)]
struct FizzleEvent {event:EventRecord,present:bool,consumed:bool}
#[derive(Clone,Copy,Default)]
struct NeckCache {points:[P;9],positions:[P;3],radius:f64,len:usize,valid:bool}
pub(super) struct History {necks:[NeckCache;MAX_SNAKES],held:[Held;MAX_SNAKES],pub(super) drops:[Drop;crate::MAX_CAPSULES],drop_head:usize,novas:[Nova;crate::MAX_EVENTS],nova_head:usize,has_novas:bool,fizzles:[FizzleEvent;crate::MAX_EVENTS],fizzle_head:usize,fizzle_pending:bool}
impl Default for History {
    fn default()->Self {Self {necks:[NeckCache::default();MAX_SNAKES],held:[Held::default();MAX_SNAKES],drops:[Drop::default();crate::MAX_CAPSULES],drop_head:0,novas:[Nova::default();crate::MAX_EVENTS],nova_head:0,has_novas:false,fizzles:[FizzleEvent::default();crate::MAX_EVENTS],fizzle_head:0,fizzle_pending:false}}
}
#[inline]
pub(super) fn trailing(s:&SnakeRecord)->bool {
    s.flags&flags::BOOSTING!=0 || (s.effect_kind==1 && s.effect_ticks>0)
}
/// Arc walk over the prepared neck. Ordinary 1.18r edges need at most six;
/// malformed or degenerate borrowed records have a bounded eight-edge walk.
fn neck(points:&[P],distance:f64,forward:P)->P {
    let Some(&head)=points.first() else {return P::new(f64::NAN,f64::NAN);};
    if distance<=0.0 {return head+forward*(-distance);}
    let mut left=distance;
    let mut last=head;
    for edge in points.windows(2).take(8) {
        let d=edge[1]-edge[0];let length=(d.x*d.x+d.y*d.y).sqrt();
        if !length.is_finite() {return P::new(f64::NAN,f64::NAN);}
        if length<1e-9 {continue;}
        if left<=length {return edge[0]+d*(left/length);}
        left-=length;last=edge[1];
    }
    last
}
fn event_neck(s:&SnakeRecord,segments:&[SegmentRecord],slot:usize,info:&FrameInfo)->P {
    let mut points=[P::default();9];let n=(s.segment_count as usize).min(9);
    for i in 0..n {
        let seg=segments[s.segment_offset as usize+i];
        if !segment_valid(&seg) {points[i]=P::new(f64::NAN,f64::NAN);continue;}
        let mut point=P::new(seg.x as f64,seg.y as f64);
        if i>0 {let prev=points[i-1];point=prev+P::new(delta(prev.x,point.x,info.world_width,false),delta(prev.y,point.y,info.world_height,false));}
        points[i]=point;
    }
    neck(&points[..n],OFFSETS[slot]*s.radius,P::new(s.angle.cos(),s.angle.sin()))
}
#[inline]
fn smooth(p:f64)->f64 {let p=p.clamp(0.0,1.0);p*p*(3.0-2.0*p)}
#[inline]
fn close(p:f64)->f64 {1.0-(1.0-p.clamp(0.0,1.0)).powi(3)}
impl History {
    pub(super) fn observe(&mut self,info:&FrameInfo,snakes:&[SnakeRecord],segments:&[SegmentRecord],events:&[EventRecord],cutoff:Option<u64>) {
        for s in snakes {
            if (s.id as usize)>=MAX_SNAKES {continue;}
            let h=&mut self.held[s.id as usize];
            if h.generation!=s.generation {*h=Held {generation:s.generation,..Held::default()};}
            if s.flags&flags::FROZEN==0 {h.nova.active=false;}
            if h.animating {
                for m in &mut h.stash {if info.simulation_time-m.time>=0.37 {m.active=false;}}
                for m in &mut h.slide {if info.simulation_time-m.time>=0.25 {m.active=false;}}
                for m in &mut h.fizzle {if info.simulation_time-m.time>=0.25 {m.active=false;}}
                h.animating=h.stash.iter().chain(h.slide.iter()).chain(h.fizzle.iter()).any(|m|m.active);
            }
        }
        for e in events {
            // Ordinary food/death/world events do not mutate inventory history.
            // Reject them before searching and validating their snake records.
            if !matches!(e.kind,2|3|5|13|14) {continue;}
            if e.tick>info.tick || cutoff.is_some_and(|t|e.tick<=t) || !coordinate32(e.x) || !coordinate32(e.y) {continue;}
            let Some(s)=snakes.iter().find(|s|s.id==e.snake_id && s.generation==e.generation && snake_valid(s)) else {continue;};
            let time=info.simulation_time-(info.tick-e.tick) as f64*crate::STEP_SECONDS;
            if e.kind==crate::EventKind::Nova as u8 && e.value.is_finite() && e.value>0.0 {
                self.has_novas=true;
                self.novas[self.nova_head]=Nova {motion:Motion {time,origin:P::new(e.x as f64,e.y as f64),active:true,..Motion::default()},radius:e.value as f64,duration:e.duration_ticks as f64*crate::STEP_SECONDS,tick:e.tick,owner:s.id};
                self.nova_head=(self.nova_head+1)%self.novas.len();
                continue;
            }
            let h=&mut self.held[s.id as usize];let slot=e.cut_index as usize;
            match e.kind {
                13 if e.duration_ticks==0=>{}, // touch activation never completes a held use
                13 if slot<3 && e.duration_ticks>0=>{
                    h.use_slot=slot;h.use_motion=Motion {time,from:OFFSETS[slot],kind:e.other_snake_id as u8,active:true,..Motion::default()};
                },
                14 if slot<3=>{
                    h.animating=true;
                    h.slide[slot]=Motion::default();
                    h.stash[slot]=Motion {time,origin:P::new(e.x as f64,e.y as f64),kind:e.other_snake_id as u8,active:true,..Motion::default()};
                },
                2=>{
                    if e.other_snake_id!=5 {h.activation=Motion {time,kind:e.other_snake_id as u8,active:true,..Motion::default()};}
                    if h.use_motion.active && e.flags&crate::event_flags::HELD_ACTIVATION!=0 {
                        h.animating=true;
                        let slot=h.use_slot;let slide_time=h.use_motion.time;
                        for j in slot..2 {h.stash[j]=h.stash[j+1];h.slide[j]=Motion {time:slide_time,from:OFFSETS[j+1],active:true,..Motion::default()};}
                        h.stash[2]=Motion::default();h.slide[2]=Motion::default();h.use_motion.active=false;
                    }
                },
                5 if slot<3 && e.duration_ticks==15=>{
                    self.drops[self.drop_head]=Drop {tick:e.tick,time,origin:event_neck(s,segments,slot,info),target:P::new(e.x as f64,e.y as f64),kind:e.other_snake_id as u8,active:true,snake_id:s.id,generation:s.generation,slot,pending:s.segment_count<=1};
                    self.drop_head=(self.drop_head+1)%self.drops.len();
                },_=>{}
            }
        }
    }
    #[inline]
    pub(super) fn needs_fizzles(&self,events:&[EventRecord])->bool {
        self.fizzle_pending || events.iter().any(|e|e.kind==15)
    }
    /// Configuration can append events after the same tick has been presented.
    /// Retain next-tick events too: World::step may clear its export ring first.
    /// The bounded event key makes compact/full retries and both paths idempotent.
    pub(super) fn observe_fizzles(&mut self,info:&FrameInfo,snakes:&[SnakeRecord],events:&[EventRecord],cutoff:Option<u64>)->[Option<EventRecord>;crate::MAX_EVENTS] {
        if !self.fizzle_pending && !events.iter().any(|e|e.kind==15) {return [None;crate::MAX_EVENTS];}
        for e in events {
            if e.kind!=15 || e.cut_index>=3 || e.tick>info.tick.saturating_add(1) || cutoff.is_some_and(|t|e.tick<t)
                || !coordinate32(e.x) || !coordinate32(e.y) {continue;}
            if !snakes.iter().any(|s|s.id==e.snake_id && s.generation==e.generation && snake_valid(s)) {continue;}
            if self.fizzles.iter().any(|f|f.present && f.event.tick==e.tick && f.event.snake_id==e.snake_id
                && f.event.generation==e.generation && f.event.cut_index==e.cut_index && f.event.other_snake_id==e.other_snake_id
                && f.event.x==e.x && f.event.y==e.y) {continue;}
            self.fizzles[self.fizzle_head]=FizzleEvent {event:*e,present:true,consumed:false};
            self.fizzle_head=(self.fizzle_head+1)%self.fizzles.len();
        }
        let mut accepted=[None;crate::MAX_EVENTS];let mut count=0;self.fizzle_pending=false;
        for index in 0..self.fizzles.len() {
            let f=&mut self.fizzles[(self.fizzle_head+index)%crate::MAX_EVENTS];
            if !f.present || f.consumed {continue;}
            if f.event.tick>info.tick {self.fizzle_pending=true;continue;}
            f.consumed=true;let e=f.event;
            let Some(s)=snakes.iter().find(|s|s.id==e.snake_id && s.generation==e.generation && snake_valid(s)) else {continue;};
            let h=&mut self.held[s.id as usize];
            if h.generation!=s.generation {*h=Held {generation:s.generation,..Held::default()};}
            let slot=e.cut_index as usize;let time=info.simulation_time-(info.tick-e.tick) as f64*crate::STEP_SECONDS;
            let index=h.fizzle.iter().position(|m|!m.active || time-m.time>=8.0*crate::STEP_SECONDS).unwrap_or(slot);
            h.animating=true;
            h.fizzle[index]=Motion {time,origin:P::new(e.x as f64,e.y as f64),kind:e.other_snake_id as u8,active:true,..Motion::default()};
            for j in slot..2 {h.stash[j]=h.stash[j+1];h.slide[j]=Motion {time,from:OFFSETS[j+1],active:true,..Motion::default()};}
            h.stash[2]=Motion::default();h.slide[2]=Motion::default();
            if h.use_motion.active {if slot<h.use_slot {h.use_slot-=1;} else if slot==h.use_slot {h.use_motion.active=false;}}
            accepted[count]=Some(e);count+=1;
        }
        accepted
    }
    pub(super) fn resolve_novas(&mut self,info:&FrameInfo,snakes:&[SnakeRecord],segments:&[SegmentRecord],walls:bool) {
        if !self.has_novas {return;}
        self.has_novas=false;
        for index in 0..self.novas.len() {
            let nova=&mut self.novas[(self.nova_head+index)%crate::MAX_EVENTS];
            if !nova.motion.active {continue;}
            if info.simulation_time-nova.motion.time>=nova.duration.max(crate::STEP_SECONDS) {nova.motion.active=false;continue;}
            self.has_novas=true;
            for victim in snakes {
                // Compact records contain tails, never heads. Keep candidates
                // until full geometry arrives before deciding distance eligibility.
                if !snake_valid(victim) || victim.id==nova.owner || victim.flags&flags::FROZEN==0 || victim.segment_count<=1 {continue;}
                let h=&mut self.held[victim.id as usize];
                if h.nova.active || victim.frozen_ticks>0 && victim.frozen_ticks as u64+info.tick.saturating_sub(nova.tick)!=75 {continue;}
                let head=segments[victim.segment_offset as usize];
                if !segment_valid(&head) {continue;}
                let d=P::new(delta(nova.motion.origin.x,head.x as f64,info.world_width,walls),delta(nova.motion.origin.y,head.y as f64,info.world_height,walls));
                if d.length()>nova.radius {continue;}
                h.nova=nova.motion;h.nova_radius=nova.radius;h.nova_duration=nova.duration;
            }
        }
    }
    pub(super) fn has_transients(&self,s:&SnakeRecord)->bool {
        let h=&self.held[s.id as usize];
        h.animating && h.stash.iter().chain(h.fizzle.iter()).any(|m|m.active)
    }
    pub(super) fn has_fizzle(&self,s:&SnakeRecord)->bool {let h=&self.held[s.id as usize];h.animating && h.fizzle.iter().any(|m|m.active)}
    pub(super) fn phase_fade(&self,s:&SnakeRecord,info:&FrameInfo,p:&Params,calm:bool)->u8 {
        // Use the retained activation clock, independent of procedural motion.
        let m=self.held[s.id as usize].activation; // shared activation clock below
        if !m.active || m.kind!=3 {return 255;}
        ((event_time(info,p,calm)-m.time)/0.1*255.0).clamp(0.0,255.0).round() as u8
    }
    pub(super) fn resolve_drops(&mut self,info:&FrameInfo,snakes:&[SnakeRecord],segments:&[SegmentRecord],items:&[ItemRecord]) {
        for d in &mut self.drops {
            if !d.active {continue;}
            // Keep the current boundary for compact/full frame retries, where
            // the capsule list may arrive after its event. Later boundaries
            // retire disappeared capsules and completed animations.
            if info.simulation_time-d.time>=1.0 || info.tick>d.tick && !items.iter().any(|item|d.matches(item,info)) {
                d.active=false;continue;
            }
            if d.pending {
                if let Some(s)=snakes.iter().find(|s|s.id==d.snake_id && s.generation==d.generation && s.segment_count>1 && snake_valid(s)) {
                    d.origin=event_neck(s,segments,d.slot,info);d.pending=false;
                }
            }
        }
    }
    pub(super) fn magnet_opening(&self,s:&SnakeRecord,info:&FrameInfo,p:&Params,calm:bool)->u8 {
        let m=self.held[s.id as usize].activation;
        if !m.active || m.kind!=2 || calm {return 255;}
        ((event_time(info,p,calm)-m.time)/0.35*255.0).clamp(1.0,255.0).round() as u8
    }
    pub(super) fn visual_snake(&self,s:&SnakeRecord,info:&FrameInfo,p:&Params,segments:&[SegmentRecord],calm:bool)->SnakeRecord {
        let mut visual=*s;
        if !snake_valid(s) {return visual;}
        let h=&self.held[s.id as usize];
        if s.flags&flags::FROZEN!=0 && h.nova.active && s.segment_count>0 {
            let age=(event_time(info,p,calm)-h.nova.time)/h.nova_duration.max(crate::STEP_SECONDS);
            let front=h.nova_radius*(0.08+0.88*close(age));
            let head=position(&segments[s.segment_offset as usize],moving(s),info,p);
            let d=P::new(delta(h.nova.origin.x,head.x,info.world_width,p.deadly_walls!=0),delta(h.nova.origin.y,head.y,info.world_height,p.deadly_walls!=0));
            if age<1.0 && d.length()>front {
                visual.flags&=!flags::FROZEN;visual.frozen_ticks=0;visual.breath_ticks=0;
                if visual.mood==8 {visual.mood=0;visual.mood_intensity=0;}
            }
        }
        visual
    }
    pub(super) fn drop_pose(&self,item:&ItemRecord,info:&FrameInfo,p:&Params,calm:bool)->(P,f64) {
        let target=P::new(item.x as f64,item.y as f64);
        if item.state!=1 {return (target,1.0);}
        let Some(d)=self.drops.iter().find(|d|d.active && d.matches(item,info)) else {return (target,1.0);};
        let age=(event_time(info,p,calm)-d.time).max(0.0);
        let move_p=close(age/if calm {0.21} else {0.35});
        let delta=P::new(delta(d.origin.x,target.x,info.world_width,p.deadly_walls!=0),delta(d.origin.y,target.y,info.world_height,p.deadly_walls!=0));
        (d.origin+delta*move_p,1.0/2.47+(1.0-1.0/2.47)*smooth(age/0.12))
    }
}
#[derive(Clone,Copy,Default)]
struct Pip {pos:P,kind:u8,life:u8,anim:u8,scale:f64}
fn pips(h:&History,s:&SnakeRecord,points:&[P],info:&FrameInfo,p:&Params,calm:bool)->([Pip;3],usize) {
    let held=&h.held[s.id as usize];let now=event_time(info,p,calm);
    let forward=if s.inv_windup>0 {P::new(s.angle.cos(),s.angle.sin())} else {P::default()};
    let mut lengths=[0.0;9];
    let neck_count=points.len().min(9);
    for j in 1..neck_count {let d=points[j]-points[j-1];lengths[j]=lengths[j-1]+(d.x*d.x+d.y*d.y).sqrt();}
    let locate=|distance:f64| {
        if distance<=0.0 {return points[0]+forward*(-distance);}
        for j in 1..neck_count {if !lengths[j].is_finite() {return P::new(f64::NAN,f64::NAN);}if distance<=lengths[j] {let span=lengths[j]-lengths[j-1];if span>1e-9 {return points[j-1]+(points[j]-points[j-1])*((distance-lengths[j-1])/span);}}}
        points[neck_count-1]
    };
    let mut out=[Pip::default();3];let mut count=0;let mut flying=None;
    if s.alive!=0 && s.flags&flags::CORPSE==0 {
        for slot in 0..(s.inv_count as usize).min(3) {
            let kind=s.inv_kind[slot];if !(1..=6).contains(&kind) {continue;}
            let mut distance=OFFSETS[slot];let slide=held.slide[slot];
            if slide.active {distance=slide.from+(distance-slide.from)*close((now-slide.time)/if calm {0.15} else {0.25});}
            if s.inv_windup>0 && slot+1>s.inv_windup as usize && held.use_motion.active {
                distance=OFFSETS[slot]+(OFFSETS[slot-1]-OFFSETS[slot])*close((now-held.use_motion.time)/if calm {0.15} else {0.25});
            }
            let mut pip=Pip {pos:locate(distance*s.radius),kind,life:s.inv_life[slot],scale:1.0,anim:0};
            // The ABI windup follows compaction; never trust a stale event slot.
            if s.inv_windup as usize==slot+1 && held.use_motion.active {
                let progress=smooth((now-held.use_motion.time)/(4.0*crate::STEP_SECONDS));
                pip.pos=locate((held.use_motion.from+(-0.35-held.use_motion.from)*progress)*s.radius);
                pip.scale=1.0-0.45*progress;pip.anim=128+(progress*127.0).round() as u8;
                flying=Some(pip);continue;
            }
            let stash=held.stash[slot];
            if stash.active && stash.kind==kind {
                let age=(now-stash.time).max(0.0);
                if !calm && age<0.22 {
                    let progress=smooth(age/0.22);
                    let d=P::new(delta(stash.origin.x,pip.pos.x,info.world_width,p.deadly_walls!=0),delta(stash.origin.y,pip.pos.y,info.world_height,p.deadly_walls!=0));
                    pip.pos=stash.origin+d*progress;pip.scale=2.47+(1.0-2.47)*progress;
                } else if !calm && age<0.37 {pip.scale=1.0+0.15*(1.0-smooth((age-0.22)/0.15));}
            }
            out[count]=pip;count+=1;
        }
    }
    for f in held.fizzle {
        let age=now-f.time;let duration=if calm {0.15} else {0.25};
        if count<3-usize::from(flying.is_some()) && f.active && (0.0..duration).contains(&age) {
            out[count]=Pip {pos:f.origin,kind:f.kind,life:0,anim:0,scale:1.0-smooth(age/duration)};count+=1;
        }
    }
    if let Some(pip)=flying {out[count]=pip;count+=1;}
    (out,count)
}
#[inline]
pub(super) fn shader_pips(h:&mut History,s:&SnakeRecord,points:&[P],r:f64,info:&FrameInfo,p:&Params,palette:&[Color],calm:bool,budget:&mut usize,sink:&mut SpriteSink<'_>) {
    if *budget>=MAX_VISIBLE_PIPS || (s.inv_count==0 && !h.has_fizzle(s)) {return;}
    // Resting gems share the same neck arc samples across a redraw/retry.
    // Compare the actual borrowed points: even same-tick mutable diagnostics
    // and viewport interpolation changes invalidate this cache correctly.
    let held=&h.held[s.id as usize];
    let resting=s.alive!=0 && s.flags&flags::CORPSE==0 && s.inv_windup==0
        && !held.animating;
    let (pips,n)=if resting {
        let cache=&mut h.necks[s.id as usize];let len=points.len().min(9);
        if !cache.valid || cache.len!=len || cache.radius!=s.radius || cache.points[..len]!=points[..len] {
            cache.points[..len].copy_from_slice(&points[..len]);cache.len=len;cache.radius=s.radius;cache.valid=true;
            let mut edge=1;let mut distance=0.0;let d=points[1]-points[0];let mut span=(d.x*d.x+d.y*d.y).sqrt();
            for (slot,pos) in cache.positions.iter_mut().enumerate() {
                let wanted=OFFSETS[slot]*s.radius;
                while edge<len && distance+span<wanted {
                    distance+=span;edge+=1;
                    if edge<len {let d=points[edge]-points[edge-1];span=(d.x*d.x+d.y*d.y).sqrt();}
                }
                *pos=if !span.is_finite() {P::new(f64::NAN,f64::NAN)} else if edge<len && span>1e-9 {
                    points[edge-1]+(points[edge]-points[edge-1])*((wanted-distance)/span)
                } else {points[len-1]};
            }
        }
        let mut pips=[Pip::default();3];let mut n=0;
        for slot in 0..(s.inv_count as usize).min(3) {
            if !(1..=6).contains(&s.inv_kind[slot]) {continue;}
            pips[n]=Pip {pos:cache.positions[slot],kind:s.inv_kind[slot],life:s.inv_life[slot],scale:1.0,anim:0};n+=1;
        }
        (pips,n)
    } else {pips(h,s,points,info,p,calm)};
    shader_pip_list(pips,n,r,info,p,palette,budget,sink);
}
fn shader_pip_list(pips:[Pip;3],n:usize,r:f64,info:&FrameInfo,p:&Params,palette:&[Color],budget:&mut usize,sink:&mut SpriteSink<'_>) {
    for pip in &pips[..n] {
        let extent=r*0.85*2.0*pip.scale;
        if p.deadly_walls!=0 {
            let before=sink.count;
            sink.sprite(P::new(pip.pos.x*p.scale_x+p.offset_x,pip.pos.y*p.scale_y+p.offset_y),extent,items::accent(pip.kind,palette),[26,pip.kind,pip.life,pip.anim]);
            *budget+=usize::from(sink.count>before);if *budget>=MAX_VISIBLE_PIPS {break;}continue;
        }
        let (xs,ys)=copies(pip.pos,pip.pos,P::new(extent/p.scale_x,extent/p.scale_y),P::new(info.world_width,info.world_height),false);
        for x in xs.first..=xs.last {for y in ys.first..=ys.last {
            let pos=pip.pos+P::new(x as f64*xs.extent,y as f64*ys.extent);
            if *budget>=MAX_VISIBLE_PIPS {continue;}
            let before=sink.count;
            sink.sprite(P::new(pos.x*p.scale_x+p.offset_x,pos.y*p.scale_y+p.offset_y),extent,items::accent(pip.kind,palette),[26,pip.kind,pip.life,pip.anim]);
            *budget+=usize::from(sink.count>before);
        }}
    }
}
pub(super) fn slipstream(s:&SnakeRecord,points:&[P],mapped:&[P],normals:&[P],valid:&[bool],widths:&[f64],r:f64,gulp:bool,info:&FrameInfo,p:&Params,palette:&[Color],sink:&mut SpriteSink<'_>) {
    slipstream_impl::<true>(s,points,mapped,normals,valid,widths,r,gulp,info,p,palette,sink);
}
fn slipstream_impl<const REUSE:bool>(s:&SnakeRecord,points:&[P],mapped:&[P],normals:&[P],valid:&[bool],widths:&[f64],r:f64,gulp:bool,info:&FrameInfo,p:&Params,palette:&[Color],sink:&mut SpriteSink<'_>) {
    if r<=0.0 || s.radius<=0.0 {return;}
    let n=points.len();
    let mapped=&mapped[..n];let normals=&normals[..n];let valid=&valid[..n];let widths=&widths[..n];
    let length=if s.flags&flags::BOOSTING!=0 {14.0} else {10.0};let mut along=-1.0;
    let color=items::accent(1,palette);let begin=sink.count;
    // Prepared normals are unit length; feeding is their only enlargement.
    // A shared conservative bound avoids two square roots on every edge.
    let width=r*2.0*if gulp {1.35} else {1.0};
    let params=[27,0,0,(length/14.0*255.0_f64).round() as u8];
    // One leading edge starts beside the head; the other thirteen follow the
    // prepared body. Wrapped visible copies share the same 84-vertex budget.
    let forward=points[0]-points[1];let span=(forward.x*forward.x+forward.y*forward.y).sqrt();
    if !span.is_finite() || span<1e-9 {return;}
    let prefix=points[0]+forward*(s.radius/span);
    let mut previous=None;
    for i in 0..points.len().min(14) {
        let (wa,wb,a,b,an,bn,next)=if i==0 {
            let a=P::new(prefix.x*p.scale_x+p.offset_x,prefix.y*p.scale_y+p.offset_y);
            (prefix,points[0],a,mapped[0],normals[0]*(r*2.0*widths[0]),normals[0]*(r*2.0*widths[0]),0.0)
        } else {
            let d=points[i]-points[i-1];let next=along+(d.x*d.x+d.y*d.y).sqrt()/s.radius;
            (points[i-1],points[i],mapped[i-1],mapped[i],normals[i-1]*(r*2.0*widths[i-1]),normals[i]*(r*2.0*widths[i]),next)
        };
        if valid[i] && valid[i.saturating_sub(1)] {
            if p.deadly_walls!=0 {
                if sink.count-begin>=84 {return;}
                // Adjacent wall-bounded quads share their two corners and
                // attributes. Convert each pair once, retaining exact f64
                // arithmetic and the same triangle-list order.
                if !REUSE {
                    sink.body_edge(a,b,an,bn,width,[1.0;2],[255;2],[along,next],color,params,params[3],None);
                } else if visible(a.x.min(b.x)-width,a.y.min(b.y)-width,(b.x-a.x).abs()+2.0*width,(b.y-a.y).abs()+2.0*width,sink.view) {
                    let pair=previous.unwrap_or_else(||[
                        SpriteSink::make_vertex(a+an,1.0,along,color,params),
                        SpriteSink::make_vertex(a-an,-1.0,along,color,params)]);
                    let current=[SpriteSink::make_vertex(b+bn,1.0,next,color,params),
                        SpriteSink::make_vertex(b-bn,-1.0,next,color,params)];
                    sink.push_quad([pair[0],pair[1],current[0],current[1]]);
                    previous=Some(current);
                } else {previous=None;}
            } else {
                let (xs,ys)=copies(P::new(wa.x.min(wb.x),wa.y.min(wb.y)),P::new(wa.x.max(wb.x),wa.y.max(wb.y)),P::new(width/p.scale_x,width/p.scale_y),P::new(info.world_width,info.world_height),false);
                for x in xs.first..=xs.last {for y in ys.first..=ys.last {
                    if sink.count-begin>=84 {return;}
                    let shift=P::new(x as f64*xs.extent*p.scale_x,y as f64*ys.extent*p.scale_y);
                    sink.body_edge(a+shift,b+shift,an,bn,width,[1.0;2],[255;2],[along,next],color,params,params[3],None);
                }}
            }
        } else {previous=None;}
        along=next;if along>length+1.0 {break;}
    }
}

impl Renderer {
    pub(super) fn classic_pips(&self,s:&SnakeRecord,points:&[P],info:&FrameInfo,p:&Params,palette:&[Color],sink:&mut Sink<'_>) {
        if s.inv_count==0 && !self.inventory.has_fizzle(s) {return;}
        let (pips,n)=pips(&self.inventory,s,points,info,p,self.reduced_motion);
        for pip in &pips[..n] {
            let radius=s.radius*(p.scale_x*p.scale_y).sqrt()*0.85*pip.scale;
            let (xs,ys)=copies(pip.pos,pip.pos,P::new(radius*2.0/p.scale_x,radius*2.0/p.scale_y),P::new(info.world_width,info.world_height),p.deadly_walls!=0);
            for x in xs.first..=xs.last {for y in ys.first..=ys.last {
                let pos=pip.pos+P::new(x as f64*xs.extent,y as f64*ys.extent);let center=P::new(pos.x*p.scale_x+p.offset_x,pos.y*p.scale_y+p.offset_y);
                let c=items::accent(pip.kind,palette);
                sink.disc(center,radius*1.19,Color::new(2,3,6,255),6);sink.disc(center,radius,c,6);
                sink.disc(center,radius*0.78,Color::new(6,7,14,255),6);
                self.classic_icon(center,radius*0.64,pip.kind,c.boost().fade(0.6+0.4*pip.life as f64/255.0),sink);
            }}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{World,Config,RuleSet,Point,Inventory,controller::{Controller,Steering}};
    #[test]
    fn slipstream_shared_corners_preserve_vertices_at_viewport_breaks_and_capacity_limits() {
        let points:Vec<_>=(0..40).map(|i|P::new(120.0-i as f64*7.0,10.0+(i as f64*0.3).sin()*8.0)).collect();
        let mut normals=vec![P::default();40];let mut valid=vec![false;40];
        prepare(&points,&mut normals,&mut valid);
        let widths:Vec<_>=(0..40).map(|i|0.9-i as f64*0.01).collect();
        let info=FrameInfo {world_width:200.0,world_height:100.0,..Default::default()};
        let snake=SnakeRecord {radius:6.0,..Default::default()};
        for walls in [0,1] {for gulp in [false,true] {for offset in [-110.0,0.0,100.0] {for gap in [false,true] {for capacity in [0,1,5,6,23,84] {
            let p=Params {scale_x:1.0,scale_y:1.0,offset_x:offset,deadly_walls:walls,..Default::default()};
            let mapped:Vec<_>=points.iter().map(|q|P::new(q.x+offset,q.y)).collect();
            let mut valid=valid.clone();if gap {valid[3]=false;valid[8]=false;}
            let render=|reuse:bool| {
                let mut out=vec![ShaderVertex::default();capacity];
                let mut sink=SpriteSink {out:&mut out,count:0,view:P::new(100.0,100.0)};
                if reuse {slipstream_impl::<true>(&snake,&points,&mapped,&normals,&valid,&widths,6.0,gulp,&info,&p,&[],&mut sink);}
                else {slipstream_impl::<false>(&snake,&points,&mapped,&normals,&valid,&widths,6.0,gulp,&info,&p,&[],&mut sink);}
                (sink.count,out)
            };
            assert_eq!(render(true),render(false),"walls={walls} gulp={gulp} offset={offset} gap={gap} capacity={capacity}");
        }}}}}
    }
    #[test]
    fn drop_history_matches_spawn_boundary_and_retires_stale_records() {
        let item=ItemRecord {state:1,kind:1,x:10.0,y:20.0,age_ticks:0,..Default::default()};
        let mut info=FrameInfo {tick:10,simulation_time:10.0/30.0,..Default::default()};
        let mut h=History::default();
        h.drops[0]=Drop {tick:10,time:info.simulation_time,kind:1,target:P::new(10.0,20.0),active:true,..Default::default()};
        assert!(h.drops[0].matches(&item,&info));
        info.tick=11;info.simulation_time=11.0/30.0;
        assert!(!h.drops[0].matches(&item,&info),"a new spawn at the same target is a different event");
        h.drops[1]=Drop {tick:11,time:info.simulation_time,origin:P::new(30.0,40.0),..h.drops[0]};
        let p=Params {presentation_time:info.simulation_time,..Default::default()};
        for calm in [false,true] {
            let (origin,scale)=h.drop_pose(&item,&info,&p,calm);
            assert_eq!(origin,P::new(30.0,40.0));assert!((scale-1.0/2.47).abs()<1e-12);
        }
        h.drops[1].active=false;
        h.resolve_drops(&info,&[],&[],&[ItemRecord {age_ticks:1,..item}]);
        assert!(h.drops[0].active);
        h.resolve_drops(&info,&[],&[],&[]);assert!(!h.drops[0].active,"missing capsule retires its drop");
        h.drops[0].active=true;info.tick=40;info.simulation_time=40.0/30.0;
        h.resolve_drops(&info,&[],&[],&[ItemRecord {age_ticks:30,..item}]);
        assert!(!h.drops[0].active,"completed animation retires even a surviving capsule");
    }
    struct Use(u32);
    impl Controller for Use {
        fn steer(&mut self,_:&World,s:crate::SnakeView<'_>)->Steering {Steering {desired_angle:s.angle,rush:0.0}}
        fn use_request(&self,_:u32)->u32 {self.0}
    }
    fn arena(points:&[Point])->World {
        let snakes:Vec<_>=points.iter().map(|&p|(p,0.0,30,0.0)).collect();
        World::diagnostic_arena(Config {rules:RuleSet::V2,width:4000.0,height:2000.0,density:0.0,
            deadly_walls:false,self_collisions:false,..Default::default()},&snakes,&[]).unwrap()
    }
    // Borrow the same production views used by snakes_core_export_frame/extras.
    // Keep just the render inputs relevant to these state-machine regressions.
    fn exported(w:&World)->(FrameInfo,Vec<SnakeRecord>,Vec<SegmentRecord>,Vec<EventRecord>) {
        let mut body=Vec::new();let mut snakes=Vec::new();
        for s in w.snakes() {
            let offset=body.len();
            for seg in s.segments {body.push(SegmentRecord {x:seg.current.x as f32,y:seg.current.y as f32,previous_x:seg.previous.x as f32,previous_y:seg.previous.y as f32});}
            snakes.push(SnakeRecord {id:s.id,generation:s.generation,alive:u32::from(s.alive),radius:s.radius,angle:s.angle,
                segment_count:s.segments.len() as u32,segment_offset:offset as u32,flags:s.flags,
                frozen_ticks:s.face.frozen_ticks,effect_kind:s.effect_kind,effect_ticks:s.effect_ticks,
                inv_count:s.inventory.count,inv_kind:s.inventory.kinds,inv_life:s.inventory.life_bytes(),inv_windup:s.inventory.windup,..Default::default()});
        }
        let events=w.frame_events().map(|e|EventRecord {tick:e.tick,kind:e.kind as u8,flags:e.flags,snake_id:e.snake_id,generation:e.generation,
            x:e.position.x as f32,y:e.position.y as f32,other_snake_id:e.other_snake_id,cut_index:e.cut_index,duration_ticks:e.duration_ticks,value:e.value,..Default::default()}).collect();
        (FrameInfo {tick:w.tick,simulation_time:w.time,world_width:w.config.width,world_height:w.config.height,..Default::default()},snakes,body,events)
    }
    fn draw(r:&mut Renderer,w:&World)->Vec<ShaderVertex> {
        let (info,snakes,body,events)=exported(w);let mut out=vec![ShaderVertex::default();4096];
        let p=Params {viewport_width:4000.0,viewport_height:2000.0,scale_x:1.0,scale_y:1.0,interpolation:1.0,presentation_time:info.simulation_time,deadly_walls:1,..Default::default()};
        let n=r.build_shader(&info,&snakes,&body,&[],&events,&[Color::new(77,230,255,255)],&p,&mut out).vertex_count;
        out.truncate(n);out
    }
    fn capsule(w:&mut World,kind:crate::effects::EffectKind) {
        w.items.push(crate::Item {kind,position:w.segments[0].current,radius:16.8,life_ticks:750,..Default::default()});
    }
    #[test]
    fn world_step_touch_preserves_selected_flight_until_real_activation() {
        let mut w=arena(&[Point{x:1500.0,y:1000.0}]);
        w.snakes[0].inventory=Inventory {kinds:[3,1,5],life:[150;3],count:3,..Default::default()};
        let mut r=Renderer::new();w.step(&mut Use(2));draw(&mut r,&w);
        capsule(&mut w,crate::effects::EffectKind::Venom);w.step(&mut Use(0));
        assert!(w.frame_events().any(|e|e.kind==crate::EventKind::Use && e.duration_ticks==0));
        assert_eq!(w.snakes[0].inventory.windup,2);
        let out=draw(&mut r,&w);assert_eq!(out.iter().filter(|v|v.params[0]==26 && v.params[3]>=128).count(),6);
        for _ in 0..3 {w.step(&mut Use(0));draw(&mut r,&w);}
        assert_eq!(w.snakes[0].inventory.count,2);assert_eq!(w.snakes[0].inventory.windup,0);
        assert!(!r.inventory.held[0].use_motion.active);assert!(r.inventory.held[0].slide[1].active);
    }
    #[test]
    fn world_step_held_completion_then_same_kind_touch_compacts_surviving_gems() {
        for calm in [false,true] {for kind in 1..=5 {
            let mut w=arena(&[Point{x:1500.0,y:1000.0}]);
            w.snakes[0].inventory=Inventory {kinds:[3,kind,5],life:[150;3],count:3,..Default::default()};
            let mut r=Renderer::new();r.reduced_motion=calm;
            w.step(&mut Use(2));draw(&mut r,&w);
            for _ in 0..3 {w.step(&mut Use(0));draw(&mut r,&w);}
            capsule(&mut w,crate::effects::EffectKind::Magnet);
            capsule(&mut w,crate::effects::EffectKind::from_byte(kind));
            w.step(&mut Use(0));
            let activations:Vec<_>=w.frame_events().filter(|e|matches!(e.kind,crate::EventKind::Pickup|crate::EventKind::Stash|crate::EventKind::Use)).map(|e|(e.kind,e.other_snake_id)).collect();
            assert_eq!(activations,[(crate::EventKind::Pickup,kind as u32),(crate::EventKind::Stash,2),(crate::EventKind::Use,kind as u32),(crate::EventKind::Pickup,kind as u32)]);
            assert_eq!(w.snakes[0].inventory.kinds,[3,5,2]);
            let pickups:Vec<_>=w.frame_events().filter(|e|e.kind==crate::EventKind::Pickup).collect();
            assert_eq!(pickups[0].flags,crate::event_flags::HELD_ACTIVATION);
            assert_eq!(pickups[0].cut_index,1);assert_eq!(pickups[1].flags,0);
            draw(&mut r,&w);
            let held=&r.inventory.held[0];
            assert!(!held.use_motion.active,"later same-kind touch must not suppress held completion");
            assert!(held.slide[1].active,"surviving Frost gem must close the gap");
            assert_eq!(held.slide[1].from,OFFSETS[2]);
            assert!(held.stash[2].active);assert_eq!(held.stash[2].kind,2);
        }}
    }
    #[test]
    fn world_step_expire_and_collect_starts_new_slot_at_rest() {
        let mut w=arena(&[Point{x:1500.0,y:1000.0}]);
        w.snakes[0].inventory=Inventory {kinds:[3,0,0],life:[1,0,0],count:1,..Default::default()};
        capsule(&mut w,crate::effects::EffectKind::Magnet);w.step(&mut Use(0));
        let events:Vec<_>=w.frame_events().map(|e|e.kind).collect();
        assert!(events.contains(&crate::EventKind::Fizzle));assert!(events.contains(&crate::EventKind::Stash));
        let mut r=Renderer::new();r.reduced_motion=true;draw(&mut r,&w);
        assert!(r.inventory.held[0].stash[0].active);assert!(!r.inventory.held[0].slide[0].active);
    }
    #[test]
    fn world_configuration_fizzle_survives_next_step_clearing_export() {
        for calm in [false,true] {
            let mut w=arena(&[Point{x:1500.0,y:1000.0}]);
            w.snakes[0].inventory=Inventory {kinds:[3,1,5],life:[150;3],count:3,..Default::default()};
            w.step(&mut Use(0));let mut r=Renderer::new();r.reduced_motion=calm;draw(&mut r,&w);
            w.reconfigure(Config {store_power_ups:false,..w.config()}).unwrap();
            assert_eq!(w.frame_events().filter(|e|e.kind==crate::EventKind::Fizzle).count(),3);
            draw(&mut r,&w);draw(&mut r,&w);w.step(&mut Use(0));
            let out=draw(&mut r,&w);assert_eq!(out.iter().filter(|v|v.params[0]==26).count(),18);
        }
    }
    #[test]
    fn world_step_two_novas_keep_compact_history_ownership_spatial() {
        let mut w=arena(&[Point{x:800.0,y:600.0},Point{x:2800.0,y:600.0},Point{x:2800.0,y:690.0}]);
        for id in 0..2 {w.snakes[id].inventory=Inventory {kinds:[5,0,0],life:[150,0,0],count:1,..Default::default()};}
        let mut r=Renderer::new();let mut saw_novas=false;
        for _ in 0..17 {
            w.step(&mut Use(1));let (info,mut snakes,body,events)=exported(&w);
            saw_novas|=events.iter().filter(|e|e.kind==3).count()==2;
            let tails:Vec<_>=snakes.iter().map(|s|body[(s.segment_offset+s.segment_count-1) as usize]).collect();
            for (id,s) in snakes.iter_mut().enumerate() {s.segment_offset=id as u32;s.segment_count=1;}
            let p=Params::default();r.build_shader(&info,&snakes,&tails,&[],&events,&[],&p,&mut []);
        }
        assert!(saw_novas);assert!(w.snakes[2].frozen_ticks>0);
        let out=draw(&mut r,&w);
        assert!(out.iter().any(|v|v.params[0]==0 && v.x>2700.0 && v.params[1]&128!=0));
        let owner=r.inventory.held[2].nova.origin.x;
        assert!(owner>2700.0,"victim must belong to the nearby second Nova");
    }
}
