// SPDX-License-Identifier: GPL-3.0-or-later
//! Observer work is outside timed World::step. Pending prizes are reported
//! separately; traces permit pooling exact pickup delays across RNG seeds.
use snakes_core::{FoodKind,Point,World,EventKind,MAX_SNAKES,normalize_angle};
use super::{diagnostics::ScoreController,accounting::Tactics};
#[derive(Default)]
struct Prize {
    id:u64,ripe:u64,eaten:Option<u64>,resolved:bool,
    contenders:usize,orbit_turn:[f64;MAX_SNAKES],circled:bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use snakes_core::{Config,RuleSet,SnakeView,controller::{Controller,Steering}};
    struct Straight;
    impl Controller for Straight {
        fn steer(&mut self,_:&World,s:SnakeView<'_>)->Steering {
            Steering {desired_angle:s.angle,rush:0.0}
        }
    }
    impl ScoreController for Straight {}
    #[test]
    fn replacement_life_cannot_inherit_prize_orbit_progress() {
        let w=World::diagnostic_arena(Config {rules:RuleSet::V2,..Default::default()},
            &[(Point{x:400.0,y:300.0},0.0,24,0.9)],&[]).unwrap();
        let mut p=Prisms::default();p.generation[0]=w.snake(0).unwrap().generation+1;
        let mut record=Prize::default();record.orbit_turn[0]=0.3;p.records.push(record);
        p.after(&w,&Straight,&[Tactics::default();MAX_SNAKES]);
        assert_eq!(p.records[0].orbit_turn[0],0.0);
        assert!(!p.records[0].circled);
    }
    #[test]
    fn near_prize_kills_require_an_opponent_owner() {
        for opponent in [false,true] {
            let p=Point{x:if opponent {400.0} else {799.0},y:300.0};
            let snakes=if opponent {
                vec![(p,0.0,48,0.9),(p,std::f64::consts::PI,24,0.9)]
            } else {vec![(p,0.0,24,0.9)]};
            let mut w=World::diagnostic_arena(Config {width:800.0,height:600.0,
                density:0.0,speed:50.0,rules:RuleSet::V2,self_collisions:false,
                deadly_walls:true,..Config::default()},&snakes,&[]).unwrap();
            let mut c=Straight;w.step(&mut c);
            // Overlapping bodies kill both opponents; the wall kills one.
            let deaths=if opponent {2} else {1};
            assert_eq!(w.frame_events().filter(|e|e.kind==EventKind::Kill).count(),deaths);
            let mut observer=Prisms {seed:Some(p),prize:Some(p),..Default::default()};
            observer.after(&w,&c,&[Tactics::default();MAX_SNAKES]);
            let owned=if opponent {2} else {0};
            assert_eq!(observer.kills_near_seed,owned);
            assert_eq!(observer.kills_near_prize,owned);
        }
    }
}
#[derive(Default)]
pub struct Prisms {
    records:Vec<Prize>, seed:Option<Point>,prize:Option<Point>,
    active:[bool;MAX_SNAKES],generation:[u32;MAX_SNAKES],
    turn:[f64;MAX_SNAKES],angle:[f64;MAX_SNAKES],
    episodes:u64,full_circles:u64,vulture_ticks:u64,kills_near_seed:u64,kills_near_prize:u64,
}
impl Prisms {
    pub fn before(&mut self,w:&World) {
        self.seed=w.foods().find(|f|f.kind==FoodKind::PrismSeed).map(|f|f.position);
        self.prize=w.foods().find(|f|matches!(f.kind,FoodKind::Prism|FoodKind::PrismSeed)).map(|f|f.position);
    }
    pub fn after<C:ScoreController>(&mut self,w:&World,c:&C,tactics:&[Tactics;MAX_SNAKES]) {
        for s in w.snakes().filter(|s|s.alive) {
            let id=s.id as usize;
            if self.generation[id]!=s.generation {
                for record in &mut self.records {record.orbit_turn[id]=0.0;}
            }
        }
        for f in w.foods().filter(|f|matches!(f.kind,FoodKind::Prism|FoodKind::PrismSeed)) {
            if !self.records.iter().any(|r|r.id==f.id) {
                self.records.push(Prize {id:f.id,ripe:w.tick()+f.motion_ticks as u64,..Default::default()});
            }
            if f.kind==FoodKind::PrismSeed {
                let record=self.records.iter_mut().find(|r|r.id==f.id).unwrap();
                let contenders=w.snakes().filter(|s|s.alive && tactics[s.id as usize].generation==s.generation
                    && tactics[s.id as usize].target==f.id).count();
                record.contenders=record.contenders.max(contenders);
                for s in w.snakes().filter(|s|s.alive) {
                    let id=s.id as usize;
                    if c.ai().is_some_and(|ai|ai.debug(id).is_some_and(|d|d.generation==s.generation) && ai.vulturing(id)) && self.active[id] && self.generation[id]==s.generation {
                        record.orbit_turn[id]+=normalize_angle(s.angle-self.angle[id]).abs();
                        // At least twenty degrees of intentional orbit, rather
                        // than a single boolean frame masquerading as circling.
                        record.circled|=record.orbit_turn[id]>=0.35;
                    }
                }
            }
        }
        for (id,_,_,_,_) in w.consumption_events() {
            if let Some(r)=self.records.iter_mut().find(|r|r.id==id) {r.eaten=Some(w.tick());r.resolved=true;}
        }
        for r in self.records.iter_mut().filter(|r|!r.resolved) {
            if !w.foods().any(|f|f.id==r.id) {r.resolved=true;}
        }
        for id in 0..MAX_SNAKES {
            let snake=w.snake(id);
            let v=snake.is_some_and(|s|s.alive && c.ai().is_some_and(|ai|ai.debug(id).is_some_and(|d|d.generation==s.generation) && ai.vulturing(id)));
            let angle=snake.map_or(0.0,|s|s.angle);
            let generation=snake.map_or(0,|s|s.generation);
            if v {
                self.vulture_ticks+=1;
                if !self.active[id] || self.generation[id]!=generation {self.episodes+=1;self.turn[id]=0.0;}
                else {
                    let old=self.turn[id];self.turn[id]+=normalize_angle(angle-self.angle[id]).abs();
                    if old<std::f64::consts::TAU && self.turn[id]>=std::f64::consts::TAU {self.full_circles+=1;}
                }
            }
            self.angle[id]=angle;self.active[id]=v;self.generation[id]=generation;
        }
        // Kill is also the presentation event for self/wall deaths. Require
        // an opponent owner, matching the scorecard's collision accounting.
        for e in w.frame_events().filter(|e|e.kind==EventKind::Kill && e.other_snake_id!=u32::MAX) {
            let r=w.snake(e.snake_id as usize).map_or(1.0,|s|s.radius);
            if self.seed.is_some_and(|p|w.distance_squared(e.position,p)<(12.0*r).powi(2)) {self.kills_near_seed+=1;}
            if self.prize.is_some_and(|p|w.distance_squared(e.position,p)<(12.0*r).powi(2)) {self.kills_near_prize+=1;}
        }
    }
    pub fn report(&self,trace:bool) {
        let mut times:Vec<_>=self.records.iter().filter_map(|r|r.eaten.map(|t|t.saturating_sub(r.ripe))).collect();times.sort_unstable();
        let contested=self.records.iter().filter(|r|r.contenders>=2).count();
        let circled=self.records.iter().filter(|r|r.contenders>=2 && r.circled).count();
        let pending=self.records.iter().filter(|r|!r.resolved).count();
        let median=if times.is_empty() {f64::NAN} else {(times[(times.len()-1)/2]+times[times.len()/2]) as f64/60.0};
        println!("prisms picked={}/{} pending={pending} pickup_rate={:.2}% ripe_to_eaten_median_s={median:.3} contested_before_ripe={contested} contested_circled={circled} vulture_episodes={} full_circle_episodes={} vulture_snake_s={:.3} kills_within_12r_of_seed={} kills_within_12r_of_prize={}",
            times.len(),self.records.len(),times.len() as f64/self.records.len().max(1) as f64*100.0,
            self.episodes,self.full_circles,self.vulture_ticks as f64/30.0,self.kills_near_seed,self.kills_near_prize);
        if trace {for r in &self.records {
            println!("prism id={} ripe_tick={} eaten_tick={} resolved={} contenders={} circled={} delay_ticks={}",r.id,r.ripe,
                r.eaten.map_or(-1,|t|t as i64),r.resolved,r.contenders,r.circled,r.eaten.map_or(-1,|t|t.saturating_sub(r.ripe) as i64));
        }}
    }
}
