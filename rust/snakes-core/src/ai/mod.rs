// SPDX-License-Identifier: GPL-3.0-or-later
//! Deterministic receding-horizon controller. One controller belongs to one
//! world. All scratch storage is reserved by new(); steer never allocates.
mod spatial;
mod attack;
mod aggression;
mod pocket;
mod surge;
mod magnet;
mod phase;
mod target;
mod race;
mod events;
mod forecast;
mod venom;
pub(crate) mod frost;
use attack::Attack;
use crate::{Point, SnakeView, World, MAX_FOOD, MAX_SEGMENTS, MAX_SNAKES, STEP_SECONDS};
use crate::controller::{Controller, Steering};
use spatial::Spatial;
const STEPS: usize = 138;
const NORMAL_STEPS: usize = 72;
const VENOM_EXIT_STEPS:usize=12;
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
pub struct CompetitionDebug {pub prey:Option<usize>,pub coil_radius:f64,pub rush:f64,pub attack_side:i8,pub attack_stage:u8,pub track_goal:bool,pub venom_standoff:bool,pub venom_target:Option<usize>}
#[derive(Clone, Copy, Default)]
struct VenomPlan {goal:Point,target:usize,generation:u32,index:usize,standoff:bool}
#[derive(Clone, Copy, Default)]
struct VenomHunt {best:Option<VenomPlan>,nearest:Option<VenomPlan>}
#[derive(Clone, Copy, Default)]
struct State {
    generation: u32,
    #[cfg(feature="desktop-diag")]
    abandon: AbandonReason,
    target: u64,
    suspended_target: u64,
    suspended_index: usize,
    barrier:bool,
    area_hint:usize, area_hint_body:usize, area_hint_radius:f64,
    race_losing_ticks:u8,
    guarding:bool,guard_radius:f64,vulturing:bool,vulture_until:u64,vulture_center:Point,
    target_index: usize, // frame-local hint, checked against target identity
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
    venom_target:usize,venom_generation:u32,venom_index:usize,venom_standoff:bool,venom_alternative:Option<VenomPlan>,
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
    /// Race observations and guard hysteresis belong to one target identity.
    /// Recovery commitments/rejections belong to motion or their explicit ID.
    fn set_target(&mut self,target:u64) {
        if self.target!=target {
            #[cfg(feature="desktop-diag")]
            if self.target!=0 && self.abandon==AbandonReason::None {self.abandon=AbandonReason::Rescore;}
            if self.vulturing {self.turn_accum=0.0;self.waypoint=None;}
            self.waypoint=None;
            self.race_losing_ticks=0;self.guarding=false;self.vulturing=false;self.vulture_until=0;self.guard_radius=0.0;
        }
        self.target=target;
    }

    fn suspend_objective(&mut self) {
        if self.target!=0 {self.suspended_target=self.target;self.suspended_index=self.target_index;}
    }

