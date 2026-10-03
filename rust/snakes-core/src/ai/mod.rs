// SPDX-License-Identifier: GPL-3.0-or-later
//! Deterministic receding-horizon controller. One controller belongs to one
//! world. All scratch storage is reserved by new(); steer never allocates.
mod spatial;
mod attack;
mod pocket;
use attack::Attack;
use crate::{Point, SnakeView, World, MAX_FOOD, MAX_SEGMENTS, MAX_SNAKES, STEP_SECONDS};
use crate::controller::{Controller, Steering};
use spatial::Spatial;
const STEPS: usize = 138;
const NORMAL_STEPS: usize = 72;
const CANDIDATES: usize = 13;
const STRATEGY_QUOTA: usize = 2;
const URGENT_QUOTA: usize = 2;
const NARROW_LIMIT: usize = 4096;

// Controller headings are normally within one revolution. Avoid libm's
// general remainder in the inner forecast loops, preserving its exact result
// (including signed zero) and retaining the general path for external angles.
fn normalize_angle(angle: f64) -> f64 {
    use std::f64::consts::{PI,TAU};
    if angle.abs() < TAU {
        if angle > PI { angle-TAU }
        else if angle < -PI { angle+TAU }
        else { angle }
    } else { crate::normalize_angle(angle) }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DebugInfo {
    pub generation: u32,
    pub target_food_ids: [u64; 5],
    pub target_count: u32,
    pub path: [Point; 16],
    pub path_count: u32,
    /// Bit 0: capped area search, bit 1: capped safety query, bit 2: no safe
    /// full-horizon candidate, bit 3: active interception, bit 4: revalidated plan reuse,
    /// bit 5: trapped (uncapped area below 1.5x turnaround requirement).
    pub flags: u32,
    pub reachable_cells: u32,
    pub safe_seconds: f64,
}
/// Rust observer only; the stable C debug layout is unchanged.
#[derive(Clone, Copy, Debug, Default)]
pub struct CompetitionDebug {pub prey:Option<usize>,pub coil_radius:f64,pub rush:f64,pub attack_side:i8,pub attack_stage:u8}
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
    prey: usize, // slot + 1; zero means forage
    prey_generation: u32,
    hunt_until: u64,
    coil_center: Point,
    coil_radius: f64,
    coil_sign: f64,
    coil_initial_radius: f64,
    coil_pitch: f64,
    coil_last_angle: f64,
    coil_progress: f64,
    coil_distance: f64,
    coil_last_head: Point,
    coil_release_distance: f64,
    coil_body: Option<(usize,f64)>,
    coil_until: u64,
    harvest_until: u64,
    dodge_until: u64,
    rush: f64,
    escape_boost: bool,
    turn_sign: i8,
    last_turn_tick: u64,
    track_goal: bool,
    attack: Attack,
    attack_options: [Attack;2],
    next_response: u64,
    next_forecast: u64,
    revise_opponents: bool,
    debug: DebugInfo,
}
impl State {
    /// Install the controls carried by the rollout after bookkeeping that can
    /// clear old tactics. Never reconstruct a maneuver from its scratch slot.
    fn commit_candidate(state:&mut State,c:&Candidate,best:usize,w:&World,s:SnakeView<'_>,turn:f64,need:usize) {
        if !c.tracks_goal {state.clear_coil(s.angle);}
        if best!=1 && normalize_angle(c.desired-s.angle).abs()>0.5 {
            let duration=if c.kind==7 || c.kind==8 {16} else if c.kind==9 || c.kind==10 {64}
                else {(normalize_angle(c.desired-s.angle).abs()/turn/STEP_SECONDS).ceil().clamp(12.0,STEPS as f64) as u64};
            state.commit_until=w.tick()+duration;
        }
        if c.area<need && !c.uncertain && !c.tracks_goal && !c.attack.valid
            && s.segments.len()>=24 && w.tick()>=state.escape_until && w.tick()>=state.orbit_until {
            state.escape_until=w.tick()+45;
            state.rejected=state.target;state.reject_until=state.escape_until;
            // Escape mode invalidates attack/pocket stages and changes goals.
            // Enter it only for a validated nontracking fallback; tracking and
            // attack candidates retain every dependency of their rollout.
            state.prey=0;state.clear_coil(s.angle);state.clear_attacks(s.angle);
            state.target=0;state.waypoint=None;
        }
        state.attack=c.attack;
        state.track_goal=c.tracks_goal;
        state.turn_until=c.turn_until;
        state.exit_angle=c.exit_angle;
        state.rush=c.rush;
        state.desired=c.desired;
    }
    fn clear_coil(&mut self,angle:f64) {
        if self.coil_radius!=0.0 || self.coil_initial_radius>0.0 {
            self.desired=angle;self.exit_angle=angle;self.turn_until=u64::MAX;
            self.track_goal=false;self.commit_until=0;self.rush=0.0;
            self.target=0;self.waypoint=None;
        }
        self.coil_radius=0.0;
        self.coil_initial_radius=0.0;
        self.coil_body=None;
    }
    fn clear_attacks(&mut self,angle:f64) {
        if self.attack.valid {
            // Heading, commitment and rush are retained attack controls too.
            self.desired=angle;self.exit_angle=angle;self.turn_until=u64::MAX;
            self.track_goal=false;self.commit_until=0;self.rush=0.0;
        }
        self.attack=Attack::default();
        self.attack_options=[Attack::default();2];
    }
    fn response_interval(s:SnakeView<'_>)->u64 {
        2+((s.traits.turn_bias/0.175+1.0)*3.0).round().clamp(0.0,6.0) as u64
    }
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
    distance: [f64; STEPS+1],
    escape: [Point;3],
}
#[derive(Clone, Copy)]
struct Candidate {
    body_len: usize,
    kind: usize,
    checked: bool,
    tracks_goal: bool,
    desired: f64,
    turn_until: u64,
    exit_angle: f64,
    angle: f64,
    path: [Point; STEPS+1],
    steps: usize,
    score: f64,
    clearance: f64,
    capped: bool,
    area: usize,
    uncertain: bool,
    attack: Attack,
    rush: f64,
}

impl Default for Rival {
    fn default()->Self {Self {alive:false,radius:0.0,len:0,speed:0.0,turn:0.0,growth_delay:0.0,release_rate:0.0,angle:0.0,path:[Point::default();STEPS+1],envelope:[0.0;STEPS+1],distance:[0.0;STEPS+1],escape:[Point::default();3]}}
}
impl Default for Candidate {
    fn default()->Self {Self {body_len:0,kind:2,checked:false,tracks_goal:false,desired:0.0,turn_until:u64::MAX,exit_angle:0.0,angle:0.0,path:[Point::default();STEPS+1],steps:0,score:0.0,clearance:0.0,capped:false,area:0,uncertain:false,attack:Attack::default(),rush:0.0}}
}

/// Cached single-burst schedules; all steady-state storage belongs to the controller.
#[derive(Clone, Copy)]
struct Motion { limits: [(f64,f64);25], max_speed:f64, max_curve:f64 }
impl Default for Motion {fn default()->Self {Self {limits:[(0.0,0.0);25],max_speed:0.0,max_curve:0.0}}}
impl Motion {
    fn at(&self,offset:usize)->(f64,f64) {self.limits[offset.min(24)]}
    fn forecast(w:&World,id:usize,rush:f64)->Self {
        let limits=w.forecast_motion_schedule(id,rush).unwrap();
        let mut m=Self {limits,max_speed:limits[0].0,max_curve:limits[0].0*limits[0].1};
        for &(speed,turn) in &limits[1..] {m.max_speed=m.max_speed.max(speed);m.max_curve=m.max_curve.max(speed*turn);}
        m
    }
}

