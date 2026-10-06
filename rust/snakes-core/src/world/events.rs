// SPDX-License-Identifier: GPL-3.0-or-later
//! Independent V2 schedules and a fixed, precomputed meteor flight plan.
//! All event randomness is consumed at schedule/start time, never in flight.
use super::*;

pub(crate) const ZONE_ID:u64=1<<62;
pub const METEORS:usize=24;
pub const TELEGRAPH:u64=45;
pub const FLIGHT:u64=14;
pub const STARFALL_LIFE:u64=TELEGRAPH+60+FLIGHT;
pub const NIGHT_LIFE:u64=750;
#[inline]
pub(crate) fn night_intensity(tick:u64,start:u64)->f32 {
    if tick<start {return 0.0;}
    let age=tick-start;
    if age>=NIGHT_LIFE {0.0} else {(age as f32/90.0).min((NIGHT_LIFE-age) as f32/90.0).min(1.0)}
}

#[derive(Clone,Copy,Debug,Default)]
pub struct EventStats {
    pub starfalls:u32,pub nightfalls:u32,pub participants:u32,pub stars_eaten:u32,pub frenzy_kills:u32,pub zone_kills:u32,pub commitments:u32,
}
#[derive(Clone,Copy,Default)]
pub(super) struct MeteorPlan {landing:Point,phase:f64}
#[derive(Clone,Copy,Default)]
pub(super) struct Events {
    pub star_due:u64,pub night_due:u64,pub star_start:Option<u64>,pub night_start:Option<u64>,
    pub race_ids:[u32;2],pub race_etas:[f64;2],pub race_leader:u32,
    feeding_until:u64,zone:Point,zone_radius:f64,
    plans:[MeteorPlan;METEORS],diagonal:Point,emitted:usize,participants:u16,commitments:u16,pub stats:EventStats,
}
/// Mechanics and every food-contact forecast use this same endpoint rule.
#[inline]
pub(crate) fn food_pickup_eligible(kind:FoodKind,tick:u64,ready:u64,captured:u16)->bool {
    kind!=FoodKind::Meteor && captured==0 && Item::endpoint_eligible(tick,ready,0,usize::MAX,true)
}
impl World {
    pub(super) fn events_enabled(&self)->bool {self.config.rules==RuleSet::V2 && self.config.world_events}
    fn schedule_star(&mut self,tick:u64) {self.event_schedule.star_due=tick+5400+(self.rng.random()*3601.0) as u64;}
    fn schedule_night(&mut self,tick:u64) {self.event_schedule.night_due=tick+12600+(self.rng.random()*5401.0) as u64;}
    pub(super) fn reset_events(&mut self) {
        self.event_schedule=Events::default();self.world_event=WorldEventState::default();
        if self.events_enabled() {self.schedule_star(self.tick);self.schedule_night(self.tick);}
    }
    pub(super) fn clear_world_events(&mut self) {
        self.event_schedule=Events::default();self.world_event=WorldEventState::default();
        self.food.retain(|f|!matches!(f.kind,FoodKind::Meteor|FoodKind::Star));
        for f in &mut self.faces {if f.target_id==ZONE_ID {f.target_id=0;f.target_ticks=0;f.has_target=false;}}
    }
    pub fn world_event_stats(&self)->EventStats {self.event_schedule.stats}
    /// The shower is brief, but its feeding objective lasts while stars remain.
    pub(crate) fn starfall_target(&self)->Option<(Point,f64,u64)> {
        (self.tick<self.event_schedule.feeding_until).then_some((self.event_schedule.zone,
            self.event_schedule.zone_radius,self.event_schedule.feeding_until))
    }
    #[inline]
    pub(crate) fn forecast_night_start(&self)->Option<u64> {
        self.event_schedule.night_start.or_else(||(self.events_enabled() && self.event_schedule.night_due!=0).then_some(self.event_schedule.night_due))
    }
    pub(crate) fn forecast_night(&self,offset:usize)->f32 {
        let tick=(self.tick+1).saturating_add(offset as u64);
        let start=if let Some(start)=self.event_schedule.night_start {start}
            else if self.events_enabled() && self.event_schedule.night_due!=0 && tick>=self.event_schedule.night_due {self.event_schedule.night_due}
            else {return self.world_event.night;};
        night_intensity(tick,start)
    }
    pub(crate) fn starfall_landed(&self)->bool {
        self.tick>=self.event_schedule.feeding_until.saturating_sub(STARFALL_LIFE+900)+TELEGRAPH+FLIGHT
    }
    pub(crate) fn event_race_flags(&self,id:u32)->u8 {
        if self.world_event.kind!=1 || self.faces[id as usize].target_id!=ZONE_ID {return 0;}
        let eta=self.event_schedule.race_etas;let contested=eta.iter().all(|x|x.is_finite()) && eta[0].max(eta[1])<=eta[0].min(eta[1])*1.18;
        8 | if self.event_schedule.race_ids[0]==id {16} else {0} | if self.event_schedule.race_ids[1]==id {32} else {0}
            | if self.event_schedule.race_leader==id {64} else {0} | if contested {128} else {0}
    }
    /// Measurement/test hook: schedule a real event, keeping the production
    /// placement, RNG, motion and AI paths. Does not create synthetic food.
    pub fn diagnostic_event_schedule(&mut self,star_in:u64,night_in:u64) {
        self.event_schedule.star_due=self.tick+star_in.max(1);self.event_schedule.night_due=self.tick+night_in.max(1);
    }
    fn world_event_marker(&mut self,kind:u32,start:bool,duration:u16) {
        self.push_event(FrameEvent {tick:self.tick+1,position:Point{x:self.world_event.x as f64,y:self.world_event.y as f64},
            snake_id:u32::MAX,other_snake_id:kind,kind:EventKind::WorldEvent,
            duration_ticks:if start {duration} else {0},release_tick:if start {self.tick+1+duration as u64} else {self.tick+1},
            ..FrameEvent::default()});
    }
    fn starfall_location(&mut self)->Option<Point> {
        let r=self.config.base_radius();let radius=12.0*r;
        // Reserve an exit lane beyond the landing disk at deadly walls.
        let margin=if self.config.deadly_walls {radius+6.0*r} else {0.0};
        if self.config.width<2.0*margin || self.config.height<2.0*margin {return None;}
        let live=self.snakes.iter().filter(|s|s.alive).count();if live<2 {return None;}
        let mut best=None;let mut score=f64::NEG_INFINITY;
        for attempt in 0..48 {
            let x=self.rng.random();let y=self.rng.random();
            let (id,s)=self.snakes.iter().enumerate().filter(|(_,s)|s.alive).nth(((x*live as f64) as usize).min(live-1)).unwrap();
            let head=self.segments[id*MAX_SEGMENTS].current;
            let speed=self.motion_limits(id,0.0).unwrap().0;
            let horizon=1.5+0.5*y;
            let approach=Point{x:head.x+s.angle.cos()*speed*horizon,y:head.y+s.angle.sin()*speed*horizon};
            // Pair projected approaches, not remote heads: the telegraph is
            // only 1.5 seconds, and one diagonal feeds a whole disk of prizes.
            let partner=self.snakes.iter().enumerate().filter(|(other,s)|*other!=id && s.alive).min_by(|(a,sa),(b,sb)| {
                let pa=self.segments[*a*MAX_SEGMENTS].current;let pb=self.segments[*b*MAX_SEGMENTS].current;
                let va=self.motion_limits(*a,0.0).unwrap().0;let vb=self.motion_limits(*b,0.0).unwrap().0;
                let a=Point{x:pa.x+sa.angle.cos()*va*horizon,y:pa.y+sa.angle.sin()*va*horizon};
                let b=Point{x:pb.x+sb.angle.cos()*vb*horizon,y:pb.y+sb.angle.sin()*vb*horizon};
                self.distance_squared(approach,a).total_cmp(&self.distance_squared(approach,b))
            }).unwrap();
            let other=self.segments[partner.0*MAX_SEGMENTS].current;let v=self.motion_limits(partner.0,0.0).unwrap().0;
            let projected=Point{x:other.x+partner.1.angle.cos()*v*horizon,y:other.y+partner.1.angle.sin()*v*horizon};
            let d=self.displacement(approach,projected);
            let mut p=self.canonical_point(Point{x:approach.x+d.x*0.5,y:approach.y+d.y*0.5});
            // Half the candidates centre the four nearest projected heads.
            // Pair-only placement stranded the fourth racer beyond the meal.
            if attempt%2!=0 {
                let mut nearby=[(f64::INFINITY,Point::default());4];
                for (other,snake) in self.snakes.iter().enumerate().filter(|(_,s)|s.alive) {
                    let h=self.segments[other*MAX_SEGMENTS].current;
                    let v=self.motion_limits(other,0.0).unwrap().0;
                    let projected=Point{x:h.x+snake.angle.cos()*v*horizon,y:h.y+snake.angle.sin()*v*horizon};
                    let distance=self.distance_squared(approach,projected);
                    if let Some(i)=nearby.iter().position(|&(d,_)|distance<d) {
                        for j in (i+1..nearby.len()).rev() {nearby[j]=nearby[j-1];}
                        nearby[i]=(distance,projected);
                    }
                }
                let count=live.min(4);let mut offset=Point::default();
                for &(_,point) in &nearby[..count] {
                    let d=self.displacement(approach,point);offset.x+=d.x;offset.y+=d.y;
                }
                p=self.canonical_point(Point{x:approach.x+offset.x/count as f64,y:approach.y+offset.y/count as f64});
            }
            if self.config.deadly_walls {p.x=p.x.clamp(margin,self.config.width-margin);p.y=p.y.clamp(margin,self.config.height-margin);}
            if self.items.iter().any(|item|self.distance_squared(p,item.position)<(20.0*r).powi(2)) {continue;}
            // Same six-radius continuous-body clearance as announced prizes.
            let mut clearance=f64::INFINITY;let mut etas=[f64::INFINITY;4];
            for (id,s) in self.snakes.iter().enumerate().filter(|(_,s)|s.alive) {
                let body=&self.segments[id*MAX_SEGMENTS..id*MAX_SEGMENTS+s.len];
                let mut nearest=self.distance_squared(p,body[0].current);
                for edge in body.windows(2) {nearest=nearest.min(self.segment_distance_squared(p,edge[0].current,edge[1].current));}
                clearance=clearance.min(nearest.sqrt()-s.radius);
                let d=self.displacement(body[0].current,p);let distance=(d.x*d.x+d.y*d.y).sqrt();
                let (speed,turn)=self.motion_limits(id,0.0).unwrap();
                let eta=(distance-radius).max(0.0)/speed+normalize_angle(d.y.atan2(d.x)-s.angle).abs()/turn;
                if let Some(i)=etas.iter().position(|&e|eta<e) {
                    for j in (i+1..etas.len()).rev() {etas[j]=etas[j-1];}etas[i]=eta;
                }
            }
            if clearance<6.0*r {continue;}
            // Prefer a whole reachable crowd; retain two-head placement in sparse worlds.
            let last=etas[live.min(4)-1];
            // Balanced arrival times give the crowd a chance before one head clears the stars.
            let utility=-last-2.0*(last-etas[0]).max(0.0)+clearance.min(12.0*r)*0.001;
            if utility>score {score=utility;best=Some(p);}
        }
        best
    }
    fn start_starfall(&mut self)->bool {
        let Some(p)=self.starfall_location() else {return false;};
        let tick=self.tick+1;let radius=12.0*self.config.base_radius();
        self.event_schedule.star_start=Some(tick);self.event_schedule.emitted=0;self.event_schedule.participants=0;self.event_schedule.commitments=0;
        self.event_schedule.feeding_until=tick+STARFALL_LIFE+900;
        self.event_schedule.zone=p;self.event_schedule.zone_radius=radius;
        self.event_schedule.stats.starfalls+=1;
        let diagonal=(self.rng.random()*4.0) as u8;
        self.event_schedule.diagonal=Point {x:if diagonal&1==0 {1.0} else {-1.0},y:if diagonal&2==0 {1.0} else {-1.0}};
        let travel=34.0*self.config.base_radius()/2.0_f64.sqrt();
        self.event_schedule.diagonal.x*=travel;self.event_schedule.diagonal.y*=travel;
        for plan in &mut self.event_schedule.plans {
            let a=self.rng.random()*TAU;let r=self.rng.random().sqrt()*radius;
            plan.landing=self.config.geometry().wrap(Point{x:p.x+a.cos()*r,y:p.y+a.sin()*r});
            plan.phase=self.rng.random()*TAU;
        }
        self.world_event.start_tick=tick;self.world_event.end_tick=tick+STARFALL_LIFE;
        self.world_event.x=p.x as f32;self.world_event.y=p.y as f32;self.world_event.radius=radius as f32;
        self.world_event.kind=1;self.world_event.phase=1;
        self.world_event_marker(1,true,STARFALL_LIFE as u16);true
    }
    pub(super) fn advance_world_events(&mut self) {
        // Diagnostic fixtures can opt into V2 after constructing a Classic
        // world. Zero is an unscheduled deadline, never an immediate event.
        if self.event_schedule.star_due==0 || self.event_schedule.night_due==0 {self.reset_events();}
        let tick=self.tick+1;
        if tick>=self.event_schedule.star_due {
            if self.snakes.iter().filter(|s|s.alive).count()>=2 {self.start_starfall();}
            self.schedule_star(tick);
        }
        if tick>=self.event_schedule.night_due {
            self.event_schedule.night_start=Some(tick);self.event_schedule.stats.nightfalls+=1;
            self.world_event_marker(2,true,NIGHT_LIFE as u16);self.schedule_night(tick);
        }
        let night=if let Some(start)=self.event_schedule.night_start {
            let age=tick-start;
            if age>=NIGHT_LIFE {self.event_schedule.night_start=None;self.world_event_marker(2,false,0);0.0}
            else {night_intensity(tick,start)}
        } else {0.0};
        self.world_event.night=night;self.world_event.ambient=1.0-0.72*night;
        if let Some(start)=self.event_schedule.star_start {
            let age=tick-start;
            if age>=STARFALL_LIFE {
                self.event_schedule.star_start=None;self.world_event_marker(1,false,0);
            } else {
                self.world_event.kind=1;self.world_event.phase=if age<TELEGRAPH {1} else {2};
                // Integer half-ticks alternate 3/2 spacing without rounding drift.
                while self.event_schedule.emitted<METEORS && age>=TELEGRAPH+(self.event_schedule.emitted as u64*5).div_ceil(2) {
                    let j=self.event_schedule.emitted;self.event_schedule.emitted+=1;
                    if self.food.len()>=self.config.maximum_food() {
                        if let Some(i)=self.food.iter().position(|f|f.owner<0 && !matches!(f.kind,FoodKind::Prism|FoodKind::PrismSeed|FoodKind::Meteor)) {self.food.remove(i);}
                    }
                    if self.food.len()>=MAX_FOOD {continue;}
                    let plan=self.event_schedule.plans[j];let d=self.event_schedule.diagonal;
                    let origin=Point{x:plan.landing.x-d.x,y:plan.landing.y-d.y};
                    // Deliberately bypass add_food's random phase and wall clamp:
                    // a meteor may enter the visible world from beyond a wall.
                    self.food.push(Food {id:self.next_food,p:self.canonical_point(origin),target:plan.landing,
                        kind:FoodKind::Meteor,value:1.0,size:self.config.base_radius()*0.36,
                        phase:plan.phase,life:30.0,original_life:30.0,owner:-1,ripe_tick:tick+FLIGHT,
                        motion_origin:origin,motion_ticks:FLIGHT as u16,velocity:Point{x:d.x/(FLIGHT as f64*STEP_SECONDS),y:d.y/(FLIGHT as f64*STEP_SECONDS)},
                        ..Food::default()});
                    self.next_food=self.next_food.wrapping_add(1);
                }
            }
        }
        if self.event_schedule.star_start.is_none() {
            if tick>=self.event_schedule.feeding_until || !self.food.iter().any(|f|f.kind==FoodKind::Star) {
                self.event_schedule.feeding_until=0;
            }
            if let Some(start)=self.event_schedule.night_start {
                self.world_event.kind=2;self.world_event.phase=if tick-start<90 || tick-start>NIGHT_LIFE-90 {3} else {2};
                self.world_event.start_tick=start;self.world_event.end_tick=start+NIGHT_LIFE;
                self.world_event.radius=0.0;
            } else {self.world_event.kind=0;self.world_event.phase=0;self.world_event.radius=0.0;}
        }
        self.world_event.meteor_count=if self.event_schedule.star_start.is_some() {self.food.iter().filter(|f|f.kind==FoodKind::Meteor).count() as u8} else {0};
    }
    pub(super) fn advance_meteor(&mut self,index:usize)->bool {
        let f=&mut self.food[index];
        if matches!(f.kind,FoodKind::Meteor|FoodKind::Star) {f.attraction=0.0;}
        if f.kind==FoodKind::Star && f.ripe_tick!=0 {
            if f.owner<0 {
                let remaining=(f.ripe_tick+900).saturating_sub(self.tick+1);
                if remaining==0 {self.food.remove(index);return true;}
                f.life=remaining as f64*STEP_SECONDS;
            }
            return true;
        }
        if f.kind!=FoodKind::Meteor {return false;}
        f.motion_ticks=f.ripe_tick.saturating_sub(self.tick+1).min(FLIGHT) as u16;
        let q=1.0-f.motion_ticks as f64/FLIGHT as f64;
        let p=Point{x:f.motion_origin.x+f.velocity.x*FLIGHT as f64*STEP_SECONDS*q,
            y:f.motion_origin.y+f.velocity.y*FLIGHT as f64*STEP_SECONDS*q};
        f.p=if self.config.deadly_walls {p} else {self.config.geometry().wrap(p)};
        if f.motion_ticks==0 {f.kind=FoodKind::Star;f.p=f.target;f.velocity=Point::default();f.life=30.0;f.original_life=30.0;}
        true
    }
    pub(super) fn observe_starfall(&mut self) {
        let Some((p,radius,_))=self.starfall_target() else {return;};
        for (id,_) in self.snakes.iter().enumerate().filter(|(_,s)|s.alive) {
            if self.event_schedule.participants&(1<<id)==0 && self.distance_squared(p,self.segments[id*MAX_SEGMENTS].current)<radius.powi(2) {
                self.event_schedule.participants|=1<<id;self.event_schedule.stats.participants+=1;
            }
        }
        if self.event_schedule.star_start.is_some() {
            self.event_schedule.stats.frenzy_kills+=self.frame_events().filter(|e|e.kind==EventKind::Kill).count() as u32;
        }
        self.event_schedule.stats.zone_kills+=self.frame_events().filter(|e|e.kind==EventKind::Kill
            && self.distance_squared(p,e.position)<radius.powi(2)).count() as u32;
        if self.event_schedule.star_start.is_some_and(|start|self.tick.saturating_sub(start)<TELEGRAPH) {
            for (id,_) in self.snakes.iter().enumerate().filter(|(id,s)|s.alive && self.faces[*id].target_id==ZONE_ID) {
                if self.event_schedule.commitments&(1<<id)==0 {
                    self.event_schedule.commitments|=1<<id;self.event_schedule.stats.commitments+=1;
                }
            }
        }
        self.event_schedule.race_ids=[u32::MAX;2];self.event_schedule.race_etas=[f64::INFINITY;2];self.event_schedule.race_leader=u32::MAX;
        let mut distances=[f64::INFINITY;2];let mut leading=f64::INFINITY;
        for (id,s) in self.snakes.iter().enumerate().filter(|(id,s)|s.alive && self.faces[*id].target_id==ZONE_ID) {
            let d=self.displacement(self.segments[id*MAX_SEGMENTS].current,p);
            let distance=(d.x*d.x+d.y*d.y).sqrt();let (speed,turn)=self.motion_limits(id,0.0).unwrap();
            let eta=(distance-radius).max(0.0)/speed+normalize_angle(d.y.atan2(d.x)-s.angle).abs()/turn;
            if eta<leading {leading=eta;self.event_schedule.race_leader=id as u32;}
            if let Some(i)=(0..2).find(|&i|distance<distances[i]) {
                if i==0 {distances[1]=distances[0];self.event_schedule.race_ids[1]=self.event_schedule.race_ids[0];self.event_schedule.race_etas[1]=self.event_schedule.race_etas[0];}
                distances[i]=distance;self.event_schedule.race_ids[i]=id as u32;self.event_schedule.race_etas[i]=eta;
            }
        }
    }
    pub(super) fn scale_world_events(&mut self,old:Config) {
        let sx=self.config.width/old.width;let sy=self.config.height/old.height;
        self.world_event.x*=sx as f32;self.world_event.y*=sy as f32;
        self.world_event.radius=if self.event_schedule.star_start.is_some() {self.event_schedule.zone_radius as f32} else {0.0};
        self.event_schedule.zone.x*=sx;self.event_schedule.zone.y*=sy;
        // Keep the flight's affine resize, enclosing its landing ellipse in
        // the same circular telegraph/AI objective. Derive the envelope from
        // plans so successive unequal resizes cannot compound empty margins.
        let mut radius=12.0*self.config.base_radius();
        for plan in &self.event_schedule.plans {
            let d=old.geometry().delta(Point{x:self.event_schedule.zone.x/sx,y:self.event_schedule.zone.y/sy},plan.landing);
            radius=radius.max((d.x*sx).hypot(d.y*sy));
        }
        self.event_schedule.zone_radius=radius;
        if self.event_schedule.star_start.is_some() {self.world_event.radius=radius as f32;}
        self.event_schedule.diagonal.x*=sx;self.event_schedule.diagonal.y*=sy;
        for p in &mut self.event_schedule.plans {p.landing.x*=sx;p.landing.y*=sy;}
        // scale_geometry handles current positions/velocities, these are cold.
        for f in &mut self.food {if matches!(f.kind,FoodKind::Meteor|FoodKind::Star) {f.motion_origin.x*=sx;f.motion_origin.y*=sy;}}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::BaselineController;
    fn arena()->World {
        World::diagnostic_arena(Config {width:3440.0,height:1440.0,rules:RuleSet::V2,power_ups:false,
            density:0.0,deadly_walls:true,..Default::default()},&[
            (Point{x:1300.0,y:600.0},0.0,24,0.0),(Point{x:1750.0,y:800.0},std::f64::consts::PI,24,0.0)],&[]).unwrap()
    }
    #[test]
    fn schedules_are_absolute_bounded_and_classic_never_draws() {
        let a=World::new(Config {rules:RuleSet::Classic,world_events:true,..Default::default()}).unwrap();
        let b=World::new(Config {rules:RuleSet::Classic,world_events:false,..Default::default()}).unwrap();
        assert_eq!(a.rng_state(),b.rng_state());
        let mut w=arena();assert!((5400..=9000).contains(&w.event_schedule.star_due));assert!((12600..=18000).contains(&w.event_schedule.night_due));
        w.tick=u16::MAX as u64;w.reset_events();assert!(w.event_schedule.star_due>u16::MAX as u64);
        let mut snapshot=w.diagnostic_snapshot();
        w.step(&mut BaselineController);snapshot.step(&mut BaselineController);
        assert_eq!(w.world_event,snapshot.world_event);assert_eq!(w.rng_state(),snapshot.rng_state());
    }
    #[test]
    fn enabling_v2_schedules_future_events_instead_of_starting_night_immediately() {
        let mut w=World::new(Config {rules:RuleSet::Classic,..Default::default()}).unwrap();
        let mut config=w.config();config.rules=RuleSet::V2;
        w.reconfigure(config).unwrap();
        assert!((5400..=9000).contains(&w.event_schedule.star_due));
        assert!((12600..=18000).contains(&w.event_schedule.night_due));
        w.step(&mut BaselineController);
        assert_eq!(w.world_event.night,0.0);assert_eq!(w.event_schedule.stats.nightfalls,0);
        // Direct configuration changes used by diagnostic mechanics fixtures.
        let mut w=World::new(Config {rules:RuleSet::Classic,..Default::default()}).unwrap();
        w.config.rules=RuleSet::V2;w.step(&mut BaselineController);
        assert_eq!(w.world_event.night,0.0);
        assert!(w.event_schedule.night_due>=12600);
    }
    #[test]
    fn twenty_four_meteors_launch_alternate_half_ticks_and_land_in_disk() {
        let mut w=arena();assert!(w.start_starfall());let start=w.event_schedule.star_start.unwrap();
        let p=Point{x:w.world_event.x as f64,y:w.world_event.y as f64};let r=w.world_event.radius as f64;
        // Remove snakes after placement: no feeding or ordinary respawn RNG.
        for s in &mut w.snakes {s.alive=false;}
        w.food.clear();let rng=w.rng_state();let mut launches=0;
        for age in 0..STARFALL_LIFE {
            w.tick=start+age-1;
            for i in (0..w.food.len()).rev() {w.advance_meteor(i);}
            let previous=w.food.len();w.advance_world_events();
            if w.food.len()>previous {
                assert_eq!(age,TELEGRAPH+(launches*5u64).div_ceil(2));launches+=1;
                let f=w.food.last().unwrap();assert_eq!(f.motion_ticks,14);
                assert!(!f.pickup_eligible(u64::MAX),"in-flight food is never edible, even in long forecasts");
            }
            for f in &w.food {
                if f.kind==FoodKind::Meteor {assert!(f.motion_ticks>0);}
                if f.kind==FoodKind::Star {assert!(w.distance_squared(p,f.p)<=r*r+0.1);assert!(f.pickup_eligible(w.tick+1));}
            }
        }
        assert_eq!(launches,24);assert_eq!(w.food.len(),24);assert!(w.food.iter().all(|f|f.kind==FoodKind::Star));
        assert_eq!(w.rng_state(),rng,"all flight randomness was drawn at announcement");
    }
    #[test]
    fn night_fades_independently_of_starfall_with_exact_duration_and_sleepy_priority() {
        let mut w=arena();w.event_schedule.night_due=1;w.advance_world_events();
        assert_eq!(w.world_event.night,0.0);assert_eq!(w.world_event.kind,2);
        for (age,night) in [(45,0.5),(90,1.0),(660,1.0),(705,0.5),(750,0.0)] {
            w.tick=age;w.advance_world_events();assert!((w.world_event.night-night).abs()<1e-6);
            assert!((w.world_event.ambient-(1.0-0.72*night)).abs()<1e-6);
        }
        w.tick=0;w.event_schedule.night_due=1;w.advance_world_events();w.tick=90;w.advance_world_events();
        assert!(w.start_starfall());w.advance_world_events();assert_eq!(w.world_event.kind,1);assert_eq!(w.world_event.night,1.0);
        w.snakes[0].intent_flags=flags::HUNTING;
        for _ in 0..8 {w.update_presentation();}
        assert_eq!(w.faces[0].mood,Mood::Hunting);assert_eq!(w.faces[1].mood,Mood::Sleepy);
        w.reconfigure(Config {world_events:false,..w.config}).unwrap();assert_eq!(w.world_event,WorldEventState::default());
        assert!(!w.food.iter().any(|f|matches!(f.kind,FoodKind::Meteor|FoodKind::Star)));
    }
    #[test]
    fn dawn_forecast_and_frost_replacement_match_executed_motion() {
        let mut w=arena();w.food.clear();w.items.clear();
        w.config.power_ups=true;w.item_timer=1000;w.prism_timer=1000;
        w.snakes[1].alive=false;w.snakes[1].len=0;w.snakes[1].respawn=1000.0;
        w.event_schedule.night_due=1;w.advance_world_events();
        w.tick=684;w.advance_world_events();w.tick=685;
        w.snakes[0].frozen_ticks=40;w.snakes[0].effect_ticks=60;
        w.snakes[0].effect_kind=effects::EffectKind::Surge as u8;
        w.food.resize(w.config.food_count(),Food {p:Point{x:10.0,y:10.0},life:1000.0,value:0.0,owner:-1,..Default::default()});
        let original=w.diagnostic_snapshot();
        let schedule=original.forecast_motion_schedule(0,0.0).unwrap();
        let mut straight=crate::controller::ScriptedController::new(|_,s:SnakeView<'_>|crate::controller::Steering {desired_angle:s.angle,rush:0.0});
        for offset in 0..90 {
            let expected=original.forecast_motion_limits(0,0.0,offset).unwrap();
            w.step(&mut straight);
            assert_eq!(expected,w.motion_limits(0,0.0).unwrap(),"offset={offset}");
            if offset<25 {assert_eq!(schedule[offset],expected);}
        }
    }

    #[test]
    fn landing_lifetime_and_resize_preserve_the_absolute_flight() {
        let mut w=arena();w.food.clear();
        w.food.push(Food {id:88,p:Point{x:700.0,y:500.0},target:Point{x:900.0,y:700.0},motion_origin:Point{x:500.0,y:300.0},
            velocity:Point{x:400.0/(14.0*STEP_SECONDS),y:400.0/(14.0*STEP_SECONDS)},kind:FoodKind::Meteor,
            ripe_tick:14,motion_ticks:7,value:1.0,owner:-1,life:30.0,..Default::default()});
        w.tick=7;w.resize(6880.0,2880.0).unwrap();w.advance_meteor(0);
        assert_eq!(w.food[0].motion_origin,Point{x:1000.0,y:600.0});
        assert!((w.food[0].p.x-(1000.0+800.0*8.0/14.0)).abs()<1e-8);
        w.tick=13;w.advance_meteor(0);assert_eq!(w.food[0].p,Point{x:1800.0,y:1400.0});
        w.tick=912;w.advance_meteor(0);assert!((w.food[0].life-STEP_SECONDS).abs()<1e-9);
        w.tick=913;w.advance_meteor(0);assert!(w.food.is_empty());
    }
    #[test]
    fn meteor_passes_a_head_and_magnet_unclaimed_then_star_is_collectible() {
        let mut w=arena();w.food.clear();let p=w.segments[0].current;
        w.snakes[0].effect_kind=effects::EffectKind::Magnet as u8;w.snakes[0].effect_ticks=100;
        w.food.push(Food {id:88,p,target:p,kind:FoodKind::Meteor,value:1.0,owner:-1,ripe_tick:14,motion_ticks:14,life:30.0,..Default::default()});
        w.feed_snakes(STEP_SECONDS);assert_eq!(w.food.len(),1);assert_eq!(w.food[0].owner,-1);
        w.tick=13;w.advance_meteor(0);assert_eq!(w.food[0].kind,FoodKind::Star);
        w.feed_snakes(STEP_SECONDS);assert_eq!(w.event_schedule.stats.stars_eaten,1);
    }
}

#[cfg(test)]
mod advisory_regressions {
    use super::*;
    use crate::controller::{BaselineController,Controller,Steering,FaceIntent};
    use crate::ffi::{FrameInfo,SnakeRecord,SegmentRecord,FoodRecord,FACE_OBSERVED};
    use crate::render::{Renderer,Params,Vertex,ShaderVertex,Color};
    fn arena(walls:bool)->World {
        World::diagnostic_arena(Config {width:3440.0,height:1440.0,rules:RuleSet::V2,power_ups:false,
            density:0.0,deadly_walls:walls,self_collisions:false,..Default::default()},&[
            (Point{x:1300.0,y:600.0},0.0,24,0.0),(Point{x:1750.0,y:800.0},std::f64::consts::PI,24,0.0)],&[]).unwrap()
    }
    fn frame(w:&World)->FrameInfo {
        FrameInfo {tick:w.tick,simulation_time:w.time,world_width:w.config.width,world_height:w.config.height,
            world_event:w.world_event,ambient:w.world_event.ambient,..Default::default()}
    }
    fn params(w:&World)->Params {
        Params {viewport_width:w.config.width,viewport_height:w.config.height,scale_x:1.0,scale_y:1.0,
            interpolation:1.0,presentation_time:w.time,deadly_walls:w.config.deadly_walls as u32,..Default::default()}
    }
    // Mirror the ABI's borrowed snapshot mapping; all flags and geometry come
    // from the completed production step, rather than synthetic race ETAs.
    fn records(w:&World)->(Vec<SnakeRecord>,Vec<SegmentRecord>) {
        let mut records=Vec::new();let mut segments=Vec::new();
        for s in w.snakes() {
            records.push(SnakeRecord {id:s.id,generation:s.generation,alive:s.alive as u32,color_index:s.color_index,
                radius:s.radius,angle:s.angle,desired_angle:s.desired_angle,flags:s.flags,
                segment_offset:segments.len() as u32,segment_count:s.segments.len() as u32,
                face_flags:w.event_race_flags(s.id)|FACE_OBSERVED,mood:s.face.mood as u8,mood_intensity:s.face.intensity,
                target_item:255,..Default::default()});
            segments.extend(s.segments.iter().map(|s|SegmentRecord {x:s.current.x as f32,y:s.current.y as f32,
                previous_x:s.previous.x as f32,previous_y:s.previous.y as f32}));
        }
        (records,segments)
    }
    fn food(f:&Food)->FoodRecord {
        FoodRecord {id:f.id,x:f.p.x as f32,y:f.p.y as f32,size:f.size as f32,kind:f.kind as u8,
            attraction_x:f.target.x as f32,attraction_y:f.target.y as f32,motion_origin_x:f.motion_origin.x as f32,
            motion_origin_y:f.motion_origin.y as f32,motion_ticks:f.motion_ticks,life_fraction:255,..Default::default()}
    }
    #[test]
    fn advisory_resize_contains_planned_flying_and_landed_stars_through_step() {
        for walls in [false,true] {for age in [0,50,130] {
            let mut w=arena(walls);w.diagnostic_event_schedule(1,10000);w.step(&mut BaselineController);
            assert!(w.event_schedule.star_start.is_some());
            for s in &mut w.snakes {s.alive=false;s.len=0;s.respawn=1000.0;}
            for _ in 0..age {w.step(&mut BaselineController);}
            for (width,height) in [(6880.0,1440.0),(1720.0,2880.0),(3440.0,1440.0)] {
                let old=w.config;let flights:Vec<_>=w.food.iter().filter(|f|f.kind==FoodKind::Meteor).copied().collect();
                w.resize(width,height).unwrap();let (center,radius,_)=w.starfall_target().unwrap();
                for plan in &w.event_schedule.plans {assert!(w.distance_squared(center,plan.landing)<=radius.powi(2)+1e-6);}
                for f in w.food.iter().filter(|f|matches!(f.kind,FoodKind::Meteor|FoodKind::Star)) {
                    assert!(w.distance_squared(center,f.target)<=radius.powi(2)+1e-6,"age={age} resize={width},{height}");
                }
                if w.world_event.kind==1 {assert!((w.world_event.radius as f64-radius).abs()<1e-4);}
                w.step(&mut BaselineController);
                for before in flights {
                    let after=w.food.iter().find(|f|f.id==before.id).unwrap();
                    let q=1.0-after.motion_ticks as f64/FLIGHT as f64;
                    let expected=w.canonical_point(Point{x:(before.motion_origin.x+before.velocity.x*FLIGHT as f64*STEP_SECONDS*q)*width/old.width,
                        y:(before.motion_origin.y+before.velocity.y*FLIGHT as f64*STEP_SECONDS*q)*height/old.height});
                    assert!(w.distance_squared(expected,after.p)<1e-16,"flight must remain continuous");
                }
            }
            for _ in 0..STARFALL_LIFE {w.step(&mut BaselineController);}
            let (center,radius,_)=w.starfall_target().unwrap();
            let stars:Vec<_>=w.food.iter().filter(|f|f.kind==FoodKind::Star).collect();assert_eq!(stars.len(),METEORS);
            assert!(stars.iter().all(|f|w.distance_squared(center,f.p)<=radius.powi(2)+1e-6));
        }}
    }
    #[test]
    fn advisory_zone_copy_bounds_match_explicit_screen_tiles_through_step() {
        let mut w=arena(false);w.resize(1000.0,1000.0).unwrap();w.diagnostic_event_schedule(1,10000);
        w.step(&mut BaselineController);assert_eq!(w.world_event.kind,1);
        // Place a valid announced disk next to a seam before its next step.
        w.event_schedule.zone=Point{x:150.0,y:150.0};w.event_schedule.zone_radius=100.0;
        w.world_event.x=150.0;w.world_event.y=150.0;w.world_event.radius=100.0;
        w.step(&mut BaselineController);
        let f=frame(&w);let pal=[Color {red:255,green:222,blue:86,alpha:255}];
        for (sx,sy) in [(0.5,2.0),(2.0,0.5),(0.25,4.0),(0.001,0.001)] {
            let mut p=params(&w);p.scale_x=sx;p.scale_y=sy;p.viewport_width=1000.0*sx;p.viewport_height=1000.0*sy;
            let mut classic=vec![Vertex::default();10000];let mut shader=vec![ShaderVertex::default();10000];
            let a=Renderer::new().build(&f,&[],&[],&[],&[],&pal,&p,&mut classic).vertex_count;
            let b=Renderer::new().build_shader(&f,&[],&[],&[],&[],&pal,&p,&mut shader).vertex_count;
            let mut expected=(0,0);
            for x in -2..=2 {for y in -2..=2 {
                let mut tile=p;tile.deadly_walls=1;tile.offset_x=x as f64*1000.0*sx;tile.offset_y=y as f64*1000.0*sy;
                expected.0+=Renderer::new().build(&f,&[],&[],&[],&[],&pal,&tile,&mut classic).vertex_count;
                expected.1+=Renderer::new().build_shader(&f,&[],&[],&[],&[],&pal,&tile,&mut shader).vertex_count;
            }}
            assert_eq!((a,b),expected,"projection {sx},{sy}");
        }
    }
    #[test]
    fn advisory_starfall_contested_flag_survives_third_leader_through_step() {
        let mut w=arena(true);w.diagnostic_event_schedule(1,10000);w.step(&mut BaselineController);
        w.event_schedule.zone=Point{x:2200.0,y:700.0};w.world_event.x=2200.0;w.world_event.y=700.0;
        struct Racers;
        impl Controller for Racers {
            fn steer(&mut self,_:&World,s:SnakeView<'_>)->Steering {Steering {desired_angle:s.angle,rush:0.0}}
            fn face_intent(&self,_:u32)->FaceIntent {FaceIntent {target_id:ZONE_ID,has_target:true,..Default::default()}}
        }
        for contested in [false,true] {for third_leader in [false,true] {
            for id in 0..3 {
                let s=&mut w.snakes[id];s.alive=true;s.len=24;s.radius=6.0;s.base_radius=6.0;s.angle=0.0;
                s.traits.speed_bias=if id==2 {2.0} else if id==1 && !contested {0.8} else {1.0};
                let x=2200.0-[1100.0,1150.0,if third_leader {1200.0} else {2500.0}][id];
                for j in 0..24 {let p=Point{x:x-j as f64*7.08,y:700.0+id as f64*40.0};w.segments[id*MAX_SEGMENTS+j]=Segment {current:p,previous:p};}
            }
            w.step(&mut Racers);
            assert_eq!(w.event_schedule.race_ids,[0,1]);
            assert_eq!(w.event_schedule.race_leader==2,third_leader);
            assert_eq!(w.event_race_flags(0)&128!=0,contested);
            let (snakes,segments)=records(&w);let mut output=vec![ShaderVertex::default();10000];
            let n=Renderer::new().build_shader(&frame(&w),&snakes,&segments,&[],&[],&[],&params(&w),&mut output).vertex_count;
            let races:Vec<_>=output[..n].iter().filter(|v|v.params[0]==17).collect();assert_eq!(races.len(),12);
            assert!(races.iter().all(|v|(v.params[3]&2!=0)==contested));
            assert!(races.iter().all(|v|v.params[3]&1==0)==third_leader);
        }}
    }
    #[test]
    fn advisory_meteor_tail_uses_screen_direction_and_length_through_step() {
        let mut w=arena(true);w.diagnostic_event_schedule(1,10000);w.step(&mut BaselineController);
        for s in &mut w.snakes {s.alive=false;s.len=0;s.respawn=1000.0;}
        for _ in 0..TELEGRAPH+7 {w.step(&mut BaselineController);}
        let meteor=w.food.iter().find(|f|f.kind==FoodKind::Meteor).unwrap();let f=food(meteor);
        let mut i=frame(&w);i.world_event.kind=0;
        for (sx,sy) in [(0.5,0.5),(2.0,2.0),(0.5,2.0),(2.0,0.5)] {
            let mut p=params(&w);p.scale_x=sx;p.scale_y=sy;p.viewport_width*=sx;p.viewport_height*=sy;
            let mut output=vec![Vertex::default();1000];let mut shader=vec![ShaderVertex::default();1000];
            let n=Renderer::new().build(&i,&[],&[],&[f],&[],&[],&p,&mut output).vertex_count;assert!(n>=6);
            let n=Renderer::new().build_shader(&i,&[],&[],&[f],&[],&[],&p,&mut shader).vertex_count;assert!(n>=12);
            let head=((output[0].x+output[1].x) as f64/2.0,(output[0].y+output[1].y) as f64/2.0);
            let tail=((output[2].x+output[5].x) as f64/2.0,(output[2].y+output[5].y) as f64/2.0);
            let d=(head.0-tail.0,head.1-tail.1);
            let expected=f.size as f64*(sx*sy).sqrt()*12.0;
            assert!((d.0.hypot(d.1)-expected).abs()<0.001,"projection {sx},{sy}, tail={d:?} expected={expected}");
            let projected=((f.attraction_x-f.motion_origin_x) as f64*sx,(f.attraction_y-f.motion_origin_y) as f64*sy);
            assert!((d.0*projected.1-d.1*projected.0).abs()<1.0,"tail must follow projected flight");
            let shader_head=((shader[0].x+shader[1].x) as f64/2.0,(shader[0].y+shader[1].y) as f64/2.0);
            let shader_tail=((shader[2].x+shader[5].x) as f64/2.0,(shader[2].y+shader[5].y) as f64/2.0);
            assert!(((shader_head.0-shader_tail.0).hypot(shader_head.1-shader_tail.1)-expected).abs()<0.001);
        }
    }
    #[test]
    fn advisory_night_corpse_eye_halos_fade_through_step() {
        let mut w=arena(true);w.diagnostic_event_schedule(10000,1);
        for _ in 0..91 {w.step(&mut BaselineController);}
        w.snakes[0].angle=std::f64::consts::PI;
        for j in 0..w.snakes[0].len {
            let p=Point{x:1.0+j as f64*20.0,y:600.0};
            w.segments[j]=Segment {current:p,previous:p};
        }
        w.step(&mut crate::controller::ScriptedController::new(|_,s:SnakeView<'_>|Steering {desired_angle:s.angle,rush:0.0}));assert!(!w.snakes[0].alive && w.snakes[0].corpse_ticks>0);
        let (snakes,segments)=records(&w);let mut out=vec![Vertex::default();20000];let mut r=Renderer::new();
        let pal=[Color {red:30,green:160,blue:200,alpha:255}];
        let mut view=params(&w);view.offset_x=100.0;
        r.build(&frame(&w),&snakes[..1],&segments,&[],&[],&pal,&view,&mut out);
        for _ in 0..15 {w.step(&mut BaselineController);}
        let (snakes,segments)=records(&w);assert!(snakes[0].segment_count>0);
        view.presentation_time=w.time;
        let n=r.build(&frame(&w),&snakes[..1],&segments,&[],&[],&pal,&view,&mut out).vertex_count;
        let whites:Vec<_>=out[..n].iter().filter(|v|v.color.red>140 && v.color.green>180 && v.color.blue>200).collect();
        assert!(!whites.is_empty());
        assert!(whites.iter().all(|v|v.color.alpha<=24),"retained corpse has an unfaded eye/halo: {:?}",whites.iter().map(|v|v.color.alpha).max());
    }
}

#[cfg(test)]
mod resized_zone_limit_regression {
    use super::*;
    use crate::controller::BaselineController;
    use crate::ffi::FrameInfo;
    use crate::render::{Renderer,Params,Vertex,ShaderVertex};
    #[test]
    fn advisory_resized_zone_remains_renderable_at_geometry_limits_through_step() {
        let mut w=World::diagnostic_arena(Config {width:80.0,height:1440.0,rules:RuleSet::V2,
            density:0.0,power_ups:false,deadly_walls:false,self_collisions:false,..Default::default()},&[
            (Point{x:30.0,y:600.0},0.0,24,0.0),(Point{x:50.0,y:800.0},std::f64::consts::PI,24,0.0)],&[]).unwrap();
        w.diagnostic_event_schedule(1,10000);w.step(&mut BaselineController);
        assert_eq!(w.world_event.kind,1);
        for s in &mut w.snakes {s.alive=false;s.len=0;s.respawn=1000.0;}
        w.resize(16384.0,1440.0).unwrap();w.step(&mut BaselineController);
        assert!(w.world_event.radius>4096.0,"fixture must exceed the old render cap");
        let (center,radius,_)=w.starfall_target().unwrap();
        assert!(w.event_schedule.plans.iter().all(|p|w.distance_squared(center,p.landing)<=radius.powi(2)+1e-6));
        let info=FrameInfo {tick:w.tick,simulation_time:w.time,world_width:w.config.width,
            world_height:w.config.height,world_event:w.world_event,ambient:1.0,..Default::default()};
        let params=Params {viewport_width:w.config.width,viewport_height:w.config.height,
            scale_x:1.0,scale_y:1.0,interpolation:1.0,presentation_time:w.time,..Default::default()};
        let mut classic=vec![Vertex::default();10000];let mut shader=vec![ShaderVertex::default();10000];
        assert!(Renderer::new().build(&info,&[],&[],&[],&[],&[],&params,&mut classic).vertex_count>0,"fallback must retain the resized telegraph");
        assert!(Renderer::new().build_shader(&info,&[],&[],&[],&[],&[],&params,&mut shader).vertex_count>0,"shader must retain the resized telegraph");
    }
}
