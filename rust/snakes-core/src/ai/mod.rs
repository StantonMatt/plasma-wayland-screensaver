// SPDX-License-Identifier: GPL-3.0-or-later
//! Deterministic receding-horizon controller. One controller belongs to one
//! world. All scratch storage is reserved by new(); steer never allocates.
mod spatial;
use crate::{normalize_angle, Point, SnakeView, World, MAX_FOOD, MAX_SEGMENTS, MAX_SNAKES, STEP_SECONDS};
use crate::controller::{Controller, Steering};
use spatial::Spatial;
const STEPS: usize = 72;
const CANDIDATES: usize = 9;
const STRATEGY_QUOTA: usize = 2;
const URGENT_QUOTA: usize = 2;
const NARROW_LIMIT: usize = 4096;

#[derive(Clone, Copy, Debug, Default)]
pub struct DebugInfo {
    pub generation: u32,
    pub target_food_ids: [u64; 5],
    pub target_count: u32,
    pub path: [Point; 16],
    pub path_count: u32,
    /// Bit 0: capped area search, bit 1: capped safety query, bit 2: no safe
    /// full-horizon candidate, bit 3: active interception, bit 4: revalidated plan reuse.
    pub flags: u32,
    pub reachable_cells: u32,
    pub safe_seconds: f64,
}
#[derive(Clone, Copy, Default)]
struct State {
    generation: u32,
    target: u64,
    rejected: u64,
    reject_until: u64,
    goal: Point,
    waypoint: Option<Point>,
    last_strategy: u64,
    last_progress: u64,
    best_distance: f64,
    desired: f64,
    turn_until: u64,
    exit_angle: f64,
    last_angle: f64,
    turn_accum: f64,
    orbit_until: u64,
    escape_until: u64,
    commit_until: u64,
    turn_sign: i8,
    debug: DebugInfo,
}
#[derive(Clone, Copy)]
struct Rival {
    alive: bool,
    radius: f64,
    len: usize,
    speed: f64,
    turn: f64,
    growth_delay: f64,
    release_rate: f64,
    angle: f64,
    path: [Point; STEPS+1],
    envelope: [f64; STEPS+1],
}
#[derive(Clone, Copy)]
struct Candidate {
    desired: f64,
    exit_angle: f64,
    angle: f64,
    path: [Point; STEPS+1],
    steps: usize,
    score: f64,
    clearance: f64,
    capped: bool,
    area: usize,
    uncertain: bool,
}

impl Default for Rival {
    fn default()->Self {Self {alive:false,radius:0.0,len:0,speed:0.0,turn:0.0,growth_delay:0.0,release_rate:0.0,angle:0.0,path:[Point::default();STEPS+1],envelope:[0.0;STEPS+1]}}
}
impl Default for Candidate {
    fn default()->Self {Self {desired:0.0,exit_angle:0.0,angle:0.0,path:[Point::default();STEPS+1],steps:0,score:0.0,clearance:0.0,capped:false,area:0,uncertain:false}}
}

#[derive(Clone, Copy, Default)]
struct BodyCache {
    epoch: u64,
    id: usize,
    a: Point,
    b: Point,
    time: f64,
    padding: f64,
    visits: usize,
    result: (bool,f64,bool),
}