#[derive(Clone, Copy, Default)]
struct BodyCache {
    epoch: u64,
    id: usize,
    a: Point,
    b: Point,
    time: f64,
    padding: f64,
    speed: f64,
    visits: usize,
    result: (bool,f64,bool),
}

/// Optional diagnostics are separate from the stable C debug layout.
#[derive(Clone, Copy, Debug, Default)]
pub struct CandidateDiagnostic {
    pub checked: bool,
    pub desired: f64,
    pub safe_ticks: usize,
    pub area: usize,
    pub area_capped: bool,
    pub turn_ticks: usize,
    pub track_goal: bool,
    pub exit_angle: f64,
    pub capped: bool,
    pub score: f64,
    pub rush: f64,
    pub coil_center: Point,
    pub coil_radius: f64,
    pub coil_sign: f64,
    pub coil_pitch: f64,
    pub coil_progress: f64,
    pub attack_valid: bool,
    pub attack_turn_ticks: usize,
    pub attack_crossing: f64,
    pub attack_crossing_rush: f64,
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
    candidates: Option<Box<[Candidate;CANDIDATES]>>,
    rollout_distance: [f64;STEPS+1],
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
    planning_speed: [f64;MAX_SNAKES],
    motion: [Motion;MAX_SNAKES],
    boosted_motion: [Motion;MAX_SNAKES],
    observed_angles: [f64;MAX_SNAKES],
    observed_generations: [u32;MAX_SNAKES],
}
impl Default for AiController {fn default() -> Self {Self::new()}}
impl AiController {
    pub fn new() -> Self {
        Self {spatial:Spatial::new(),candidates:Some(Box::new([Candidate::default();CANDIDATES])),rollout_distance:[0.0;STEPS+1],body_cache:[BodyCache::default();256],cache_epoch:0, diagnostic_enabled:false,profile_enabled:false,profile:[0;5], decisions:[DecisionDiagnostic::default();MAX_SNAKES], states:[State::default(); MAX_SNAKES],
            rivals:[Rival::default();MAX_SNAKES], food:[None;MAX_FOOD],
            tick:u64::MAX,geometry:0,seed:0,rng:0x9e3779b97f4a7c15,urgent_used:0,max_radius:0.0,planning_speed:[0.0;MAX_SNAKES],motion:[Motion::default();MAX_SNAKES],boosted_motion:[Motion::default();MAX_SNAKES],observed_angles:[0.0;MAX_SNAKES],observed_generations:[0;MAX_SNAKES]}
    }
    pub fn enable_profile(&mut self) {self.profile_enabled=true;}
    pub fn profile(&self)->[u128;5] {self.profile}
    pub fn enable_diagnostics(&mut self) { self.diagnostic_enabled = true; }
    pub fn disable_observers(&mut self) {self.diagnostic_enabled=false;self.profile_enabled=false;}
    pub fn decision(&self, id:usize) -> DecisionDiagnostic { self.decisions[id] }
    pub fn debug(&self,id:usize) -> Option<DebugInfo> {self.states.get(id).map(|s|s.debug)}
    pub fn competition_debug(&self,id:usize)->Option<CompetitionDebug> {
        self.states.get(id).map(|s|CompetitionDebug {prey:s.prey.checked_sub(1),coil_radius:s.coil_radius,rush:s.rush,attack_side:if s.attack.valid {s.attack.side} else {0},attack_stage:if !s.attack.valid {0} else if self.tick<s.attack.turn_at {1} else {2}})
    }
    fn random(&mut self) -> f64 {
        self.rng ^= self.rng << 13; self.rng ^= self.rng >> 7; self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64/(1u64<<53) as f64
    }
    fn prepare(&mut self,w:&World) {
        if self.tick==w.tick() && self.geometry==w.geometry_generation() {return;}
        if self.geometry!=w.geometry_generation() || w.tick()<self.tick || self.seed!=w.config().seed {
            self.states.fill(State::default());
            self.observed_generations.fill(0);
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
            let id=s.id as usize;
            self.motion[id]=Motion::forecast(w,id,if w.config().rules==crate::RuleSet::Classic {w.observed_rush(id).unwrap()} else {0.0});
            self.boosted_motion[id]=if w.boost_ready(id) {Motion::forecast(w,id,0.6)} else {self.motion[id]};
            let (speed,turn)=self.motion[id].at(0);
            self.max_radius=self.max_radius.max(s.radius);self.planning_speed[s.id as usize]=speed;
            let r=&mut self.rivals[s.id as usize];
            let release_speed=if w.config().rules==crate::RuleSet::Classic {w.motion_limits(id,0.0).unwrap().0}
                else {self.motion[id].limits.iter().map(|m|m.0).fold(speed,f64::min)};
            let growth_scale=if w.config().rules==crate::RuleSet::Classic {1.0} else {speed/release_speed};
            *r=Rival {alive:true,radius:s.radius,len:s.segments.len(),speed,turn,growth_delay:w.tail_growth_delay(id).unwrap()*growth_scale,release_rate:s.radius*1.18/(release_speed*0.65).max(1.0),
                angle:s.angle,..Rival::default()};
            r.path[0]=s.segments[0].current;
            let id=s.id as usize;
            let observed_turn=if self.observed_generations[id]==s.generation {
                normalize_angle(s.angle-self.observed_angles[id]).clamp(-turn*STEP_SECONDS,turn*STEP_SECONDS)
            } else {0.0};
            self.observed_generations[id]=s.generation;self.observed_angles[id]=s.angle;
            let mut angle=s.angle;
            let mut rotation=(turn*STEP_SECONDS).sin_cos();
            let mut rotation_turn=turn;
            let mut direction=Point {x:angle.cos(),y:angle.sin()};
            for j in 1..=STEPS {
                let (speed,turn)=self.motion[id].at(j-1);
                if turn!=rotation_turn {rotation=(turn*STEP_SECONDS).sin_cos();rotation_turn=turn;}
                let t=j as f64*STEP_SECONDS;
                // The envelope is monotone and capped. Once capped, later
                // entries are exactly the cap and need no trigonometry.
                let cap=s.radius*2.0+12.0;
                r.envelope[j]=if r.envelope[j-1]==cap {cap}
                    else {(speed*t*(turn*t).min(1.2).sin()*0.3).min(cap)};
                // Extrapolate recently observed curvature for 0.3s, then
                // straighten. No desired heading, retained exit or orbit data.
                let desired=angle+observed_turn*(1.0-(j-1) as f64/9.0).max(0.0);
                let delta=normalize_angle(desired-angle);
                if delta.abs()>turn*STEP_SECONDS {
                    let sine=rotation.0*delta.signum();
                    direction=Point {x:direction.x*rotation.1-direction.y*sine,y:direction.x*sine+direction.y*rotation.1};
                    angle=normalize_angle(angle+turn*STEP_SECONDS*delta.signum());
                } else if delta.abs()>1e-12 {
                    angle=desired;
                    let (sin,cos)=angle.sin_cos();direction=Point{x:cos,y:sin};
                }
                r.distance[j]=r.distance[j-1]+speed*STEP_SECONDS;
                r.path[j]=w.canonical_point(Point {x:r.path[j-1].x+direction.x*speed*STEP_SECONDS,
                    y:r.path[j-1].y+direction.y*speed*STEP_SECONDS});
            }
            let heading=w.displacement(r.path[32],r.path[36]);
            let theta=heading.y.atan2(heading.x);
            for (i,offset) in [-0.9,0.0,0.9].into_iter().enumerate() {
                let angle=theta+offset;
                r.escape[i]=w.canonical_point(Point{x:r.path[36].x+angle.cos()*speed*0.85,
                    y:r.path[36].y+angle.sin()*speed*0.85});
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
        let revise_tactics=w.tick()>=state.next_response;
        state.attack_options=[Attack::default();2];
        if w.tick()<state.escape_until {
            state.clear_attacks(s.angle);
            state.debug.flags&=!8;
            state.goal=s.segments.last().unwrap().current;
            state.target=0;state.waypoint=None;
            state.debug.target_count=0;state.debug.target_food_ids=[0;5];
            state.last_strategy=w.tick();return;
        }
        if w.tick()<state.orbit_until {
            state.clear_attacks(s.angle);
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
        let tactical_goal=state.goal;
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
        if revise_tactics {self.tactics(w,s,state,intelligence);}
        else if state.prey!=0 || w.tick()<state.dodge_until {
            state.goal=tactical_goal;state.target=0;
            if state.prey!=0 {state.debug.flags|=8;}
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
        if revise_tactics {
            state.attack_options=self.cutoffs(w,s,*state);
            // Consume a response only after tactics and its candidate generation
            // actually run. Quota waits and recovery early returns keep it due.
            state.next_response=w.tick()+State::response_interval(s);
        }
        state.last_strategy=w.tick();
    }
    fn trajectory_goal(w:&World,state:State,head:Point)->Point {
        if state.coil_radius>0.0 {
            let d=w.displacement(state.coil_center,head);
            let radius=(d.x*d.x+d.y*d.y).sqrt().max(1.0);
            let tangent=d.y.atan2(d.x)+state.coil_sign*std::f64::consts::FRAC_PI_2;
            let contraction=if state.coil_progress>0.9 {state.coil_pitch/std::f64::consts::TAU} else {0.0};
            let correction=((radius-state.coil_radius)/30.0).atan().clamp(-0.35,0.35);
            let angle=tangent+state.coil_sign*((contraction/radius).atan()+correction);
            w.canonical_point(Point{x:head.x+angle.cos()*80.0,y:head.y+angle.sin()*80.0})
        } else {state.waypoint.unwrap_or(state.goal)}
    }
    fn boost_request(w:&World,s:SnakeView<'_>,rush:f64)->f64 {
        if w.config().rules == crate::RuleSet::Classic { rush }
        else if rush >= 0.5 && w.boost_ready(s.id as usize) { 0.6 } else { 0.0 }
    }
    fn rush_for(&self,w:&World,s:SnakeView<'_>,state:State)->f64 {
        if self.attack_usable(w,s,state,state.attack) {return Self::boost_request(w,s,state.attack.control(w.tick()).1);}
        if w.config().rules==crate::RuleSet::V2 {
            // A pursuit is not itself permission to spend tail. Only a checked
            // cutoff, a valuable contested target or a differential escape is.
            if state.prey==0 && state.coil_radius==0.0 && self.food_race(w,s,state) {return 0.6;}
            return 0.0;
        }
        let d=w.displacement(s.segments[0].current,state.goal);
        let alignment=normalize_angle(d.y.atan2(d.x)-s.angle).abs();
        let settled=normalize_angle(state.desired-s.angle).abs()<0.6;
        if state.prey!=0 && state.coil_radius==0.0 && alignment<0.65 && settled {s.traits.aggression}
        else if w.tick()<state.dodge_until && settled {0.25} else {0.0}
    }
    fn food_race(&self,w:&World,s:SnakeView<'_>,state:State)->bool {
        if !w.boost_ready(s.id as usize) || state.target==0 || state.waypoint.is_some() {return false;}
        let Some(f)=self.food.iter().flatten().find(|f|f.id==state.target && f.vacuum_owner<0) else {return false;};
        let worth=f.value.max(self.spatial.cluster_weight(self.spatial.key(f.position)));
        if worth<3.0 {return false;}
        let d=w.displacement(s.segments[0].current,f.position);
        let distance=(d.x*d.x+d.y*d.y).sqrt();
        let (speed,turn)=self.boosted_motion[s.id as usize].at(0);
        let angle=normalize_angle(d.y.atan2(d.x)-s.angle).abs();
        if distance<speed/turn*(1.0-angle.cos())+s.radius*2.7 || angle>0.6 {return false;}
        self.rivals.iter().enumerate().any(|(id,r)|id!=s.id as usize && r.alive
            && (w.distance_squared(r.path[0],f.position).sqrt()-distance).abs()<=distance*0.25)
    }

    fn attack_usable(&self,w:&World,s:SnakeView<'_>,state:State,a:Attack)->bool {
        if !a.valid || state.prey==0 || a.prey!=state.prey || a.prey_generation!=state.prey_generation
            || w.tick()<a.start || w.tick()>=a.end || w.tick()>state.hunt_until
            || w.tick()<state.escape_until || w.tick()<state.orbit_until || state.coil_radius>0.0 {return false;}
        let Some(victim)=w.snake(state.prey-1).filter(|v|v.alive && v.generation==a.prey_generation) else {return false;};
        if s.segments.len()<victim.segments.len()+if w.config().rules==crate::RuleSet::V2 {4} else {8} {return false;}
        if w.config().rules==crate::RuleSet::Classic {
            if a.limits.is_some_and(|limits|limits!=Self::attack_limits(w,s,a)) {return false;}
        } else {
            if w.tick()==a.start && (!w.boost_ready(s.id as usize) || s.segments.len()<16) {return false;}
            if s.flags & crate::flags::FROZEN!=0 {return false;}
            if let Some(limits)=a.limits {
                let original=limits[5] as usize;
                let elapsed=w.tick().saturating_sub(a.start) as usize;
                let expected=original.saturating_sub((2+original/100)*elapsed.min(21)/21);
                // Scheduled payments are part of the plan. Growth, unrelated
                // shrinkage and size changes invalidate it immediately.
                if s.segments.len()!=expected || s.radius>limits[4] {return false;}
                let ratio=(1.0+original.saturating_sub(24) as f64*0.004)
                    /(1.0+expected.saturating_sub(24) as f64*0.004);
                let expected_speed=if elapsed<=24 {limits[0]} else {limits[2]}*ratio;
                let actual=w.motion_limits(s.id as usize,a.burst).unwrap().0;
                if (actual-expected_speed).abs()>1e-9 {return false;}
            }
        }
        let passed=w.displacement(a.point,victim.segments[0].current);
        if passed.x*a.prey_heading.cos()+passed.y*a.prey_heading.sin()>30.0 {return false;}
        let fast=w.motion_limits(s.id as usize,a.burst).unwrap().0;
        let rival=w.motion_limits(victim.id as usize,w.observed_rush(victim.id as usize).unwrap()).unwrap().0;
        !(fast<rival && w.distance_squared(s.segments[0].current,a.point)>60.0*60.0)
    }
    fn validate_attack(&self,w:&World,s:SnakeView<'_>,state:&mut State) {
        if state.attack.valid && !self.attack_usable(w,s,*state,state.attack) {
            state.clear_attacks(s.angle);
        }
        for i in 0..2 {
            if !self.attack_usable(w,s,*state,state.attack_options[i]) {state.attack_options[i]=Attack::default();}
        }
    }
    /// Only matching single-tick sweeps can certify a lethal head contest.
    /// A multi-tick intersection can instead be a lethal deposited neck.
    fn head_contact(w:&World,a:&[Point],b:&[Point],from:usize,to:usize,reach:f64)->bool {
        (from+1..=to).any(|j|w.segments_distance_squared(a[j-1],a[j],b[j-1],b[j])<reach*reach)
    }
    /// Tactical goals compete in the same candidate ranking as foraging.
    /// A hunt is sticky by rival generation, never by a stale food position.
    fn tactics(&self,w:&World,s:SnakeView<'_>,state:&mut State,intelligence:f64) {
        let id=s.id as usize;let head=s.segments[0].current;
        self.validate_attack(w,s,state);
        let mut own=self.rivals[id];
        let aggression=s.traits.aggression*(0.65+0.35*intelligence);
        let advantage=if w.config().rules==crate::RuleSet::V2 {4+2+s.segments.len()/100} else {8};
        let interest=if w.config().rules==crate::RuleSet::V2 {0.2} else {0.32};
        if aggression>interest {
            let (speed,turn)=w.motion_limits(id,s.traits.aggression).unwrap();own.speed=speed;own.turn=turn;
        }
        if state.prey!=0 {
            let prey=state.prey-1;
            let victim=w.snake(prey).unwrap();
            if !victim.alive && victim.generation==state.prey_generation {
                // The body becomes a feast. Keep its last observed center as
                // a short-lived harvest goal until ordinary food scoring sees it.
                state.goal=state.coil_center;state.harvest_until=w.tick()+60;
            }
            // A validated attack already accounts for its scheduled payments.
            // The full new-burst budget applies only to ordinary hunts; charging
            // it again after a payment would abandon a still-winning crossing.
            if !victim.alive || victim.generation!=state.prey_generation || (!state.attack.valid && own.len<victim.segments.len()+advantage)
                || w.tick()>state.hunt_until || w.distance_squared(head,victim.segments[0].current)>650.0*650.0 {
                state.prey=0;state.clear_coil(s.angle);state.track_goal=false;state.clear_attacks(s.angle);
            }
        }
        if state.prey==0 && w.tick()<state.harvest_until {
            state.goal=state.coil_center;state.target=0;state.debug.flags|=8;return;
        }
        // Early speed/gap escape. Aim along an open lateral route rather than
        // reversing toward the hunter's neck; exact safety chooses the gap.
        let mut threat=None;let mut threat_value=0.0;
        for (other,r) in self.rivals.iter().enumerate() {
            if !r.alive || other==id || r.len<own.len+6 {continue;}
            let d=w.displacement(r.path[0],head);let distance=(d.x*d.x+d.y*d.y).sqrt();
            let approaching=(d.x*r.angle.cos()+d.y*r.angle.sin())/distance.max(1.0);
            let v=(220.0-distance)*(0.3+approaching.max(0.0));
            if v>threat_value {threat_value=v;threat=Some(other);}
        }
        if let Some(other)=threat.filter(|_|threat_value>32.0) {
            let r=&self.rivals[other];let away=w.displacement(r.path[18],head);
            let distance=(away.x*away.x+away.y*away.y).sqrt().max(1.0);
            // Retain forward travel to exploit the smaller snake's speed.
            state.goal=w.canonical_point(Point{x:head.x+away.x/distance*240.0+s.angle.cos()*160.0,
                y:head.y+away.y/distance*240.0+s.angle.sin()*160.0});
            state.target=0;state.prey=0;state.clear_coil(s.angle);state.track_goal=false;state.clear_attacks(s.angle);state.waypoint=None;state.dodge_until=w.tick()+18;
            return;
        }
        if state.prey==0 && aggression>interest {
            let mut value=0.0;
            for (other,r) in self.rivals.iter().enumerate() {
                if !r.alive || other==id || own.len<r.len+advantage {continue;}
                let distance=w.distance_squared(head,r.path[0]).sqrt();
                if distance>620.0 {continue;}
                // Very long snakes cannot catch a fast rival in open space.
                // Hunt only if it is already crossing our reachable route;
                // otherwise preserve the cheap food plan until a real cutoff
                // opportunity appears. This matters in the mature cap fixture.
                if own.speed<r.speed*0.85 && [24,48,72].into_iter().all(|j|
                    self.arrival(w,&own,r.path[j])>j as f64*STEP_SECONDS*1.15) {continue;}
                let eta=self.arrival(w,&own,r.path[36]);
                let facing=w.displacement(head,r.path[36]);
                let alignment=normalize_angle(facing.y.atan2(facing.x)-s.angle).cos();
                let v=aggression*(r.len as f64).sqrt()*(1.3+alignment)/(eta+1.0);
                if v>value {value=v;state.prey=other+1;}
            }
            // Paid boosts already pass their own value and readiness gates;
            // admit nearby moderate-interest hunts without spending on chase.
            if value<if w.config().rules==crate::RuleSet::V2 {0.75} else {1.0} {state.prey=0;}
            if state.prey!=0 {
                state.prey_generation=w.snake(state.prey-1).unwrap().generation;
                state.hunt_until=w.tick()+180;
                state.coil_sign=if s.traits.turn_bias<0.0 {-1.0} else {1.0};
            }
        }
        if state.prey==0 {return;}
        let r=&self.rivals[state.prey-1];
        self.pocket(w,s,state);
        if state.coil_radius==0.0 {
            // Lead the actually forecast turn schedule and its envelope. If
            // ahead of the rival, cross beyond its route to deposit a barrier;
            // a wall on the far side turns that barrier into a herding move.
            let mut lead=r.path[48];let mut best=f64::MAX;
            for j in [12,24,36,48,60,72] {
                let eta=self.arrival(w,&own,r.path[j]);let t=j as f64*STEP_SECONDS;
                let error=(eta-t).abs();
                if error<best {best=error;lead=r.path[j];}
            }
            let eta=self.arrival(w,&own,lead);
            let across=w.displacement(head,lead);let norm=(across.x*across.x+across.y*across.y).sqrt().max(1.0);
            let extension=if eta<1.7 {(s.radius+r.radius)*2.0+r.envelope[48]} else {0.0};
            let relative=w.displacement(head,r.path[0]);
            let behind=relative.x*r.angle.cos()+relative.y*r.angle.sin();
            let lateral=-relative.x*r.angle.sin()+relative.y*r.angle.cos();
            let lane=own.speed/own.turn.max(0.01)*1.4+(s.radius+r.radius)*2.0;
            if behind>20.0 && lateral.abs()<lane*1.4 {
                // Catch alongside, then turn across the head. Direct pursuit
                // from behind can only strike its body, regardless of size.
                let side=if lateral.abs()>8.0 {-lateral.signum()} else {state.coil_sign};
                state.goal=w.canonical_point(Point{x:lead.x-r.angle.sin()*lane*side,
                    y:lead.y+r.angle.cos()*lane*side});
            } else {
                state.goal=w.canonical_point(Point{x:lead.x+across.x/norm*extension,y:lead.y+across.y/norm*extension});
            }
            state.coil_center=r.path[0];
        }
        state.target=0;state.debug.flags|=8;
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
        let neck_advance=self.planning_speed[s.id as usize]*time/(s.radius*1.18);
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
                let body=if w.config().rules==crate::RuleSet::V2 {
                    // Advancing an old neck sample may widen it; retain the
                    // maximum width until it has passed the full-width band.
                    if (j as f64)<0.07*r.len.saturating_sub(1).max(1) as f64 {
                        let advance=((time-r.growth_delay).max(0.0)/r.release_rate).max(0.0);
                        let scale=r.len.saturating_sub(1).max(1) as f64;
                        crate::world::taper::span_radius(r.radius,j as f64/scale,(j as f64+advance)/scale)
                    } else {r.radius*self.spatial.widths[encoded]}
                } else {r.radius};
                let threshold=crate::world::taper::contact_radius(w.config().rules,s.radius,body,same);
                // Half a tick's motion and a small radius reserve protect the
                // first swept tick against segments that shift along corners.
                let margin=if time<=STEP_SECONDS+1e-9 {if same && w.config().rules==crate::RuleSet::V2 {0.75+1.25*body/r.radius} else {0.75}} else {1.5+(r.speed*STEP_SECONDS*0.35).min(r.radius*0.25)+padding};
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
        if old.epoch==self.cache_epoch && old.id==s.id as usize && old.a==a && old.b==b && old.time==time && old.padding==padding && old.speed==self.planning_speed[s.id as usize] {
            *tests+=old.visits;
            if *tests>NARROW_LIMIT {return (true,old.result.1,true);}
            return old.result;
        }
        let before=*tests;
        let result=self.body_blocked(w,s,a,b,time,padding,tests);
        if !result.2 {
            self.body_cache[key]=BodyCache {epoch:self.cache_epoch,id:s.id as usize,a,b,time,padding,speed:self.planning_speed[s.id as usize],visits:*tests-before,result};
        }
        result
    }
    #[cfg(test)]
    fn rollout(&mut self,w:&World,s:SnakeView<'_>,state:State,kind:usize,horizon:usize) -> Candidate {
        let mut candidate=Candidate::default();
        self.rollout_into(w,s,state,kind,horizon,&mut candidate);
        candidate
    }
    fn candidate_rush(w:&World,s:SnakeView<'_>,state:State,kind:usize,attack:Attack)->f64 {
        if kind==11 {0.0} else if kind==12 {Self::boost_request(w,s,0.6)}
            else {Self::boost_request(w,s,if attack.valid {attack.control(w.tick()).1} else if w.config().rules==crate::RuleSet::Classic || kind==0 || kind==1 || state.escape_boost {state.rush} else {0.0})}
    }
    fn rollout_into(&mut self,w:&World,s:SnakeView<'_>,state:State,kind:usize,horizon:usize,c:&mut Candidate) {
        let proposed=if kind==1 {state.attack} else if kind==7 || kind==8 {state.attack_options[kind-7]} else {Attack::default()};
        let attack=if self.attack_usable(w,s,state,proposed) {proposed} else {Attack::default()};
        let rush=Self::candidate_rush(w,s,state,kind,attack);
        let motion=if rush>0.0 {self.boosted_motion[s.id as usize]} else {self.motion[s.id as usize]};
        let (mut speed,mut turn)=if w.config().rules==crate::RuleSet::V2 {motion.at(0)} else {w.motion_limits(s.id as usize,rush).unwrap()};
        let maximum_speed=if w.config().rules==crate::RuleSet::V2 {motion.max_speed} else if attack.valid {w.motion_limits(s.id as usize,attack.burst).unwrap().0} else {speed};
        let crossing_limits=if attack.valid {w.motion_limits(s.id as usize,attack.crossing_rush).unwrap()} else {(speed,turn)};
        self.planning_speed[s.id as usize]=maximum_speed;
        let tracks_goal=!attack.valid && (((kind==0 || kind==11) && (state.coil_radius>0.0 || state.track_goal)) || ((kind==1 || kind==12) && state.track_goal));
        // Reset metadata only. Every path/distance entry that can be read is
        // overwritten by this rollout; clearing the unused horizon for every
        // candidate used to stream hundreds of KB of zeros per snake/tick.
        c.body_len=if w.config().rules==crate::RuleSet::V2 && (rush>0.0 || s.boost_ticks>1) {
            s.segments.len().saturating_sub(w.boost_segment_cost(s.id as usize).unwrap())
        } else {s.segments.len()};
        c.kind=kind;c.checked=false;c.tracks_goal=tracks_goal;c.desired=0.0;
        c.turn_until=match kind {1|12=>state.turn_until,7|8=>w.tick()+16,9|10=>w.tick()+32,_=>u64::MAX};
        c.exit_angle=if kind==1 || kind==12 {state.exit_angle} else {0.0};
        c.angle=s.angle;c.steps=0;c.score=0.0;c.clearance=200.0;
        c.capped=false;c.area=0;c.uncertain=false;c.attack=attack;c.rush=rush;
        c.path[0]=s.segments[0].current;
        let goal=state.waypoint.unwrap_or(state.goal);
        let d=w.displacement(c.path[0],goal);
        let fixed=match kind {0|11=>d.y.atan2(d.x),1|12=>state.desired,2=>s.angle,3=>s.angle+0.6,4=>s.angle-0.6,
            5|7|9=>s.angle+3.0,6|8|10=>s.angle-3.0,_=>s.angle};
        if kind==7 || kind==8 {
            c.exit_angle=normalize_angle(s.angle+normalize_angle(fixed-s.angle).clamp(-turn*STEP_SECONDS*16.0,turn*STEP_SECONDS*16.0));
        }
        if kind==9 || kind==10 {c.exit_angle=s.angle;}
        let mut tests=0;
        let mut trajectory_state=state;
        let mut direction=Point {x:s.angle.cos(),y:s.angle.sin()};
        let mut rotation=(turn*STEP_SECONDS).sin_cos();
        self.rollout_distance[0]=0.0;
        let mut checked=0;
        let max_curve=if w.config().rules==crate::RuleSet::V2 {motion.max_curve} else if attack.valid {let limits=w.motion_limits(s.id as usize,attack.burst).unwrap();
            (limits.0*limits.1).max(crossing_limits.0*crossing_limits.1)} else {speed*turn};
        let mut near=0u16;let mut defeated=0u16;let mut reached=false;
        for (other,r) in self.rivals.iter().enumerate() {
            if r.alive && other!=s.id as usize && w.distance_squared(c.path[0],r.path[0])
                <((speed+r.speed)*horizon as f64*STEP_SECONDS+(s.radius+r.radius)*3.0+20.0).powi(2) {
                near|=1<<other;
            }
        }
        for j in 1..=horizon {
            let attack_control=attack.control(w.tick()+j as u64-1);
            if w.config().rules==crate::RuleSet::V2 {
                let limits=motion.at(j-1);
                if limits!=(speed,turn) {(speed,turn)=limits;rotation=(turn*STEP_SECONDS).sin_cos();}
            } else if attack.valid {
                if w.tick()+j as u64-1>=attack.turn_at {(speed,turn)=crossing_limits;}
                rotation=(turn*STEP_SECONDS).sin_cos();
            }
            if tracks_goal {
                Self::advance_spiral(w,&mut trajectory_state,c.path[j-1]);
                if state.coil_radius>0.0 && (trajectory_state.coil_radius<=0.0 || speed*Self::spiral_curvature(trajectory_state.coil_radius,trajectory_state.coil_pitch)>turn) {break;}
            }
            let desired=if attack.valid {attack_control.0} else if tracks_goal {
                let goal=Self::trajectory_goal(w,trajectory_state,c.path[j-1]);
                if state.target!=0 && w.distance_squared(c.path[j-1],goal)<(s.radius*2.7).powi(2) {reached=true;}
                let d=w.displacement(c.path[j-1],goal);
                if reached || (state.prey!=0 && defeated&(1<<(state.prey-1))!=0) {c.angle} else {d.y.atan2(d.x)}
            } else if w.tick()+j as u64>c.turn_until {c.exit_angle} else {fixed};
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
            if j==16 && kind!=1 && kind!=12 && kind!=9 && kind!=10 {c.exit_angle=c.angle;}
            let p=w.canonical_point(Point {x:c.path[j-1].x+direction.x*speed*STEP_SECONDS,
                y:c.path[j-1].y+direction.y*speed*STEP_SECONDS});
            c.path[j]=p;
            // Even a first-step collision is checked. A tactic feasibility
            // rejection before constructing that step is not a fallback.
            c.checked=true;
            self.rollout_distance[j]=self.rollout_distance[j-1]+speed*STEP_SECONDS;
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
            // Finish the old stage before changing speed/turn. Every consumer
            // uses this same interval, including head and deposited-self sweeps.
            let sweep=j==1 || j%4==0 || j==horizon || (attack.valid && w.tick()+j as u64==attack.turn_at)
                || (w.config().rules==crate::RuleSet::V2 && motion.at(j)!=motion.at(j-1));
            let from_index=checked;
            let span=(j-from_index) as f64*STEP_SECONDS;
            let padding=speed*turn*span*span/8.0;
            if sweep {
                let from=c.path[from_index];
                checked=j;
                let (hit,clearance,capped)=self.cached_body_blocked(w,s,from,p,t,padding,&mut tests);
                c.clearance=c.clearance.min(clearance); c.capped|=capped;
                if hit {break;}
            }
            let mut hit=false;
            let mut active=if sweep && (j==1 || state.revise_opponents) {near & !defeated} else {0};
            while active!=0 {
                let other=active.trailing_zeros() as usize;active&=active-1;
                let r=&self.rivals[other];
                let winning=c.body_len>=r.len+if w.config().rules==crate::RuleSet::V2 {4} else {6};
                let reach=(s.radius+r.radius)*0.82;
                // A bounded turn envelope widens with time, but not into an
                // arbitrary reachable disk that would paralyze all pursuit.
                let envelope=r.envelope[j]+padding+r.speed*r.turn*span*span/8.0;
                let head_near=w.distance_squared(p,r.path[j])<(reach+envelope+3.0+self.rollout_distance[j]-self.rollout_distance[from_index]+r.speed*span).powi(2);
                if !winning && head_near && w.segments_distance_squared(c.path[from_index],p,r.path[from_index],r.path[j])<(reach+envelope+3.0).powi(2) {hit=true;break;}
                let lethal_head=winning && head_near && Self::head_contact(w,&c.path,&r.path,from_index,j,reach);
                // Future rival neck deposition is lethal even for a winner.
                // Skip the last head-sized piece: that is the head contest.
                let neck_steps=((reach+5.0)/(r.speed*STEP_SECONDS).max(0.1)).ceil() as usize;
                if sweep && j>neck_steps && w.distance_squared(p,r.path[0])<(r.speed*t+reach+speed*STEP_SECONDS).powi(2) {
                    for k in (1..=j-neck_steps).step_by(3) {
                        let end=(k+2).min(j-neck_steps);
                        // Full-width broad phase before any tapered tail powf.
                        let reserve=reach+2.0+self.rollout_distance[j]-self.rollout_distance[from_index]+3.0*r.speed*STEP_SECONDS;
                        if w.distance_squared(p,r.path[end])>=reserve*reserve {continue;}
                        let reach=if w.config().rules==crate::RuleSet::V2 {
                            let length=r.len.saturating_sub(1).max(1) as f64*r.radius*1.18;
                            let age=r.distance[j]-r.distance[end];
                            if age>length {continue;}
                            let body=crate::world::taper::span_radius(r.radius,
                                age/length,(r.distance[j]-r.distance[k-1])/length);
                            crate::world::taper::contact_radius(w.config().rules,s.radius,body,false)
                        } else {reach};
                        if w.distance_squared(p,r.path[end])<(reach+2.0+self.rollout_distance[j]-self.rollout_distance[from_index]+3.0*r.speed*STEP_SECONDS).powi(2)
                            && w.segments_distance_squared(c.path[from_index],p,r.path[k-1],r.path[end])<(reach+2.0).powi(2) {hit=true;break;}
                    }
                }
                if hit {break;}
                // Check the deposited neck before dropping later geometry:
                // an earlier body contact cannot be rescued by a later kill.
                if lethal_head {defeated|=1<<other;}
            }
            if hit {break;}
            if w.config().self_collisions {
                let age=(s.radius*1.18*10.0/(if attack.valid || w.config().rules==crate::RuleSet::V2 {maximum_speed} else {speed})/STEP_SECONDS).ceil() as usize;
                if sweep && j>age {
                    for k in (0..j-age).step_by(3) {
                        let end=(k+3).min(j-age);
                        if self.rollout_distance[j]-self.rollout_distance[end]<s.radius*1.18*10.0 {continue;}
                        let full=s.radius*1.48+3.0+max_curve*(4.0*STEP_SECONDS).powi(2)/8.0;
                        let reserve=full+self.rollout_distance[j]-self.rollout_distance[from_index];
                        if w.distance_squared(p,c.path[end])>=reserve*reserve {continue;}
                        let body=if w.config().rules==crate::RuleSet::V2 {
                            let length=c.body_len.saturating_sub(1).max(1) as f64*s.radius*1.18;
                            if self.rollout_distance[j]-self.rollout_distance[end]>length {continue;}
                            crate::world::taper::span_radius(s.radius,
                                (self.rollout_distance[j]-self.rollout_distance[end])/length,
                                (self.rollout_distance[j]-self.rollout_distance[k])/length)
                        } else {s.radius};
                        let reach=crate::world::taper::contact_radius(w.config().rules,s.radius,body,true)+3.0+max_curve*(4.0*STEP_SECONDS).powi(2)/8.0;
                        if w.distance_squared(p,c.path[end])<(reach+self.rollout_distance[j]-self.rollout_distance[from_index]).powi(2)
                            && w.segments_distance_squared(c.path[from_index],p,c.path[k],c.path[end])<reach*reach {hit=true;break;}
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
        let hysteresis=if sign!=0 && state.turn_sign!=0 && sign!=state.turn_sign {
            if w.tick().saturating_sub(state.last_turn_tick)<18 {320.0} else {55.0}
        } else {0.0};
        let heading=w.displacement(endpoint,goal);
        let alignment=if closest<(s.radius*2.7).powi(2) {1.0} else {normalize_angle(heading.y.atan2(heading.x)-c.angle).cos()};
        c.score=(old-new)*0.9+alignment*12.0+c.clearance.clamp(-10.0,50.0)*0.22
            -diff.abs()*4.0+diff*s.traits.turn_bias*3.0-hysteresis-normalize_angle(c.desired-state.desired).abs()*2.0;
        if state.prey!=0 {
            let prey=&self.rivals[state.prey-1];
            let mut intercept=f64::MAX;let mut cutoff=f64::MAX;
            for j in (4..=c.steps).step_by(4) {
                intercept=intercept.min(w.distance_squared(c.path[j],prey.path[j]));
                // Placing a neck 0.4--0.8s ahead of the target leaves a body
                // barrier rather than a tail chase. Both still need safety.
                for lead in [12,24] {
                    let future=(j+lead).min(STEPS);
                    cutoff=cutoff.min(w.segment_distance_squared(prey.path[future],c.path[j-4],c.path[j]));
                }
            }
            let reach=s.radius+prey.radius+prey.envelope[36]+25.0;
            let pressure=(1.0-intercept.sqrt()/reach).max(0.0);
            let barrier=(1.0-cutoff.sqrt()/(reach+15.0)).max(0.0);
            let mut closed=0;
            for exit in prey.escape {
                let cfg=w.config();
                let wall=cfg.deadly_walls && (exit.x<prey.radius*4.0 || exit.x>cfg.width-prey.radius*4.0
                    || exit.y<prey.radius*4.0 || exit.y>cfg.height-prey.radius*4.0);
                let mut blocks=wall;
                for k in (4..=c.steps.min(24)).step_by(4) {
                    if w.segments_distance_squared(prey.path[36],exit,c.path[k-4],c.path[k])<((s.radius+prey.radius)*1.15).powi(2) {blocks=true;break;}
                }
                if blocks {closed+=1;}
            }
            // Reward reducing several turn-envelope exits. Walls provide one
            // side of a trap; depositing our neck supplies the other side.
            c.score+=closed as f64*70.0*s.traits.aggression;
            c.score+=(pressure*155.0+barrier*110.0)*s.traits.aggression;
        }
        if w.config().rules==crate::RuleSet::V2 && rush>0.0 {
            // One nutrition unit is about 30 pixels of ordinary progress.
            // More mature snakes spend more segments for the same burst.
            c.score-=30.0*2.5*(2+s.segments.len()/100) as f64/2.0;
        }
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
    }
}
impl Controller for AiController {
    fn intent_flags(&self, id: u32) -> Option<u32> {
        let s = self.states[id as usize];
        Some((if s.prey != 0 || s.coil_radius > 0.0 { crate::flags::HUNTING } else { 0 })
            | if s.debug.flags & 32 != 0 { crate::flags::TRAPPED } else { 0 })
    }
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
        // Forecast selection is performed in every rollout and has its own
        // cadence. Tactical revision waits for an actual strategy slot.
        state.revise_opponents=w.tick()>=state.next_forecast;
        if state.revise_opponents {state.next_forecast=w.tick()+State::response_interval(s);}
        // Finalists are scratch for this decision, never reusable life state.
        state.attack_options=[Attack::default();2];
        self.validate_attack(w,s,&mut state);
        if state.prey!=0 && w.snake(state.prey-1).is_none_or(|v|!v.alive || v.generation!=state.prey_generation) {
            // Identity/liveness are immediate observations, not tactical delay.
            if w.snake(state.prey-1).is_some_and(|v|!v.alive && v.generation==state.prey_generation) {state.harvest_until=w.tick()+60;state.goal=state.coil_center;
            }
            state.prey=0;state.clear_attacks(s.angle);state.clear_coil(s.angle);state.track_goal=false;
        }
        let had_coil=state.coil_radius>0.0;
        Self::advance_spiral(w,&mut state,s.segments[0].current);
        if had_coil && !self.pocket_usable(w,s,state) {
            state.clear_coil(s.angle);
            state.goal=w.canonical_point(Point{x:s.segments[0].current.x+s.angle.cos()*300.0,y:s.segments[0].current.y+s.angle.sin()*300.0});
        }
        let turned=normalize_angle(s.angle-state.last_angle);
        state.last_angle=s.angle;
        if turned.abs()<0.015 {state.turn_accum*=0.90;}
        else if turned*state.turn_accum<0.0 {state.turn_accum=turned;}
        else {state.turn_accum+=turned;}
        if state.turn_accum.abs()>2.8 && w.tick()>=state.orbit_until && state.coil_radius==0.0 {
            state.rejected=state.target;state.reject_until=w.tick()+150;state.target=0;state.waypoint=None;
            state.orbit_until=w.tick()+36;state.escape_until=0;state.turn_accum=0.0;state.prey=0;state.clear_attacks(s.angle);
            state.desired=s.angle;state.turn_until=u64::MAX;state.track_goal=false;state.commit_until=w.tick()+12;
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
                if state.track_goal && (f.vacuum_owner==s.id as i32 || d<s.radius*2.7) {
                    state.target=0;state.track_goal=false;state.desired=s.angle;state.turn_until=u64::MAX;
                }
                if state.waypoint.is_some_and(|p|w.distance_squared(s.segments[0].current,p)<self.spatial.dx.powi(2)*0.5) {state.waypoint=None;}
            } else {state.target=0;state.waypoint=None;}
        }
        if w.tick()<state.escape_until {
            let need=(s.segments.len() as f64*s.radius*s.radius*8.0/(self.spatial.dx*self.spatial.dy)).ceil().max(24.0) as usize;
            let (room,_)=self.spatial.space(s.segments[0].current,self.mask(w,id),(need*2).max(64),0.0);
            if room>=need {
                state.escape_until=0;state.desired=s.angle;state.turn_until=u64::MAX;state.track_goal=false;
                state.commit_until=w.tick()+12;
                state.goal=w.canonical_point(Point{x:s.segments[0].current.x+s.angle.cos()*300.0,y:s.segments[0].current.y+s.angle.sin()*300.0});
            } else {state.goal=s.segments.last().unwrap().current;}
        }
        let scheduled=(id+w.snake_count()-(w.tick() as usize*STRATEGY_QUOTA)%w.snake_count())%w.snake_count()<STRATEGY_QUOTA;
        let mut strategic=scheduled;
        if strategic {self.strategy(w,s,&mut state,intelligence);}
        state.rush=self.rush_for(w,s,state);
        self.planning_speed[id]=w.motion_limits(id,state.rush).unwrap().0;
        // Equal safety coverage at every IQ. Wall-turn viability separately
        // catches inevitable wall hits beyond this fixed cost bound.
        // Extend only sustained turns by long snakes: this detects closure
        // before the old horizon loses every exit. Extra S maneuvers are
        // reserved for these states, keeping open-space work unchanged.
        let enclosure_risk=w.config().self_collisions && s.segments.len()>=40
            && (state.turn_accum.abs()>0.8 || normalize_angle(state.desired-s.angle).abs()>1.5);
        let extended=enclosure_risk && scheduled;
        let horizon=if extended {STEPS} else {NORMAL_STEPS};
        let strategy_time=clock.map(|c|c.elapsed().as_nanos());
        if let Some(t)=strategy_time {self.profile[1]+=t-prepared.unwrap();}
        // Temporarily lease the preallocated scratch so rollout methods can
        // also mutably access shared caches. No allocation or array reset.
        let mut candidates=self.candidates.take().expect("non-reentrant controller");
        let mut area_alias=[None;CANDIDATES];
        self.rollout_into(w,s,state,1,horizon,&mut candidates[1]);
        if !strategic && (candidates[1].steps<horizon || (state.target==0 && state.prey==0 && w.tick().saturating_sub(state.last_strategy)>6)) && self.urgent_used<URGENT_QUOTA {
            self.urgent_used+=1;strategic=true;
            self.strategy(w,s,&mut state,intelligence);
            state.rush=self.rush_for(w,s,state);
            self.planning_speed[id]=w.motion_limits(id,state.rush).unwrap().0;
            self.rollout_into(w,s,state,1,horizon,&mut candidates[1]);
        }
        let need=(s.segments.len() as f64*s.radius*s.radius*8.0/(self.spatial.dx*self.spatial.dy)).ceil().max(24.0) as usize;
        let (area,capped)=if enclosure_risk && strategic {
            let rate=s.radius*1.18/(w.motion_limits(id,state.rush).unwrap().0*STEP_SECONDS).max(0.1);
            self.spatial.trajectory_space(candidates[1].path[candidates[1].steps],self.mask(w,id),(need*2).max(64),candidates[1].steps as f64*STEP_SECONDS,
                &candidates[1].path[..=candidates[1].steps],(rate*10.0).ceil() as usize,(s.segments.len() as f64*self.rivals[id].release_rate/STEP_SECONDS+self.rivals[id].growth_delay/STEP_SECONDS).ceil() as usize)
        } else {self.spatial.space(candidates[1].path[candidates[1].steps],self.mask(w,id),(need*2).max(64),candidates[1].steps as f64*STEP_SECONDS)};
        candidates[1].area=area;candidates[1].uncertain=capped;
        // Revalidate the retained plan against CURRENT bodies and forecasts.
        // An open, checked continuation can wait for its scheduled strategy
        // slot; danger, a missing target or a pocket triggers full search.
        let head=s.segments[0].current;
        let wall_room=!w.config().deadly_walls || head.x.min(w.config().width-head.x).min(head.y).min(w.config().height-head.y)
            >self.rivals[id].speed/self.rivals[id].turn.max(0.01)*3.0+s.radius*3.0;
        let reuse_plan=!strategic && wall_room && candidates[1].steps==horizon
            && (area>=need || w.tick()<state.commit_until);
        if !reuse_plan || self.diagnostic_enabled {
            for kind in 0..CANDIDATES {
                if kind==1 {continue;}
                if (kind==9 || kind==10) && !extended {candidates[kind]=candidates[2];area_alias[kind]=Some(2);continue;}
                if kind==11 && (w.config().rules==crate::RuleSet::Classic || state.rush==0.0) {candidates[kind]=candidates[0];area_alias[kind]=Some(0);continue;}
                if kind==12 && (w.config().rules!=crate::RuleSet::V2 || !w.boost_ready(id) || candidates[1].steps>=18) {candidates[kind]=candidates[2];area_alias[kind]=Some(2);continue;}
                if kind==2 && candidates[1].checked && !candidates[1].attack.valid && !state.track_goal && state.turn_until==u64::MAX && normalize_angle(state.desired-s.angle).abs()<1e-12
                    && candidates[1].rush==Self::candidate_rush(w,s,state,kind,Attack::default()) {
                    candidates[kind]=candidates[1];area_alias[kind]=Some(1);
                } else {self.rollout_into(w,s,state,kind,horizon,&mut candidates[kind]);}
            }
        }
        if !reuse_plan && w.config().rules==crate::RuleSet::V2 && w.boost_ready(id) {
            let unboosted=candidates.iter().filter(|c|c.rush==0.0 && !c.attack.valid).map(|c|c.steps).max().unwrap_or(0);
            if unboosted<18 {
                let mut trial=Candidate::default();
                for kind in [0,2,3,4,5,6,7,8] {
                    let mut escape=state;escape.rush=0.6;escape.escape_boost=true;escape.attack=Attack::default();escape.attack_options=[Attack::default();2];
                    // kind 12 follows retained control; ordinary slots below
                    // explicitly request boost for this differential probe.
                    self.rollout_into(w,s,escape,kind,horizon,&mut trial);
                    if trial.rush>0.0 && trial.steps>candidates[12].steps {candidates[12]=trial;area_alias[12]=None;}
                }
                if candidates[12].steps!=horizon {candidates[12]=candidates[2];area_alias[12]=Some(2);}
            } else {candidates[12]=candidates[2];area_alias[12]=Some(2);}
        }
        // A reused decision selects only slot 1. Leave other scratch slots
        // untouched instead of copying its full path into ten unused slots.
        if strategic && !reuse_plan {
            for kind in [7,8] {
                if candidates[kind].attack.valid && candidates[kind].steps==horizon {
                    let blocked=self.replies_blocked(w,s,state,&candidates[kind]);
                    candidates[kind].score+=blocked as f64*100.0*s.traits.aggression-candidates[kind].attack.error*0.7;
                }
            }
        }
        let safe=candidates.iter().filter(|c|c.steps==horizon).count();
        let rollout_time=clock.map(|c|c.elapsed().as_nanos());
        if let Some(t)=rollout_time {self.profile[2]+=t-strategy_time.unwrap();}
        for index in 0..CANDIDATES {
            if reuse_plan && !self.diagnostic_enabled && index!=1 {continue;}
            let alias=area_alias[index].map(|source|(candidates[source].area,candidates[source].uncertain));
            let c=&mut candidates[index];
            if let Some((area,capped))=alias {c.area=area;c.uncertain=capped;}
            if c.area==0 {
                let (area,capped)=if extended && normalize_angle(c.angle-s.angle).abs()>0.7 {
                    let rate=s.radius*1.18/(w.motion_limits(id,state.rush).unwrap().0*STEP_SECONDS).max(0.1);
                    self.spatial.trajectory_space(c.path[c.steps],self.mask(w,id),(need*2).max(64),c.steps as f64*STEP_SECONDS,
                        &c.path[..=c.steps],(rate*10.0).ceil() as usize,(s.segments.len() as f64*self.rivals[id].release_rate/STEP_SECONDS+self.rivals[id].growth_delay/STEP_SECONDS).ceil() as usize)
                } else {self.spatial.space(c.path[c.steps],self.mask(w,id),(need*2).max(64),c.steps as f64*STEP_SECONDS)};
                c.area=area;c.uncertain=capped;
            }
            let (area,capped)=(c.area,c.uncertain);
            if !capped && area<need {c.score-=600.0+(need-area) as f64*8.0;}
            else {c.score+=area.min(need*2) as f64*0.08;}
        }
        if let Some(c)=clock {self.profile[3]+=c.elapsed().as_nanos()-rollout_time.unwrap();}
        candidates[1].score+=if state.prey!=0 {20.0} else {90.0}+if w.tick()<state.commit_until {if state.prey!=0 {65.0} else {160.0}} else {0.0};
        let viability=|c:&Candidate|if c.steps==horizon {if c.area>=need || c.uncertain {2} else {1}} else {0};
        // A physical straight rollout always exists, even if every first step
        // collides. Never rank a pre-step tactical rejection as an escape.
        let best=if reuse_plan {1} else {(0..CANDIDATES).filter(|i|candidates[*i].checked).max_by(|a,b|viability(&candidates[*a]).cmp(&viability(&candidates[*b]))
            .then_with(||candidates[*a].steps.cmp(&candidates[*b].steps))
            .then_with(||if safe==0 {candidates[*a].area.cmp(&candidates[*b].area)} else {std::cmp::Ordering::Equal})
            .then_with(||if safe==0 {candidates[*a].clearance.total_cmp(&candidates[*b].clearance)} else {std::cmp::Ordering::Equal})
            .then_with(||candidates[*a].score.total_cmp(&candidates[*b].score)).then_with(||b.cmp(a))).expect("straight rollout always checks a heading")};
        if self.diagnostic_enabled {
                let mut d=DecisionDiagnostic {generation:s.generation,selected:best,horizon,required_cells:need,length:s.segments.len(),goal:state.waypoint.unwrap_or(state.goal),reused_plan:reuse_plan,..Default::default()};
            for (i,c) in candidates.iter().enumerate() {
                let turn_ticks=if c.turn_until!=u64::MAX {c.turn_until.saturating_sub(w.tick()) as usize} else {0};
                d.candidates[i]=CandidateDiagnostic {checked:c.checked,desired:c.desired,safe_ticks:c.steps,area:c.area,area_capped:c.uncertain,
                    turn_ticks,track_goal:c.tracks_goal,exit_angle:c.exit_angle,capped:c.capped,score:c.score,rush:c.rush,coil_center:state.coil_center,coil_radius:state.coil_radius,coil_sign:state.coil_sign,
                    coil_pitch:state.coil_pitch,coil_progress:state.coil_progress,
                    attack_valid:c.attack.valid,
                    attack_turn_ticks:if c.attack.valid {c.attack.turn_at.saturating_sub(w.tick()) as usize} else {0},
                    attack_crossing:if c.attack.valid {c.attack.crossing} else {c.desired},
                    attack_crossing_rush:if c.attack.valid {c.attack.crossing_rush} else {c.rush}};
            }
            self.decisions[id]=d;
        }
        let c=&candidates[best];
        State::commit_candidate(&mut state,c,best,w,s,self.rivals[id].turn,need);
        let diff=normalize_angle(c.desired-s.angle);
        let sign=if diff>0.07 {1} else if diff< -0.07 {-1} else {0};
        if sign!=0 {state.last_turn_tick=w.tick();state.turn_sign=sign;}
        state.debug.path_count=16.min(c.steps+1) as u32;
        for j in 0..state.debug.path_count as usize {
            state.debug.path[j]=c.path[j*c.steps/(state.debug.path_count as usize-1).max(1)];
        }
        state.debug.safe_seconds=c.steps as f64*STEP_SECONDS;
        state.debug.reachable_cells=c.area as u32;
        if (c.area as f64) < 1.5 * need as f64 && !c.uncertain {state.debug.flags|=32;}
        if c.uncertain {state.debug.flags|=1;}
        if c.capped {state.debug.flags|=2;}
        if c.steps<horizon {state.debug.flags|=4;}
        if reuse_plan {state.debug.flags|=16;}
        self.states[id]=state;
        self.candidates=Some(candidates);
        if let Some(c)=clock {self.profile[4]+=c.elapsed().as_nanos();}
        Steering {desired_angle:state.desired,rush:Self::boost_request(w,s,state.rush)}
    }
}

#[cfg(test)]
mod tests;
