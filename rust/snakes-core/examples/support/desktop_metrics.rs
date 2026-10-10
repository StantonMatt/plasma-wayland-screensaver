// SPDX-License-Identifier: GPL-3.0-or-later
//! Observer accounting stays outside World::step and is not a production cost.
use snakes_core::{ai::DesktopObservation, *};
#[derive(Clone,Copy,Default)]
struct Episode {generation:u32,target:u64,kind:u8,ticks:u64,start:f64,last:f64,path:f64,near:bool,bearing:f64,heading_change:f64}
pub struct Metrics {
    episodes:[Episode;MAX_SNAKES], previous:[Episode;MAX_SNAKES],positions:[Point;MAX_SNAKES],
    dwell:[Vec<f64>;4],efficiency:Vec<f64>,pub abandons:[u64;9],pub misses:u64,
    pub deliberate:u64,pub escapes:u64,pub competitive:u64,pub retained:u64,
    drift:[Vec<f64>;4],pub short_goals:u64,pub goals:u64,pub deaths_by_length:[[u64;5];3],
}
impl Default for Metrics {fn default()->Self {Self {episodes:[Episode::default();MAX_SNAKES],previous:[Episode::default();MAX_SNAKES],positions:[Point::default();MAX_SNAKES],dwell:Default::default(),efficiency:Vec::new(),abandons:[0;9],misses:0,deliberate:0,escapes:0,competitive:0,retained:0,drift:Default::default(),short_goals:0,goals:0,deaths_by_length:[[0;5];3]}}}
impl Metrics {
    pub fn deliberate(&self,id:usize,generation:u32,target:u64)->bool {
        [self.episodes[id],self.previous[id]].iter().any(|e|e.target==target && e.generation==generation && (e.ticks>=30 || e.heading_change>=0.25))
    }
    pub fn observe(&mut self,w:&World,obs:&[DesktopObservation;MAX_SNAKES],called:&[bool;MAX_SNAKES],captured:&[(u64,usize)],_pre_lengths:&[usize;MAX_SNAKES]) {
        // Mechanics can spawn and kill a life without requesting steering.
        for event in w.collision_events() {self.record_death(event);}
        for id in 0..w.snake_count() {
            let s=w.snake(id).unwrap();let d=obs[id];let mut e=self.episodes[id];
            let current=called[id] && d.generation==s.generation;
            let same=current && e.generation==s.generation && e.target==d.target && s.alive;
            if !same && e.target!=0 {
                let category=e.kind as usize;
                self.dwell[category].push(e.ticks as f64/30.0);
                if e.path>0.0 {self.efficiency.push(((e.start-e.last)/e.path).clamp(-1.0,1.0));}
                let picked=captured.contains(&(e.target,id)) || w.foods().any(|f|f.id==e.target && f.vacuum_owner==id as i32);
                if !picked {
                    let lost=captured.iter().any(|&(target,owner)|target==e.target && owner!=id) || w.foods().any(|f|f.id==e.target && f.vacuum_owner>=0 && f.vacuum_owner!=id as i32);
                    let reason=if !current || !s.alive || e.generation!=s.generation {6} else if lost {7} else {if d.abandon as usize==0 {4} else {d.abandon as usize}};
                    self.abandons[reason]+=1;self.misses+=u64::from(e.near);
                }
                self.previous[id]=e;
            }
            if !s.alive || !current {self.episodes[id]=Episode::default();if e.generation!=s.generation {self.previous[id]=Episode::default();}continue;}
            let p=s.segments[0].current;
            if !same {e=Episode {generation:s.generation,target:d.target,kind:d.target_kind,start:d.distance,last:d.distance,bearing:d.target_bearing.abs(),..Default::default()};}
            else {e.path+=w.distance_squared(self.positions[id],p).sqrt();}
            e.ticks+=1;e.last=d.distance;e.near|=d.distance<=2.0*d.reach;
            if e.ticks<=45 {e.heading_change=e.heading_change.max(e.bearing-d.target_bearing.abs());}
            self.positions[id]=p;self.episodes[id]=e;
            self.escapes+=u64::from(d.escape_started);
            if !d.reused_plan {self.competitive+=1;if d.selected==1 && d.target!=0 {self.retained+=1;let bucket=if d.distance<150.0 {0} else if d.distance<400.0 {1} else if d.distance<800.0 {2} else {3};self.drift[bucket].push(d.retained_drift);}}
            if d.target!=0 {self.goals+=1;self.short_goals+=u64::from(w.distance_squared(p,d.goal)<=400.0*400.0);}
        }
    }
    fn record_death(&mut self,event:&CollisionEvent) {
        let cohort=if event.victim_length<40 {0} else if event.victim_length<100 {1} else {2};
        self.deaths_by_length[cohort][event.reason as usize]+=1;
    }
    pub fn save(&self,prefix:&str) {
        use std::io::Write;
        let mut out=std::io::BufWriter::new(std::fs::File::create(format!("{prefix}.intent.json")).unwrap());
        writeln!(out,"{{\"dwell\":{:?},\"efficiency\":{:?},\"drift\":{:?},\"abandons\":{:?},\"deliberate\":{},\"misses\":{},\"escapes\":{},\"competitive\":{},\"retained\":{},\"short_goals\":{},\"goals\":{},\"deaths_by_length\":{:?}}}",self.dwell,self.efficiency,self.drift,self.abandons,self.deliberate,self.misses,self.escapes,self.competitive,self.retained,self.short_goals,self.goals,self.deaths_by_length).unwrap();
    }
    pub fn print(&mut self,live_ticks:u64) {
        fn median(v:&mut [f64])->f64 {v.sort_unstable_by(f64::total_cmp);if v.is_empty(){0.0}else{v[v.len()/2]}}
        let dwell=self.dwell.each_mut().map(|v|median(v));let drift=self.drift.each_mut().map(|v|median(v));
        println!("intent dwell_food_s={:.3} dwell_capsule_s={:.3} dwell_prism_s={:.3} efficiency_median={:.3} abandons={:?} abandon_snake_min={:.3} passby_misses={} deliberate_food={} escape_entries={} escape_snake_min={:.3} competitive={} retained_target_wins={} drift_median_rad={:?} goal_short_pct={:.3} deaths_length_cohorts={:?}",dwell[0],dwell[1],dwell[2],median(&mut self.efficiency),self.abandons,self.abandons.iter().sum::<u64>() as f64/(live_ticks as f64/1800.0),self.misses,self.deliberate,self.escapes,self.escapes as f64/(live_ticks as f64/1800.0),self.competitive,self.retained,drift,100.0*self.short_goals as f64/self.goals.max(1) as f64,self.deaths_by_length);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snakes_core::controller::{Controller,Steering};
    struct Straight {called:[bool;MAX_SNAKES],obs:[DesktopObservation;MAX_SNAKES]}
    impl Controller for Straight {
        fn steer(&mut self,_:&World,s:SnakeView<'_>)->Steering {
            self.called[s.id as usize]=true;
            self.obs[s.id as usize]=DesktopObservation {generation:s.generation,..Default::default()};
            Steering {desired_angle:s.angle,rush:0.0}
        }
    }
    #[test]
    fn every_collision_is_counted_even_when_a_newborn_has_no_controller_call() {
        let mut w=World::new(Config {width:80.0,height:80.0,scale:1000.0,density:100.0,self_collisions:true,seed:73,..Default::default()}).unwrap();
        let mut c=Straight {called:[false;MAX_SNAKES],obs:[DesktopObservation::default();MAX_SNAKES]};
        let mut metrics=Metrics::default();let mut newborn=0;
        for _ in 0..360 {
            c.called.fill(false);w.step(&mut c);
            newborn+=w.collision_events().filter(|e|!c.called[e.victim as usize]).count();
            // Deliberately stale giant lengths must never override an event.
            metrics.observe(&w,&c.obs,&c.called,&[],&[6000;MAX_SNAKES]);
        }
        assert!(newborn>0);
        assert_eq!(metrics.deaths_by_length.iter().flatten().sum::<u64>(),w.stats().deaths);
        assert_eq!(metrics.deaths_by_length[2],[0;5]);
    }
    #[test]
    fn stale_called_observations_cannot_start_replacement_episodes() {
        let w=World::new(Config::default()).unwrap();let mut metrics=Metrics::default();
        let mut obs=[DesktopObservation::default();MAX_SNAKES];
        obs[0]=DesktopObservation {generation:w.snake(0).unwrap().generation+1,target:77,escape_started:true,..Default::default()};
        let mut called=[false;MAX_SNAKES];called[0]=true;
        metrics.observe(&w,&obs,&called,&[],&[0;MAX_SNAKES]);
        assert_eq!((metrics.episodes[0].target,metrics.escapes,metrics.goals),(0,0,0));
    }
}
