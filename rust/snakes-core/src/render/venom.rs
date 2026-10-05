// SPDX-License-Identifier: GPL-3.0-or-later
//! Last full-frame trail plus two bounded orphan buffers. Compact Qt history
//! can deliver Sever events between full draws without replacing these trails.
use super::*;
const CAP:usize=MAX_SEGMENTS/2;
const EDGES:usize=100;
// Long-only, bounded best-first Douglas–Peucker selection. Retain original
// source indices for taper and shader markings; never change collision geometry.
fn select(points:&[P],widths:&[f64],radius:f64,scale:P,indices:&mut [u16;EDGES+1])->usize {
    fn span(points:&[P],widths:&[f64],radius:f64,scale:P,a:usize,b:usize)->(f64,usize) {
        let start=points[a];let end=points[b];
        if !start.finite() || !end.finite() {return (0.0,a);}
        let dx=(end.x-start.x)*scale.x;let dy=(end.y-start.y)*scale.y;
        let length=dx*dx+dy*dy;let mut best=(0.0,a);
        for (j,p) in points.iter().enumerate().take(b).skip(a+1) {
            let x=(p.x-start.x)*scale.x;let y=(p.y-start.y)*scale.y;
            let t=if length>0.0 {((x*dx+y*dy)/length).clamp(0.0,1.0)} else {0.0};
            let center_error=(x-t*dx).powi(2)+(y-t*dy).powi(2);
            let fraction=(j-a) as f64/(b-a) as f64;
            let width_error=(widths[j]-widths[a]-(widths[b]-widths[a])*fraction)*radius*scale.x.max(scale.y);
            let error=center_error.max(width_error*width_error);
            if error>best.0 {best=(error,j);}
        }
        best
    }
    indices[0]=0;
    let mut n=1;
    // Each invalid run must remain a break in the selected polyline. If the
    // bounded output cannot represent every break, omit this malformed orphan
    // rather than connect unrelated finite runs.
    for j in 1..points.len()-1 {
        // Retain the finite endpoints on either side too, so valid runs
        // still receive simplification rather than disappear with the gap.
        if points[j].finite() && points[j-1].finite() && points[j+1].finite() {continue;}
        if !points[j].finite() && !points[j-1].finite() {continue;}
        if n==EDGES {return 0;}
        indices[n]=j as u16;n+=1;
    }
    indices[n]=(points.len()-1) as u16;n+=1;
    let mut spans=[(0.0,0usize);EDGES];
    for i in 0..n-1 {spans[i]=span(points,widths,radius,scale,indices[i] as usize,indices[i+1] as usize);}
    while n<EDGES+1 {
        let mut worst=0;
        for i in 1..n-1 {if spans[i].0>spans[worst].0 {worst=i;}}
        if spans[worst].0<=0.75*0.75 {break;}
        let split=spans[worst].1;
        indices.copy_within(worst+1..n,worst+2);indices[worst+1]=split as u16;
        spans.copy_within(worst+1..n-1,worst+2);
        spans[worst]=span(points,widths,radius,scale,indices[worst] as usize,split);
        spans[worst+1]=span(points,widths,radius,scale,split,indices[worst+2] as usize);
        n+=1;
    }
    n
}
#[derive(Clone,Copy,Default)]
struct Orphan {len:usize,time:f64,duration:f64,radius:f64,color:u32,tier:u8}
pub(super) struct History {
    bodies:Vec<SegmentRecord>,records:[SnakeRecord;MAX_SNAKES],
    tails:Vec<P>,widths:Vec<f64>,orphans:[Orphan;2],sine:[f64;256],
    indices:[[u16;EDGES+1];2],selected:[usize;2],scales:[P;2],arenas:[P;2],walls:[bool;2],
}
impl History {
    pub fn new()->Self {Self {bodies:vec![SegmentRecord::default();MAX_SNAKES*MAX_SEGMENTS],
        records:[SnakeRecord::default();MAX_SNAKES],tails:vec![P::default();2*CAP],widths:vec![0.0;2*CAP],
        indices:[[0;EDGES+1];2],selected:[0;2],scales:[P::default();2],arenas:[P::default();2],walls:[false;2],
        orphans:[Orphan::default();2],sine:std::array::from_fn(|i|(i as f64*std::f64::consts::TAU/256.0).sin())}}
    pub fn reset(&mut self) {self.records.fill(SnakeRecord::default());self.orphans.fill(Orphan::default());}
    pub fn observe(&mut self,info:&FrameInfo,snakes:&[SnakeRecord],segments:&[SegmentRecord],events:&[EventRecord],through:Option<u64>) {
        for e in events {
            if e.kind!=crate::EventKind::Sever as u8 || e.tick>info.tick || through.is_some_and(|t|e.tick<=t) {continue;}
            let id=e.snake_id as usize;if id>=MAX_SNAKES {continue;}
            let r=self.records[id];let cut=e.cut_index as usize;let n=r.segment_count as usize;
            if r.generation!=e.generation || cut<=n/2 || cut<=3 || cut>=n || n-cut>CAP || !snake_valid(&r) {continue;}
            let slot=self.orphans.iter().position(|o|o.len==0 || info.simulation_time-o.time>o.duration+0.55)
                .unwrap_or_else(||if self.orphans[0].time<=self.orphans[1].time {0} else {1});
            self.selected[slot]=0;
            let start=slot*CAP;
            for j in cut..n {
                let seg=self.bodies[id*MAX_SEGMENTS+j];
                self.tails[start+j-cut]=if segment_valid(&seg) {P::new(seg.x as f64,seg.y as f64)} else {P::new(f64::NAN,f64::NAN)};
                self.widths[start+j-cut]=crate::shape::taper(j as f64/(n-1) as f64);
            }
            self.orphans[slot]=Orphan {len:n-cut,time:info.simulation_time-(info.tick-e.tick) as f64*crate::STEP_SECONDS,
                duration:e.duration_ticks as f64*crate::STEP_SECONDS,radius:r.radius,color:r.color_index,
                // Match the pattern the shortened victim now shows, so the piece reads as that snake.
                tier:if cut<24 {0} else if cut<100 {1} else if cut<250 {2} else {3}};
            // A second sever must not recover segments already detached.
            self.records[id].segment_count=cut as u32;
        }
        for s in snakes {
            let id=s.id as usize;let n=s.segment_count as usize;
            if id>=MAX_SNAKES || n<2 || n>MAX_SEGMENTS || !snake_valid(s) {continue;}
            let start=s.segment_offset as usize;
            if start+n>segments.len() {continue;}
            self.bodies[id*MAX_SEGMENTS..id*MAX_SEGMENTS+n].copy_from_slice(&segments[start..start+n]);self.records[id]=*s;
        }
    }
}
impl Renderer {
    fn legacy_orphan_points(&mut self,slot:usize,info:&FrameInfo,p:&Params)->Option<(usize,Color,f64,f64)> {
        let o=self.venom.orphans[slot];if o.len==0 {return None;}
        let age=(event_time(info,p,self.reduced_motion)-o.time).max(0.0);
        let motion=if self.reduced_motion {0.6} else {1.0};let wriggle=o.duration; // sim-timed: shards release after exactly duration ticks, Calm included
        let fade=1.0-((age-wriggle).max(0.0)/(0.55*motion)).min(1.0);
        if fade<=0.0 {return None;}
        let n=(o.len-1).min(EDGES)+1;
        let arena=P::new(info.world_width,info.world_height);let walls=p.deadly_walls!=0;
        let base=slot*CAP;
        let mut unwrapped=self.venom.tails[base];let mut previous=unwrapped;
        let mut i=0;
        // Unwrap every source edge before LOD. A sampled edge can exceed half
        // a narrow arena, so shortest-wrap on selected points reverses it.
        for j in 0..o.len {
            let source=self.venom.tails[base+j];
            if j>0 {
                unwrapped=if walls || !previous.finite() || !unwrapped.finite() {source} else {unwrapped+P::new(
                    delta(previous.x,source.x,arena.x,false),delta(previous.y,source.y,arena.y,false))};
            }
            previous=source;
            if j!=i*(o.len-1)/n.saturating_sub(1).max(1) {continue;}
            let mut q=unwrapped;
            if o.len>1 && !self.reduced_motion && age<wriggle {
                let before=self.venom.tails[base+j.saturating_sub(1)];let after=self.venom.tails[base+(j+1).min(o.len-1)];
                let d=P::new(delta(before.x,after.x,arena.x,walls),delta(before.y,after.y,arena.y,walls));
                let length=d.length().max(0.001);let phase=(0.75*j as f64-17.0*age)/std::f64::consts::TAU;
                let wave=self.venom.sine[((phase*256.0) as i64).rem_euclid(256) as usize];
                q=q+P::new(-d.y/length,d.x/length)*(o.radius*(1.0-age/wriggle).sqrt()*(0.4+0.6*j as f64/(o.len-1) as f64)*wave);
            }
            self.points[i]=q;
            self.mapped[i]=P::new(q.x*p.scale_x+p.offset_x,q.y*p.scale_y+p.offset_y);
            self.brightness[i]=(self.venom.widths[base+j]*255.0).round() as u8;
            i+=1;if i==n {break;}
        }
        prepare(&self.mapped[..n],&mut self.normals[..n],&mut self.valid[..n]);
        if n==1 {self.valid[0]=self.mapped[0].finite();self.normals[0]=P::new(0.0,1.0);}
        Some((n,Color::new(0,0,0,255),o.radius*(p.scale_x*p.scale_y).sqrt(),fade))
    }
    fn orphan_points(&mut self,slot:usize,info:&FrameInfo,p:&Params)->Option<(usize,Color,f64,f64)> {
        let o=self.venom.orphans[slot];if o.len==0 {return None;}
        if o.len<=800 {return self.legacy_orphan_points(slot,info,p);}
        let age=(event_time(info,p,self.reduced_motion)-o.time).max(0.0);
        let motion=if self.reduced_motion {0.6} else {1.0};let wriggle=o.duration; // sim-timed: shards release after exactly duration ticks, Calm included
        let fade=1.0-((age-wriggle).max(0.0)/(0.55*motion)).min(1.0);
        if fade<=0.0 {return None;}
        let n;
        let arena=P::new(info.world_width,info.world_height);let walls=p.deadly_walls!=0;
        let base=slot*CAP;
        let mut unwrapped=self.venom.tails[base];let mut previous=unwrapped;
        // Unwrap every source edge before LOD. A sampled edge can exceed half
        // a narrow arena, so shortest-wrap on selected points reverses it.
        for j in 0..o.len {
            let source=self.venom.tails[base+j];
            if j>0 {
                unwrapped=if walls || !previous.finite() || !unwrapped.finite() {source} else {unwrapped+P::new(
                    delta(previous.x,source.x,arena.x,false),delta(previous.y,source.y,arena.y,false))};
            }
            previous=source;
            self.points[j]=unwrapped;
        }
        let scale=P::new(p.scale_x,p.scale_y);
        {
            // Cache once per sever/projection change. Wriggle is applied only
            // after selecting the retained source curve, so ordinary frames
            // retain the old bounded linear walk with no selection/allocation.
            if self.venom.selected[slot]==0 || self.venom.scales[slot].x!=scale.x || self.venom.scales[slot].y!=scale.y
                || self.venom.arenas[slot].x!=arena.x || self.venom.arenas[slot].y!=arena.y || self.venom.walls[slot]!=walls {
                self.venom.selected[slot]=select(&self.points[..o.len],&self.venom.widths[base..base+o.len],o.radius,scale,&mut self.venom.indices[slot]);
                self.venom.scales[slot]=scale;self.venom.arenas[slot]=arena;self.venom.walls[slot]=walls;
            }
            n=self.venom.selected[slot];
        }
        if n==0 {return None;}
        for i in 0..n {
            let j=self.venom.indices[slot][i] as usize;
            let mut q=self.points[j];
            if o.len>1 && !self.reduced_motion && age<wriggle {
                let before=self.venom.tails[base+j.saturating_sub(1)];let after=self.venom.tails[base+(j+1).min(o.len-1)];
                let d=P::new(delta(before.x,after.x,arena.x,walls),delta(before.y,after.y,arena.y,walls));
                let length=d.length().max(0.001);let phase=(0.75*j as f64-17.0*age)/std::f64::consts::TAU;
                let wave=self.venom.sine[((phase*256.0) as i64).rem_euclid(256) as usize];
                q=q+P::new(-d.y/length,d.x/length)*(o.radius*(1.0-age/wriggle).sqrt()*(0.4+0.6*j as f64/(o.len-1) as f64)*wave);
            }
            self.points[i]=q;
            self.mapped[i]=P::new(q.x*p.scale_x+p.offset_x,q.y*p.scale_y+p.offset_y);
            self.brightness[i]=(self.venom.widths[base+j]*255.0).round() as u8;
        }
        prepare(&self.mapped[..n],&mut self.normals[..n],&mut self.valid[..n]);
        if n==1 {self.valid[0]=self.mapped[0].finite();self.normals[0]=P::new(0.0,1.0);}
        Some((n,Color::new(0,0,0,255),o.radius*(p.scale_x*p.scale_y).sqrt(),fade))
    }
    pub(super) fn shader_orphans(&mut self,info:&FrameInfo,p:&Params,palette:&[Color],sink:&mut shader::SpriteSink<'_>) {
        use shader::bounds::BODY;
        let mut budget=200;
        let mut glows=[None;2];
        // Reserve both cut ends before either trail spends the shared budget.
        for (slot,glow) in glows.iter_mut().enumerate() {
            let Some((_,_,r,_))=self.orphan_points(slot,info,p) else {continue;};
            let o=self.venom.orphans[slot];
            let age=(event_time(info,p,self.reduced_motion)-o.time).max(0.0);
            if age>o.duration || !self.valid[0] {continue;}
            let extent=r*2.4;let q=self.points[0];
            let (xs,ys)=copies(q,q,P::new(extent/p.scale_x,extent/p.scale_y),P::new(info.world_width,info.world_height),p.deadly_walls!=0);
            let count=((xs.last-xs.first+1)*(ys.last-ys.first+1)) as usize;
            if count<=budget {
                budget-=count;
                *glow=Some((xs,ys,extent,((1.0-age/o.duration).max(0.0)*0.6*255.0).round() as u8));
            }
        }
        for (slot,glow) in glows.into_iter().enumerate() {
            let Some((n,_,r,fade))=self.orphan_points(slot,info,p) else {continue;};
            let o=self.venom.orphans[slot];let c=palette.get(o.color as usize%palette.len().max(1)).copied().unwrap_or(Color::new(0,255,255,255));
            let motion=if self.reduced_motion {0.6} else {1.0};
            let age=(event_time(info,p,self.reduced_motion)-o.time).max(0.0);
            let wriggle=o.duration; // sim-timed: shards release after exactly duration ticks, Calm included
            let dissolve=(age-wriggle).max(0.0)/(0.55*motion);
            let arena=P::new(info.world_width,info.world_height);let walls=p.deadly_walls!=0;
            // The piece is the victim's own tube: a 1/8 s white snap (prototype), then
            // its palette colour; dissolving shards use the ordinary corpse boost.
            let flash=(1.0-age*8.0).max(0.0)*0.6;
            let snap=|x:u8|(x as f64+(255-x) as f64*flash).round() as u8;
            let cc=if dissolve>0.0 {c.boost()} else {Color::new(snap(c.red),snap(c.green),snap(c.blue),c.alpha)};
            if n==1 && self.valid[0] {
                let width=self.brightness[0] as f64/255.0;
                let q=self.points[0];let extent=r*BODY*width;
                let (xs,ys)=copies(q,q,P::new(extent/p.scale_x,extent/p.scale_y),arena,walls);
                'point: for x in xs.first..=xs.last {for y in ys.first..=ys.last {
                    if budget==0 {break 'point;}
                    let center=self.mapped[0]+P::new(x as f64*xs.extent*p.scale_x,y as f64*ys.extent*p.scale_y);
                    let begin=sink.count;let axis=P::new(r*width*0.5,0.0);let normal=P::new(0.0,extent);
                    let alpha=(fade*255.0).round() as u8;
                    sink.body_edge(center-axis,center+axis,normal,normal,extent,[width;2],
                        [self.brightness[0];2],[1.0;2],cc,[0,o.tier,flags::CORPSE as u8,alpha],alpha,None);
                    if sink.count>begin {budget-=1;}
                }}
            }
            'edges: for i in 1..n {
                if budget==0 {break;}if !self.valid[i-1] || !self.valid[i] {continue;}
                let a=self.points[i-1];let b=self.points[i];let margin=P::new(r*4.3/p.scale_x,r*4.3/p.scale_y);
                // Same three-edge alternating breakup as ordinary corpses;
                // the wriggle duration comes from the Sever payload.
                let local=if dissolve>0.0 {(dissolve*1.5-(if o.len>800 {self.venom.indices[slot][i-1] as f64/(o.len-1) as f64*0.5} else {((i-1)/3*3) as f64/(n-1) as f64*0.5})).clamp(0.0,1.0)} else {0.0};
                if local>=1.0 {continue;}
                let alpha=if dissolve>0.0 {1.0-local} else {fade};
                let shrink=1.0-local*0.5;
                let drift=self.normals[i-1]*(local*r*1.6*if (i-1)/3%2==0 {1.0} else {-1.0});
                let (xs,ys)=copies(P::new(a.x.min(b.x),a.y.min(b.y)),P::new(a.x.max(b.x),a.y.max(b.y)),margin,arena,walls);
                let widths=[self.brightness[i-1] as f64/255.0*shrink,self.brightness[i] as f64/255.0*shrink];
                // Chevrons/saddles keep the victim's spacing: along counts source segments.
                let (j0,j1)=if o.len>800 {(self.venom.indices[slot][i-1] as usize,self.venom.indices[slot][i] as usize)}
                    else {((i-1)*(o.len-1)/(n-1),i*(o.len-1)/(n-1))};
                let along=[(o.len-1-j0) as f64,(o.len-1-j1) as f64];
                let alpha_byte=(alpha*255.0).round() as u8;
                for x in xs.first..=xs.last {for y in ys.first..=ys.last {
                    if budget==0 {break 'edges;}
                    let shift=P::new(x as f64*xs.extent*p.scale_x,y as f64*ys.extent*p.scale_y);let begin=sink.count;
                    // Same contract as live bodies: extrusion r*BODY*w, across UV = w, alpha = taper.
                    sink.body_edge(self.mapped[i-1]+shift+drift,self.mapped[i]+shift+drift,
                        self.normals[i-1]*(r*BODY*widths[0]),self.normals[i]*(r*BODY*widths[1]),r*BODY*widths[0].max(widths[1]),
                        widths,[(widths[0]*255.0).round() as u8,(widths[1]*255.0).round() as u8],along,cc,
                        [0,o.tier,flags::CORPSE as u8,alpha_byte],alpha_byte,None);
                    if sink.count>begin {budget-=1;}
                }}
            }
            if let Some((xs,ys,extent,a))=glow {
                for x in xs.first..=xs.last {for y in ys.first..=ys.last {
                    sink.sprite(self.mapped[0]+P::new(x as f64*xs.extent*p.scale_x,y as f64*ys.extent*p.scale_y),extent,
                        Color::new(255,255,255,a),[shader::ACID_GLOW,0,0,0]);
                }}
            }
        }
    }
    pub(super) fn classic_orphans(&mut self,info:&FrameInfo,p:&Params,palette:&[Color],sink:&mut Sink<'_>) {
        let mut budget=1200usize;
        // Point-only cuts have no edge. Reserve their discs ahead of trails.
        for slot in 0..2 {
            if self.venom.orphans[slot].len!=1 {continue;}
            let Some((_,_,r,fade))=self.orphan_points(slot,info,p) else {continue;};
            let o=self.venom.orphans[slot];let c=palette.get(o.color as usize%palette.len().max(1)).copied().unwrap_or(Color::new(0,255,255,255)).alpha(220).fade(fade);
            let q=self.points[0];let extent=r*self.brightness[0] as f64/255.0;
            let (xs,ys)=copies(q,q,P::new(extent/p.scale_x,extent/p.scale_y),P::new(info.world_width,info.world_height),p.deadly_walls!=0);
            'point: for x in xs.first..=xs.last {for y in ys.first..=ys.last {
                if budget<24 {break 'point;}
                let begin=sink.count;
                sink.disc(self.mapped[0]+P::new(x as f64*xs.extent*p.scale_x,y as f64*ys.extent*p.scale_y),extent,c,8);
                budget-=sink.count-begin;
            }}
        }
        for slot in 0..2 {
            let Some((n,_,r,fade))=self.orphan_points(slot,info,p) else {continue;};
            let o=self.venom.orphans[slot];let c=palette.get(o.color as usize%palette.len().max(1)).copied().unwrap_or(Color::new(0,255,255,255)).alpha(220).fade(fade);
            'edges: for i in 1..n {
                if !self.valid[i-1] || !self.valid[i] {continue;}
                let a=self.points[i-1];let b=self.points[i];let margin=P::new(r*1.4/p.scale_x,r*1.4/p.scale_y);
                let (xs,ys)=copies(P::new(a.x.min(b.x),a.y.min(b.y)),P::new(a.x.max(b.x),a.y.max(b.y)),margin,P::new(info.world_width,info.world_height),p.deadly_walls!=0);
                for x in xs.first..=xs.last {for y in ys.first..=ys.last {
                    let shift=P::new(x as f64*xs.extent*p.scale_x,y as f64*ys.extent*p.scale_y);
                    if budget<6 {break 'edges;}
                    let begin=sink.count;
                    sink.segment(self.mapped[i-1]+shift,self.mapped[i]+shift,r*(self.brightness[i-1] as f64/255.0),c);
                    budget-=sink.count-begin;
                }}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(width:f64,height:f64,len:usize,vertical:bool)->(Renderer,FrameInfo,Params) {
        let mut r=Renderer::new();r.reduced_motion=true;
        let info=FrameInfo {tick:30,simulation_time:1.0,world_width:width,world_height:height,..Default::default()};
        let p=Params {viewport_width:width,viewport_height:height,scale_x:1.0,scale_y:1.0,
            interpolation:1.0,presentation_time:1.0,deadly_walls:0,..Default::default()};
        for slot in 0..2 {
            r.venom.orphans[slot]=Orphan {len,time:1.0,duration:1.1,radius:6.0,color:slot as u32,..Default::default()};
            for j in 0..len {
                let (x,y)=if vertical {(0.0,(7.08*j as f64).rem_euclid(height))}
                    else {((7.08*j as f64).rem_euclid(width),0.0)};
                r.venom.tails[slot*CAP+j]=P::new(x,y);r.venom.widths[slot*CAP+j]=0.5;
            }
        }
        (r,info,p)
    }
    #[test]
    fn venom_lod_unwraps_all_source_edges_before_selecting_points() {
        for vertical in [false,true] {
            let (mut r,info,p)=fixture(80.0,80.0,799,vertical);
            let (n,_,_,_)=r.orphan_points(0,&info,&p).unwrap();assert_eq!(n,101);
            for i in 1..n {
                let edge=r.points[i]-r.points[i-1];
                let expected=(i*798/100-(i-1)*798/100) as f64*7.08;
                assert!(((if vertical {edge.y} else {edge.x})-expected).abs()<1e-8);
                assert!(expected>40.0,"every decimated edge exceeds half the arena");
            }
        }
    }
    #[test]
    fn venom_both_paths_share_the_1200_vertex_seam_budget_and_keep_both_glows() {
        let colors=[Color::new(255,0,0,255),Color::new(0,255,0,255)];
        for (width,height) in [(8000.0,80.0),(80.0,8000.0),(80.0,80.0)] {for vertical in [false,true] {
            let (mut r,info,p)=fixture(width,height,799,vertical);
            let mut shader_out=vec![shader::ShaderVertex::default();6000];
            let mut sink=shader::SpriteSink {out:&mut shader_out,count:0,view:P::new(width,height)};
            r.shader_orphans(&info,&p,&colors,&mut sink);
            let count=sink.count;assert!(count>0 && count<=1200,"shader {width}x{height}, vertical={vertical}: {count}");
            let glows=shader_out[..count].iter().filter(|v|v.params[0]==shader::ACID_GLOW).collect::<Vec<_>>();
            assert!(glows.len()>=24,"both two-copy glows remain visible at the seam");
            // Each slot starts at (0,0), so it has four corner glow copies.
            assert_eq!(glows.len(),48);
            let mut out=vec![Vertex::default();6000];
            let circles=r.circles;
            let mut sink=Sink {output:&mut out,count:0,view:P::new(width,height),circles:&circles};
            r.classic_orphans(&info,&p,&colors,&mut sink);
            assert!(sink.count>0 && sink.count<=1200,"fallback {width}x{height}, vertical={vertical}: {}",sink.count);
        }}
    }
    #[test]
    fn venom_single_point_orphans_draw_both_paths_until_hold_and_then_fade() {
        let colors=[Color::new(255,0,0,255),Color::new(0,255,0,255)];
        let (mut r,mut info,p)=fixture(800.0,800.0,1,false);
        for age in [0.0,32.0/30.0,34.0/30.0,2.0] {
            info.simulation_time=1.0+age;
            let mut out=[shader::ShaderVertex::default();256];
            let mut sink=shader::SpriteSink {out:&mut out,count:0,view:P::new(800.0,800.0)};
            r.shader_orphans(&info,&p,&colors,&mut sink);let count=sink.count;
            if age<1.1 {
                assert!(out[..count].iter().any(|v|v.params[0]==25));
                assert!(out[..count].iter().filter(|v|v.params[2]&128!=0).all(|v|v.params[3]==255),
                    "both ends of the terminal tube stay opaque throughout the hold");
            }
            if age<1.43 {assert!(out[..count].iter().any(|v|v.params[2]&128!=0 && v.along>=1.0));}
            else {assert_eq!(count,0);}
            let mut out=[Vertex::default();256];let circles=r.circles;
            let mut sink=Sink {output:&mut out,count:0,view:P::new(800.0,800.0),circles:&circles};
            r.classic_orphans(&info,&p,&colors,&mut sink);
            if age<1.43 {assert!(sink.count>0);}
            else {assert_eq!(sink.count,0);}
        }
    }
}

#[cfg(test)]
mod giant_tests {
    use super::*;
    #[test]
    fn giant_orphan_invalid_points_remain_breaks_in_both_render_paths() {
        for walls in [false,true] {
            let mut r=Renderer::new();r.reduced_motion=true;
            let info=FrameInfo {world_width:2000.0,world_height:800.0,simulation_time:1.0,..Default::default()};
            let p=Params {viewport_width:2000.0,viewport_height:800.0,scale_x:1.0,scale_y:1.0,deadly_walls:u32::from(walls),..Default::default()};
            r.venom.orphans[0]=Orphan {len:801,time:1.0,duration:1.1,radius:18.0,..Default::default()};
            for j in 0..801 {r.venom.tails[j]=P::new(100.0+j as f64,400.0);r.venom.widths[j]=crate::shape::taper((801+j) as f64/1601.0);}
            r.venom.tails[400].x=f64::NAN;
            let (n,_,_,_)=r.orphan_points(0,&info,&p).unwrap();
            let indices=&r.venom.indices[0][..n];
            assert!(indices.contains(&399) && indices.contains(&400) && indices.contains(&401));
            for edge in indices.windows(2) {assert!(!(edge[0]<400 && edge[1]>400));}
            assert!(r.points[n-1].finite(),"finite geometry after a wrapping break must recover");
            let palette=[Color::new(255,0,0,255)];
            let mut shader_out=[shader::ShaderVertex::default();2048];
            let mut gpu=shader::SpriteSink {out:&mut shader_out,count:0,view:P::new(2000.0,800.0)};
            r.shader_orphans(&info,&p,&palette,&mut gpu);assert!(gpu.count>0);
            let mut output=[Vertex::default();2048];let circles=r.circles;
            let mut cpu=Sink {output:&mut output,count:0,view:P::new(2000.0,800.0),circles:&circles};
            r.classic_orphans(&info,&p,&palette,&mut cpu);assert!(cpu.count>0);
        }
        // A malformed curve with more breaks than the vertex budget permits
        // must not silently reconnect the remaining spans.
        let points:Vec<_>=(0..801).map(|j|P::new(if j%2==0 {j as f64} else {f64::NAN},0.0)).collect();
        assert_eq!(select(&points,&[0.5;801],18.0,P::new(1.0,1.0),&mut [0;EDGES+1]),0);
    }
    #[test]
    fn long_orphan_selection_preserves_turns_taper_and_source_endpoints() {
        let points:Vec<_>=(0..2999).map(|j|if j<=1000 {P::new(j as f64*2.0,0.0)}
            else if j<=2000 {P::new(2000.0,(j-1000) as f64*2.0)}
            else {P::new(2000.0+(j-2000) as f64*2.0,2000.0)}).collect();
        let widths:Vec<_>=(0..2999).map(|j|crate::shape::taper((3001+j) as f64/5999.0)).collect();
        let mut indices=[0;EDGES+1];
        let n=select(&points,&widths,18.0,P::new(1.0,1.0),&mut indices);
        assert!(n<=101);assert_eq!(indices[0],0);assert_eq!(indices[n-1],2998);
        assert!(indices[..n].contains(&1000) && indices[..n].contains(&2000));
        assert!(indices[..n].windows(2).all(|p|p[0]<p[1]));
        for edge in indices[..n].windows(2) {
            let a=edge[0] as usize;let b=edge[1] as usize;
            for j in a..=b {
                let fraction=(j-a) as f64/(b-a) as f64;
                let expected=widths[a]+(widths[b]-widths[a])*fraction;
                assert!((widths[j]-expected).abs()*18.0<=0.75);
            }
        }
    }
    #[test]
    fn long_orphan_selection_unwraps_before_lod_and_reprojects_without_allocating() {
        let mut r=Renderer::new();
        let info=FrameInfo{world_width:80.0,world_height:80.0,simulation_time:1.0,..Default::default()};
        let mut p=Params{viewport_width:80.0,viewport_height:80.0,scale_x:1.0,scale_y:1.0,..Default::default()};
        r.venom.orphans[0]=Orphan{len:2999,time:1.0,duration:1.1,radius:6.0,..Default::default()};
        for j in 0..2999 {r.venom.tails[j]=P::new((j as f64*7.08).rem_euclid(80.0),0.0);r.venom.widths[j]=0.5;}
        let (n,_,_,_)=r.orphan_points(0,&info,&p).unwrap();
        assert_eq!(n,2);assert!((r.points[1].x-2998.0*7.08).abs()<1e-6);
        p.scale_x=0.5;p.scale_y=2.0;
        let (n,_,_,_)=r.orphan_points(0,&info,&p).unwrap();
        assert_eq!(n,2);assert_eq!(r.venom.scales[0].x,0.5);
        assert!((r.mapped[1].x-2998.0*7.08*0.5).abs()<1e-6);
        p.deadly_walls=1;
        let (n,_,_,_)=r.orphan_points(0,&info,&p).unwrap();
        assert!(n>2);assert!(r.venom.walls[0]);
        assert!((r.points[n-1].x-(2998.0_f64*7.08).rem_euclid(80.0)).abs()<1e-6);
    }
}