/// Optional diagnostics are separate from the stable C debug layout.
#[derive(Clone, Copy, Debug, Default)]
pub struct CandidateDiagnostic {
    pub desired: f64,
    pub safe_ticks: usize,
    pub area: usize,
    pub area_capped: bool,
    pub turn_ticks: usize,
    pub track_goal: bool,
    pub exit_angle: f64,
    pub capped: bool,
    pub score: f64,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct DecisionDiagnostic {
    pub generation: u32,
    pub selected: usize,
    pub horizon: usize,
    pub required_cells: usize,
    pub length: usize,
    pub goal: Point,
    pub reused_plan: bool,
    pub candidates: [CandidateDiagnostic; CANDIDATES],
}
#[derive(Clone)]
pub struct AiController {
    spatial: Spatial,
    body_cache: [BodyCache;256],
    cache_epoch: u64,
    diagnostic_enabled: bool,
    profile_enabled: bool,
    profile: [u128;5],
    decisions: [DecisionDiagnostic; MAX_SNAKES],
    states: [State; MAX_SNAKES],
    rivals: [Rival; MAX_SNAKES],
    food: [Option<crate::FoodView>; MAX_FOOD],
    tick: u64,
    geometry: u64,
    seed: i32,
    rng: u64,
    urgent_used: usize,
    max_radius: f64,
}
impl Default for AiController {fn default() -> Self {Self::new()}}
impl AiController {
    pub fn new() -> Self {
        Self {spatial:Spatial::new(),body_cache:[BodyCache::default();256],cache_epoch:0, diagnostic_enabled:false,profile_enabled:false,profile:[0;5], decisions:[DecisionDiagnostic::default();MAX_SNAKES], states:[State::default(); MAX_SNAKES],
            rivals:[Rival::default();MAX_SNAKES], food:[None;MAX_FOOD],
            tick:u64::MAX,geometry:0,seed:0,rng:0x9e3779b97f4a7c15,urgent_used:0,max_radius:0.0}
    }
    pub fn enable_profile(&mut self) {self.profile_enabled=true;}
    pub fn profile(&self)->[u128;5] {self.profile}
    pub fn enable_diagnostics(&mut self) { self.diagnostic_enabled = true; }
    pub fn disable_observers(&mut self) {self.diagnostic_enabled=false;self.profile_enabled=false;}
    pub fn decision(&self, id:usize) -> DecisionDiagnostic { self.decisions[id] }
    pub fn debug(&self,id:usize) -> Option<DebugInfo> {self.states.get(id).map(|s|s.debug)}
    fn random(&mut self) -> f64 {
        self.rng ^= self.rng << 13; self.rng ^= self.rng >> 7; self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64/(1u64<<53) as f64
    }
    fn prepare(&mut self,w:&World) {
        if self.tick==w.tick() && self.geometry==w.geometry_generation() {return;}
        if self.geometry!=w.geometry_generation() || w.tick()<self.tick || self.seed!=w.config().seed {
            self.states.fill(State::default());
            self.seed=w.config().seed;
            self.rng=0x9e3779b97f4a7c15 ^ (self.seed as u32 as u64).wrapping_mul(0xd1342543de82ef95);
        }
        self.cache_epoch=self.cache_epoch.wrapping_add(1);
        if self.cache_epoch==0 {self.body_cache.fill(BodyCache::default());self.cache_epoch=1;}
        self.tick=w.tick(); self.geometry=w.geometry_generation(); self.urgent_used=0;
        self.spatial.rebuild(w);
        self.food.fill(None);
        for (i,f) in w.foods().enumerate() {self.food[i]=Some(f);}
        self.rivals.fill(Rival::default());self.max_radius=0.0;
        for s in w.snakes().filter(|s|s.alive) {
            let (speed,turn)=w.motion_limits(s.id as usize,0.0).unwrap();
            self.max_radius=self.max_radius.max(s.radius);
            let r=&mut self.rivals[s.id as usize];
            *r=Rival {alive:true,radius:s.radius,len:s.segments.len(),speed,turn,growth_delay:w.tail_growth_delay(s.id as usize).unwrap(),release_rate:s.radius*1.18/(speed*0.65).max(1.0),
                angle:s.angle,..Rival::default()};
            r.path[0]=s.segments[0].current;
            let previous=self.states[s.id as usize];
            let known_plan=previous.generation==s.generation && normalize_angle(previous.desired-s.desired_angle).abs()<1e-9;
            let mut angle=s.angle;
            let rotation=(turn*STEP_SECONDS).sin_cos();
            let mut direction=Point {x:angle.cos(),y:angle.sin()};
            for j in 1..=STEPS {
                let t=j as f64*STEP_SECONDS;
                r.envelope[j]=(speed*t*(turn*t).min(1.2).sin()*0.3).min(s.radius*2.0+12.0);
                let desired=if known_plan && previous.turn_until!=u64::MAX && w.tick()+j as u64>previous.turn_until {previous.exit_angle} else {s.desired_angle};
                let delta=normalize_angle(desired-angle);
                if delta.abs()>turn*STEP_SECONDS {
                    let sine=rotation.0*delta.signum();
                    direction=Point {x:direction.x*rotation.1-direction.y*sine,y:direction.x*sine+direction.y*rotation.1};
                    angle=normalize_angle(angle+turn*STEP_SECONDS*delta.signum());
                } else if delta.abs()>1e-12 {
                    angle=desired;
                    let (sin,cos)=angle.sin_cos();direction=Point{x:cos,y:sin};
                }
                r.path[j]=w.canonical_point(Point {x:r.path[j-1].x+direction.x*speed*STEP_SECONDS,
                    y:r.path[j-1].y+direction.y*speed*STEP_SECONDS});
            }
        }
    }
    fn mask(&self,w:&World,id:usize) -> u16 {
        if w.config().self_collisions {u16::MAX} else {u16::MAX ^ (1<<id)}
    }
    fn arrival(&self,w:&World,r:&Rival,p:Point) -> f64 {
        let d=w.displacement(r.path[0],p);
        let distance=(d.x*d.x+d.y*d.y).sqrt();
        let turn=normalize_angle(d.y.atan2(d.x)-r.angle).abs();
        // Turns consume forward travel too. The extra term accounts for the
        // loop needed to approach a point inside the minimum turning circle.
        let radius=r.speed/r.turn.max(0.01);
        let inside=if turn>1.2 && distance<radius*2.0 {radius*(turn-1.2)/r.speed.max(1.0)} else {0.0};
        distance/r.speed.max(1.0)+turn/r.turn.max(0.01)*0.6+inside
    }
    fn strategy(&mut self,w:&World,s:SnakeView<'_>,state:&mut State,intelligence:f64) {
        let id=s.id as usize; let head=s.segments[0].current;
        if w.tick()<state.escape_until {
            state.debug.flags&=!8;
            state.goal=s.segments.last().unwrap().current;
            state.target=0;state.waypoint=None;
            state.debug.target_count=0;state.debug.target_food_ids=[0;5];
            state.last_strategy=w.tick();return;
        }
        if w.tick()<state.orbit_until {
            state.debug.flags&=!8;
            state.target=0;state.waypoint=None;
            state.debug.target_count=0;state.debug.target_food_ids=[0;5];
            state.last_strategy=w.tick();return;
        }
        let root=self.spatial.key(head);
        let mut shortlist=[(0.0,0usize);5]; let mut inspected=0;let mut cells=0;
        // Ring traversal avoids both full food scans per candidate and F^2
        // cluster work. A distant-food fallback visits cells, not segments.
        'rings: for ring in 0isize..=self.spatial.cols.max(self.spatial.rows) as isize {
            for (x,y) in self.spatial.ring_offsets(ring) {
                    let Some(key)=self.spatial.offset(root,x,y) else {continue;};
                    cells+=1;if cells>1024 {break 'rings;}
                    let mut at=self.spatial.food_heads[key];
                    while at>=0 {
                        let fi=at as usize; at=self.spatial.food_next[fi]; inspected+=1;
                        if inspected>64 {break 'rings;}
                        let f=self.food[fi].unwrap();
                        if f.vacuum_owner>=0 && f.vacuum_owner!=s.id as i32 {continue;}
                        if f.id==state.rejected && w.tick()<state.reject_until {continue;}
                        let d=w.displacement(head,f.position); let distance=(d.x*d.x+d.y*d.y).sqrt();
                        // Once vacuumed, food follows us; pursuing it would
                        // produce endless loops around the turning circle.
                        if f.vacuum_owner==s.id as i32 || distance<s.radius*2.7 {continue;}
                        let eta=self.arrival(w,&self.rivals[id],f.position);
                        let bearing=normalize_angle(d.y.atan2(d.x)-s.angle).abs();
                        if bearing>1.5 && distance<self.rivals[id].speed/self.rivals[id].turn.max(0.01)*1.8 {continue;}
                        let cluster=self.spatial.cluster_weight(key);
                        let mut rival_eta=f64::MAX; let mut rival_len=0;
                        for (other,r) in self.rivals.iter().enumerate() {
                            if other==id || !r.alive {continue;}
                            let e=self.arrival(w,r,f.position);
                            if e<rival_eta {rival_eta=e;rival_len=r.len;}
                        }
                        let advantage=s.segments.len()>=rival_len+6;
                        let competition=if rival_eta<eta*0.8 {if advantage {0.85} else {0.2}}
                            else if rival_eta<eta*1.2 {if advantage {1.35} else {0.65}} else {1.0};
                        let sticky=if f.id==state.target {1.28} else {1.0};
                        let forward=(0.15+0.85*((bearing.cos()+1.0)*0.5).powi(3)).max(0.05);
                        let score=(f.value+cluster.min(8.0)*0.18+if f.feast_id>0 {0.4} else {0.0})
                            *competition*sticky*forward/(eta+0.7);
                        for k in 0..5 {
                            if score>shortlist[k].0 {
                                for n in (k+1..5).rev() {shortlist[n]=shortlist[n-1];}
                                shortlist[k]=(score,fi); break;
                            }
                        }
                        if inspected>=64 {break 'rings;}
                    }
            }
            if ring>=4 && shortlist[4].0>0.0 {break;}
        }
        state.debug.target_food_ids=[0;5]; state.debug.target_count=0;
        for (score,fi) in shortlist {
            if score<=0.0 {continue;}
            state.debug.target_food_ids[state.debug.target_count as usize]=self.food[fi].unwrap().id;
            state.debug.target_count+=1;
        }
        let old=state.target;
        state.target=if shortlist[0].0>0.0 {self.food[shortlist[0].1].unwrap().id} else {0};
        if state.target!=0 {
            state.goal=self.food[shortlist[0].1].unwrap().position;
        } else {
            // Sparse-food fallback: a stable distant point. Independent RNG
            // consumes nothing from World, and traits differentiate explorers.
            let angle=s.angle+(self.random()-0.5)*1.2+s.traits.wander_phase.sin()*0.25;
            state.goal=w.canonical_point(Point {x:head.x+angle.cos()*350.0,y:head.y+angle.sin()*350.0});
            if w.config().deadly_walls {
                let mx=(w.config().width*0.2).min(100.0);let my=(w.config().height*0.2).min(40.0);
                state.goal.x=state.goal.x.clamp(mx,w.config().width-mx);
                state.goal.y=state.goal.y.clamp(my,w.config().height-my);
            }
        }
        if old!=state.target {state.best_distance=f64::MAX;state.last_progress=w.tick();}
        state.debug.flags&=!8;
        // Intercept where a smaller rival will be, with an arrival margin.
        // A lateral lead deposits our neck across its route rather than simply
        // chasing its slower-to-catch current position. Safety still decides.
        if intelligence>=0.45 {
            let aggression=(s.traits.aggression*0.7+0.3)*intelligence;
            let mut value=0.0; let mut intercept=None;
            for (other,r) in self.rivals.iter().enumerate() {
                if !r.alive || other==id || s.segments.len()<r.len+8 {continue;}
                for t in [0.65,1.0,1.5] {
                    let future=w.canonical_point(Point {x:r.path[30].x+r.angle.cos()*r.speed*(t-1.0),
                        y:r.path[30].y+r.angle.sin()*r.speed*(t-1.0)});
                    let eta=self.arrival(w,&self.rivals[id],future);
                    if eta<t*0.85 && eta>0.2 && w.distance_squared(head,future)<360.0*360.0 {
                        let v=aggression*(r.len as f64).sqrt()/(eta+0.8);
                        if v>value {value=v;intercept=Some(future);}
                    }
                }
            }
            if value>2.5 && aggression>0.45 {
                state.goal=intercept.unwrap();state.target=0;state.debug.flags|=8;
            }
        }
        state.waypoint=None;
        let mut tests=0;
        if self.body_blocked(w,s,head,state.goal,0.0,0.0,&mut tests).0 {
            state.waypoint=self.spatial.waypoint(head,state.goal,self.mask(w,id));
            if state.waypoint.is_none() && state.debug.flags&8==0 {
                // Do not commit to the nutritional maximum behind a body
                // barrier. Try another shortlisted food with a clear approach.
                let mut alternative=None;
                for (score,fi) in shortlist.into_iter().skip(1) {
                    if score<=0.0 {continue;}
                    let f=self.food[fi].unwrap();let mut tests=0;
                    if !self.body_blocked(w,s,head,f.position,0.0,0.0,&mut tests).0 {alternative=Some(f);break;}
                }
                if let Some(f)=alternative {state.target=f.id;state.goal=f.position;}
                else {
                    state.rejected=state.target;state.reject_until=w.tick()+90;state.target=0;
                    state.goal=w.canonical_point(Point{x:head.x+s.angle.cos()*300.0,y:head.y+s.angle.sin()*300.0});
                }
            }
        }
        state.last_strategy=w.tick();
    }
    /// Static trail occupancy with conservative tail release. Interior body
    /// locations stay occupied until the tail arrives; current growth adds a
    /// delay rather than making those locations permanent obstacles.
    fn body_blocked(&self,w:&World,s:SnakeView<'_>,a:Point,b:Point,time:f64,padding:f64,tests:&mut usize) -> (bool,f64,bool) {
        let key=self.spatial.key(w.canonical_point(a));
        let ab=w.displacement(a,b);
        let reach=s.radius+self.max_radius+8.0+padding+if time<=STEP_SECONDS+1e-9 {self.spatial.max_motion} else {0.0};
        let root_x=(key%self.spatial.cols) as isize;
        let root_y=(key/self.spatial.cols) as isize;
        let x0=((a.x+ab.x.min(0.0)-reach)/self.spatial.dx).floor() as isize-root_x;
        let x1=((a.x+ab.x.max(0.0)+reach)/self.spatial.dx).floor() as isize-root_x;
        let y0=((a.y+ab.y.min(0.0)-reach)/self.spatial.dy).floor() as isize-root_y;
        let y1=((a.y+ab.y.max(0.0)+reach)/self.spatial.dy).floor() as isize-root_y;
        let neck_advance=self.rivals[s.id as usize].speed*time/(s.radius*1.18);
        let mut clearance=200.0_f64;
        let length2=ab.x*ab.x+ab.y*ab.y;
        let (xs,ys)=self.spatial.spans(key,x0,x1,y0,y1);
        for y in ys {for x in xs.clone() {
            let Some(k)=self.spatial.offset(key,x,y) else {continue;};
            if !w.config().self_collisions && self.spatial.occupied[k]&!(1<<s.id)==0 {continue;}
            let mut at=self.spatial.heads[k];
            while at>=0 {
                *tests+=1;
                if *tests>NARROW_LIMIT {return (true,clearance,true);}
                let encoded=at as usize; at=self.spatial.next[encoded];
                let other=encoded/MAX_SEGMENTS;let j=encoded%MAX_SEGMENTS;
                let same=other==s.id as usize;
                if same && (!w.config().self_collisions || j as f64+neck_advance<10.0) {continue;}
                let r=&self.rivals[other];
                // 65% of current speed allows for growth, slowing and curved
                // tails. Current growth adds a finite release delay.
                let release=(r.len-j) as f64*r.release_rate+r.growth_delay;
                if time>release+0.15 {continue;}
                let p=w.snake(other).unwrap().segments[j].current;
                let threshold=if same {s.radius*1.48} else {(s.radius+r.radius)*0.78};
                // Half a tick's motion and a small radius reserve protect the
                // first swept tick against segments that shift along corners.
                let margin=if time<=STEP_SECONDS+1e-9 {0.75} else {1.5+(r.speed*STEP_SECONDS*0.35).min(r.radius*0.25)+padding};
                let ap=w.displacement(a,p);
                let reserve=threshold+margin+if time<=STEP_SECONDS+1e-9 {self.spatial.max_motion} else {0.0};
                if ap.x<ab.x.min(0.0)-reserve || ap.x>ab.x.max(0.0)+reserve
                    || ap.y<ab.y.min(0.0)-reserve || ap.y>ab.y.max(0.0)+reserve {continue;}
                let fraction=if length2>0.0001 {((ap.x*ab.x+ap.y*ab.y)/length2).clamp(0.0,1.0)} else {0.0};
                let mut d=(ap.x-ab.x*fraction).powi(2)+(ap.y-ab.y*fraction).powi(2);
                if time<=STEP_SECONDS+1e-9 {
                    let seg=w.snake(other).unwrap().segments[j];
                    let velocity=w.displacement(seg.previous,seg.current);
                    let future=w.canonical_point(Point{x:p.x+velocity.x,y:p.y+velocity.y});
                    d=w.segments_distance_squared(a,b,p,future);
                }
                clearance=clearance.min(d.sqrt()-threshold);
                if d<(threshold+margin).powi(2) {return (true,clearance,false);}
            }
        }}
        (false,clearance,false)
    }
    fn cached_body_blocked(&mut self,w:&World,s:SnakeView<'_>,a:Point,b:Point,time:f64,padding:f64,tests:&mut usize)->(bool,f64,bool) {
        let hash=a.x.to_bits().wrapping_mul(0x9e3779b97f4a7c15)^a.y.to_bits().rotate_left(23)
            ^b.x.to_bits().rotate_left(41)^b.y.to_bits().rotate_left(11)^time.to_bits().rotate_left(7);
        let key=(hash^(hash>>32)) as usize&255;
        let old=self.body_cache[key];
        if old.epoch==self.cache_epoch && old.id==s.id as usize && old.a==a && old.b==b && old.time==time && old.padding==padding {
            *tests+=old.visits;
            if *tests>NARROW_LIMIT {return (true,old.result.1,true);}
            return old.result;
        }
        let before=*tests;
        let result=self.body_blocked(w,s,a,b,time,padding,tests);
        if !result.2 {
            self.body_cache[key]=BodyCache {epoch:self.cache_epoch,id:s.id as usize,a,b,time,padding,visits:*tests-before,result};
        }
        result
    }
    fn rollout(&mut self,w:&World,s:SnakeView<'_>,state:State,kind:usize,horizon:usize) -> Candidate {
        let r=&self.rivals[s.id as usize];let (speed,turn)=(r.speed,r.turn);
        let mut c=Candidate {angle:s.angle, clearance:200.0,..Candidate::default()};
        c.path[0]=s.segments[0].current;
        let goal=state.waypoint.unwrap_or(state.goal);
        let d=w.displacement(c.path[0],goal);
        let fixed=match kind {0=>d.y.atan2(d.x),1=>state.desired,2=>s.angle,3=>s.angle+0.6,4=>s.angle-0.6,
            5|7=>s.angle+3.0,6|8=>s.angle-3.0,_=>s.angle};
        if kind==7 || kind==8 {
            c.exit_angle=normalize_angle(s.angle+normalize_angle(fixed-s.angle).clamp(-turn*STEP_SECONDS*16.0,turn*STEP_SECONDS*16.0));
        }
        let mut tests=0;
        let mut direction=Point {x:s.angle.cos(),y:s.angle.sin()};
        let rotation=(turn*STEP_SECONDS).sin_cos();
        let mut checked=0;
        let mut near=0u16;
        for (other,r) in self.rivals.iter().enumerate() {
            if r.alive && other!=s.id as usize && w.distance_squared(c.path[0],r.path[0])
                <((speed+r.speed)*horizon as f64*STEP_SECONDS+(s.radius+r.radius)*3.0+20.0).powi(2) {
                near|=1<<other;
            }
        }
        for j in 1..=horizon {
            let desired=if kind==1 && w.tick()+j as u64>state.turn_until {state.exit_angle}
                else if (kind==7 || kind==8) && j>16 {c.exit_angle} else {fixed};
            if j==1 {c.desired=normalize_angle(desired);}
            let delta=normalize_angle(desired-c.angle);
            let step=turn*STEP_SECONDS;
            if delta.abs()>step {
                let sine=rotation.0*delta.signum();
                direction=Point {x:direction.x*rotation.1-direction.y*sine,y:direction.x*sine+direction.y*rotation.1};
                c.angle=normalize_angle(c.angle+step*delta.signum());
            } else if delta.abs()>1e-12 {
                c.angle=normalize_angle(desired);
                let (sin,cos)=c.angle.sin_cos();direction=Point{x:cos,y:sin};
            }
            if j==16 {c.exit_angle=c.angle;}
            let p=w.canonical_point(Point {x:c.path[j-1].x+direction.x*speed*STEP_SECONDS,
                y:c.path[j-1].y+direction.y*speed*STEP_SECONDS});
            c.path[j]=p;
            let cfg=w.config();
            if cfg.deadly_walls {
                let wall=p.x.min(cfg.width-p.x).min(p.y).min(cfg.height-p.y);
                c.clearance=c.clearance.min(wall);
                let radius=speed/turn.max(0.01)*1.10;
                let x_escape=radius*(1.0-direction.y.abs())+s.radius*0.5+2.0;
                let y_escape=radius*(1.0-direction.x.abs())+s.radius*0.5+2.0;
                // Reject states from which neither maximum-rate turn can
                // become parallel to an approaching wall before crossing it.
                // This sees inevitable wall hits beyond the rollout horizon.
                let x_room=if direction.x>=0.0 {cfg.width-p.x} else {p.x};
                let y_room=if direction.y>=0.0 {cfg.height-p.y} else {p.y};
                if wall<s.radius*0.5+2.0 || x_room<x_escape || y_room<y_escape {break;}
            }
            let t=j as f64*STEP_SECONDS;
            // Sweep up to four exact 30 Hz steps with a curvature-error bound.
            // The published first step is always checked separately.
            if j==1 || j%4==0 || j==horizon {
                let from=c.path[checked];
                let span=(j-checked) as f64*STEP_SECONDS;
                let padding=speed*turn*span*span/8.0;
                checked=j;
                let (hit,clearance,capped)=self.cached_body_blocked(w,s,from,p,t,padding,&mut tests);
                c.clearance=c.clearance.min(clearance); c.capped|=capped;
                if hit {break;}
            }
            let mut hit=false;
            let sweep=j==1 || j%4==0 || j==horizon;
            for (other,r) in self.rivals.iter().enumerate() {
                if !sweep || near&(1<<other)==0 {continue;}
                let winning=s.segments.len()>=r.len+6;
                let reach=(s.radius+r.radius)*0.82;
                // A bounded turn envelope widens with time, but not into an
                // arbitrary reachable disk that would paralyze all pursuit.
                let envelope=r.envelope[j]+(speed*turn+r.speed*r.turn)*(4.0*STEP_SECONDS).powi(2)/8.0;
                let head_near=w.distance_squared(p,r.path[j])<(reach+envelope+3.0+(speed+r.speed)*STEP_SECONDS*4.0).powi(2);
                if !winning && head_near && w.segments_distance_squared(c.path[j.saturating_sub(4)],p,r.path[j.saturating_sub(4)],r.path[j])<(reach+envelope+3.0).powi(2) {hit=true;break;}
                // Future rival neck deposition is lethal even for a winner.
                // Skip the last head-sized piece: that is the head contest.
                let neck_steps=((reach+5.0)/(r.speed*STEP_SECONDS).max(0.1)).ceil() as usize;
                if (j==1 || j%4==0 || j==horizon) && j>neck_steps && w.distance_squared(p,r.path[0])<(r.speed*t+reach+speed*STEP_SECONDS).powi(2) {
                    for k in (1..=j-neck_steps).step_by(3) {
                        let end=(k+2).min(j-neck_steps);
                        if w.distance_squared(p,r.path[end])<(reach+2.0+(speed+3.0*r.speed)*STEP_SECONDS).powi(2)
                            && w.segments_distance_squared(c.path[j.saturating_sub(4)],p,r.path[k-1],r.path[end])<(reach+2.0).powi(2) {hit=true;break;}
                    }
                }
                if hit {break;}
            }
            if hit {break;}
            if w.config().self_collisions {
                let age=(s.radius*1.18*10.0/speed/STEP_SECONDS).ceil() as usize;
                if (j%4==0 || j==horizon) && j>age {
                    for k in (0..j-age).step_by(3) {
                        let end=(k+3).min(j-age);
                        let reach=s.radius*1.48+3.0+speed*turn*(4.0*STEP_SECONDS).powi(2)/8.0;
                        if w.distance_squared(p,c.path[end])<(reach+speed*STEP_SECONDS*4.0).powi(2)
                            && w.segments_distance_squared(c.path[j.saturating_sub(4)],p,c.path[k],c.path[end])<reach*reach {hit=true;break;}
                    }
                }
            }
            if hit {break;}
            if sweep {c.steps=j;}
        }
        let endpoint=c.path[c.steps];
        let old=w.distance_squared(c.path[0],goal).sqrt();
        let mut closest=w.distance_squared(endpoint,goal);
        for &p in c.path[..=c.steps].iter().step_by(4) {closest=closest.min(w.distance_squared(p,goal));}
        let new=closest.sqrt();
        let diff=normalize_angle(c.desired-s.angle);
        let sign=if diff>0.07 {1} else if diff< -0.07 {-1} else {0};
        let hysteresis=if sign!=0 && state.turn_sign!=0 && sign!=state.turn_sign {35.0} else {0.0};
        let heading=w.displacement(endpoint,goal);
        let alignment=if closest<(s.radius*2.7).powi(2) {1.0} else {normalize_angle(heading.y.atan2(heading.x)-c.angle).cos()};
        c.score=(old-new)*0.9+alignment*12.0+c.clearance.clamp(-10.0,50.0)*0.22
            -diff.abs()*4.0+diff*s.traits.turn_bias*3.0-hysteresis-normalize_angle(c.desired-state.desired).abs()*2.0;
        // Favor room for a future escape turn, including when a parallel
        // corridor is immediately safe but leaves no room for a later turn.
        if w.config().deadly_walls {
            let margin=speed/turn.max(0.01)*2.0+s.radius*3.0;
            let wall=endpoint.x.min(w.config().width-endpoint.x).min(endpoint.y).min(w.config().height-endpoint.y);
            if wall<margin {
                let center=Point{x:w.config().width/2.0,y:w.config().height/2.0};
                let d=w.displacement(endpoint,center);
                c.score+=(normalize_angle(d.y.atan2(d.x)-c.angle).cos()*2.0-2.0)*(margin-wall);
            }
        }
        c
    }
}
impl Controller for AiController {
    fn steer(&mut self,w:&World,s:SnakeView<'_>) -> Steering {
        let clock=self.profile_enabled.then(std::time::Instant::now);
        self.prepare(w);
        let prepared=clock.map(|c|c.elapsed().as_nanos());
        if let Some(t)=prepared {self.profile[0]+=t;}
        let id=s.id as usize;
        let intelligence=(w.config().intelligence/100.0).clamp(0.0,1.0);
        let mut state=self.states[id];
        if state.generation!=s.generation {
            state=State {generation:s.generation,desired:s.angle,turn_until:u64::MAX,exit_angle:s.angle,last_angle:s.angle,
                goal:w.canonical_point(Point{x:s.segments[0].current.x+s.angle.cos()*350.0,y:s.segments[0].current.y+s.angle.sin()*350.0}),
                best_distance:f64::MAX,debug:DebugInfo {generation:s.generation,..DebugInfo::default()},..State::default()};
        }
        let turned=normalize_angle(s.angle-state.last_angle);
        state.last_angle=s.angle;
        if turned.abs()<0.015 {state.turn_accum*=0.90;}
        else if turned*state.turn_accum<0.0 {state.turn_accum=turned;}
        else {state.turn_accum+=turned;}
        if state.turn_accum.abs()>2.8 && w.tick()>=state.orbit_until {
            state.rejected=state.target;state.reject_until=w.tick()+150;state.target=0;state.waypoint=None;
            state.orbit_until=w.tick()+36;state.escape_until=0;state.turn_accum=0.0;
            state.desired=s.angle;state.turn_until=u64::MAX;state.commit_until=w.tick()+12;
            state.goal=w.canonical_point(Point{x:s.segments[0].current.x+s.angle.cos()*300.0,y:s.segments[0].current.y+s.angle.sin()*300.0});
        }
        state.debug.flags&=8;
        if state.target!=0 {
            if let Some(f)=self.food.iter().flatten().find(|f|f.id==state.target) {
                state.goal=f.position;
                let d=w.distance_squared(s.segments[0].current,f.position).sqrt();
                if d+8.0<state.best_distance {state.best_distance=d;state.last_progress=w.tick();}
                if w.tick().saturating_sub(state.last_progress)>75 {
                    state.rejected=state.target;state.reject_until=w.tick()+150;state.target=0;
                }
                if state.waypoint.is_some_and(|p|w.distance_squared(s.segments[0].current,p)<self.spatial.dx.powi(2)*0.5) {state.waypoint=None;}
            } else {state.target=0;state.waypoint=None;}
        }
        if w.tick()<state.escape_until {
            let need=(s.segments.len() as f64*s.radius*s.radius*8.0/(self.spatial.dx*self.spatial.dy)).ceil().max(24.0) as usize;
            let (room,_)=self.spatial.space(s.segments[0].current,self.mask(w,id),(need*2).max(64),0.0);
            if room>=need {
                state.escape_until=0;state.desired=s.angle;state.turn_until=u64::MAX;
                state.commit_until=w.tick()+12;
                state.goal=w.canonical_point(Point{x:s.segments[0].current.x+s.angle.cos()*300.0,y:s.segments[0].current.y+s.angle.sin()*300.0});
            } else {state.goal=s.segments.last().unwrap().current;}
        }
        let scheduled=(id+w.snake_count()-(w.tick() as usize*STRATEGY_QUOTA)%w.snake_count())%w.snake_count()<STRATEGY_QUOTA;
        let mut strategic=scheduled;
        if strategic {self.strategy(w,s,&mut state,intelligence);}
        // Equal safety coverage at every IQ. Wall-turn viability separately
        // catches inevitable wall hits beyond this fixed cost bound.
        let horizon=STEPS;
        let strategy_time=clock.map(|c|c.elapsed().as_nanos());
        if let Some(t)=strategy_time {self.profile[1]+=t-prepared.unwrap();}
        let mut candidates=[Candidate::default();CANDIDATES];
        candidates[1]=self.rollout(w,s,state,1,horizon);
        if !strategic && (candidates[1].steps<horizon || state.target==0) && self.urgent_used<URGENT_QUOTA {
            self.urgent_used+=1;strategic=true;
            self.strategy(w,s,&mut state,intelligence);
            candidates[1]=self.rollout(w,s,state,1,horizon);
        }
        let need=(s.segments.len() as f64*s.radius*s.radius*8.0/(self.spatial.dx*self.spatial.dy)).ceil().max(24.0) as usize;
        let (area,capped)=self.spatial.space(candidates[1].path[candidates[1].steps],self.mask(w,id),(need*2).max(64),candidates[1].steps as f64*STEP_SECONDS);
        candidates[1].area=area;candidates[1].uncertain=capped;
        // Revalidate the retained plan against CURRENT bodies and forecasts.
        // An open, checked continuation can wait for its scheduled strategy
        // slot; danger, a missing target or a pocket triggers full search.
        let head=s.segments[0].current;
        let wall_room=!w.config().deadly_walls || head.x.min(w.config().width-head.x).min(head.y).min(w.config().height-head.y)
            >self.rivals[id].speed/self.rivals[id].turn.max(0.01)*3.0+s.radius*3.0;
        let reuse_plan=!strategic && wall_room && state.target!=0 && candidates[1].steps==horizon
            && (area>=need || w.tick()<state.commit_until);
        if !reuse_plan || self.diagnostic_enabled {
            for kind in 0..CANDIDATES {
                if kind==1 {continue;}
                if kind==2 && state.turn_until==u64::MAX && normalize_angle(state.desired-s.angle).abs()<1e-12 {
                    candidates[kind]=candidates[1];
                } else {candidates[kind]=self.rollout(w,s,state,kind,horizon);}
            }
        } else {let retained=candidates[1];candidates.fill(retained);}
        let safe=candidates.iter().filter(|c|c.steps==horizon).count();
        let rollout_time=clock.map(|c|c.elapsed().as_nanos());
        if let Some(t)=rollout_time {self.profile[2]+=t-strategy_time.unwrap();}
        for c in &mut candidates {
            if c.area==0 {
                let (area,capped)=self.spatial.space(c.path[c.steps],self.mask(w,id),(need*2).max(64),c.steps as f64*STEP_SECONDS);
                c.area=area;c.uncertain=capped;
            }
            let (area,capped)=(c.area,c.uncertain);
            if !capped && area<need {c.score-=600.0+(need-area) as f64*8.0;}
            else {c.score+=area.min(need*2) as f64*0.08;}
        }
        if let Some(c)=clock {self.profile[3]+=c.elapsed().as_nanos()-rollout_time.unwrap();}
        candidates[1].score+=90.0+if w.tick()<state.commit_until {160.0} else {0.0};
        let best=if reuse_plan {1} else {(0..CANDIDATES).max_by(|a,b|candidates[*a].steps.cmp(&candidates[*b].steps)
            .then_with(||if safe==0 {candidates[*a].clearance.total_cmp(&candidates[*b].clearance)} else {std::cmp::Ordering::Equal})
            .then_with(||candidates[*a].score.total_cmp(&candidates[*b].score)).then_with(||b.cmp(a))).unwrap()};
        if self.diagnostic_enabled {
                let mut d=DecisionDiagnostic {generation:s.generation,selected:best,horizon,required_cells:need,length:s.segments.len(),goal:state.waypoint.unwrap_or(state.goal),reused_plan:reuse_plan,..Default::default()};
            for (i,c) in candidates.iter().enumerate() {
                let turn_ticks=if i==7 || i==8 {16} else if i==1 && state.turn_until!=u64::MAX {state.turn_until.saturating_sub(w.tick()) as usize} else {0};
                d.candidates[i]=CandidateDiagnostic {desired:c.desired,safe_ticks:c.steps,area:c.area,area_capped:c.uncertain,
                    turn_ticks,track_goal:false,exit_angle:if i==1 {state.exit_angle} else {c.exit_angle},capped:c.capped,score:c.score};
            }
            self.decisions[id]=d;
        }
        let c=candidates[best];
        if best!=1 {
            if normalize_angle(c.desired-s.angle).abs()>0.5 {
                let duration=if best==7 || best==8 {16} else {(normalize_angle(c.desired-s.angle).abs()/self.rivals[id].turn/STEP_SECONDS).ceil().clamp(12.0,STEPS as f64) as u64};
                state.commit_until=w.tick()+duration;
            }
            state.turn_until=if best==7 || best==8 {w.tick()+16} else {u64::MAX};
            state.exit_angle=c.exit_angle;
        }
        if c.area<need && !c.uncertain && s.segments.len()>=24 && w.tick()>=state.escape_until && w.tick()>=state.orbit_until {
            state.escape_until=w.tick()+45;
            state.rejected=state.target;state.reject_until=state.escape_until;
            state.target=0;state.waypoint=None;
        }
        state.desired=c.desired;
        let diff=normalize_angle(c.desired-s.angle);
        state.turn_sign=if diff>0.07 {1} else if diff< -0.07 {-1} else {0};
        state.debug.path_count=16.min(c.steps+1) as u32;
        for j in 0..state.debug.path_count as usize {
            state.debug.path[j]=c.path[j*c.steps/(state.debug.path_count as usize-1).max(1)];
        }
        state.debug.safe_seconds=c.steps as f64*STEP_SECONDS;
        state.debug.reachable_cells=c.area as u32;
        if c.uncertain {state.debug.flags|=1;}
        if c.capped {state.debug.flags|=2;}
        if c.steps<horizon {state.debug.flags|=4;}
        if reuse_plan {state.debug.flags|=16;}
        self.states[id]=state;
        if let Some(c)=clock {self.profile[4]+=c.elapsed().as_nanos();}
        Steering {desired_angle:state.desired,rush:0.0}
    }
}

#[cfg(test)]
mod tests;