    /// Install the controls carried by the rollout after bookkeeping that can
    /// clear old tactics. Never reconstruct a maneuver from its scratch slot.
    fn commit_candidate(state:&mut State,c:&Candidate,best:usize,w:&World,s:SnakeView<'_>,turn:f64,need:usize) {
        if !c.tracks_goal {state.clear_coil(s.angle);}
        if best!=1 && normalize_angle(c.desired-s.angle).abs()>0.5 {
            let duration=if c.kind==7 || c.kind==8 {16} else if c.kind==9 || c.kind==10 {64}
                else {(normalize_angle(c.desired-s.angle).abs()/turn/STEP_SECONDS).ceil().clamp(12.0,STEPS as f64) as u64};
            state.commit_until=w.tick()+duration;
        }
        let retreat=c.area<need && !c.uncertain && !c.attack.valid
            && (!c.tracks_goal || (aggression::bold(w)>0.0 && state.target!=0 && !state.guarding && !state.vulturing));
        if retreat
            && s.segments.len()>=24 && w.tick()>=state.escape_until && w.tick()>=state.orbit_until {
            #[cfg(feature="desktop-diag")] {state.abandon=AbandonReason::Escape;}
            state.escape_until=w.tick()+45;
            if w.config().rules==crate::RuleSet::V2 {
                state.suspend_objective();
            } else {state.rejected=state.target;state.reject_until=state.escape_until;}
            // Escape mode invalidates attack/pocket stages and changes goals.
            // Enter it only for a validated nontracking fallback; tracking and
            // attack candidates retain every dependency of their rollout.
            state.prey=0;state.clear_coil(s.angle);state.clear_attacks(s.angle);
            state.set_target(0);state.waypoint=None;
        }
        if let Some(plan)=c.venom_goal {
            if state.venom_target!=plan.target || state.venom_generation!=plan.generation {state.turn_accum=0.0;state.commit_until=0;}
            state.waypoint=None;state.goal=plan.goal;state.venom_index=plan.index;state.venom_target=plan.target;state.venom_generation=plan.generation;
            state.prey=plan.target;state.prey_generation=plan.generation;state.venom_standoff=plan.standoff;
        }
        state.attack=c.attack;
        state.track_goal=c.tracks_goal && !(retreat && w.tick()<state.escape_until);
        state.turn_until=c.turn_until;
        state.exit_angle=c.exit_angle;
        state.rush=c.rush;
        state.desired=c.desired;
        if state.target==0 {state.guarding=false;}
    }
    fn clear_coil(&mut self,angle:f64) {
        if self.coil_radius!=0.0 || self.coil_initial_radius>0.0 {
            self.desired=angle;self.exit_angle=angle;self.turn_until=u64::MAX;
            self.track_goal=false;self.commit_until=0;self.rush=0.0;
            self.set_target(0);self.waypoint=None;
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
    release_slowdown: f64,
    angle: f64,
    observed_turn: f64,
    forecast_angle: f64,
    direction: Point,
    dynamic: bool,
    path: [Point; STEPS+1],
    envelope: [f64; STEPS+1],
    distance: [f64; STEPS+1],
    escape: [Point;3],
}
#[derive(Clone, Copy)]
struct Candidate {
    body_len: usize,
    effects: forecast::Snapshot,
    kind: usize,
    checked: bool,
    tracks_goal: bool,
    desired: f64,
    turn_until: u64,
    exit_angle: f64,
    angle: f64,
    turn_exit: f64,
    path: [Point; STEPS+1],
    steps: usize,
    simulated_steps:usize,
    venom_bite:usize,
    score: f64,
    venom_goal:Option<VenomPlan>,
    clearance: f64,
    capped: bool,
    area: usize,
    uncertain: bool,
    attack: Attack,
    replies: usize,
    rush: f64,
}

impl Default for Rival {
    fn default()->Self {Self {alive:false,radius:0.0,len:0,speed:0.0,turn:0.0,growth_delay:0.0,release_rate:0.0,release_slowdown:1.0,angle:0.0,observed_turn:0.0,forecast_angle:0.0,direction:Point::default(),dynamic:false,path:[Point::default();STEPS+1],envelope:[0.0;STEPS+1],distance:[0.0;STEPS+1],escape:[Point::default();3]}}
}
impl Default for Candidate {
    fn default()->Self {Self {body_len:0,effects:forecast::Snapshot::default(),kind:2,checked:false,tracks_goal:false,desired:0.0,turn_until:u64::MAX,exit_angle:0.0,angle:0.0,turn_exit:0.0,path:[Point::default();STEPS+1],steps:0,simulated_steps:0,venom_bite:0,score:0.0,venom_goal:None,clearance:0.0,capped:false,area:0,uncertain:false,attack:Attack::default(),replies:0,rush:0.0}}
}

#[derive(Clone, Copy)]
struct NightMotion {first_tick:u64,start:u64,daylight:[(f64,f64,f64);3]}
impl NightMotion {
    fn at(&self,offset:usize,stage:usize)->(f64,f64) {
        let tick=self.first_tick.saturating_add(offset as u64);
        let night=crate::world::events::night_intensity(tick,self.start);
        let (speed,turn,per_speed)=self.daylight[stage];
        let speed=speed*(1.0-0.1*night as f64);
        (speed,turn.min(speed*per_speed))
    }
}

/// Cached single-burst schedules; all steady-state storage belongs to the controller.
#[derive(Clone, Copy)]
struct Motion { night:Option<NightMotion>,radii:[f64;25],limits: [(f64,f64);25], max_speed:f64, max_curve:f64, frozen_speed:f64, final_len:usize, expiry:usize, expired_limits:(f64,f64),next_expiry:usize,next_limits:(f64,f64) }
impl Default for Motion {fn default()->Self {Self {night:None,radii:[0.0;25],limits:[(0.0,0.0);25],max_speed:0.0,max_curve:0.0,frozen_speed:0.0,final_len:0,expiry:usize::MAX,expired_limits:(0.0,0.0),next_expiry:usize::MAX,next_limits:(0.0,0.0)}}}
impl Motion {
    #[inline]
    fn at(&self,offset:usize)->(f64,f64) {
        if offset<=24 {return self.limits[offset];}
        let stage=if offset>=self.next_expiry {2} else if offset>=self.expiry {1} else {0};
        if let Some(night)=&self.night {return night.at(offset,stage);}
        match stage {2=>self.next_limits,1=>self.expired_limits,_=>self.limits[24]}
    }
    fn forecast(w:&World,id:usize,rush:f64)->Self {
        if w.config().rules==crate::RuleSet::Classic {
            let first=w.motion_limits(id,rush).unwrap();
            return Self {radii:[w.snake(id).unwrap().radius;25],limits:[first;25],max_speed:first.0,max_curve:first.0*first.1,final_len:w.snakes[id].len,..Self::default()};
        }
        let s=w.snake(id).unwrap();
        let first=crate::effects::forecast_motion(w,id,rush,0);
        let (boost_end,cost,_,_)=crate::effects::forecast_boost(w,id,rush);
        let effect=forecast::Effect::observed(w,s);
        let effect_expiry=if effect.ticks>0 {effect.ticks as usize} else {usize::MAX};
        let thaw=if s.face.frozen_ticks>0 {s.face.frozen_ticks as usize} else {usize::MAX};
        let expiry=effect_expiry.min(thaw);
        let next_expiry=if effect_expiry==thaw {usize::MAX} else {effect_expiry.max(thaw)};
        let next_limits=if next_expiry==usize::MAX {first} else {crate::effects::forecast_motion(w,id,rush,next_expiry.max(24))};
        let expired_limits=if expiry==usize::MAX {first}
            else {crate::effects::forecast_motion(w,id,rush,expiry.max(24))};
        // No burst or Surge: this bounds every predicted Nova/replacement
        // stage, even when a freeze cancels an observed paid burst.
        let frozen_speed=w.ai_forecast_frost_motion(id,0.0,0,0,0,None,Some(1),1).0;
        let night=w.forecast_night_start().filter(|_|w.forecast_night(0)!=w.forecast_night(STEPS))
            .map(|start|NightMotion {first_tick:w.tick+1,start,daylight:[
                crate::effects::forecast_daylight_motion(w,id,rush,24),
                crate::effects::forecast_daylight_motion(w,id,rush,if expiry==usize::MAX {24} else {expiry.max(24)}),
                crate::effects::forecast_daylight_motion(w,id,rush,if next_expiry==usize::MAX {24} else {next_expiry.max(24)})]});
        let reserve_daylight=night.is_some() || w.world_event.night>0.0;
        let mut m=Self {night,radii:[w.ai_forecast_radius(id,rush,0,None,None);25],limits:[first;25],max_speed:first.0,max_curve:first.0*first.1,frozen_speed,final_len:w.ai_forecast_len(id,rush,24,None,None),expiry,expired_limits,next_expiry,next_limits};
        let mut daylight_speed=0.0_f64;
        let mut due=if boost_end>0 {cost*(25-boost_end).min(21)/21} else {0};
        for j in 1..=24 {
            let next_due=if boost_end>0 {cost*(25-boost_end+j).min(21)/21} else {0};
            m.limits[j]=if next_due!=due || j==boost_end || j==expiry || j==next_expiry || w.forecast_night(j)!=w.forecast_night(j-1) {
                crate::effects::forecast_motion(w,id,rush,j)
            } else {m.limits[j-1]};
            m.radii[j]=if next_due!=due {w.ai_forecast_radius(id,rush,j,None,None)} else {m.radii[j-1]};
            due=next_due;
            m.max_speed=m.max_speed.max(m.limits[j].0);
            m.max_curve=m.max_curve.max(m.limits[j].0*m.limits[j].1);
            if reserve_daylight {
                let (speed,turn,_)=crate::effects::forecast_daylight_motion(w,id,rush,j);
                daylight_speed=daylight_speed.max(speed);m.max_curve=m.max_curve.max(speed*turn);
            }
        }
        if next_expiry!=usize::MAX {m.max_speed=m.max_speed.max(next_limits.0);m.max_curve=m.max_curve.max(next_limits.0*next_limits.1);}
        if expiry!=usize::MAX {
            m.max_speed=m.max_speed.max(expired_limits.0);
            m.max_curve=m.max_curve.max(expired_limits.0*expired_limits.1);
        }
        // Bound speed and turn together, including a change of turn limiter
        // at dawn. Dividing observed curvature by one night factor is unsafe.
        if reserve_daylight {
            for offset in [0,24,if expiry==usize::MAX {24} else {expiry.max(24)},if next_expiry==usize::MAX {24} else {next_expiry.max(24)}] {
                let (speed,turn,_)=crate::effects::forecast_daylight_motion(w,id,rush,offset);
                daylight_speed=daylight_speed.max(speed);m.max_curve=m.max_curve.max(speed*turn);
            }
            // Retain the existing conservative speed margin: besides query
            // bounds, it determines neck/planning horizons. Correcting the
            // curvature bound does not require reducing that valid padding.
            let factor=1.0-0.1*w.world_event.night as f64;
            m.max_speed=(m.max_speed/factor).max(daylight_speed);
        }
        m
    }
}

/// Every input read by the single-burst forecast and its speed/turn/radius
/// helpers. Pose, trail and intent do not affect these limits. Exact keys
/// retain exact expiry counters. Cooldown only affects whether a new boosted
/// schedule is available; unavailable requests reuse the plain schedule.
#[derive(Clone, Copy, PartialEq)]
struct MotionKey {
    generation:u32,len:usize,birth_len:usize,radius:f64,base_radius:f64,growth:f64,speed_bias:f64,
    effect_kind:u8,effect_ticks:u16,frozen_ticks:u16,boost_ticks:u8,boost_cost:u8,boost_paid:u8,boost_ready:bool,
    width:f64,height:f64,speed:f64,intelligence:f64,rules:crate::RuleSet,growth_available:bool,snake_length_limit:bool,night:f32,night_horizon:f32,night_tick:u64,rush:f64,
}
impl MotionKey {
    fn observed(w:&World,id:usize)->Self {
        let s=&w.snakes[id];let c=w.config();
        let (boost_cost,boost_paid,growth_available)=w.motion_cache_counters(id);
        Self {generation:s.generation,len:s.len,birth_len:s.birth_len,radius:s.radius,base_radius:s.base_radius,
            growth:s.growth,speed_bias:s.traits.speed_bias,effect_kind:s.effect_kind,effect_ticks:s.effect_ticks,frozen_ticks:s.frozen_ticks,
            boost_ticks:s.boost_ticks,boost_cost,boost_paid,boost_ready:w.boost_ready(id),
            width:c.width,height:c.height,speed:c.speed,intelligence:c.intelligence,rules:c.rules,
            growth_available,snake_length_limit:c.rules==crate::RuleSet::V2 && c.snake_length_limit,night:w.world_event.night,night_horizon:w.forecast_night(24),night_tick:if w.forecast_night_start().is_some() && w.forecast_night(0)!=w.forecast_night(STEPS) {w.tick+1} else {0},rush:if c.rules==crate::RuleSet::Classic {s.rush} else {0.0}}
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
    phased: bool,
    phase_bits: u16,
    surge_bits: u16,
    release_key:[u32;crate::MAX_ITEMS],
    result: (bool,f64,bool),
}

#[derive(Clone, Copy, Default)]
struct TrailBounds {min:Point,max:Point}
impl TrailBounds {
    fn point(p:Point)->Self {Self {min:p,max:p}}
    fn include(self,p:Point)->Self {
        Self {min:Point {x:self.min.x.min(p.x),y:self.min.y.min(p.y)},
            max:Point {x:self.max.x.max(p.x),y:self.max.y.max(p.y)}}
    }
    fn separated(self,a:Point,b:Point,reach:f64)->bool {
        a.x.min(b.x)>self.max.x+reach || a.x.max(b.x)<self.min.x-reach
            || a.y.min(b.y)>self.max.y+reach || a.y.max(b.y)<self.min.y-reach
    }
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
    /// Physical sampled replies blocked by this attacker, excluding prey self/wall deaths.
    pub attack_replies: usize,
    pub attack_error: f64,
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
/// Why an objective was relinquished; diagnostic-only, no policy switches.
#[cfg(feature="desktop-diag")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum AbandonReason {#[default] None,Recovery,Stall,Escape,Rescore,Blocked,Death,LostRace,Expiry}
#[cfg(feature="desktop-diag")]
#[derive(Clone, Copy, Debug, Default)]
pub struct DesktopObservation {
    pub contested:bool, pub fleeing:bool, pub cutoff:bool, pub cutoff_start:u64, pub boosted:bool, pub power_up:u8,
    pub safe_ticks:usize, pub horizon:usize, pub continuation:bool, pub continuation_unresolved:bool,
    pub any_first_step:bool, pub next_head:Point, pub rival_next:[Point;MAX_SNAKES],
    pub rival_generation:[u32;MAX_SNAKES],
    pub generation:u32, pub mode:u8, pub target:u64, pub goal:Point,
    pub reused_plan:bool, pub retained_drift:f64, pub reach:f64, pub abandon:AbandonReason,
    pub escape_started:bool, pub target_kind:u8,
    pub distance:f64, pub selected:u8, pub direct_safe:bool, pub direct_viable:bool,
    pub retained_viable:bool, pub direct_score:f64, pub selected_score:f64,
    pub recovery_started:bool, pub progress_rejected:bool, pub strategic:bool,
    pub turn_accum:f64, pub tracking:bool, pub target_bearing:f64,
    pub recovery_target:u64, pub recovery_progress_age:u64,
    pub prey:u32, pub prey_generation:u32, pub venom_target:u32,
}
#[derive(Clone)]
pub struct AiController {
    #[cfg(test)]
    reference_queries:bool,
    #[cfg(test)]
    venom_searches:std::cell::Cell<usize>,
    spatial: Spatial,
    motion_keys:[Option<MotionKey>;MAX_SNAKES],
    body_cache: [BodyCache;256],
    candidates: Option<Box<[Candidate;CANDIDATES]>>,
    rollout_distance: [f64;STEPS+1],
    rollout_projection: [f64;STEPS+1],
    rival_necks:[[u16;STEPS+1];MAX_SNAKES],
    rival_bounds:[[TrailBounds;STEPS+1];MAX_SNAKES],
    rival_bounds_horizons:[usize;MAX_SNAKES],
    neck_owner:usize,
    neck_horizons:[u16;MAX_SNAKES],
    cache_epoch: u64,
    diagnostic_enabled: bool,
    profile_enabled: bool,
    collision_effects:bool,
    profile: [u128;5],
    forecast_profile: [u128;5],
    strategy_profile: [u128;4],
    race_profile:[u128;2],
    decisions: [DecisionDiagnostic; MAX_SNAKES],
    #[cfg(feature="desktop-diag")]
    desktop: [DesktopObservation;MAX_SNAKES],
    states: [State; MAX_SNAKES],
    policy_aggression:u8,
    rivals: [Rival; MAX_SNAKES],
    rival_limits:[[(f64,f64);STEPS+1];MAX_SNAKES],
    opportunity_rivals:Box<[Rival;MAX_SNAKES]>,
    opportunity_mask:u16,
    simulation_rivals: Option<Box<[Rival;MAX_SNAKES]>>,
    item_forecast: forecast::Items,
    initial_effects: forecast::Timeline,
    effects: forecast::Timeline,
    opportunities: forecast::Timeline,
    look_deltas:[Point;MAX_SNAKES],
    capsule_etas:[[f64;MAX_SNAKES];crate::MAX_ITEMS+1],
    prism:Option<(usize,target::TargetFood)>,
    food: [Option<target::TargetFood>; MAX_FOOD + crate::MAX_ITEMS+1],
    tick: u64,
    geometry: u64,
    seed: i32,
    rng: u64,
    urgent_used: usize,
    max_radius: f64,
    max_forecast_speed: f64,
    phase_sweeps: [u64;3],
    planning_speed: [f64;MAX_SNAKES],
    motion: [Motion;MAX_SNAKES],
    boosted_motion: [Motion;MAX_SNAKES],
    observed_angles: [f64;MAX_SNAKES],
    observed_generations: [u32;MAX_SNAKES],
}
impl Default for AiController {fn default() -> Self {Self::new()}}
impl AiController {
    pub fn new() -> Self {
        Self {
            race_profile:[0;2],
            rival_bounds:[[TrailBounds::default();STEPS+1];MAX_SNAKES],rival_bounds_horizons:[0;MAX_SNAKES],
            #[cfg(feature="desktop-diag")] desktop:[DesktopObservation::default();MAX_SNAKES],
            #[cfg(test)] reference_queries:false,spatial:Spatial::new(),motion_keys:[None;MAX_SNAKES],candidates:Some(Box::new([Candidate::default();CANDIDATES])),rollout_distance:[0.0;STEPS+1],rollout_projection:[0.0;STEPS+1],rival_necks:[[0;STEPS+1];MAX_SNAKES],neck_owner:MAX_SNAKES,neck_horizons:[0;MAX_SNAKES],body_cache:[BodyCache::default();256],cache_epoch:0, diagnostic_enabled:false,profile_enabled:false,collision_effects:false,profile:[0;5],forecast_profile:[0;5],strategy_profile:[0;4], decisions:[DecisionDiagnostic::default();MAX_SNAKES], states:[State::default(); MAX_SNAKES],policy_aggression:100,
            rivals:[Rival::default();MAX_SNAKES],
            #[cfg(test)]
            venom_searches:std::cell::Cell::new(0),opportunity_rivals:Box::new([Rival::default();MAX_SNAKES]),opportunity_mask:0,rival_limits:[[(0.0,0.0);STEPS+1];MAX_SNAKES], simulation_rivals:Some(Box::new([Rival::default();MAX_SNAKES])),item_forecast:forecast::Items::default(),initial_effects:forecast::Timeline::default(),effects:forecast::Timeline::default(),opportunities:forecast::Timeline::default(),look_deltas:[Point::default();MAX_SNAKES],capsule_etas:[[f64::INFINITY;MAX_SNAKES];crate::MAX_ITEMS+1],prism:None,food:[None;MAX_FOOD + crate::MAX_ITEMS+1],
            tick:u64::MAX,geometry:0,seed:0,rng:0x9e3779b97f4a7c15,urgent_used:0,max_radius:0.0,max_forecast_speed:0.0,phase_sweeps:[0;3],planning_speed:[0.0;MAX_SNAKES],motion:[Motion::default();MAX_SNAKES],boosted_motion:[Motion::default();MAX_SNAKES],observed_angles:[0.0;MAX_SNAKES],observed_generations:[0;MAX_SNAKES]}
    }
    #[cfg(feature="desktop-diag")]
    pub fn desktop_observation(&self,id:usize)->DesktopObservation {self.desktop[id]}
    pub fn enable_profile(&mut self) {self.profile_enabled=true;self.spatial.profile_enabled=true;}
    pub fn profile(&self)->[u128;5] {self.profile}
    /// Grid insertion, dilation (including bitmaps), room, finish, future marks, tail route, food route, lazy mask build, bitmap preparation.
    pub fn spatial_profile(&self)->([u128;9],[u64;11]) {(self.spatial.profile,self.spatial.counts)}
    /// Bounds, shared paths, contests, candidate setup, candidate participant updates.
    pub fn forecast_profile(&self)->[u128;5] {self.forecast_profile}
    /// Hunt selection, cutoff construction, pocket/barrier admission, proposed attack rollouts.
    pub fn strategy_profile(&self)->[u128;4] {self.strategy_profile}
    /// Shared capsule ETA preparation, retained capsule/prism race updates.
    pub fn race_profile(&self)->[u128;2] {self.race_profile}
    pub fn enable_diagnostics(&mut self) { self.diagnostic_enabled = true; }
    pub fn disable_observers(&mut self) {self.diagnostic_enabled=false;self.profile_enabled=false;self.spatial.profile_enabled=false;}
    pub fn decision(&self, id:usize) -> DecisionDiagnostic { self.decisions[id] }
    pub fn debug(&self,id:usize) -> Option<DebugInfo> {self.states.get(id).map(|s|s.debug)}
    /// Actual retained objective, as distinct from the diagnostic shortlist.
    pub fn vulturing(&self,id:usize)->bool {self.states.get(id).is_some_and(|s|s.vulturing)}
    pub fn selected_target(&self,id:usize) -> Option<u64> {self.states.get(id).map(|s|s.target)}
    pub fn competition_debug(&self,id:usize)->Option<CompetitionDebug> {
        self.states.get(id).map(|s|CompetitionDebug {prey:s.prey.checked_sub(1),coil_radius:s.coil_radius,rush:s.rush,attack_side:if s.attack.valid {s.attack.side} else {0},attack_stage:if !s.attack.valid {0} else if self.tick<s.attack.turn_at {1} else {2},track_goal:s.track_goal,venom_standoff:s.venom_standoff,venom_target:s.venom_target.checked_sub(1)})
    }
    fn random(&mut self) -> f64 {
        self.rng ^= self.rng << 13; self.rng ^= self.rng >> 7; self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64/(1u64<<53) as f64
    }
    fn prepare(&mut self,w:&World) {
        let aggression_changed=w.config().rules==crate::RuleSet::V2 && self.policy_aggression!=w.config().aggression;
        if self.tick==w.tick() && self.geometry==w.geometry_generation() && !aggression_changed {return;}
        if aggression_changed {self.states.fill(State::default());self.policy_aggression=w.config().aggression;}
        if self.geometry!=w.geometry_generation() || w.tick()<self.tick || self.seed!=w.config().seed {
            self.states.fill(State::default());
            self.observed_generations.fill(0);
            self.seed=w.config().seed;
            self.rng=0x9e3779b97f4a7c15 ^ (self.seed as u32 as u64).wrapping_mul(0xd1342543de82ef95);
        }
        self.cache_epoch=self.cache_epoch.wrapping_add(1);
        if self.cache_epoch==0 {self.body_cache.fill(BodyCache::default());self.cache_epoch=1;}
        self.tick=w.tick(); self.geometry=w.geometry_generation(); self.urgent_used=0;
        self.food.fill(None);self.prism=None;
        for (i,f) in w.foods().enumerate() {
            if f.kind==crate::FoodKind::Meteor || w.food[i].captured_by!=0 {continue;}
            let mut target:target::TargetFood=f.into();
            // Like capsule landing, the hot readiness clock is an absolute
            // endpoint delta. This also handles steering from completed frames.
            if f.kind==crate::FoodKind::PrismSeed {
                target.motion_ticks=w.food[i].ripe_tick.saturating_sub(w.tick()).min(u16::MAX as u64) as u16;
            }
            if w.config().rules==crate::RuleSet::V2 && matches!(f.kind,crate::FoodKind::Prism|crate::FoodKind::PrismSeed) {target.value=90.0/(1.0+target.motion_ticks as f64*STEP_SECONDS*0.35);self.prism=Some((i,target));}
            self.food[i]=Some(target);
        }
        // Fixed extra target slots. High-bit IDs distinguish items from food;
        // they reuse target persistence, routing, race and contest handling.
        for (i,item) in w.items().enumerate() {
            self.food[MAX_FOOD+i]=Some(target::TargetFood {id:item.id | target::ITEM_BIT,position:item.position,
                value:item.kind.base_value(),size:item.radius,vacuum_owner:-1,feast_id:0,kind:crate::FoodKind::Spark,
                motion_ticks:item.pickable_from_tick.saturating_sub(w.tick()).min(u16::MAX as u64) as u16});
        }
        self.prepare_event_target(w);
        self.initial_effects=forecast::Timeline::new(w);
        self.neck_owner=MAX_SNAKES;self.opportunity_mask=0;
        self.rival_bounds_horizons.fill(0);
        for r in &mut self.rivals {r.alive=false;r.dynamic=false;}
        self.max_radius=0.0;self.max_forecast_speed=0.0;self.phase_sweeps.fill(0);
        // A fading-in night can slow tail clearance after the 24-tick burst
        // cache. Reserve its darkest known horizon for ordinary and Nova tails.
        let night_reserve=(1.0-0.1*w.world_event.night.max(w.forecast_night(STEPS-1)) as f64)
            /(1.0-0.1*w.world_event.night as f64);
        for s in w.snakes().filter(|s|s.alive) {
            let id=s.id as usize;
            let key=MotionKey::observed(w,id);
            let refresh=self.motion_keys[id]!=Some(key);
            #[cfg(test)] let refresh=refresh || self.reference_queries;
            if refresh {
                self.motion[id]=Motion::forecast(w,id,key.rush);
                self.boosted_motion[id]=if w.boost_ready(id) {Motion::forecast(w,id,0.6)} else {self.motion[id]};
                self.motion_keys[id]=Some(key);
            }
            let (speed,turn)=self.motion[id].at(0);
            self.max_forecast_speed=self.max_forecast_speed.max(self.motion[id].max_speed);
            self.max_radius=self.max_radius.max(s.radius);self.planning_speed[s.id as usize]=speed;
            let r=&mut self.rivals[s.id as usize];
            let release_speed=if w.config().rules==crate::RuleSet::Classic {w.motion_limits(id,0.0).unwrap().0}
                else {self.motion[id].limits.iter().map(|m|m.0).fold(if self.motion[id].expiry==usize::MAX {speed} else {speed.min(self.motion[id].expired_limits.0)},f64::min)*night_reserve};
            let growth_scale=if w.config().rules==crate::RuleSet::Classic {1.0} else {speed/release_speed};
            // All live horizon entries and escapes are overwritten below.
            // Preserve their storage instead of clearing two full paths per tick.
            r.alive=true;r.radius=s.radius;r.len=s.segments.len();r.speed=speed;r.turn=turn;
            r.growth_delay=w.tail_growth_delay(id).unwrap()*growth_scale;
            r.release_rate=s.radius*1.18/(release_speed*0.65).max(1.0);
            r.release_slowdown=if w.config().rules==crate::RuleSet::V2 {(self.motion[id].frozen_speed*night_reserve*0.65).max(1.0)/(release_speed*0.65).max(1.0)} else {1.0};
            r.angle=s.angle;r.forecast_angle=s.angle;r.direction=Point{x:s.angle.cos(),y:s.angle.sin()};
            r.distance[0]=0.0;r.envelope[0]=0.0;
            r.path[0]=s.segments[0].current;
            let id=s.id as usize;
            let observed_turn=if self.observed_generations[id]==s.generation {
                normalize_angle(s.angle-self.observed_angles[id]).clamp(-turn*STEP_SECONDS,turn*STEP_SECONDS)
            } else {0.0};
            self.observed_generations[id]=s.generation;self.observed_angles[id]=s.angle;
            r.observed_turn=observed_turn;
        }
        self.spatial.rebuild_with_rivals(w,&self.rivals);
        let race_clock=self.profile_enabled.then(std::time::Instant::now);
        self.prepare_races(w);
        if let Some(c)=race_clock {self.race_profile[0]+=c.elapsed().as_nanos();}
        let forecast_clock=self.profile_enabled.then(std::time::Instant::now);
        self.opportunities=forecast::Timeline::opportunities(w,self.initial_effects,&self.motion,&self.boosted_motion);
        self.item_forecast=forecast::Items::prepare(w,&self.motion,&self.boosted_motion);
        let mut forecast=forecast::Forecast::cached(w,self.initial_effects,&self.item_forecast,STEPS,0);
        let bounds_time=forecast_clock.map(|t|t.elapsed().as_nanos());
        if let Some(t)=bounds_time {self.forecast_profile[0]+=t;}
        self.prepare_static_rivals(w);
        if forecast.has_items() {
            // Independent observed tracks stay hot while they are integrated.
            // Capsule ownership only rebuilds rows whose movement changes.
            let mut scratch=self.simulation_rivals.take().unwrap();
            for rival in scratch.iter_mut() {rival.dynamic=false;}
            self.phase_sweeps.fill(0);
            for j in 1..=STEPS {
                Self::advance_rivals(w,&mut scratch,&self.motion,&forecast,j,0,Some((&self.initial_effects,&self.rivals)),Some(&mut self.rival_limits));
                if forecast.has_items() {
                    let radii=std::array::from_fn::<_,MAX_SNAKES,_>(|id|forecast.radius(w,id,0.0,&self.motion[id],j));
                    forecast.set_radii(|id|radii[id]);
                }
                forecast.advance(w,j,|id|Self::forecast_rival(&scratch,&self.rivals,id).path[j],0.0);
                // Reach envelopes change on the pickup movement itself, even
                // though its new speed applies only on the next movement.
                let mut changed=forecast.effects.movement_bits();
                while changed!=0 {
                    let id=changed.trailing_zeros() as usize;changed&=changed-1;
                    if scratch[id].dynamic {continue;}
                    scratch[id]=self.rivals[id];scratch[id].dynamic=true;
                    let direction=w.displacement(scratch[id].path[j-1],scratch[id].path[j]);
                    scratch[id].forecast_angle=direction.y.atan2(direction.x);
                    let (sin,cos)=scratch[id].forecast_angle.sin_cos();
                    scratch[id].direction=Point{x:cos,y:sin};
                }
                Self::update_envelopes(w,&mut scratch,&self.motion,&forecast,j,0,Some((&self.initial_effects,&self.rivals)));
                if forecast.effects.phase_bits(j)!=forecast.effects.phase_bits(j.saturating_sub(1)) {
                    self.phase_sweeps[j/64]|=1u64<<(j%64);
                }
            }
            // Speculative speed changes belong to physical scenarios, not
            // opportunity targets. Save only affected observed rows.
            self.opportunity_mask=forecast.effects.movement_bits()|self.opportunities.movement_bits();
            let mut save=self.opportunity_mask;
            while save!=0 {
                let id=save.trailing_zeros() as usize;save&=save-1;
                self.opportunity_rivals[id]=self.rivals[id];
            }
            for (id,rival) in scratch.iter().enumerate().filter(|(_,rival)|rival.dynamic) {
                self.rivals[id]=*rival;self.rivals[id].dynamic=false;
            }
            let certain=self.opportunities.movement_bits();
            if certain!=0 {
                let known=forecast::Forecast::empty(self.opportunities);
                for j in 1..=STEPS {
                    Self::advance_rivals(w,&mut self.opportunity_rivals,&self.motion,&known,j,!certain,None,None);
                    Self::update_envelopes(w,&mut self.opportunity_rivals,&self.motion,&known,j,!certain,None);
                }
            }
            self.simulation_rivals=Some(scratch);
        }
        self.effects=forecast.effects;
        self.collision_effects=self.effects.collision_effects();
        #[cfg(test)] {self.collision_effects|=self.reference_queries;}
        let paths_time=forecast_clock.map(|t|t.elapsed().as_nanos());
        if let Some(t)=paths_time {self.forecast_profile[1]+=t-bounds_time.unwrap();}
        self.item_forecast.contests(w,&self.rivals,&self.motion,&self.effects);
        self.item_forecast.shared(&forecast);
        if let Some(t)=forecast_clock {self.forecast_profile[2]+=t.elapsed().as_nanos()-paths_time.unwrap();}
        for r in self.rivals.iter_mut().filter(|r|r.alive) {
            let travel=r.distance[61]-r.distance[36]+0.5*(r.distance[62]-r.distance[61]);
            let heading=w.displacement(r.path[32],r.path[36]);
            let theta=heading.y.atan2(heading.x);
            for (i,offset) in [-0.9,0.0,0.9].into_iter().enumerate() {
                let angle=theta+offset;
                r.escape[i]=w.canonical_point(Point{x:r.path[36].x+angle.cos()*travel,
                    y:r.path[36].y+angle.sin()*travel});
            }
        }
        let mut saved=self.opportunity_mask;
        while saved!=0 {
            let id=saved.trailing_zeros() as usize;saved&=saved-1;
            let r=&mut self.opportunity_rivals[id];
            let travel=r.distance[61]-r.distance[36]+0.5*(r.distance[62]-r.distance[61]);
            let heading=w.displacement(r.path[32],r.path[36]);let theta=heading.y.atan2(heading.x);
            for (i,offset) in [-0.9,0.0,0.9].into_iter().enumerate() {
                let angle=theta+offset;
                r.escape[i]=w.canonical_point(Point{x:r.path[36].x+angle.cos()*travel,
                    y:r.path[36].y+angle.sin()*travel});
            }
        }
    }
    fn opportunity_rival(&self,id:usize)->&Rival {
        if self.opportunity_mask&(1<<id)!=0 {&self.opportunity_rivals[id]} else {&self.rivals[id]}
    }
    /// Independent tracks have only observed motion events.
    /// Keep each snake's path hot instead of revisiting all tracks twice per step.
    fn prepare_static_rivals(&mut self,w:&World) {
        for (id,r) in self.rivals.iter_mut().enumerate().filter(|(_,r)|r.alive) {
            let motion=&self.motion[id];
            let mut angle=r.angle;
            let mut direction=r.direction;
            for j in 1..=STEPS {
                let (speed,turn)=motion.at(j-1);self.rival_limits[id][j]=(speed,turn);
                let t=j as f64*STEP_SECONDS;
                let reserve=self.initial_effects.reach_scale(id,j);
                let cap=(r.radius*2.0+12.0)*reserve;
                r.envelope[j]=if r.envelope[j-1]>=cap {cap}
                    else {(speed*t*(turn*t).min(1.2).sin()*0.3*reserve).min(cap)};
                let desired=angle+r.observed_turn*(1.0-(j-1) as f64/9.0).max(0.0);
                let delta=normalize_angle(desired-angle);
                // Observed curvature is bounded by movement one's turn limit;
                // later observed deltas decay, but expiry can lower the limit.
                if delta.abs()>turn*STEP_SECONDS {
                    angle=normalize_angle(angle+turn*STEP_SECONDS*delta.signum());
                    let (sin,cos)=angle.sin_cos();direction=Point{x:cos,y:sin};
                } else if delta.abs()>1e-12 {
                    angle=desired;
                    let (sin,cos)=angle.sin_cos();direction=Point{x:cos,y:sin};
                }
                r.distance[j]=r.distance[j-1]+speed*STEP_SECONDS;
                r.path[j]=w.canonical_point(Point{x:r.path[j-1].x+direction.x*speed*STEP_SECONDS,y:r.path[j-1].y+direction.y*speed*STEP_SECONDS});
            }
        }
        for j in 1..=STEPS {
            if self.initial_effects.phase_bits(j)!=self.initial_effects.phase_bits(j.saturating_sub(1)) {self.phase_sweeps[j/64]|=1u64<<(j%64);}
        }
    }
    /// Same integration for observed paths, candidate rivals and reply rivals.
    /// Only the candidate/reply participants supply their own endpoints.
    fn advance_rivals(w:&World,rivals:&mut [Rival;MAX_SNAKES],motions:&[Motion;MAX_SNAKES],forecast:&forecast::Forecast,j:usize,excluded:u16,baseline:Option<(&forecast::Timeline,&[Rival;MAX_SNAKES])>,mut limits:Option<&mut [[(f64,f64);STEPS+1];MAX_SNAKES]>) {
        if let Some((effects,_))=baseline {
            if (effects.movement_bits()|forecast.effects.movement_bits()) & !excluded==0 {return;}
        }
        for (id,r) in rivals.iter_mut().enumerate() {
            if excluded&(1<<id)!=0 {continue;}
            if let Some((effects,observed))=baseline {
                if !observed[id].alive {continue;}
                if !r.dynamic {
                    if ((forecast.effects.movement_bits()|effects.movement_bits())&(1<<id)==0) || forecast.effects.same_movement(effects,id,j) {continue;}
                    *r=observed[id];
                    if j>1 {
                        let direction=w.displacement(r.path[j-2],r.path[j-1]);
                        r.forecast_angle=direction.y.atan2(direction.x);
                        let (sin,cos)=r.forecast_angle.sin_cos();r.direction=Point{x:cos,y:sin};
                    }
                    r.dynamic=true;
                }
            } else if !r.alive {continue;}
            let (speed,turn)=forecast.motion(w,id,if w.config().rules==crate::RuleSet::Classic {w.observed_rush(id).unwrap_or(0.0)} else {0.0},&motions[id],j);
            if let Some(limits)=limits.as_mut() {limits[id][j]=(speed,turn);}
            let desired=r.forecast_angle+r.observed_turn*(1.0-(j-1) as f64/9.0).max(0.0);
            let delta=normalize_angle(desired-r.forecast_angle);
            if delta.abs()>turn*STEP_SECONDS {
                r.forecast_angle=normalize_angle(r.forecast_angle+turn*STEP_SECONDS*delta.signum());
                let (sin,cos)=r.forecast_angle.sin_cos();r.direction=Point{x:cos,y:sin};
            } else if delta.abs()>1e-12 {
                r.forecast_angle=desired;
                let (sin,cos)=r.forecast_angle.sin_cos();r.direction=Point{x:cos,y:sin};
            }
            r.distance[j]=r.distance[j-1]+speed*STEP_SECONDS;
            r.path[j]=w.canonical_point(Point {x:r.path[j-1].x+r.direction.x*speed*STEP_SECONDS,y:r.path[j-1].y+r.direction.y*speed*STEP_SECONDS});
        }
    }
    #[inline]
    fn forecast_rival<'a>(scratch:&'a [Rival;MAX_SNAKES],observed:&'a [Rival;MAX_SNAKES],id:usize)->&'a Rival {
        if scratch[id].dynamic {&scratch[id]} else {&observed[id]}
    }
    fn update_envelopes(w:&World,rivals:&mut [Rival;MAX_SNAKES],motions:&[Motion;MAX_SNAKES],forecast:&forecast::Forecast,j:usize,excluded:u16,baseline:Option<(&forecast::Timeline,&[Rival;MAX_SNAKES])>) {
        if let Some((effects,_))=baseline {
            if (effects.movement_bits()|forecast.effects.movement_bits()) & !excluded==0 {return;}
        }
        for (id,r) in rivals.iter_mut().enumerate() {
            if !r.alive || excluded&(1<<id)!=0 {continue;}
            if baseline.is_some() && !r.dynamic {continue;}
            let reserve=forecast.effects.reach_scale(id,j);
            let (speed,turn)=forecast.motion(w,id,if w.config().rules==crate::RuleSet::Classic {w.observed_rush(id).unwrap_or(0.0)} else {0.0},&motions[id],j);
            let t=j as f64*STEP_SECONDS;let cap=(r.radius*2.0+12.0)*reserve;
            r.envelope[j]=if r.envelope[j-1]>=cap {cap}
                else {(speed*t*(turn*t).min(1.2).sin()*0.3*reserve).min(cap)};
        }
    }
    fn mask(&self,w:&World,id:usize) -> u16 {
        self.phase_mask(w,id,0.0)
    }
    fn arrival(&self,w:&World,r:&Rival,p:Point) -> f64 {
        self.arrival_to(w,r,p,None)
    }
    fn arrival_to(&self,w:&World,r:&Rival,p:Point,contact:Option<target::Contact>) -> f64 {
        let d=w.displacement(r.path[0],p);
        let distance_squared=d.x*d.x+d.y*d.y;
        if contact.is_some_and(|c|c.reached(distance_squared,1)) {return 0.0;}
        let distance=distance_squared.sqrt();
        let turn=normalize_angle(d.y.atan2(d.x)-r.angle).abs();
        // Turns consume forward travel too. The extra term accounts for the
        // loop needed to approach a point inside the minimum turning circle.
        let radius=r.speed/r.turn.max(0.01);
        let inside=if turn>1.2 && distance<radius*2.0 {radius*(turn-1.2)/r.speed.max(1.0)} else {0.0};
        let turning=turn/r.turn.max(0.01)*0.6+inside;
        let travel=contact.map_or(distance,|c|c.distance(distance,1));
        let eta=travel/r.speed.max(1.0)+turning;
        // If the optimistic Magnet approach outlives its effect, use ordinary
        // capture reach. Capsules and claimed food use the same typed helper.
        contact.map_or(eta,|c|c.distance(distance,crate::effects::forecast_step(eta))/r.speed.max(1.0)+turning)
    }
    fn target_slot(&self,state:State)->Option<(usize,target::TargetFood)> {
        if state.target==0 {return None;}
        self.food.get(state.target_index).copied().flatten().filter(|f|f.id==state.target).map(|f|(state.target_index,f))
            .or_else(||self.food.iter().enumerate().filter_map(|(fi,f)|f.map(|f|(fi,f))).find(|(_,f)|f.id==state.target))
    }
    fn target_food(&self,state:State)->Option<target::TargetFood> {
        self.target_slot(state).map(|(_,f)|f)
    }
    fn target_arrival(&self,w:&World,s:SnakeView<'_>,f:impl Into<target::TargetFood>)->f64 {
        let f=f.into();
        self.arrival_to(w,&self.rivals[s.id as usize],f.position,Some(target::Contact::forecast(s,f,self.opportunities.track(s.id as usize))))
    }
    /// Food and capsules share race, persistence and heading valuation.
    fn target_score(&self,w:&World,s:SnakeView<'_>,state:&State,f:impl Into<target::TargetFood>,
        eta:f64,bearing:f64,value:f64)->f64 {
        let f=f.into();
        let id=s.id as usize;
        let mut rival_eta=f64::MAX;let mut rival_len=0;let mut rival_id=id;
        for (other,r) in self.rivals.iter().enumerate() {
            if other==id || !r.alive {continue;}
            let e=if f.id & target::ITEM_BIT!=0 {self.capsule_etas[state_item_slot(self,f)][other]} else {self.target_arrival(w,w.snake(other).unwrap(),f)};
            if e<rival_eta {rival_eta=e;rival_len=r.len;rival_id=other;}
        }
        let advantage=s.segments.len()>=rival_len+6
            && self.opportunities.contact(id,rival_id,phase::step(eta.max(rival_eta)));
        let risk=2.0*aggression::level(w)-1.0;
        let bold=risk.max(0.0);
        let capsule=f.id & target::ITEM_BIT!=0;
        let prism=matches!(f.kind,crate::FoodKind::Prism|crate::FoodKind::PrismSeed);
        let competition=if capsule {
            let own=self.capsule_etas[state_item_slot(self,f)][id];
            let guard=f.id!=events::ID && self.guard_worth(w,s,f) && s.effect_ticks>90 && w.distance_squared(s.segments[0].current,f.position)<(15.0*s.radius).powi(2);
            if f.id!=state.target && !guard && !(f.id==events::ID && own<rival_eta+2.0+4.0*bold || own<(0.9+0.35*risk)*rival_eta || (own<(1.25+0.65*risk)*rival_eta && s.segments.len()>=16 && w.boost_ready(id))) {0.0}
            else if rival_eta<own*1.2 {if advantage {1.4} else {1.1}} else {1.0}
        } else if prism {
            // Ripening synchronizes arrivals: trailing heads can wait and
            // pounce too. Ordinary-food loser penalties scattered this field.
            if rival_eta<eta*1.25 {if advantage {1.35} else {1.1}} else {1.0}
        } else if rival_eta<eta*0.8 {if advantage {0.85} else {0.2+0.5*bold}}
            else if rival_eta<eta*1.2 {if advantage {1.35} else {0.65+0.45*bold}} else {1.0};
        let sticky=if f.id==state.target {if capsule || prism {1.8} else {1.28}} else {1.0};
        // ETA already pays for turning; capsules merit deliberate reversals.
        let forward=if capsule || prism {0.8+0.2*bearing.cos()} else {(0.15+0.85*((bearing.cos()+1.0)*0.5).powi(3)).max(0.05)};
        value*competition*sticky*(forward+bold*(1.0-forward)*0.4)
            *if capsule {1.0+2.0*bold} else {1.0}/(eta+0.7)
    }
    fn shortlist(shortlist:&mut [(f64,usize);5],score:f64,fi:usize) {
        for k in 0..5 {
            if score>shortlist[k].0 {
                for n in (k+1..5).rev() {shortlist[n]=shortlist[n-1];}
                shortlist[k]=(score,fi);break;
            }
        }
    }
    #[cfg(test)]
    fn strategy(&mut self,w:&World,s:SnakeView<'_>,state:&mut State,intelligence:f64) {
        self.strategy_with_venom(w,s,state,intelligence,&mut None);
    }
    fn strategy_with_venom(&mut self,w:&World,s:SnakeView<'_>,state:&mut State,intelligence:f64,venom:&mut Option<VenomHunt>) {
        let id=s.id as usize; let head=s.segments[0].current;
        let revise_tactics=w.tick()>=state.next_response;
        state.attack_options=[Attack::default();2];
        if w.tick()<state.escape_until {
            state.guarding=false;
            state.clear_attacks(s.angle);
            state.debug.flags&=!8;
            state.goal=s.segments.last().unwrap().current;
            state.set_target(0);
            state.waypoint=if w.config().rules==crate::RuleSet::V2 {
                self.spatial.escape_waypoint(head,state.goal,self.mask(w,id))
            } else {None};
            state.debug.target_count=0;state.debug.target_food_ids=[0;5];
            state.last_strategy=w.tick();return;
        }
        if w.tick()<state.orbit_until {
            state.guarding=false;
            state.clear_attacks(s.angle);
            state.debug.flags&=!8;
            state.set_target(0);state.waypoint=None;
            state.debug.target_count=0;state.debug.target_food_ids=[0;5];
            state.last_strategy=w.tick();return;
        }
        let travel=self.motion[id].at(0).0*STEP_SECONDS;
        let straight=w.canonical_point(Point{x:head.x+s.angle.cos()*travel,y:head.y+s.angle.sin()*travel});
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
                        let contact=target::Contact::forecast(s,f,self.opportunities.track(s.id as usize)).with_guard(state.guarding);
                        if contact.collected() || contact.reached(w.distance_squared(straight,f.position),1) {continue;}
                        let eta=self.target_arrival(w,s,f);
                        let bearing=normalize_angle(d.y.atan2(d.x)-s.angle).abs();
                        if distance>contact.reach(1) && bearing>1.5 && contact.distance(distance,1)<self.rivals[id].speed/self.rivals[id].turn.max(0.01)*1.8 {continue;}
                        let cluster=self.spatial.cluster_weight(key);
                        let effect=self.opportunities.before(id,crate::effects::forecast_step(eta));
                        let value=f.value+cluster.min(8.0)*if effect.is(crate::effects::EffectKind::Magnet) {0.6} else {0.18+0.2*aggression::bold(w)}+if f.feast_id>0 {0.4} else {0.0}
                            +magnet::food_bonus_effect(f,effect);
                        let score=self.target_score(w,s,state,f,eta,bearing,value);
                        Self::shortlist(&mut shortlist,score,fi);
                        if inspected>=64 {break 'rings;}
                    }
            }
            if ring>=4 && shortlist[4].0>0.0 {break;}
        }
        if let Some((fi,f))=self.prism.filter(|(_,f)|f.vacuum_owner<0 && !(f.id==state.rejected && w.tick()<state.reject_until)) {
            // Exact prize discovery shares ordinary food's endpoint gate:
            // ripe fruit already in vacuum reach needs no steering pursuit.
            let contact=target::Contact::forecast(s,f,self.opportunities.track(s.id as usize)).with_guard(state.guarding);
            if !contact.collected() && !contact.reached(w.distance_squared(straight,f.position),1) {
                let eta=self.target_arrival(w,s,f);
                let d=w.displacement(head,f.position);let bearing=normalize_angle(d.y.atan2(d.x)-s.angle).abs();
                let score=self.target_score(w,s,state,f,eta,bearing,f.value);
                if !shortlist.iter().any(|(score,i)|*score>0.0 && *i==fi) {Self::shortlist(&mut shortlist,score,fi);}
            }
        }
        self.event_shortlist(w,s,state,&mut shortlist);
        // Items are at most three, so an exact scan is cheaper than another
        // spatial index and never consumes the 64-food discovery budget.
        for (index,item) in w.items().enumerate() {
            let fi=MAX_FOOD+index;let f=self.food[fi].unwrap();
            if f.id==state.rejected && w.tick()<state.reject_until {continue;}
            let d=w.displacement(head,item.position);
            let contact=target::Contact::forecast(s,f,self.opportunities.track(s.id as usize)).with_guard(state.guarding);
            if contact.collected() || contact.reached(w.distance_squared(straight,f.position),1) {continue;}
            let eta=self.target_arrival(w,s,f);
            if eta>item.life_ticks as f64*STEP_SECONDS {continue;}
            let bearing=normalize_angle(d.y.atan2(d.x)-s.angle).abs();
            let value=self.capsule_value(w,s,item,eta);
            let score=self.target_score(w,s,state,f,eta,bearing,value);
            Self::shortlist(&mut shortlist,score,fi);
        }
        if self.opportunities.may_have(id,crate::effects::EffectKind::Magnet) {
            // Only the bounded finalists need area queries. Extra reach changes
            // nutritional utility, never the room needed for a safe escape turn.
            let need=(s.segments.len() as f64*s.radius*s.radius*8.0/(self.spatial.dx*self.spatial.dy)).ceil().max(24.0) as usize;
            for (score,fi) in &mut shortlist {
                if *score<=0.0 {continue;}
                let f=self.food[*fi].unwrap();
                if !magnet::scavenging(f) {continue;}
                let eta=self.target_arrival(w,s,f);
                if !self.opportunities.before(id,crate::effects::forecast_step(eta)).is(crate::effects::EffectKind::Magnet) {continue;}
                let (room,capped)=self.spatial.space(f.position,self.phase_mask(w,id,eta),need,self.effects.snapshot().space_time(self.phase_mask(w,id,eta),eta,&self.rivals));
                if !capped && room<need {*score*=0.2*room as f64/need as f64;}
            }
            shortlist.sort_by(|a,b|b.0.total_cmp(&a.0));
        }
        state.debug.target_food_ids=[0;5]; state.debug.target_count=0;
        for (score,fi) in shortlist {
            if score<=0.0 {continue;}
            state.debug.target_food_ids[state.debug.target_count as usize]=self.food[fi].unwrap().id;
            state.debug.target_count+=1;
        }
        let tactical_goal=state.goal;
        let old=state.target;
        if shortlist[0].0>0.0 && (self.food[shortlist[0].1].unwrap().id & target::ITEM_BIT!=0 || matches!(self.food[shortlist[0].1].unwrap().kind,crate::FoodKind::Prism|crate::FoodKind::PrismSeed)) {
            if old!=self.food[shortlist[0].1].unwrap().id {state.prey=0;state.clear_attacks(s.angle);state.clear_coil(s.angle);}
            state.harvest_until=0;
        }
        state.set_target(if shortlist[0].0>0.0 {self.food[shortlist[0].1].unwrap().id} else {0});
        if state.target!=0 {
            state.target_index=shortlist[0].1;
            state.goal=self.food[state.target_index].unwrap().position;
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
        if w.config().rules==crate::RuleSet::V2 && aggression::bold(w)>0.0 && state.target!=0 {
            // Prize races need moving contact tracking. Ordinary food keeps
            // the checked fixed-bearing turn/exit rather than repeatedly
            // bending the whole body around successive pellets.
            if state.target & target::ITEM_BIT!=0 || self.target_food(*state).is_some_and(|f|matches!(f.kind,crate::FoodKind::Prism|crate::FoodKind::PrismSeed)) {state.track_goal=true;}
            else if old!=state.target {state.track_goal=false;}
        }
        if old!=state.target {state.best_distance=f64::MAX;state.last_progress=w.tick();}
        state.debug.flags&=!8;
        if revise_tactics {let clock=self.profile_enabled.then(std::time::Instant::now);self.tactics(w,s,state,intelligence);if let Some(c)=clock {self.strategy_profile[0]+=c.elapsed().as_nanos();}}
        else if (state.prey!=0 && state.target & target::ITEM_BIT==0 && !self.target_food(*state).is_some_and(|f|matches!(f.kind,crate::FoodKind::Prism|crate::FoodKind::PrismSeed))) || w.tick()<state.dodge_until {
            state.goal=tactical_goal;state.set_target(0);
            if state.prey!=0 {state.debug.flags|=8;}
        }
        self.guard_capsule(w,s,state);self.vulture_seed(w,s,state);
        self.venom_tactics_cached(w,s,state,venom);
        state.waypoint=None;
        let mut tests=0;
        let route_goal=self.target_food(*state).filter(|_|!state.guarding && !state.vulturing).map_or(state.goal,|f|target::Contact::forecast(s,f,self.opportunities.track(s.id as usize)).with_guard(state.guarding).approach(w,head,f.position,crate::effects::forecast_step(self.target_arrival(w,s,f))));
        // A nearby race is evaluated against moving bodies by the ordinary
        // rollouts. Static route rejection scattered contenders for five
        // seconds even when a departing tail made capture safely reachable.
        let nearby_prize=w.config().rules==crate::RuleSet::V2 && self.target_food(*state).is_some_and(|f|
            (f.id==events::ID || matches!(f.kind,crate::FoodKind::Prism|crate::FoodKind::PrismSeed))
            && self.target_arrival(w,s,f)<=4.0);
        if !nearby_prize && self.body_blocked(w,s,head,route_goal,0.0,0.0,&mut tests).0 {
            state.waypoint=self.spatial.waypoint(head,route_goal,self.mask(w,id));
            if state.waypoint.is_none() && state.debug.flags&8==0 {
                #[cfg(feature="desktop-diag")] {state.abandon=AbandonReason::Blocked;}
                // Do not commit to the nutritional maximum behind a body
                // barrier. Try another shortlisted food with a clear approach.
                let mut alternative=None;
                for (score,fi) in shortlist.into_iter().skip(1) {
                    if score<=0.0 {continue;}
                    let f=self.food[fi].unwrap();let mut tests=0;
                    let approach=target::Contact::forecast(s,f,self.opportunities.track(s.id as usize)).with_guard(state.guarding).approach(w,head,f.position,crate::effects::forecast_step(self.target_arrival(w,s,f)));
                    if !self.body_blocked(w,s,head,approach,0.0,0.0,&mut tests).0 {alternative=Some((fi,f));break;}
                }
                if let Some((fi,f))=alternative {
                    if state.target!=f.id {
                        if f.id & target::ITEM_BIT!=0 {state.prey=0;state.clear_attacks(s.angle);state.clear_coil(s.angle);state.harvest_until=0;}
                        // Classic's historical fallback/progress policy is unchanged.
                        if w.config().rules==crate::RuleSet::V2 {state.best_distance=f64::MAX;state.last_progress=w.tick();}
                    }
                    state.set_target(f.id);state.target_index=fi;state.goal=f.position;
                    self.guard_capsule(w,s,state);self.vulture_seed(w,s,state);
                    self.venom_tactics_cached(w,s,state,venom);
                }
                else {
                    state.rejected=state.target;state.reject_until=w.tick()+90;state.set_target(0);
                    state.goal=w.canonical_point(Point{x:head.x+s.angle.cos()*300.0,y:head.y+s.angle.sin()*300.0});
                }
            }
        }
        if revise_tactics {
            if state.venom_target==0 && !state.venom_standoff {let clock=self.profile_enabled.then(std::time::Instant::now);state.attack_options=self.cutoffs(w,s,*state);if let Some(c)=clock {self.strategy_profile[1]+=c.elapsed().as_nanos();}}
            // Consume a response only after tactics and its candidate generation
            // actually run. Quota waits and recovery early returns keep it due.
            state.next_response=w.tick()+State::response_interval(s);
        }
        state.last_strategy=w.tick();
    }
    fn trajectory_goal(w:&World,state:State,head:Point)->Point {
        if state.guarding || state.vulturing || state.coil_radius>0.0 {
            let center=if state.guarding || state.vulturing {state.goal} else {state.coil_center};
            let orbit_radius=if state.guarding || state.vulturing {state.guard_radius} else {state.coil_radius};
            let d=w.displacement(center,head);
            let radius=(d.x*d.x+d.y*d.y).sqrt().max(1.0);
            let tangent=d.y.atan2(d.x)+state.coil_sign*std::f64::consts::FRAC_PI_2;
            let contraction=if !state.guarding && !state.vulturing && state.coil_progress>0.9 {state.coil_pitch/std::f64::consts::TAU} else {0.0};
            let correction=((radius-orbit_radius)/if state.vulturing {orbit_radius.max(30.0)} else {30.0}).atan().clamp(-0.35,0.35);
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
            if state.venom_target!=0 && !state.venom_standoff && w.boost_ready(s.id as usize)
                && w.snake(state.venom_target-1).is_some_and(|r|r.segments.len().saturating_sub(state.venom_index)>w.boost_segment_cost(s.id as usize).unwrap_or(0)) {
                let d=w.displacement(s.segments[0].current,state.goal);
                let distance=(d.x*d.x+d.y*d.y).sqrt();
                if distance>3.0*s.radius && normalize_angle(d.y.atan2(d.x)-s.angle).abs()<0.45 {
                    return 0.6;
                }
            }
            if surge::pursuit_burst(w,s,state) {return 0.6;}
            if state.coil_radius==0.0 && (state.prey==0 || self.target_food(state).is_some_and(|f|f.kind==crate::FoodKind::Prism)) && self.food_race(w,s,state) {return 0.6;}
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
        let Some(f)=self.target_food(state).filter(|f|f.vacuum_owner<0) else {return false;};
        if state.vulturing {return false;}
        if f.kind==crate::FoodKind::PrismSeed
            && self.target_arrival(w,s,f)/1.6+0.15<f.motion_ticks as f64*STEP_SECONDS {return false;}
        let worth=f.value.max(self.spatial.cluster_weight(self.spatial.key(f.position)));
        if aggression::level(w)==0.0 || worth<3.0-1.5*aggression::bold(w) {return false;}
        let d=w.displacement(s.segments[0].current,f.position);
        let distance=(d.x*d.x+d.y*d.y).sqrt();
        let (speed,turn)=self.boosted_motion[s.id as usize].at(0);
        let angle=normalize_angle(d.y.atan2(d.x)-s.angle).abs();
        let distance=target::Contact::forecast(s,f,self.opportunities.track(s.id as usize)).distance(distance,1);
        if distance<speed/turn*(1.0-angle.cos()) || distance==0.0 || angle>0.6 {return false;}
        if f.id & target::ITEM_BIT!=0 {
            if state.guarding || s.segments.len()<16 || angle>=0.45 || w.distance_squared(s.segments[0].current,f.position)>=((18.0+12.0*aggression::bold(w))*s.radius).powi(2) {return false;}
            let etas=&self.capsule_etas[state_item_slot(self,f)];let own_eta=etas[s.id as usize];
            return etas.iter().enumerate().any(|(id,&eta)|id!=s.id as usize && eta>=own_eta*(0.75-0.2*aggression::bold(w)) && eta<=own_eta*(1.25+0.5*aggression::bold(w)));
        }
        if matches!(f.kind,crate::FoodKind::Prism|crate::FoodKind::PrismSeed) {
            let eta=self.target_arrival(w,s,f);
            return self.rivals.iter().enumerate().any(|(id,r)|id!=s.id as usize && r.alive && {
                let rival=self.target_arrival(w,w.snake(id).unwrap(),f);rival>=eta*0.75 && rival<=eta*1.25
            });
        }
        self.rivals.iter().enumerate().any(|(id,r)|id!=s.id as usize && r.alive
            && (target::Contact::forecast(w.snake(id).unwrap(),f,self.opportunities.track(id)).distance(w.distance_squared(r.path[0],f.position).sqrt(),1)-distance).abs()<=distance*0.25)
    }

    fn attack_usable(&self,w:&World,s:SnakeView<'_>,state:State,a:Attack)->bool {
        if !a.valid || state.prey==0 || a.prey!=state.prey || a.prey_generation!=state.prey_generation
            || w.tick()<a.start || w.tick()>=a.end || w.tick()>state.hunt_until
            || w.tick()<state.escape_until || w.tick()<state.orbit_until || state.coil_radius>0.0 {return false;}
        let Some(victim)=w.snake(state.prey-1).filter(|v|v.alive && v.generation==a.prey_generation) else {return false;};
        if self.opportunities.phased(victim.id as usize,1) {return false;}
        if s.segments.len()<victim.segments.len()+if w.config().rules==crate::RuleSet::V2 {4} else {8} {return false;}
        if w.config().rules==crate::RuleSet::Classic {
            if a.limits.is_some_and(|limits|limits!=Self::attack_limits(w,s,a)) {return false;}
        } else {
            if w.tick()==a.start && (!w.boost_ready(s.id as usize) || s.segments.len()<16) {return false;}
            if s.flags & crate::flags::FROZEN!=0 {return false;}
            if let Some(limits)=a.limits {
                let original=limits[5] as usize;
                let elapsed=w.tick().saturating_sub(a.start) as usize;
                let cost=if a.free_boost {0} else {2+original/100};
                let expected=original.saturating_sub(cost*elapsed.min(21)/21);
                // Scheduled payments are part of the plan. Growth, unrelated
                // shrinkage and size changes invalidate it immediately.
                if s.segments.len()!=expected || s.radius>limits[4] {return false;}
                let ratio=(1.0+original.saturating_sub(24) as f64*0.004).min(2.5)
                    /(1.0+expected.saturating_sub(24) as f64*0.004).min(2.5);
                // Expiry is part of this checked plan, including a Surge
                // burst that keeps its free price after the speed bonus ends.
                let initial=forecast::Effect {kind:a.effect_kind,ticks:a.effect_ticks};
                let remaining=initial.after(elapsed);
                let effect=crate::effects::modifiers(remaining.kind,remaining.ticks);
                let initial_effect=crate::effects::modifiers(initial.kind,initial.ticks);
                let expected_speed=if elapsed<=24 {limits[0]} else {limits[2]}*ratio
                    *effect.speed/initial_effect.speed
                    *(1.0-0.1*w.world_event.night as f64)/(1.0-0.1*a.night as f64);
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
    fn tactics(&mut self,w:&World,s:SnakeView<'_>,state:&mut State,intelligence:f64) {
        let id=s.id as usize;let head=s.segments[0].current;
        self.validate_attack(w,s,state);
        state.barrier=false;
        if self.frost_tactics(w,s,state) {return;}
        let mut own=*self.opportunity_rival(id);
        let capsule=state.target & target::ITEM_BIT!=0;
        let prize=self.target_food(*state).is_some_and(|f|matches!(f.kind,crate::FoodKind::Prism|crate::FoodKind::PrismSeed));
        let aggression=s.traits.aggression*(0.65+0.35*intelligence);
        let advantage=surge::advantage(w,s,if w.config().rules==crate::RuleSet::V2 {4+2+s.segments.len()/100} else {8});
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
            if !victim.alive || self.opportunities.phased(victim.id as usize,1) || victim.generation!=state.prey_generation || (!state.attack.valid && own.len<victim.segments.len()+advantage && !self.grudge_prey(w,s,prey))
                || w.tick()>state.hunt_until || w.distance_squared(head,victim.segments[0].current)>(650.0+350.0*aggression::bold(w)).powi(2) {
                state.prey=0;state.clear_coil(s.angle);state.track_goal=false;state.clear_attacks(s.angle);
            }
        }
        if state.prey==0 && w.tick()<state.harvest_until {
            state.goal=state.coil_center;state.set_target(0);state.debug.flags|=8;return;
        }
        // Early speed/gap escape. Aim along an open lateral route rather than
        // reversing toward the hunter's neck; exact safety chooses the gap.
        let mut threat=None;let mut threat_value=0.0;
        for (other,r) in self.rivals.iter().enumerate() {
            if !r.alive || other==id || r.len<own.len+6 || !self.effects.contact(id,other,1) {continue;}
            let d=w.displacement(r.path[0],head);let distance=(d.x*d.x+d.y*d.y).sqrt();
            let approaching=(d.x*r.angle.cos()+d.y*r.angle.sin())/distance.max(1.0);
            let v=(220.0*self.effects.reach_scale(other,1)-distance)*(0.3+approaching.max(0.0));
            if v>threat_value {threat_value=v;threat=Some(other);}
        }
        if let Some(other)=threat.filter(|_|threat_value>32.0) {
            let r=&self.rivals[other];let away=w.displacement(r.path[18],head);
            let distance=(away.x*away.x+away.y*away.y).sqrt().max(1.0);
            // Retain forward travel to exploit the smaller snake's speed.
            state.goal=w.canonical_point(Point{x:head.x+away.x/distance*240.0+s.angle.cos()*160.0,
                y:head.y+away.y/distance*240.0+s.angle.sin()*160.0});
            state.set_target(0);state.prey=0;state.clear_coil(s.angle);state.track_goal=false;state.clear_attacks(s.angle);state.waypoint=None;state.dodge_until=w.tick()+18;
            return;
        }
        if prize && self.target_food(*state).is_some_and(|f|f.kind==crate::FoodKind::Prism) {
            state.prey=0;state.clear_attacks(s.angle);state.track_goal=true;return;
        }
        if capsule || prize {
            // Bigger contenders can lay a checked cutoff across an approaching
            // rival. The ordinary capsule route remains a competing candidate;
            // smaller contenders retain the direct dart and checked race burst.
            let contender=self.capsule_contender(w,s,*state);
            if contender!=state.prey.checked_sub(1) {
                state.prey=0;state.clear_attacks(s.angle);
            }
            if let Some(other)=contender {
                state.prey=other+1;state.prey_generation=w.snake(other).unwrap().generation;
                state.hunt_until=w.tick()+90;
            }
            return;
        }
        if state.prey==0 && s.face.grudge_ticks==0 && forecast::Effect::observed(w,s).is(crate::effects::EffectKind::Magnet) {return;}
        if state.prey==0 && (aggression>interest || s.face.grudge_ticks>0) {
            let mut value=0.0;
            for other in 0..MAX_SNAKES {
                let r=self.opportunity_rival(other);
                if !r.alive || other==id || self.opportunities.phased(other,1) {continue;}
                let personal=self.grudge_prey(w,s,other);
                let seed_ambush=self.prism.is_some_and(|(_,f)|f.kind==crate::FoodKind::PrismSeed && w.distance_squared(r.path[0],f.position)<(12.0*r.radius).powi(2));
                if !personal && (own.len<r.len+advantage || aggression<=interest) {continue;}
                let distance=w.distance_squared(head,r.path[0]).sqrt();
                if distance>if surge::active(w,s) {850.0+250.0*aggression::bold(w)} else {620.0+380.0*aggression::bold(w)} {continue;}
                // Very long snakes cannot catch a fast rival in open space.
                // Hunt only if it is already crossing our reachable route;
                // otherwise preserve the cheap food plan until a real cutoff
                // opportunity appears. This matters in the mature cap fixture.
                if own.speed<r.speed*0.85 && [24,48,72].into_iter().all(|j|
                    self.arrival(w,&own,r.path[j])>j as f64*STEP_SECONDS*1.15) {continue;}
                // High aggression admits real intercepts, rather than spending
                // six seconds chasing a head across unproductive open space.
                if aggression::bold(w)>0.0 && !personal && [18,24,36,48].into_iter().all(|j|
                    self.arrival(w,&own,r.path[j])>j as f64*STEP_SECONDS*1.2) {continue;}
                let eta=self.arrival(w,&own,r.path[36]);
                let facing=w.displacement(head,r.path[36]);
                let alignment=normalize_angle(facing.y.atan2(facing.x)-s.angle).cos();
                let motive=if personal {aggression.max(interest+0.1)} else {aggression};
                let v=motive*(r.len as f64).sqrt()*(1.3+alignment)/(eta+1.0)*if personal {1.4} else if seed_ambush {2.0} else {1.0};
                if v>value {value=v;state.prey=other+1;}
            }
            // Paid boosts already pass their own value and readiness gates;
            // admit nearby moderate-interest hunts without spending on chase.
            if value<if w.config().rules==crate::RuleSet::V2 {0.75-0.35*aggression::bold(w)} else {1.0} {state.prey=0;}
            if state.prey!=0 {
                state.prey_generation=w.snake(state.prey-1).unwrap().generation;
                state.hunt_until=w.tick()+if s.face.grudge_ticks>0 && s.face.grudge_id as usize==state.prey-1 {s.face.grudge_ticks as u64} else {180};
                state.coil_sign=if s.traits.turn_bias<0.0 {-1.0} else {1.0};
            }
        }
        if state.prey==0 {return;}
        let r=*self.opportunity_rival(state.prey-1);
        let pocket_clock=self.profile_enabled.then(std::time::Instant::now);
        if let Some(goal)=self.barrier_goal(w,s,state.prey-1) {
            if let Some(c)=pocket_clock {self.strategy_profile[2]+=c.elapsed().as_nanos();}
            state.barrier=true;state.goal=goal;state.set_target(0);state.track_goal=true;state.debug.flags|=8;return;
        }
        self.pocket(w,s,state);
        if let Some(c)=pocket_clock {self.strategy_profile[2]+=c.elapsed().as_nanos();}
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
        state.set_target(0);state.debug.flags|=8;
    }
    /// Static trail occupancy with conservative tail release. Interior body
    /// locations stay occupied until the tail arrives; current growth adds a
    /// delay rather than making those locations permanent obstacles.
    fn body_blocked(&self,w:&World,s:SnakeView<'_>,a:Point,b:Point,time:f64,padding:f64,tests:&mut usize) -> (bool,f64,bool) {
        self.body_blocked_phase(w,s,a,b,time,padding,tests,self.effects.phased(s.id as usize,phase::step(time)),&self.effects)
    }
    fn body_blocked_phase(&self,w:&World,s:SnakeView<'_>,a:Point,b:Point,time:f64,padding:f64,tests:&mut usize,phased:bool,effects:&forecast::Timeline) -> (bool,f64,bool) {
        self.body_blocked_phase_flags::<true>(w,s,a,b,time,padding,tests,phased,effects)
    }
    fn body_blocked_phase_flags<const EFFECTS:bool>(&self,w:&World,s:SnakeView<'_>,a:Point,b:Point,time:f64,padding:f64,tests:&mut usize,phased:bool,effects:&forecast::Timeline) -> (bool,f64,bool) {
        let mut bite=None;
        if EFFECTS && effects.may_have(s.id as usize,crate::effects::EffectKind::Venom) {
            self.body_query::<true,true>(w,s,a,b,time,padding,tests,phased,effects,&mut bite,phase::step(time).saturating_sub(1),None)
        } else {self.body_query::<false,EFFECTS>(w,s,a,b,time,padding,tests,phased,effects,&mut bite,phase::step(time).saturating_sub(1),None)}
    }
    #[cfg(test)]
    fn body_blocked_effects<const BITES:bool>(&self,w:&World,s:SnakeView<'_>,a:Point,b:Point,time:f64,padding:f64,tests:&mut usize,phased:bool,effects:&mut forecast::Timeline)->(bool,f64,bool) {
        self.body_blocked_effects_from::<BITES>(w,s,a,b,time,padding,tests,phased,effects,phase::step(time).saturating_sub(1),None)
    }
    fn body_blocked_effects_from<const BITES:bool>(&self,w:&World,s:SnakeView<'_>,a:Point,b:Point,time:f64,padding:f64,tests:&mut usize,phased:bool,effects:&mut forecast::Timeline,start:usize,physical_rivals:Option<&[Rival;MAX_SNAKES]>)->(bool,f64,bool) {
        let mut bite=None;
        let result=self.body_query::<BITES,true>(w,s,a,b,time,padding,tests,phased,effects,&mut bite,start,physical_rivals);
        if !result.0 {if let Some((other,cut))=bite {
            let step=phase::step(time);
            effects.sever_cut[other]=cut;effects.bite_step[other]=step;effects.consumed_at[s.id as usize]=step;
        }}
        result
    }
    fn body_query<const BITES:bool,const EFFECTS:bool>(&self,w:&World,s:SnakeView<'_>,a:Point,b:Point,time:f64,padding:f64,tests:&mut usize,phased:bool,effects:&forecast::Timeline,bite_result:&mut Option<(usize,usize)>,start:usize,physical_rivals:Option<&[Rival;MAX_SNAKES]>)->(bool,f64,bool) {
        let phase_step=phase::step(time);
        if phased {return (false,200.0,false);}
        let key=self.spatial.key(w.canonical_point(a));
        let ab=w.displacement(a,b);
        let first=time<=STEP_SECONDS+1e-9;
        let motion=if first {self.spatial.max_motion} else {0.0};
        let ordinary=spatial::query_radius(s.radius+self.max_radius,8.0+padding,1.0,motion);
        let reach=if w.config().rules==crate::RuleSet::V2 {
            // Full-width contacts bound taper and advancing neck widths. Own
            // first-step margin reaches 2px; later margins are capped by r/4.
            let contact=crate::world::taper::contact_radius(w.config().rules,s.radius,self.max_radius,false);
            let margin=if first {2.0} else {1.5+self.max_radius*0.25+padding};
            ordinary.max(spatial::query_radius(contact,margin,if EFFECTS {effects.max_reach_scale(phase_step)} else {1.0},motion))
        } else {ordinary};
        let root_x=(key%self.spatial.cols) as isize;
        let root_y=(key/self.spatial.cols) as isize;
        let x0=((a.x+ab.x.min(0.0)-reach)/self.spatial.dx).floor() as isize-root_x;
        let x1=((a.x+ab.x.max(0.0)+reach)/self.spatial.dx).floor() as isize-root_x;
        let y0=((a.y+ab.y.min(0.0)-reach)/self.spatial.dy).floor() as isize-root_y;
        let y1=((a.y+ab.y.max(0.0)+reach)/self.spatial.dy).floor() as isize-root_y;
        let neck_advance=self.planning_speed[s.id as usize]*time/(s.radius*1.18);
        let mut clearance=200.0_f64;
        let mut bite=None;
        let length2=ab.x*ab.x+ab.y*ab.y;
        // Reject distant records before release/taper work. Full-width bodies
        // and the largest neck margin bound every exact threshold below. Keep
        // traversal and visit accounting unchanged, including capped queries.
        let mut reserves=[0.0;MAX_SNAKES];
        let mut release_times=[-1.0;MAX_SNAKES];
        let min_x=ab.x.min(0.0);let max_x=ab.x.max(0.0);
        let min_y=ab.y.min(0.0);let max_y=ab.y.max(0.0);
        let (xs,ys)=self.spatial.spans(key,x0,x1,y0,y1);
        for y in ys {for x in xs.clone() {
            // spans already clips wall-bounded offsets to the arena. Reuse
            // the root coordinates computed above instead of repeating the
            // general wrapped offset conversion for every fine-grid bucket.
            let k=if w.config().deadly_walls {
                (root_y+y) as usize*self.spatial.cols+(root_x+x) as usize
            } else {
                let Some(k)=self.spatial.offset(key,x,y) else {continue;};k
            };
            if !w.config().self_collisions && self.spatial.occupied[k]&!(1<<s.id)==0 {continue;}
            let mut at=self.spatial.heads[k];
            while at>=0 {
                *tests+=1;
                if *tests>NARROW_LIMIT {return (true,clearance,true);}
                let encoded=at as usize; at=self.spatial.next[encoded];
                let other=encoded/MAX_SEGMENTS;let j=encoded%MAX_SEGMENTS;
                let same=other==s.id as usize;
                if EFFECTS && !same && effects.phased(other,phase_step) {continue;}
                if same && (!w.config().self_collisions || j as f64+neck_advance<10.0) {continue;}
                let r=&self.rivals[other];
                let seg=w.segments[encoded];let p=seg.current;
                let ap=w.displacement(a,p);
                if reserves[other]==0.0 {
                    let contact=crate::world::taper::contact_radius(w.config().rules,s.radius,r.radius,same);
                    let margin=if first {if same && w.config().rules==crate::RuleSet::V2 {2.0} else {0.75}}
                        else {1.5+(r.speed*STEP_SECONDS*0.35).min(r.radius*0.25)+padding};
                    let scale=if !EFFECTS || same {1.0} else {effects.reach_scale(other,phase_step)};
                    // Match the narrow phase's multiplication/addition order.
                    reserves[other]=spatial::query_radius(contact*scale,margin*scale,1.0,motion);
                }
                let reserve=reserves[other];
                let distant=ap.x<min_x-reserve || ap.x>max_x+reserve
                    || ap.y<min_y-reserve || ap.y>max_y+reserve;
                #[cfg(test)] let distant=distant && !self.reference_queries;
                if distant {continue;}
                if BITES && effects.sever_cut[other]>0 && j>=effects.sever_cut[other] {continue;}
                if BITES && bite.is_some_and(|(victim,cut)|victim==other && j>=cut) {continue;}
                // 65% of current speed allows for growth, slowing and curved
                // tails. Current growth adds a finite release delay.
                let sever=if BITES {effects.sever_cut[other]} else {0};
                let physical=physical_rivals.map_or(r,|scratch|Self::forecast_rival(scratch,&self.rivals,other));
                let retained_index=if sever>0 {j as f64+physical.distance[start.min(STEPS)]/(r.radius*1.18)} else {j as f64};
                // A severed tail advances with the physical trail. Adding a
                // fresh conservative release delay at the bite kept old stump
                // samples behind the new tail and rejected every checked exit.
                if sever>0 && retained_index>sever as f64 {continue;}
                if release_times[other]<0.0 {release_times[other]=effects.release_clock(other,time,r.release_slowdown);}
                let release_time=release_times[other];
                let release=(r.len-j) as f64*r.release_rate+r.growth_delay;
                if release_time>release+0.15 {continue;}
                let body=if sever>0 && effects.bite_step[other]<phase_step {
                    // The retained endpoint is a tapered tail now, not the
                    // original full-width mid-body. Keeping its old width
                    // rejected safe strikes against a phantom large stump.
                    crate::world::taper::body_radius(r.radius,retained_index,sever)
                } else if w.config().rules==crate::RuleSet::V2 {
                    // Advancing an old neck sample may widen it; retain the
                    // maximum width until it has passed the full-width band.
                    if (j as f64)<0.07*r.len.saturating_sub(1).max(1) as f64 {
                        // Before the full-width band, retain the faster width
                        // bound as well as the effect-aware tail clock.
                        let advance=((time-r.growth_delay).max(0.0)/r.release_rate).max(0.0);
                        let scale=r.len.saturating_sub(1).max(1) as f64;
                        crate::world::taper::span_radius(r.radius,j as f64/scale,(j as f64+advance)/scale)
                    } else {r.radius*self.spatial.widths[encoded]}
                } else {r.radius};
                let physical_contact=crate::world::taper::contact_radius(w.config().rules,s.radius,body,same);
                // Half a tick's motion and a small radius reserve protect the
                // first swept tick against segments that shift along corners.
                let margin=if time<=STEP_SECONDS+1e-9 {if same && w.config().rules==crate::RuleSet::V2 {0.75+1.25*body/r.radius} else {0.75}} else {1.5+(r.speed*STEP_SECONDS*0.35).min(r.radius*0.25)+padding};
                let scale=if !EFFECTS || same {1.0} else {effects.reach_scale(other,phase_step)};
                let threshold=physical_contact*scale;
                let margin=margin*scale;
                let reserve=spatial::query_radius(threshold,margin,1.0,if first {self.spatial.max_motion} else {0.0});
                if ap.x<ab.x.min(0.0)-reserve || ap.x>ab.x.max(0.0)+reserve
                    || ap.y<ab.y.min(0.0)-reserve || ap.y>ab.y.max(0.0)+reserve {continue;}
                let fraction=if length2>0.0001 {((ap.x*ab.x+ap.y*ab.y)/length2).clamp(0.0,1.0)} else {0.0};
                let mut d=(ap.x-ab.x*fraction).powi(2)+(ap.y-ab.y*fraction).powi(2);
                if time<=STEP_SECONDS+1e-9 {
                    let velocity=w.displacement(seg.previous,seg.current);
                    let future=w.canonical_point(Point{x:p.x+velocity.x,y:p.y+velocity.y});
                    d=w.segments_distance_squared(a,b,p,future);
                }
                clearance=clearance.min(d.sqrt()-threshold);
                if d<spatial::query_radius(threshold,margin,1.0,0.0).powi(2) {
                    let effect=effects.at(s.id as usize,phase_step);
                    let immunity=if effects.bite_step[other]>0 {60usize.saturating_sub(phase_step-effects.bite_step[other]) as u16}
                        else {w.faces[other].bite_immunity_ticks.saturating_sub(phase_step.saturating_sub(1) as u16)};
                    let len=if effects.sever_cut[other]>0 {effects.sever_cut[other]} else {r.len};
                    // One charge can detach only one victim's rear body. Keep
                    // finding its deepest contact, but all other bodies stay lethal.
                    if BITES && bite.is_none_or(|(victim,_)|victim==other)
                        && crate::world::venom::bite_eligible(w.config().rules,effect.kind,effect.ticks,s.id as usize,other,j,len,immunity,effects.phased(other,phase_step)) {
                        // Surge widens avoidance, never the mechanics bite disk.
                        let cut=if first { (d<physical_contact*physical_contact).then_some(j) }
                            else {self.venom_physical_bite(w,s,other,j,a,b,start.min(phase_step-1),phase_step,effects,physical_rivals.map_or(r,|scratch|Self::forecast_rival(scratch,&self.rivals,other)))};
                        // Outstanding growth makes later physical cut indices
                        // unresolved. Do not drive through the reserved body
                        // on the assumption that an uncertified bite removes it.
                        if !first && cut.is_none() && w.forecast_growth_active(other) {return (true,clearance,false);}
                        if let Some(index)=cut {if bite.is_none_or(|(victim,cut)|victim==other && index<cut) {bite=Some((other,index));}}
                        continue;
                    }
                    return (true,clearance,false);
                }
            }
        }}
        *bite_result=bite;
        (false,clearance,false)
    }
    #[cfg(test)]
    fn cached_body_blocked(&mut self,w:&World,s:SnakeView<'_>,a:Point,b:Point,time:f64,padding:f64,tests:&mut usize)->(bool,f64,bool) {
        {let effects=self.effects;self.cached_body_blocked_phase(w,s,a,b,time,padding,tests,effects.phased(s.id as usize,phase::step(time)),&effects)}
    }
    #[cfg(test)]
    fn cached_body_blocked_phase(&mut self,w:&World,s:SnakeView<'_>,a:Point,b:Point,time:f64,padding:f64,tests:&mut usize,phased:bool,effects:&forecast::Timeline)->(bool,f64,bool) {
        self.cached_body_blocked_phase_flags::<true>(w,s,a,b,time,padding,tests,phased,effects)
    }
    fn cached_body_blocked_phase_flags<const EFFECTS:bool>(&mut self,w:&World,s:SnakeView<'_>,a:Point,b:Point,time:f64,padding:f64,tests:&mut usize,phased:bool,effects:&forecast::Timeline)->(bool,f64,bool) {
        if EFFECTS && effects.may_have(s.id as usize,crate::effects::EffectKind::Venom) {
            return self.body_blocked_phase(w,s,a,b,time,padding,tests,phased,effects);
        }
        let hash=a.x.to_bits().wrapping_mul(0x9e3779b97f4a7c15)^a.y.to_bits().rotate_left(23)
            ^b.x.to_bits().rotate_left(41)^b.y.to_bits().rotate_left(11)^time.to_bits().rotate_left(7);
        let key=(hash^(hash>>32)) as usize&255;
        let old=self.body_cache[key];
        let phase_bits=if EFFECTS {effects.phase_bits(phase::step(time))} else {0};
        let surge_bits=if EFFECTS {effects.surge_bits(phase::step(time))} else {0};
        let release_key=effects.release_key();
        if old.epoch==self.cache_epoch && old.id==s.id as usize && old.a==a && old.b==b && old.time==time && old.padding==padding && old.speed==self.planning_speed[s.id as usize] && old.phased==phased && old.phase_bits==phase_bits && old.surge_bits==surge_bits && old.release_key==release_key {
            *tests+=old.visits;
            if *tests>NARROW_LIMIT {return (true,old.result.1,true);}
            return old.result;
        }
        let before=*tests;
        let result=self.body_blocked_phase_flags::<EFFECTS>(w,s,a,b,time,padding,tests,phased,effects);
        if !result.2 {
            self.body_cache[key]=BodyCache {epoch:self.cache_epoch,id:s.id as usize,a,b,time,padding,speed:self.planning_speed[s.id as usize],visits:*tests-before,phased,phase_bits,surge_bits,release_key,result};
        }
        result
    }
    #[cfg(test)]
    fn rollout(&mut self,w:&World,s:SnakeView<'_>,state:State,kind:usize,horizon:usize) -> Candidate {
        let mut candidate=Candidate::default();
        self.rollout_into(w,s,state,kind,horizon,&mut candidate);
        candidate
    }
    fn candidate_mask(&self,w:&World,id:usize,c:&Candidate)->u16 {
        c.effects.mask(w,id,c.steps.max(1))
    }
    fn candidate_rush(w:&World,s:SnakeView<'_>,state:State,kind:usize,attack:Attack)->f64 {
        if kind==11 {0.0} else if kind==12 {
            if state.venom_target!=0 && w.snake(state.venom_target-1).is_none_or(|r|
                r.segments.len().saturating_sub(state.venom_index)<=w.boost_segment_cost(s.id as usize).unwrap_or(0)) {0.0}
            else {Self::boost_request(w,s,0.6)}
        }
            else {Self::boost_request(w,s,if attack.valid {attack.control(w.tick()).1} else if w.config().rules==crate::RuleSet::Classic || kind==0 || kind==1 || state.escape_boost {state.rush} else {0.0})}
    }
    /// A positive emergency prefix must leave one physical turn available.
    /// Check single-wall bands as well as corners: two independent axis
    /// escapes do not establish a shared turning circle.
    #[inline]
    fn emergency_wall_turn_room(&self,w:&World,s:SnakeView<'_>,c:&Candidate,immediate:bool)->bool {
        let id=s.id as usize;
        let motion=if c.rush>0.0 {&self.boosted_motion[id]} else {&self.motion[id]};
        let (speed,turn)=motion.at(0);
        let radius=speed/turn.max(0.01);
        let reserve=radius+s.radius*0.5+speed*STEP_SECONDS+2.0;
        let p=c.path[1];let cfg=w.config();
        let near_x=p.x.min(cfg.width-p.x)<=2.0*radius+reserve;
        let near_y=p.y.min(cfg.height-p.y)<=2.0*radius+reserve;
        if !(near_x && near_y) && (!immediate || !(near_x || near_y)) {return true;}
        let travel=(speed*STEP_SECONDS).max(0.01);
        let direction=Point {x:(p.x-c.path[0].x)/travel,y:(p.y-c.path[0].y)/travel};
        [-1.0,1.0].into_iter().any(|side| {
            let center=Point {x:p.x-side*direction.y*radius,y:p.y+side*direction.x*radius};
            center.x>=reserve && center.x<=cfg.width-reserve
                && center.y>=reserve && center.y<=cfg.height-reserve
        })
    }
    fn rollout_into(&mut self,w:&World,s:SnakeView<'_>,mut state:State,kind:usize,horizon:usize,c:&mut Candidate) {
        let alternate=state.venom_target!=0 && (kind==7 || kind==8 || kind==12);
        if alternate {
            if (kind==8 || kind==12) && state.venom_alternative.is_some() {
                let plan=state.venom_alternative.unwrap();
                state.goal=plan.goal;state.venom_target=plan.target;state.venom_generation=plan.generation;
                state.venom_index=plan.index;state.venom_standoff=plan.standoff;
                state.prey=plan.target;state.prey_generation=plan.generation;
            } else if let Some(r)=w.snake(state.venom_target-1) {
                let index=(r.segments.len()*70/100).max(4).min(r.segments.len()-1);
                state.goal=Self::venom_strike_goal(w,s,r,index,state.venom_standoff);state.venom_index=index;
            }
            state.waypoint=None;state.track_goal=true;
        }
        let mut rivals=self.simulation_rivals.take().unwrap();
        for rival in rivals.iter_mut() {rival.dynamic=false;}
        let proposed=match kind {1=>state.attack,7|8=>state.attack_options[kind-7],_=>Attack::default()};
        let attack_clock=(self.profile_enabled && proposed.valid).then(std::time::Instant::now);
        // Either accepted attack control or its ordinary fallback can run.
        // Ordinary candidates must not pay for capsules only a new burst reaches.
        let boosted=kind==12 || ((kind==0 || kind==1 || state.escape_boost) && state.rush>=0.5)
            || (proposed.valid && proposed.control(w.tick()).1>=0.5);
        if self.item_forecast.reachable(s.id as usize,horizon,boosted) {self.rollout_simulation::<true,true>(w,s,state,kind,horizon,c,&mut rivals);}
        else if self.collision_effects {self.rollout_simulation::<false,true>(w,s,state,kind,horizon,c,&mut rivals);}
        else {self.rollout_simulation::<false,false>(w,s,state,kind,horizon,c,&mut rivals);}
        c.venom_goal=if alternate {Some(VenomPlan {goal:state.goal,target:state.venom_target,
            generation:state.venom_generation,index:state.venom_index,standoff:state.venom_standoff})} else {None};
        self.simulation_rivals=Some(rivals);
        if let Some(c)=attack_clock {self.strategy_profile[3]+=c.elapsed().as_nanos();}
    }
    /// All forecasts with a proposed control use its current guard intent.
    /// World presentation can still carry the preceding tick's observation.
    fn intent_forecast<const ITEMS:bool>(&self,w:&World,s:SnakeView<'_>,state:State,horizon:usize,boosted:u16)->forecast::Forecast {
        let mut result=if ITEMS {forecast::Forecast::cached(w,self.initial_effects,&self.item_forecast,horizon,boosted)} else {forecast::Forecast::empty(self.effects)};
        result.effects.set_guard(s.id as usize,state.guarding,state.target & !target::ITEM_BIT);
        result
    }
    fn rollout_simulation<const ITEMS:bool,const EFFECTS:bool>(&mut self,w:&World,s:SnakeView<'_>,state:State,kind:usize,horizon:usize,c:&mut Candidate,rivals:&mut [Rival;MAX_SNAKES]) {
        let forecast_clock=self.profile_enabled.then(std::time::Instant::now);
        let proposed=if kind==1 {state.attack} else if kind==7 || kind==8 {state.attack_options[kind-7]} else {Attack::default()};
        let attack=if self.attack_usable(w,s,state,proposed) {proposed} else {Attack::default()};
        let rush=Self::candidate_rush(w,s,state,kind,attack);
        let motion=if rush>0.0 {self.boosted_motion[s.id as usize]} else {self.motion[s.id as usize]};
        let (mut speed,mut turn)=if w.config().rules==crate::RuleSet::V2 {motion.at(0)} else {w.motion_limits(s.id as usize,rush).unwrap()};
        let mut forecast=self.intent_forecast::<ITEMS>(w,s,state,horizon,if rush>0.0 {1<<s.id} else {0});
        let pickup_speed_scale=if ITEMS && forecast.reachable_surge(s.id as usize) {
            crate::effects::modifiers(crate::effects::EffectKind::Surge as u8,1).speed
        } else {1.0};
        let maximum_speed=(if w.config().rules==crate::RuleSet::V2 {motion.max_speed} else if attack.valid {w.motion_limits(s.id as usize,attack.burst).unwrap().0} else {speed})*pickup_speed_scale;
        let crossing_limits=if attack.valid {w.motion_limits(s.id as usize,attack.crossing_rush).unwrap()} else {(speed,turn)};
        self.planning_speed[s.id as usize]=speed;
        let tracks_goal=!attack.valid && (((kind==0 || kind==11) && (state.coil_radius>0.0 || state.track_goal)) || ((kind==1 || kind==12 || (state.venom_target!=0 && (kind==7 || kind==8))) && state.track_goal));
        // Reset metadata only. Every path/distance entry that can be read is
        // overwritten by this rollout; clearing the unused horizon for every
        // candidate used to stream hundreds of KB of zeros per snake/tick.
        c.body_len=if w.config().rules==crate::RuleSet::V2 {motion.final_len} else {s.segments.len()};
        c.kind=kind;c.checked=false;c.tracks_goal=tracks_goal;c.desired=0.0;
        c.turn_until=match kind {1|12=>state.turn_until,7|8=>w.tick()+16,9|10=>w.tick()+32,_=>u64::MAX};
        c.exit_angle=if kind==1 || kind==12 {state.exit_angle} else {0.0};
        c.angle=s.angle;c.turn_exit=0.0;c.steps=0;c.simulated_steps=0;c.score=0.0;c.clearance=200.0;
        c.capped=false;c.area=0;c.uncertain=false;c.attack=attack;c.replies=0;c.rush=rush;
        c.path[0]=s.segments[0].current;
        let goal=state.waypoint.unwrap_or(state.goal);
        let target=self.target_food(state).filter(|_|!state.guarding).map(|f|(f.position,target::Contact::forecast(s,f,self.opportunities.track(s.id as usize))));
        let goal_contact=if state.waypoint.is_none() {target.map(|(_,contact)|contact)} else {None};
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
        let forward=direction;
        let mut rotation=(turn*STEP_SECONDS).sin_cos();
        self.rollout_distance[0]=0.0;self.rollout_projection[0]=0.0;
        let mut checked=0;
        let mut self_bounds=TrailBounds::point(c.path[0]);let mut self_bounds_horizon=0;
        let mut max_curve=if w.config().rules==crate::RuleSet::V2 {motion.max_curve} else if attack.valid {let limits=w.motion_limits(s.id as usize,attack.burst).unwrap();
            (limits.0*limits.1).max(crossing_limits.0*crossing_limits.1)} else {speed*turn};

        let mut previous_phase=EFFECTS && forecast.effects.phased(s.id as usize,1);let mut previous_bits=if EFFECTS {forecast.effects.phase_bits(1)} else {0};let mut previous_surge=if EFFECTS {forecast.effects.surge_bits(1)} else {0};let mut phase_start=0;
        let mut near=0u16;let mut defeated=0u16;
        let mut reached=target.is_some_and(|(p,contact)|contact.collected() || (tracks_goal && contact.with_guard(state.guarding).ahead(w,c.path[0],s.angle,speed*STEP_SECONDS,p)));
        for (other,r) in self.rivals.iter().enumerate() {
            if !r.alive || other==s.id as usize {continue;}
            let ordinary=(speed+r.speed)*horizon as f64*STEP_SECONDS+(s.radius+r.radius)*3.0+20.0;
            let bound=if w.config().rules==crate::RuleSet::V2 {
                let rival_motion=&self.motion[other];
                let rival_scale=if ITEMS && forecast.reachable_surge(other) {1.6_f64} else {1.0};
                let curvature=(max_curve+rival_motion.max_curve*rival_scale.powi(2))*(4.0*STEP_SECONDS).powi(2)/8.0;
                ordinary.max(spatial::query_radius((s.radius+r.radius)*0.82,r.radius*2.0+15.0+curvature,
                    1.6,(maximum_speed+rival_motion.max_speed*rival_scale)*horizon as f64*STEP_SECONDS))
            } else {ordinary};
            if w.distance_squared(c.path[0],r.path[0])<bound*bound {near|=1<<other;}
        }
        if self.neck_owner!=s.id as usize {self.neck_owner=s.id as usize;self.neck_horizons.fill(0);}
        let mut missing=near;
        while missing!=0 {
            let other=missing.trailing_zeros() as usize;missing&=missing-1;
            let start=self.neck_horizons[other] as usize;
            if start>=horizon {continue;}
            let r=&self.rivals[other];let reach=(s.radius+r.radius)*0.82+5.0;
            let mut end=if start==0 {0} else {self.rival_necks[other][start] as usize};
            for j in start+1..=horizon {
                let newest=r.distance[j]-reach;
                while end+1<j && r.distance[end+1]<=newest {end+=1;}
                self.rival_necks[other][j]=end as u16;
            }
            self.neck_horizons[other]=horizon as u16;
        }
        if let Some(t)=forecast_clock {self.forecast_profile[3]+=t.elapsed().as_nanos();}
        // This neck age is invariant for the candidate: V2/attacks use the
        // fixed maximum speed; an ordinary Classic rollout never changes speed.
        // Avoid repeating division/ceil at every forecast movement.
        let self_neck_age=if w.config().self_collisions {
            (s.radius*1.18*10.0/(if attack.valid || w.config().rules==crate::RuleSet::V2 {maximum_speed} else {speed})/STEP_SECONDS).ceil() as usize
        } else {0};
        let wall_factor=1.10+0.15*aggression::bold(w);
        let mut wall_radius=speed/turn.max(0.01)*wall_factor;
        for j in 1..=horizon {
            let attack_control=attack.control(w.tick()+j as u64-1);
            if w.config().rules==crate::RuleSet::V2 {
                let limits=forecast.motion(w,s.id as usize,rush,&motion,j);
                if limits!=(speed,turn) {
                    (speed,turn)=limits;rotation=(turn*STEP_SECONDS).sin_cos();
                    wall_radius=speed/turn.max(0.01)*wall_factor;
                }
                // The cached schedule already bounds every observed motion
                // stage. Only a candidate's new pickup can increase it.
                if ITEMS {max_curve=max_curve.max(speed*turn);}
            } else if attack.valid {
                if w.tick()+j as u64-1>=attack.turn_at {(speed,turn)=crossing_limits;}
                rotation=(turn*STEP_SECONDS).sin_cos();
                wall_radius=speed/turn.max(0.01)*wall_factor;
            }
            if tracks_goal {
                // The wait and its pounce are one forecasted maneuver. An
                // endless hypothetical orbit falsely rejected safe short
                // waits because it would eventually close across our body.
                if trajectory_state.vulturing && w.tick()+j.saturating_sub(1) as u64>=trajectory_state.vulture_until {
                    trajectory_state.vulturing=false;
                    if let Some((position,_))=target {trajectory_state.goal=position;}
                }
                Self::advance_spiral(w,&mut trajectory_state,c.path[j-1]);
                if state.coil_radius>0.0 && (trajectory_state.coil_radius<=0.0 || speed*Self::spiral_curvature(trajectory_state.coil_radius,trajectory_state.coil_pitch)>turn) {break;}
            }
            let desired=if attack.valid {attack_control.0} else if tracks_goal {
                let goal=Self::trajectory_goal(w,trajectory_state,c.path[j-1]);
                let d=w.displacement(c.path[j-1],goal);
                if reached || (state.venom_target!=0 && forecast.effects.consumed_at[s.id as usize]>0) || (state.prey!=0 && defeated&(1<<(state.prey-1))!=0) {c.angle} else {d.y.atan2(d.x)}
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
            if j==16 {
                c.turn_exit=c.angle;
                if kind!=1 && kind!=12 && kind!=9 && kind!=10 {c.exit_angle=c.angle;}
            }
            let p=w.canonical_point(Point {x:c.path[j-1].x+direction.x*speed*STEP_SECONDS,
                y:c.path[j-1].y+direction.y*speed*STEP_SECONDS});
            c.path[j]=p;c.simulated_steps=j;
            let projection=(p.x-c.path[0].x)*forward.x+(p.y-c.path[0].y)*forward.y;
            self.rollout_projection[j]=self.rollout_projection[j-1].max(projection);
            if !reached && target.is_some_and(|(target,contact)|contact.with_guard(state.guarding).reached_with(w.distance_squared(p,target),forecast.effects.before(s.id as usize,j),j)) {reached=true;}
            let participant_clock=(ITEMS && self.profile_enabled).then(std::time::Instant::now);
            let changed_movement=ITEMS && forecast.movement_changed() & !(1<<s.id)!=0;
            if changed_movement {Self::advance_rivals(w,rivals,&self.motion,&forecast,j,1<<s.id,Some((&self.effects,&self.rivals)),None);}
            if ITEMS && forecast.has_items() {
                let radius=forecast.radius(w,s.id as usize,rush,&motion,j);
                forecast.set_radius(s.id as usize,radius);
                for (id,_) in rivals.iter().enumerate().filter(|(_,r)|r.dynamic) {
                    let radius=forecast.radius(w,id,0.0,&self.motion[id],j);forecast.set_radius(id,radius);
                }
                let changed=(1<<s.id) | rivals.iter().enumerate().fold(0,|mask,(id,r)|mask | if r.dynamic {1<<id} else {0});
                forecast.advance_cached(w,j,|id|if id==s.id as usize {p} else {Self::forecast_rival(rivals,&self.rivals,id).path[j]},changed,&self.item_forecast,maximum_speed.max(self.max_forecast_speed*pickup_speed_scale)*4.0*STEP_SECONDS);
            } else {forecast.sweep_needed=false;}
            if changed_movement {Self::update_envelopes(w,rivals,&self.motion,&forecast,j,1<<s.id,Some((&self.effects,&self.rivals)));}
            if let Some(t)=participant_clock {self.forecast_profile[4]+=t.elapsed().as_nanos();}
            let movement_event=forecast.effects.movement_event(j);
            if movement_event && forecast.effects.movement_bits()&(1<<s.id)!=0 {
                c.body_len=forecast.body_len(w,s.id as usize,rush,j);
            }
            let phased=EFFECTS && forecast.effects.phased(s.id as usize,j);
            let phase_changed=phased!=previous_phase;
            let approaching_phase=ITEMS && forecast.sweep_needed;previous_phase=phased;
            let rival_phase_changed=previous_bits!=(if EFFECTS {forecast.effects.phase_bits(j)} else {0});previous_bits=if EFFECTS {forecast.effects.phase_bits(j)} else {0};
            let surge_changed=previous_surge!=(if EFFECTS {forecast.effects.surge_bits(j)} else {0});previous_surge=if EFFECTS {forecast.effects.surge_bits(j)} else {0};
            if phased {phase_start=j;}

            // Even a first-step collision is checked. A tactic feasibility
            // rejection before constructing that step is not a fallback.
            c.checked=true;
            self.rollout_distance[j]=self.rollout_distance[j-1]+speed*STEP_SECONDS;
            let cfg=w.config();
            if cfg.deadly_walls {
                let wall=p.x.min(cfg.width-p.x).min(p.y).min(cfg.height-p.y);
                c.clearance=c.clearance.min(wall);
                let radius=wall_radius;
                let x_escape=radius*(1.0-direction.y.abs())+s.radius*0.5+2.0;
                let y_escape=radius*(1.0-direction.x.abs())+s.radius*0.5+2.0;
                // Once this control settles, reserve a turn parallel to an
                // approaching wall, including beyond the rollout horizon.
                // During a rate-limited turn, check its actual arc instead:
                // the inflated reserve can reject a safe inward arc halfway
                // through the maneuver and favor a shallow turn into a corner.
                // The endpoint still needs room for the next control.
                let x_room=if direction.x>=0.0 {cfg.width-p.x} else {p.x};
                let y_room=if direction.y>=0.0 {cfg.height-p.y} else {p.y};
                if wall<s.radius*0.5+2.0 || ((delta.abs()<=step || j==horizon) && (x_room<x_escape || y_room<y_escape)) {break;}
            }
            let t=j as f64*STEP_SECONDS;
            // Sweep up to four exact 30 Hz steps with a curvature-error bound.
            // The published first step is always checked separately.
            // Finish the old stage before changing speed/turn. Every consumer
            // uses this same interval, including head and deposited-self sweeps.
            let bite_exit=forecast.effects.consumed_at[s.id as usize];
            let sweep=(bite_exit>0 && j<=bite_exit+3) || ((EFFECTS && forecast.effects.may_have(s.id as usize,crate::effects::EffectKind::Venom)) && forecast.effects.at(s.id as usize,j).is(crate::effects::EffectKind::Venom)) || approaching_phase || movement_event || rival_phase_changed || surge_changed || phase_changed || j==1 || j%4==0 || j==horizon || (attack.valid && w.tick()+j as u64==attack.turn_at)
                || (w.config().rules==crate::RuleSet::V2 && forecast.motion(w,s.id as usize,rush,&motion,j+1)!=(speed,turn))
                || (if EFFECTS {forecast.effects.phase_bits(j+1)} else {0})!=previous_bits || (if EFFECTS {forecast.effects.surge_bits(j+1)} else {0})!=previous_surge
                || self.phase_sweeps[j/64]&(1u64<<(j%64))!=0;
            let from_index=if phase_changed || rival_phase_changed {j-1} else {checked};
            let span=(j-from_index) as f64*STEP_SECONDS;
            // Tracking strikes continue straight after spending their charge.
            // Applying a maximum-turn sagitta to that known straight exit made
            // a fast holder's harmless stump look almost a head-radius wider.
            // Rival envelopes and the ordinary body reserve remain in force.
            let straight_exit=state.venom_target!=0 && tracks_goal && bite_exit>0 && from_index>=bite_exit;
            let padding=if straight_exit {0.0} else {speed*turn*span*span/8.0};
            // An old neck sample becomes corporeal after actual head travel,
            // including burst/effect expiry; future maximum speed is unrelated.
            self.planning_speed[s.id as usize]=self.rollout_distance[j]/t;
            if sweep {
                let from=c.path[from_index];
                checked=j;
                let (hit,clearance,capped)=if EFFECTS && forecast.effects.may_have(s.id as usize,crate::effects::EffectKind::Venom) {
                    self.body_blocked_effects_from::<true>(w,s,from,p,t,padding,&mut tests,phased,&mut forecast.effects,from_index,Some(rivals))
                } else {self.cached_body_blocked_phase_flags::<EFFECTS>(w,s,from,p,t,padding,&mut tests,phased,&forecast.effects)};
                c.clearance=c.clearance.min(clearance); c.capped|=capped;
                if hit {break;}
            }
            let mut hit=false;
            let mut active=if sweep && (j==1 || state.revise_opponents) {near & !defeated} else {0};
            while active!=0 {
                let other=active.trailing_zeros() as usize;active&=active-1;
                if EFFECTS && !forecast.effects.contact(s.id as usize,other,j) {continue;}
                let dynamic=ITEMS && rivals[other].dynamic;
                let r=if dynamic {&rivals[other]} else {&self.rivals[other]};
                let winning=c.body_len>=r.len+if w.config().rules==crate::RuleSet::V2 {4} else {6};
                let physical_reach=(s.radius+r.radius)*0.82;
                let reach=physical_reach*(if EFFECTS {forecast.effects.reach_scale(other,j)} else {1.0});
                // A bounded turn envelope widens with time, but not into an
                // arbitrary reachable disk that would paralyze all pursuit.
                let envelope_scale=if dynamic {1.0} else {(if EFFECTS {forecast.effects.reach_scale(other,j)} else {1.0})/(if EFFECTS {self.effects.reach_scale(other,j)} else {1.0})};
                let (rival_speed,rival_turn)=if dynamic {forecast.motion(w,other,0.0,&self.motion[other],j)} else {self.rival_limits[other][j]};
                let envelope=r.envelope[j]*envelope_scale+padding+rival_speed*rival_turn*span*span/8.0;
                let head_near=w.distance_squared(p,r.path[j])<spatial::query_radius(reach,envelope+3.0,1.0,self.rollout_distance[j]-self.rollout_distance[from_index]+r.distance[j]-r.distance[from_index]).powi(2);
                if !winning && head_near && w.segments_distance_squared(c.path[from_index],p,r.path[from_index],r.path[j])<(reach+envelope+3.0).powi(2) {hit=true;break;}
                let lethal_head=winning && head_near && Self::head_contact(w,&c.path,&r.path,from_index.max(phase_start).min(j-1),j,physical_reach);
                // Future rival neck deposition is lethal even for a winner.
                // Skip the last head-sized piece: that is the head contest.
                if sweep && r.distance[j]>physical_reach+5.0 && w.distance_squared(p,r.path[0])<spatial::query_radius(reach,2.0,1.0,r.distance[j]+self.rollout_distance[j]-self.rollout_distance[from_index]).powi(2) {
                    // Actual forecast travel, including pickup/expiry speed
                    // changes, determines the exempt head-sized neck piece.
                    let newest=r.distance[j]-(physical_reach+5.0);
                    let neck_end=if dynamic {r.distance[..j].partition_point(|&distance|distance<=newest).saturating_sub(1)} else {self.rival_necks[other][j] as usize};
                    // All candidates and hunters see the same observed rival
                    // trail. Build its prefix bounds once per tick, on demand.
                    // The full-width reach bounds every tapered edge below;
                    // one pixel of slack keeps rejection conservative at the
                    // floating-point boundary. Wrapped/dynamic paths retain
                    // the original exact scan.
                    let mut separated=false;
                    if w.config().deadly_walls && !dynamic && neck_end>0 {
                        let start=self.rival_bounds_horizons[other];
                        if start==0 {self.rival_bounds[other][0]=TrailBounds::point(r.path[0]);}
                        for k in start+1..=neck_end {
                            self.rival_bounds[other][k]=self.rival_bounds[other][k-1].include(r.path[k]);
                        }
                        self.rival_bounds_horizons[other]=start.max(neck_end);
                        separated=self.rival_bounds[other][neck_end].separated(c.path[from_index],p,reach+3.0);
                    }
                    #[cfg(test)] let separated=separated && !self.reference_queries;
                    if !separated {for k in (1..=neck_end).step_by(3) {
                        let end=(k+2).min(neck_end);
                        // Full-width broad phase before any tapered tail powf.
                        let reserve=spatial::query_radius(reach,2.0,1.0,self.rollout_distance[j]-self.rollout_distance[from_index]+r.distance[end]-r.distance[k-1]);
                        if w.distance_squared(p,r.path[end])>=reserve*reserve {continue;}
                        let reach=if w.config().rules==crate::RuleSet::V2 {
                            let length=r.len.saturating_sub(1).max(1) as f64*r.radius*1.18;
                            let age=r.distance[j]-r.distance[end];
                            if age>length {continue;}
                            let body=crate::world::taper::span_radius(r.radius,
                                age/length,(r.distance[j]-r.distance[k-1])/length);
                            crate::world::taper::contact_radius(w.config().rules,s.radius,body,false)
                        } else {reach};
                        let reach=reach*(if EFFECTS {forecast.effects.reach_scale(other,j)} else {1.0});
                        if w.distance_squared(p,r.path[end])<spatial::query_radius(reach,2.0,1.0,self.rollout_distance[j]-self.rollout_distance[from_index]+r.distance[end]-r.distance[k-1]).powi(2)
                            && w.segments_distance_squared(c.path[from_index],p,r.path[k-1],r.path[end])<(reach+2.0).powi(2) {hit=true;break;}
                    }}
                }
                if hit {break;}
                // Check the deposited neck before dropping later geometry:
                // an earlier body contact cannot be rescued by a later kill.
                if lethal_head {defeated|=1<<other;}
            }
            if hit {break;}
            if w.config().self_collisions && !phased {
                #[cfg(test)] let age=if self.reference_queries {
                    (s.radius*1.18*10.0/(if attack.valid || w.config().rules==crate::RuleSet::V2 {maximum_speed} else {speed})/STEP_SECONDS).ceil() as usize
                } else {self_neck_age};
                #[cfg(not(test))] let age=self_neck_age;
                if sweep && j>age {
                    let full=s.radius*1.48+3.0+max_curve*(4.0*STEP_SECONDS).powi(2)/8.0;
                    let from=c.path[from_index];
                    let from_projection=(from.x-c.path[0].x)*forward.x+(from.y-c.path[0].y)*forward.y;
                    // Every older deposited edge is behind this prefix maximum
                    // on the initial heading's axis. If the entire current
                    // sweep is more than the full contact reserve ahead, no
                    // edge can collide. The 1px slack dwarfs floating-point
                    // projection error; seam wrapping retains the exact scan.
                    let mut separated=w.config().deadly_walls
                        && projection.min(from_projection)-self.rollout_projection[j-age]>full+1.0;
                    if !separated && w.config().deadly_walls {
                        // Curved paths may leave the old prefix on either
                        // world axis while making little forward progress.
                        // Grow its conservative bounds only when needed.
                        for k in self_bounds_horizon+1..=j-age {self_bounds=self_bounds.include(c.path[k]);}
                        self_bounds_horizon=j-age;
                        separated=self_bounds.separated(from,p,full+1.0);
                    }
                    #[cfg(test)] let separated=separated && !self.reference_queries;
                    if !separated {for k in (0..j-age).step_by(3) {
                        let end=(k+3).min(j-age);
                        if self.rollout_distance[j]-self.rollout_distance[end]<s.radius*1.18*10.0 {continue;}
                        let reserve=spatial::query_radius(full,0.0,1.0,self.rollout_distance[j]-self.rollout_distance[from_index]+self.rollout_distance[end]-self.rollout_distance[k]);
                        if w.distance_squared(p,c.path[end])>=reserve*reserve {continue;}
                        let body=if w.config().rules==crate::RuleSet::V2 {
                            let length=c.body_len.saturating_sub(1).max(1) as f64*s.radius*1.18;
                            if self.rollout_distance[j]-self.rollout_distance[end]>length {continue;}
                            crate::world::taper::span_radius(s.radius,
                                (self.rollout_distance[j]-self.rollout_distance[end])/length,
                                (self.rollout_distance[j]-self.rollout_distance[k])/length)
                        } else {s.radius};
                        let reach=crate::world::taper::contact_radius(w.config().rules,s.radius,body,true)+3.0+max_curve*(4.0*STEP_SECONDS).powi(2)/8.0;
                        if w.distance_squared(p,c.path[end])<spatial::query_radius(reach,0.0,1.0,self.rollout_distance[j]-self.rollout_distance[from_index]+self.rollout_distance[end]-self.rollout_distance[k]).powi(2)
                            && w.segments_distance_squared(c.path[from_index],p,c.path[k],c.path[end])<reach*reach {hit=true;break;}
                    }}
                }
            }
            if hit {break;}
            if sweep {c.steps=j;}
        }
        c.venom_bite=forecast.effects.consumed_at[s.id as usize];
        c.effects=forecast.effects.snapshot();
        let own_effects=forecast.effects.track(s.id as usize);
        let endpoint=c.path[c.steps];
        let old=w.distance_squared(c.path[0],goal).sqrt();
        let old=goal_contact.map_or(old,|contact|contact.distance(old,1));
        let mut closest=w.distance_squared(endpoint,goal);
        let first_effect=own_effects.before(1);
        let first_reach=goal_contact.map_or(0.0,|contact|contact.reach_with(first_effect));
        let mut initial=f64::INFINITY;let mut changed=f64::INFINITY;
        if goal_contact.is_some_and(|contact|contact.reach_with(own_effects.before(c.steps.max(1)))!=first_reach) {changed=closest;}
        else {initial=closest;}
        for j in ((if goal_contact.is_some() {4} else {0})..=c.steps).step_by(4) {
            let distance=w.distance_squared(c.path[j],goal);closest=closest.min(distance);
            if goal_contact.is_some_and(|contact|contact.reach_with(own_effects.before(j.max(1)))!=first_reach) {changed=changed.min(distance);}
            else {initial=initial.min(distance);}
        }
        let alternate=if first_effect.is(crate::effects::EffectKind::Magnet) {forecast::Effect::default()}
            else {forecast::Effect {kind:crate::effects::EffectKind::Magnet as u8,ticks:1}};
        let new=goal_contact.map_or(closest.sqrt(),|contact|contact.distance_with(initial.sqrt(),first_effect).min(contact.distance_with(changed.sqrt(),alternate)));
        let captured=goal_contact.is_some() && reached;
        let new=if captured {0.0} else {new};
        let diff=normalize_angle(c.desired-s.angle);
        let sign=if diff>0.07 {1} else if diff< -0.07 {-1} else {0};
        let hysteresis=if sign!=0 && state.turn_sign!=0 && sign!=state.turn_sign {
            if w.tick().saturating_sub(state.last_turn_tick)<18 {320.0} else {55.0}
        } else {0.0};
        let heading=w.displacement(endpoint,goal);
        let aligned=goal_contact.map_or(closest<(s.radius*2.7).powi(2),|_|captured);
        let alignment=if aligned {1.0} else {normalize_angle(heading.y.atan2(heading.x)-c.angle).cos()};
        c.score=(old-new)*0.9+alignment*12.0+c.clearance.clamp(-10.0,50.0)*0.22
            -diff.abs()*4.0+diff*s.traits.turn_bias*3.0-hysteresis-normalize_angle(c.desired-state.desired).abs()*2.0;
        if ITEMS && w.config().rules==crate::RuleSet::V2 {
            c.score+=frost::rollout_value(w,&forecast.effects,s.id as usize);
        }
        // A checked ripe capture must outweigh ordinary steering continuity;
        // otherwise a vulture keeps its old tangent after the countdown ends.
        // Viability and area still rank ahead of every utility score.
        if w.config().rules==crate::RuleSet::V2 && captured && self.target_food(state).is_some_and(|f|matches!(f.kind,crate::FoodKind::Prism|crate::FoodKind::PrismSeed)) {
            c.score+=600.0;
        }
        if w.config().rules==crate::RuleSet::V2 && captured && state.target!=0 {
            c.score+=aggression::bold(w)*if state.target & target::ITEM_BIT!=0 {400.0} else {500.0};
        }
        if state.venom_target!=0 || s.effect_kind==crate::effects::EffectKind::Venom as u8 {
            // Reward every certified bite, including a profitable opportunity
            // encountered on the approach. Earlier strikes reduce the chance
            // that a defender turns, the trail moves away or Venom expires.
            // Safety horizon and escape area still rank before this utility.
            for (victim,&cut) in forecast.effects.sever_cut.iter().enumerate() {
                if cut>0 && c.steps>=c.venom_bite+VENOM_EXIT_STEPS && !c.capped {c.score+=800.0+(self.rivals[victim].len-cut) as f64*2.0
                    +horizon.saturating_sub(forecast.effects.bite_step[victim]) as f64*12.0;}
            }
        } else if state.prey!=0 {
            let prey=self.opportunity_rival(state.prey-1);
            let mut intercept=f64::MAX;let mut cutoff=f64::MAX;
            for j in (4..=c.steps).step_by(4) {
                if !(EFFECTS && forecast.effects.phased(s.id as usize,j)) && !self.opportunities.phased(state.prey-1,j) {
                    intercept=intercept.min(w.distance_squared(c.path[j],prey.path[j]));
                }
                // Placing a neck 0.4--0.8s ahead of the target leaves a body
                // barrier rather than a tail chase. Both still need safety.
                for lead in [12,24] {
                    let future=(j+lead).min(STEPS);
                    if !forecast.effects.phased(s.id as usize,future) && !self.opportunities.phased(state.prey-1,future) {
                        cutoff=cutoff.min(w.segment_distance_squared(prey.path[future],c.path[j-4],c.path[j]));
                    }
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
                    if !forecast.effects.phased(s.id as usize,36) && !self.opportunities.phased(state.prey-1,36) && w.segments_distance_squared(prey.path[36],exit,c.path[k-4],c.path[k])<((s.radius+prey.radius)*1.15).powi(2) {blocks=true;break;}
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
            c.score-=30.0*2.5*w.boost_segment_cost(s.id as usize).unwrap_or(0) as f64/2.0;
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
fn state_item_slot(ai:&AiController,f:target::TargetFood)->usize {
    ai.food[MAX_FOOD..].iter().position(|entry|entry.is_some_and(|v|v.id==f.id)).unwrap_or(0)
}
impl Controller for AiController {
    fn face_intent(&self,id:u32)->crate::controller::FaceIntent {
        let s=self.states[id as usize];
        crate::controller::FaceIntent {target_id:if s.target & target::ITEM_BIT!=0 {s.target & !target::ITEM_BIT} else if self.target_food(s).is_some_and(|f|matches!(f.kind,crate::FoodKind::Prism|crate::FoodKind::PrismSeed)) {s.target | target::ITEM_BIT} else {0},
            prey:s.prey.checked_sub(1).map_or(u32::MAX,|i|i as u32),guarding:s.guarding,has_target:s.target!=0 || s.prey!=0,
            look:self.look_deltas[id as usize]}
    }
    fn intent_flags(&self, id: u32) -> Option<u32> {
        let s = self.states[id as usize];
        Some((if s.prey != 0 || s.coil_radius > 0.0 { crate::flags::HUNTING } else { 0 })
            | if s.debug.flags & 32 != 0 { crate::flags::TRAPPED } else { 0 })
    }
    fn steer(&mut self,w:&World,s:SnakeView<'_>) -> Steering {
        let s=surge::planner_view(w,aggression::planner_view(w,s));
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
        #[cfg(feature="desktop-diag")]
        let mut observation=DesktopObservation {generation:s.generation,..Default::default()};
        #[cfg(feature="desktop-diag")] {state.abandon=AbandonReason::None;}
        let mut venom=None;
        if state.venom_target!=0 && (s.effect_kind!=4 || s.effect_ticks==0) {self.venom_tactics_cached(w,s,&mut state,&mut venom);}
        // Forecast selection is performed in every rollout and has its own
        // cadence. Tactical revision waits for an actual strategy slot.
        state.revise_opponents=w.tick()>=state.next_forecast;
        if state.revise_opponents {state.next_forecast=w.tick()+State::response_interval(s);}
        // Finalists are scratch for this decision, never reusable life state.
        state.attack_options=[Attack::default();2];
        self.validate_attack(w,s,&mut state);
        if state.prey!=0 && w.snake(state.prey-1).is_none_or(|v|!v.alive || self.opportunities.phased(v.id as usize,1) || v.generation!=state.prey_generation) {
            // Identity/liveness are immediate observations, not tactical delay.
            if w.snake(state.prey-1).is_some_and(|v|!v.alive && v.generation==state.prey_generation) {state.harvest_until=w.tick()+60;state.goal=state.coil_center;
            }
            state.prey=0;state.clear_attacks(s.angle);state.clear_coil(s.angle);state.waypoint=None;state.track_goal=false;
        }
        let had_coil=state.coil_radius>0.0;
        Self::advance_spiral(w,&mut state,s.segments[0].current);
        if had_coil && !self.pocket_usable(w,s,state) {
            state.clear_coil(s.angle);
            state.goal=w.canonical_point(Point{x:s.segments[0].current.x+s.angle.cos()*300.0,y:s.segments[0].current.y+s.angle.sin()*300.0});
        }
        let planned_pounce=self.prism.is_some_and(|(index,f)|f.kind==crate::FoodKind::Prism
            && f.id==state.target && f.vacuum_owner<0
            && w.tick()<=w.food[index].ripe_tick.saturating_add(120));
        let turned=normalize_angle(s.angle-state.last_angle);
        state.last_angle=s.angle;
        // Guard turns are intentional just like coils. Do not carry their
        // accumulated rotation into the direct approach when the guard ends.
        if state.guarding || state.vulturing || state.venom_target!=0 || state.venom_standoff || planned_pounce
            || (aggression::bold(w)>0.0 && (state.prey!=0 || (state.target!=0 && w.tick().saturating_sub(state.last_progress)<30))) {state.turn_accum=0.0;}
        else if turned.abs()<0.015 {state.turn_accum*=0.90;}
        else if turned*state.turn_accum<0.0 {state.turn_accum=turned;}
        else {state.turn_accum+=turned;}
        if state.turn_accum.abs()>2.8 && w.tick()>=state.orbit_until && state.coil_radius==0.0 && !state.guarding && !state.vulturing && state.venom_target==0 && !state.venom_standoff && !planned_pounce {
            #[cfg(feature="desktop-diag")] {state.abandon=AbandonReason::Recovery;observation.recovery_started=true;observation.recovery_target=state.target;observation.recovery_progress_age=w.tick().saturating_sub(state.last_progress);}
            state.rejected=state.target;state.reject_until=w.tick()+150;state.set_target(0);state.waypoint=None;
            state.orbit_until=w.tick()+36;state.escape_until=0;state.turn_accum=0.0;state.prey=0;state.clear_attacks(s.angle);
            state.desired=s.angle;state.turn_until=u64::MAX;state.track_goal=false;state.commit_until=w.tick()+12;
            state.goal=w.canonical_point(Point{x:s.segments[0].current.x+s.angle.cos()*300.0,y:s.segments[0].current.y+s.angle.sin()*300.0});
        }
        state.debug.flags&=8;
        if state.target!=0 {
            if let Some((fi,f))=self.target_slot(state) {
                state.target_index=fi;state.goal=f.position;
                let distance=w.distance_squared(s.segments[0].current,f.position);
                let contact=target::Contact::forecast(s,f,self.opportunities.track(s.id as usize));
                let d=contact.distance(distance.sqrt(),1);
                if d+8.0<state.best_distance {state.best_distance=d;state.last_progress=w.tick();}
                let progress_budget=if w.config().rules==crate::RuleSet::V2 && f.kind==crate::FoodKind::Prism {120} else {75};
                if !state.guarding && !state.vulturing && !planned_pounce && w.tick().saturating_sub(state.last_progress)>progress_budget {
                    #[cfg(feature="desktop-diag")] {state.abandon=AbandonReason::Stall;observation.progress_rejected=true;}
                    state.rejected=state.target;state.reject_until=w.tick()+150;state.set_target(0);
                }
                if !state.guarding && !state.vulturing && state.track_goal && (contact.collected() || contact.with_guard(state.guarding).ahead(w,s.segments[0].current,s.angle,w.motion_limits(id,Self::boost_request(w,s,state.rush)).unwrap().0*STEP_SECONDS,f.position)) {
                    state.set_target(0);state.track_goal=false;state.desired=s.angle;state.turn_until=u64::MAX;
                }
                if state.waypoint.is_some_and(|p|w.distance_squared(s.segments[0].current,p)<self.spatial.dx.powi(2)*0.5) {state.waypoint=None;}
            } else {
                #[cfg(feature="desktop-diag")] {state.abandon=AbandonReason::Expiry;}
                state.set_target(0);state.waypoint=None;}
        }
        // Resume only after recovery and tactical goals finish. Those goals
        // have priority even when they terminate the escape early.
        if w.config().rules==crate::RuleSet::V2 && w.tick()>=state.escape_until && w.tick()>=state.orbit_until
            && state.prey==0 && state.coil_radius==0.0 && !state.guarding && !state.vulturing
            && state.venom_target==0 && !state.venom_standoff && w.tick()>=state.harvest_until
            && w.tick()>=state.dodge_until && state.target==0 && state.suspended_target!=0 {
            let target=state.suspended_target;state.suspended_target=0;
            let suspended=State {target,target_index:state.suspended_index,..state};
            if let Some((fi,f))=self.target_slot(suspended) {
                state.set_target(f.id);state.target_index=fi;state.goal=f.position;
                state.best_distance=f64::MAX;state.last_progress=w.tick();
                state.waypoint=None;
            }
        }
        if w.tick()<state.escape_until {
            let need=(s.segments.len() as f64*s.radius*s.radius*8.0/(self.spatial.dx*self.spatial.dy)).ceil().max(24.0) as usize;
            let (room,_)=self.spatial.space(s.segments[0].current,self.mask(w,id),(need*2).max(64),0.0);
            if room>=need {
                state.escape_until=0;state.waypoint=None;state.desired=s.angle;state.turn_until=u64::MAX;state.track_goal=false;
                state.commit_until=w.tick()+12;
                state.goal=w.canonical_point(Point{x:s.segments[0].current.x+s.angle.cos()*300.0,y:s.segments[0].current.y+s.angle.sin()*300.0});
            } else {state.goal=s.segments.last().unwrap().current;state.waypoint=None;}
        }
        if w.config().rules==crate::RuleSet::V2 {
            let race_clock=self.profile_enabled.then(std::time::Instant::now);
            self.advance_race(w,s,&mut state);
            if let Some(c)=race_clock {self.race_profile[1]+=c.elapsed().as_nanos();}
        }
        let scheduled=(id+w.snake_count()-(w.tick() as usize*STRATEGY_QUOTA)%w.snake_count())%w.snake_count()<STRATEGY_QUOTA;
        let mut strategic=scheduled;
        if strategic {self.strategy_with_venom(w,s,&mut state,intelligence,&mut venom);}
        // A capsule contact can occur before the next strategy slot. Near
        // Frost, refresh the instant-Nova decision on each movement so an
        // empty pickup or a closer rival does not outrun tactical cadence.
        else if w.items().any(|item|item.kind==crate::effects::EffectKind::Frost
            && w.distance_squared(s.segments[0].current,item.position)<(16.0*w.config().base_radius()).powi(2)) {
            self.frost_tactics(w,s,&mut state);
        }
        // Acquisition, expiry and moving bite points cannot wait for a food
        // strategy slot. Holder work is bounded and absent on ordinary ticks.
        if s.effect_kind==4 || state.venom_target!=0 {self.venom_tactics_cached(w,s,&mut state,&mut venom);}
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
        let horizon=phase::horizon(w,s,if extended {STEPS} else {NORMAL_STEPS});
        let strategy_time=clock.map(|c|c.elapsed().as_nanos());
        if let Some(t)=strategy_time {self.profile[1]+=t-prepared.unwrap();}
        // Temporarily lease the preallocated scratch so rollout methods can
        // also mutably access shared caches. No allocation or array reset.
        let mut candidates=self.candidates.take().expect("non-reentrant controller");
        let mut area_alias=[None;CANDIDATES];
        self.rollout_into(w,s,state,1,horizon,&mut candidates[1]);
        if !strategic && (candidates[1].steps<horizon || (state.target==0 && state.prey==0 && w.tick().saturating_sub(state.last_strategy)>6)) && self.urgent_used<URGENT_QUOTA {
            self.urgent_used+=1;strategic=true;
            self.strategy_with_venom(w,s,&mut state,intelligence,&mut venom);
            state.rush=self.rush_for(w,s,state);
            self.planning_speed[id]=w.motion_limits(id,state.rush).unwrap().0;
            self.rollout_into(w,s,state,1,horizon,&mut candidates[1]);
        }
        let need=(s.segments.len() as f64*s.radius*s.radius*8.0/(self.spatial.dx*self.spatial.dy)).ceil().max(24.0) as usize;
        // Reuse a space UTILITY hint between scheduled strategy slots. Every
        // retained physical rollout is still checked against current geometry.
        // A cached hint is unresolved, never a continuation certificate.
        let area_hint=w.config().rules==crate::RuleSet::V2 && !strategic
            && candidates[1].steps==horizon && state.area_hint>=need
            && state.area_hint_body==s.segments.len() && state.area_hint_radius==s.radius
            && w.tick()>=state.escape_until && w.tick()>=state.orbit_until;
        let (area,capped)=if area_hint {(state.area_hint,true)} else if enclosure_risk && strategic {
            let rate=s.radius*1.18/(w.motion_limits(id,state.rush).unwrap().0*STEP_SECONDS).max(0.1);
            self.spatial.trajectory_space(candidates[1].path[candidates[1].steps],self.candidate_mask(w,id,&candidates[1]),(need*2).max(64),candidates[1].effects.space_time(self.candidate_mask(w,id,&candidates[1]),candidates[1].steps as f64*STEP_SECONDS,&self.rivals),
                &candidates[1].path[..=candidates[1].steps],(rate*10.0).ceil() as usize,(candidates[1].effects.body_time(id,s.segments.len() as f64*self.rivals[id].release_rate+self.rivals[id].growth_delay,&self.rivals[id])/STEP_SECONDS).ceil() as usize)
        } else {self.spatial.space(candidates[1].path[candidates[1].steps],self.candidate_mask(w,id,&candidates[1]),(need*2).max(64),candidates[1].effects.space_time(self.candidate_mask(w,id,&candidates[1]),candidates[1].steps as f64*STEP_SECONDS,&self.rivals))};
        candidates[1].area=area;candidates[1].uncertain=capped;
        // Revalidate the retained plan against CURRENT bodies and forecasts.
        // An open, checked continuation can wait for its scheduled strategy
        // slot; danger, a missing target or a pocket triggers full search.
        let head=s.segments[0].current;
        let wall_room=!w.config().deadly_walls || head.x.min(w.config().width-head.x).min(head.y).min(w.config().height-head.y)
            >self.rivals[id].speed/self.rivals[id].turn.max(0.01)*3.0+s.radius*3.0;
        let reuse_plan=!strategic && wall_room && candidates[1].steps==horizon
            && (candidates[1].venom_bite==0 || (!candidates[1].capped && candidates[1].steps>=candidates[1].venom_bite+VENOM_EXIT_STEPS))
            && (area>=need || w.tick()<state.commit_until);
        if !reuse_plan || self.diagnostic_enabled {
            for kind in 0..CANDIDATES {
                if kind==1 {continue;}
                if (kind==9 || kind==10) && !extended {candidates[kind]=candidates[2];area_alias[kind]=Some(2);continue;}
                if kind==11 && (w.config().rules==crate::RuleSet::Classic || state.rush==0.0) {candidates[kind]=candidates[0];area_alias[kind]=Some(0);continue;}
                if kind==12 && (w.config().rules!=crate::RuleSet::V2 || !w.boost_ready(id) || (state.venom_target==0 && candidates[1].steps>=18)) {candidates[kind]=candidates[2];area_alias[kind]=Some(2);continue;}
                // Goal-tracking direct and retained controls integrate the
                // same trajectory when neither proposes an attack. Preserve
                // the direct slot's control metadata and its own area query.
                #[cfg(test)] let reuse_tracking=!self.reference_queries;
                #[cfg(not(test))] let reuse_tracking=true;
                let source=if kind==7 {5} else {6};
                if reuse_tracking && (kind==7 || kind==8) && state.venom_target==0
                    && !state.attack_options[kind-7].valid && candidates[source].checked
                    && candidates[source].simulated_steps<=16 && candidates[source].steps<horizon {
                    // These compound turns differ only AFTER movement 16.
                    // Reuse an exact failed prefix, retaining the compound
                    // slot's exit controls, tie order and independent area work.
                    candidates[kind]=candidates[source];candidates[kind].kind=kind;
                    candidates[kind].turn_until=w.tick()+16;
                    let turn=if w.config().rules==crate::RuleSet::V2 {self.motion[id].at(0).1}
                        else {w.motion_limits(id,candidates[kind].rush).unwrap().1};
                    let fixed=s.angle+if kind==7 {3.0} else {-3.0};
                    candidates[kind].exit_angle=if candidates[kind].simulated_steps==16 {candidates[kind].turn_exit}
                        else {normalize_angle(s.angle+normalize_angle(fixed-s.angle).clamp(-turn*STEP_SECONDS*16.0,turn*STEP_SECONDS*16.0))};
                } else if reuse_tracking && kind==0 && state.track_goal && !state.attack.valid
                    && candidates[1].tracks_goal && !candidates[1].attack.valid
                    && candidates[1].rush==Self::candidate_rush(w,s,state,kind,Attack::default()) {
                    candidates[kind]=candidates[1];
                    candidates[kind].kind=kind;candidates[kind].turn_until=u64::MAX;
                    candidates[kind].exit_angle=candidates[kind].turn_exit;
                    candidates[kind].area=0;candidates[kind].uncertain=false;
                } else if kind==2 && candidates[1].checked && !candidates[1].attack.valid && !state.track_goal && state.turn_until==u64::MAX && normalize_angle(state.desired-s.angle).abs()<1e-12
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
            } else if state.venom_target==0 {candidates[12]=candidates[2];area_alias[12]=Some(2);}
        }
        // A reused decision selects only slot 1. Leave other scratch slots
        // untouched instead of copying its full path into ten unused slots.
        if strategic && !reuse_plan {
            for kind in [7,8] {
                if candidates[kind].attack.valid && candidates[kind].steps==horizon {
                    let blocked=self.replies_blocked(w,s,state,&candidates[kind]);
                    // Correct Phase-aware replies are sparse. Value a real
                    // blocked response enough to compete with ordinary progress
                    // and commitment, after the same safety/area gates.
                    candidates[kind].replies=blocked;
                    // An observed Surge makes this already-checked response
                    // free; spend its short window on combat opportunities.
                    let seed_cutoff=self.prism.is_some_and(|(_,f)|f.kind==crate::FoodKind::PrismSeed
                        && w.distance_squared(self.rivals[state.prey-1].path[0],f.position)<(12.0*self.rivals[state.prey-1].radius).powi(2));
                    let response_value=if surge::active(w,s) && candidates[kind].attack.free_boost {325.0}
                        else if seed_cutoff {250.0} else {150.0};
                    candidates[kind].score+=blocked as f64*response_value*(1.0+aggression::bold(w))*s.traits.aggression-candidates[kind].attack.error*0.7;
                }
            }
        }
        let safe=candidates.iter().filter(|c|c.steps==horizon).count();
        // When every sampled continuation fails, a longer prefix must not
        // purchase travel into a wall band with neither turn left.
        // The per-axis wall reserve can choose a different escape direction
        // for each wall; a shared circle checks that one turn clears all walls.
        let mut wall_turn_room=[true;CANDIDATES];
        if safe==0 && w.config().deadly_walls {
            // A blocked prefix shorter than a half-turn cannot establish a
            // wall exit. Preserve the next inward turn in that emergency;
            // longer physically checked prefixes keep their body/area order.
            // Coupled corners always check, including Classic controls.
            let turn=self.motion[id].at(0).1;
            let immediate=w.config().rules==crate::RuleSet::V2
                && candidates.iter().all(|c|c.steps as f64*turn*STEP_SECONDS<std::f64::consts::PI);
            for (index,c) in candidates.iter().enumerate() {
                if !c.checked {continue;}
                wall_turn_room[index]=self.emergency_wall_turn_room(w,s,c,immediate);
            }
        }
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
                    self.spatial.trajectory_space(c.path[c.steps],self.candidate_mask(w,id,c),(need*2).max(64),c.effects.space_time(self.candidate_mask(w,id,c),c.steps as f64*STEP_SECONDS,&self.rivals),
                        &c.path[..=c.steps],(rate*10.0).ceil() as usize,(c.effects.body_time(id,s.segments.len() as f64*self.rivals[id].release_rate+self.rivals[id].growth_delay,&self.rivals[id])/STEP_SECONDS).ceil() as usize)
                } else {self.spatial.space(c.path[c.steps],self.candidate_mask(w,id,c),(need*2).max(64),c.effects.space_time(self.candidate_mask(w,id,c),c.steps as f64*STEP_SECONDS,&self.rivals))};
                c.area=area;c.uncertain=capped;
            }
            let (area,capped)=(c.area,c.uncertain);
            if !capped && area<need {c.score-=600.0+(need-area) as f64*8.0;}
            else {c.score+=area.min(need*2) as f64*0.08;}
        }
        if let Some(c)=clock {self.profile[3]+=c.elapsed().as_nanos()-rollout_time.unwrap();}
        candidates[1].score+=if state.prey!=0 {20.0} else {90.0}+if w.tick()<state.commit_until {if state.prey!=0 {65.0} else {160.0}} else {0.0};
        // Keep the measured physical prefix intact for emergency ranking and
        // diagnostics. A strike needs its checked exit for normal admission.
        let viability=|c:&Candidate|if c.steps==horizon && (c.venom_bite==0 || (!c.capped && c.steps>=c.venom_bite+VENOM_EXIT_STEPS)) {if c.area>=need || c.uncertain {2} else {1}} else {0};
        // A physical straight rollout always exists, even if every first step
        // collides. Never rank a pre-step tactical rejection as an escape.
        let best=if reuse_plan {1} else {(0..CANDIDATES).filter(|i|candidates[*i].checked).max_by(|a,b|viability(&candidates[*a]).cmp(&viability(&candidates[*b]))
            .then_with(||(wall_turn_room[*a] && candidates[*a].steps>0).cmp(&(wall_turn_room[*b] && candidates[*b].steps>0)))
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
                    attack_replies:c.replies,attack_error:c.attack.error,attack_valid:c.attack.valid,
                    attack_turn_ticks:if c.attack.valid {c.attack.turn_at.saturating_sub(w.tick()) as usize} else {0},
                    attack_crossing:if c.attack.valid {c.attack.crossing} else {c.desired},
                    attack_crossing_rush:if c.attack.valid {c.attack.crossing_rush} else {c.rush}};
            }
            self.decisions[id]=d;
        }
        #[cfg(feature="desktop-diag")] {
            observation.selected=best as u8;observation.reused_plan=reuse_plan;
            observation.safe_ticks=candidates[best].steps;observation.horizon=horizon;
            observation.continuation=candidates[best].steps==horizon && candidates[best].area>=need && !candidates[best].uncertain;
            observation.continuation_unresolved=candidates[best].uncertain;
            observation.any_first_step=if reuse_plan {candidates[1].steps>0} else {candidates.iter().any(|c|c.checked && c.steps>0)};
            observation.next_head=candidates[best].path[1];
            for rival in w.snakes().filter(|r|r.alive) {let rid=rival.id as usize;
                observation.rival_next[rid]=self.rivals[rid].path[1];
                observation.rival_generation[rid]=rival.generation;
            }
            let delta=w.displacement(s.segments[0].current,state.goal);
            observation.retained_drift=normalize_angle(delta.y.atan2(delta.x)-state.desired).abs();
            observation.reach=self.target_food(state).map_or(0.0,|f|target::Contact::forecast(s,f,self.opportunities.track(id)).reach(1));
            observation.direct_safe=!reuse_plan && candidates[0].steps==horizon;
            observation.direct_viable=!reuse_plan && viability(&candidates[0])==2;
            observation.retained_viable=viability(&candidates[1])==2;
            observation.direct_score=candidates[0].score;
            observation.selected_score=candidates[best].score;
            observation.strategic=strategic;
        }
        let c=&candidates[best];
        #[cfg(feature="desktop-diag")] let was_escape=w.tick()<state.escape_until;
        State::commit_candidate(&mut state,c,best,w,s,self.rivals[id].turn,need);
        state.area_hint=c.area;state.area_hint_body=s.segments.len();state.area_hint_radius=s.radius;
        #[cfg(feature="desktop-diag")] {observation.escape_started=!was_escape && w.tick()<state.escape_until;observation.abandon=state.abandon;}
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
        self.look_deltas[id]=w.displacement(s.segments[0].current,state.goal);
        #[cfg(feature="desktop-diag")] {
            observation.contested=self.contested_target(w,s,state);
            observation.fleeing=w.tick()<state.dodge_until;
            observation.cutoff=state.attack.valid || (state.barrier && state.track_goal && state.prey!=0 && w.tick()>=state.escape_until && w.tick()>=state.orbit_until);observation.cutoff_start=state.attack.start;
            observation.boosted=s.boost_ticks>0 || state.rush>0.0;
            observation.power_up=if s.effect_ticks>0 {s.effect_kind} else {0};
            observation.target=state.target;observation.goal=state.goal;
            observation.prey=state.prey as u32;observation.prey_generation=state.prey_generation;observation.venom_target=state.venom_target as u32;
            let objective=self.target_food(state);
            observation.target_kind=objective.map_or(0,|f|if f.id & target::ITEM_BIT!=0 {1} else if matches!(f.kind,crate::FoodKind::Prism|crate::FoodKind::PrismSeed) {2} else {0});
            observation.distance=w.distance_squared(s.segments[0].current,objective.map_or(state.goal,|f|f.position)).sqrt();
            observation.turn_accum=state.turn_accum;observation.tracking=state.track_goal;
            let d=w.displacement(s.segments[0].current,state.goal);
            observation.target_bearing=normalize_angle(d.y.atan2(d.x)-s.angle);
            observation.mode=if w.tick()<state.orbit_until {1} else if w.tick()<state.escape_until {2}
                else if state.coil_radius>0.0 {3} else if state.guarding {4} else if state.vulturing {5}
                else if state.venom_standoff {6} else if state.venom_target!=0 {7}
                else if state.prey!=0 {8} else if state.target & target::ITEM_BIT!=0 {9}
                else if self.target_food(state).is_some_and(|f|matches!(f.kind,crate::FoodKind::Prism|crate::FoodKind::PrismSeed)) {10}
                else if state.target!=0 {11} else {0};
            self.desktop[id]=observation;
        }
        self.states[id]=state;
        self.candidates=Some(candidates);
        if let Some(c)=clock {self.profile[4]+=c.elapsed().as_nanos();}
        crate::effects::ai_steering(w,s,Steering {desired_angle:state.desired,rush:Self::boost_request(w,s,state.rush)})
    }
}

#[cfg(test)]
mod tests;
